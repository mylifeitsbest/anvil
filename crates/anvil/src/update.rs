//! Обновления: проверка релизов на GitHub, загрузка с проверкой SHA-256, установка с перезапуском.
//!
//! Сеть — только в фоновых потоках; кадр читает канал и атомарные счётчики, поэтому
//! интерфейс не подвисает даже на медленном соединении. Пока проверять нечего, всё это
//! стоит одно сравнение времени за кадр.
//!
//! Как ставится новая версия, зависит от того, как установлен Anvil:
//! - Windows, установщик (рядом лежит `unins000.exe`): новый установщик запускается тихо в ту же
//!   папку, сам дожидается закрытия старого Anvil и открывает новый (`/RELAUNCH=1` → `--updated`);
//! - портативная копия, Linux, macOS: новый исполняемый файл кладётся на место текущего
//!   (работающий процесс это не тревожит), затем Anvil перезапускается;
//! - сборка из исходников: только плашка и ссылка на релиз — свою сборку не подменяем.

use std::cmp::Ordering;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtOrd};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::app::{AnvilApp, ToastKind};

/// Откуда брать релизы.
const API: &str = "https://api.github.com/repos/mylifeitsbest/anvil/releases";
/// Страница со всеми релизами — если своей сборки для платформы нет.
pub const RELEASES_PAGE: &str = "https://github.com/mylifeitsbest/anvil/releases";

// ---------------------------------------------------------------- версии

/// Версия вида `1.2.3` или `1.2.3-beta.1` (сравнение — как в semver).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    nums: [u64; 3],
    pre: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let s = s.split('+').next().unwrap_or(s);
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string()).filter(|p| !p.is_empty())),
            None => (s, None),
        };
        let mut nums = [0u64; 3];
        let mut parts = core.split('.');
        for n in &mut nums {
            match parts.next() {
                Some(p) => *n = p.parse().ok()?,
                None => break,
            }
        }
        if parts.next().is_some() {
            return None;
        }
        Some(Self { nums, pre })
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self { nums: [0, 0, 0], pre: None })
    }

    pub fn is_pre(&self) -> bool {
        self.pre.is_some()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c] = self.nums;
        match &self.pre {
            Some(p) => write!(f, "{a}.{b}.{c}-{p}"),
            None => write!(f, "{a}.{b}.{c}"),
        }
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> Ordering {
        self.nums.cmp(&o.nums).then_with(|| match (&self.pre, &o.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => cmp_pre(a, b),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// `beta.2` < `beta.10` < `rc.1`: числа сравниваются как числа и идут раньше слов.
fn cmp_pre(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- релизы

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 из поля `digest` (GitHub считает его сам при загрузке файла).
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub title: String,
    /// Описание релиза (markdown).
    pub notes: String,
    pub page: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// Один релиз из ответа GitHub API (черновики пропускаются).
pub fn parse_release(v: &Value) -> Option<Release> {
    if v["draft"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    let mut version = Version::parse(tag)?;
    // релиз, отмеченный «pre-release», без суффикса в теге — всё равно предварительный
    if v["prerelease"].as_bool() == Some(true) && version.pre.is_none() {
        version.pre = Some("pre".into());
    }
    let assets = v["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(Asset {
                        name: a["name"].as_str()?.to_string(),
                        url: a["browser_download_url"].as_str()?.to_string(),
                        size: a["size"].as_u64().unwrap_or(0),
                        sha256: a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_ascii_lowercase),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release {
        version,
        title: v["name"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(tag).to_string(),
        notes: v["body"].as_str().unwrap_or_default().to_string(),
        page: v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
        assets,
    })
}

/// Самый новый релиз из ответа: объект (`/releases/latest`) или список (`/releases`).
pub fn newest(v: &Value, prerelease: bool) -> Option<Release> {
    let list: Vec<&Value> = match v.as_array() {
        Some(a) => a.iter().collect(),
        None => vec![v],
    };
    list.into_iter().filter_map(parse_release).filter(|r| prerelease || !r.version.is_pre()).max_by(|a, b| a.version.cmp(&b.version))
}

/// Строка описания релиза без разметки: заголовок или обычный текст.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLine {
    pub heading: bool,
    pub text: String,
}

/// Описание релиза без разметки — для плашки «Что нового».
pub fn plain_notes(md: &str, max_lines: usize) -> Vec<NoteLine> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || t.is_empty() || t.starts_with("![") || t.starts_with("<") || t.starts_with("---") || t.starts_with('|') {
            continue;
        }
        let heading = t.starts_with('#');
        let t = t.trim_start_matches('#').trim();
        let (bullet, t) = match t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            Some(rest) => ("• ", rest),
            None => ("", t),
        };
        let text = format!("{bullet}{}", strip_inline(t));
        if !text.trim().is_empty() {
            out.push(NoteLine { heading, text });
        }
        if out.len() >= max_lines {
            break;
        }
    }
    out
}

/// `**жирный**`, `` `код` ``, `[текст](ссылка)` → просто текст.
fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        // ссылка — только `[текст](адрес)`; `[update]` без адреса остаётся как есть
        let link = after.find(']').filter(|&j| after[j + 1..].starts_with('(')).and_then(|j| after[j + 2..].find(')').map(|k| (j, j + 2 + k)));
        match link {
            Some((j, end)) => {
                out.push_str(&after[..j]);
                rest = &after[end + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "")
}

// ---------------------------------------------------------------- как ставить

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// Windows: тихий запуск нового установщика (Inno Setup) в папку `dir`.
    Installer { dir: PathBuf },
    /// Новый исполняемый файл на место `exe`.
    Replace { exe: PathBuf },
    /// Поставить самостоятельно нельзя — только ссылка на релиз. Внутри — почему.
    Manual(&'static str),
}

impl Method {
    pub fn detect() -> Self {
        let Ok(exe) = std::env::current_exe() else { return Self::Manual("не удалось узнать, где лежит Anvil") };
        let exe = std::fs::canonicalize(&exe).map(strip_verbatim).unwrap_or(exe);
        if cfg!(debug_assertions) || is_cargo_build(&exe) {
            return Self::Manual("это сборка из исходников — обновите её через git pull");
        }
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        if cfg!(windows) && dir.join("unins000.exe").is_file() {
            return Self::Installer { dir };
        }
        if binary_asset().is_none() {
            return Self::Manual("готовой сборки для этой системы нет");
        }
        Self::Replace { exe }
    }

    /// Какой файл релиза нужен.
    pub fn asset_name(&self) -> Option<&'static str> {
        match self {
            Self::Installer { .. } => Some("anvil-setup-x64.exe"),
            Self::Replace { .. } => binary_asset(),
            Self::Manual(_) => None,
        }
    }
}

/// `\\?\C:\…` → `C:\…`: установщику и `Command` нужен обычный путь.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

fn binary_asset() -> Option<&'static str> {
    if cfg!(windows) {
        Some("anvil.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("anvil-linux-x64.tar.gz")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("anvil-macos-arm64.tar.gz")
    } else {
        None
    }
}

/// `…/target/release/anvil` рядом с `Cargo.toml` — своя сборка.
fn is_cargo_build(exe: &Path) -> bool {
    exe.ancestors().skip(1).take(3).any(|d| d.file_name().is_some_and(|n| n == "target") && d.parent().is_some_and(|p| p.join("Cargo.toml").is_file()))
}

// ---------------------------------------------------------------- сеть

fn agent(download: bool) -> ureq::Agent {
    let cfg = ureq::Agent::config_builder()
        .user_agent(concat!("Anvil/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // загрузка может идти сколько угодно, но не стоять на месте
        .timeout_per_read(Some(Duration::from_secs(30)))
        .timeout_global(if download { None } else { Some(Duration::from_secs(30)) })
        .build();
    ureq::Agent::new_with_config(cfg)
}

fn net_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(403 | 429) => "GitHub временно ограничил число запросов — попробуйте позже".into(),
        ureq::Error::StatusCode(404) => "релиз не найден".into(),
        ureq::Error::StatusCode(c) => format!("сервер ответил {c}"),
        ureq::Error::Timeout(_) => "сервер не отвечает".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => "нет соединения с GitHub".into(),
        other => other.to_string(),
    }
}

/// Адрес, где искать релизы. `ANVIL_UPDATE_FEED` — свой адрес (для проверок и зеркал).
pub fn feed_url(prerelease: bool) -> String {
    match std::env::var("ANVIL_UPDATE_FEED") {
        Ok(u) if !u.trim().is_empty() => u,
        _ if prerelease => format!("{API}?per_page=15"),
        _ => format!("{API}/latest"),
    }
}

/// Самый новый релиз (`None` — релизов нет).
pub fn fetch(url: &str, prerelease: bool) -> Result<Option<Release>, String> {
    let resp = agent(false).get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28").call();
    let mut resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(net_error(e)),
    };
    let text = resp.body_mut().read_to_string().map_err(net_error)?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("непонятный ответ сервера: {e}"))?;
    Ok(newest(&v, prerelease))
}

/// Ход загрузки — общий для потока загрузки и кадра.
#[derive(Default)]
pub struct Progress {
    pub got: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(hex(ctx.finish().as_ref()))
}

/// Папка для загрузок (общая для всех копий Anvil этого пользователя).
pub fn download_dir() -> PathBuf {
    std::env::temp_dir().join("anvil-update")
}

/// Скачать файл релиза в `dir` и сверить SHA-256. Уже скачанный и целый файл не качается заново.
pub fn download(asset: &Asset, dir: &Path, prog: &Progress, wake: &dyn Fn()) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("не удалось создать {}: {e}", dir.display()))?;
    let dest = dir.join(&asset.name);
    if let Some(want) = &asset.sha256
        && dest.is_file()
        && sha256_file(&dest).is_ok_and(|h| h == *want)
    {
        prog.total.store(asset.size, AtOrd::Relaxed);
        prog.got.store(asset.size, AtOrd::Relaxed);
        return Ok(dest);
    }
    let part = dir.join(format!("{}.part", asset.name));
    let resp = agent(true).get(&asset.url).call().map_err(net_error)?;
    let total = resp.body().content_length().unwrap_or(asset.size);
    prog.total.store(total, AtOrd::Relaxed);
    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| format!("не удалось записать {}: {e}", part.display()))?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    let mut woke = Instant::now();
    let fail = |e: String| {
        let _ = std::fs::remove_file(&part);
        e
    };
    loop {
        if prog.cancel.load(AtOrd::Relaxed) {
            drop(file);
            return Err(fail("загрузка отменена".into()));
        }
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                drop(file);
                return Err(fail(format!("загрузка прервалась: {e}")));
            }
        };
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
        if let Err(e) = file.write_all(&buf[..n]) {
            drop(file);
            return Err(fail(format!("не удалось записать файл: {e}")));
        }
        got += n as u64;
        prog.got.store(got, AtOrd::Relaxed);
        if woke.elapsed() > Duration::from_millis(60) {
            woke = Instant::now();
            wake();
        }
    }
    let flushed = file.flush().and_then(|()| file.sync_all());
    drop(file);
    flushed.map_err(|e| fail(format!("не удалось записать файл: {e}")))?;
    if asset.size > 0 && got != asset.size {
        return Err(fail(format!("файл скачался не полностью ({got} из {} байт)", asset.size)));
    }
    if let Some(want) = &asset.sha256
        && hex(ctx.finish().as_ref()) != *want
    {
        return Err(fail("контрольная сумма не совпала — файл повреждён или подменён".into()));
    }
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest).map_err(|e| fail(format!("не удалось сохранить файл: {e}")))?;
    Ok(dest)
}

/// Архив `.tar.gz` → исполняемый файл `anvil` (системным `tar`).
fn unpack(archive: &Path, dir: &Path) -> Result<PathBuf, String> {
    let out = dir.join("unpacked");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| format!("не удалось создать {}: {e}", out.display()))?;
    let st = Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&out).arg("anvil").status();
    match st {
        Ok(s) if s.success() && out.join("anvil").is_file() => Ok(out.join("anvil")),
        Ok(_) => Err("не удалось распаковать архив".into()),
        Err(e) => Err(format!("не удалось запустить tar: {e}")),
    }
}

/// Скачать и подготовить к установке: установщик или новый исполняемый файл.
pub fn fetch_update(asset: &Asset, prog: &Progress, wake: &dyn Fn()) -> Result<PathBuf, String> {
    let dir = download_dir();
    let file = download(asset, &dir, prog, wake)?;
    if asset.name.ends_with(".tar.gz") { unpack(&file, &dir) } else { Ok(file) }
}

/// `путь.old` — куда отодвигается работающий exe на Windows.
fn old_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Положить `new` на место `exe`. Работающий процесс продолжает работать со старой копией.
fn replace_exe(exe: &Path, new: &Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("непонятная папка Anvil")?;
    let name = exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "anvil".into());
    let staged = dir.join(format!(".{name}.new"));
    let denied = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("нет прав на запись в {} — скачайте новую версию вручную", dir.display())
        } else {
            format!("не удалось заменить {}: {e}", exe.display())
        }
    };
    std::fs::copy(new, &staged).map_err(denied)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(denied)?;
    }
    if cfg!(windows) {
        // работающий exe нельзя перезаписать, но можно переименовать
        let old = old_path(exe);
        let _ = std::fs::remove_file(&old);
        if let Err(e) = std::fs::rename(exe, &old) {
            let _ = std::fs::remove_file(&staged);
            return Err(denied(e));
        }
        if let Err(e) = std::fs::rename(&staged, exe) {
            let _ = std::fs::rename(&old, exe);
            let _ = std::fs::remove_file(&staged);
            return Err(denied(e));
        }
    } else if let Err(e) = std::fs::rename(&staged, exe) {
        let _ = std::fs::remove_file(&staged);
        return Err(denied(e));
    }
    Ok(())
}

/// Установка в папку Program Files — значит, ставили для всех пользователей.
fn all_users(dir: &Path) -> bool {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).any(|pf| dir.starts_with(PathBuf::from(pf)))
}

/// Аргументы тихого установщика.
fn installer_args(dir: &Path, relaunch: bool) -> Vec<String> {
    let mut a: Vec<String> = vec![
        if relaunch { "/SILENT" } else { "/VERYSILENT" }.into(),
        "/SUPPRESSMSGBOXES".into(),
        "/NORESTART".into(),
        "/NOCANCEL".into(),
        "/CLOSEAPPLICATIONS".into(),
        "/SP-".into(),
        format!("/DIR={}", dir.display()),
        if all_users(dir) { "/ALLUSERS" } else { "/CURRENTUSER" }.into(),
        format!("/LOG={}", download_dir().join("install.log").display()),
    ];
    if relaunch {
        a.push("/RELAUNCH=1".into());
    }
    a
}

/// Поставить скачанное. `relaunch` — открыть новую версию сразу после установки.
pub fn apply(method: &Method, file: &Path, relaunch: bool) -> Result<(), String> {
    match method {
        Method::Installer { dir } => {
            Command::new(file).args(installer_args(dir, relaunch)).spawn().map_err(|e| format!("не удалось запустить установщик: {e}"))?;
            Ok(())
        }
        Method::Replace { exe } => {
            replace_exe(exe, file)?;
            if relaunch {
                Command::new(exe).arg("--updated").spawn().map_err(|e| format!("новая версия установлена, но не запустилась: {e}"))?;
            }
            Ok(())
        }
        Method::Manual(why) => Err((*why).to_string()),
    }
}

/// После обновления: убрать старую копию exe и скачанные файлы.
pub fn cleanup(method: &Method, updated: bool) {
    if let Method::Replace { exe } = method {
        let _ = std::fs::remove_file(old_path(exe));
    }
    if updated {
        let _ = std::fs::remove_dir_all(download_dir());
    }
}

// ---------------------------------------------------------------- состояние в приложении

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    /// Есть новая версия, ничего не скачано.
    Available,
    Downloading,
    /// Скачано и проверено: установщик или новый exe.
    Ready(PathBuf),
    Installing,
    Failed(String),
}

enum Msg {
    Checked(Result<Option<Release>, String>),
    Fetched(Result<PathBuf, String>),
}

pub struct Updater {
    pub method: Method,
    pub current: Version,
    pub phase: Phase,
    pub release: Option<Release>,
    pub progress: Arc<Progress>,
    rx: Option<Receiver<Msg>>,
    /// Ручная проверка: сообщить и об отсутствии обновлений, и об ошибке.
    manual: bool,
    /// Когда проверить в следующий раз (время кадра); `None` — не проверять.
    pub next_check: Option<f64>,
    /// Плашка убрана («Позже») — остаётся значок в строке состояния.
    pub hidden: bool,
    /// Раскрыто «Что нового».
    pub notes: bool,
    /// Когда плашка появилась (анимация).
    pub shown_at: f64,
    /// «Перезапустить», нажатое при несохранённых файлах (ждём второго нажатия).
    pub armed: Option<f64>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            method: Method::Manual(""),
            current: Version::current(),
            phase: Phase::Idle,
            release: None,
            progress: Arc::default(),
            rx: None,
            manual: false,
            next_check: None,
            hidden: false,
            notes: false,
            shown_at: 0.0,
            armed: None,
        }
    }
}

impl Updater {
    /// Плашка на экране.
    pub fn card_visible(&self) -> bool {
        self.release.is_some() && !self.hidden && !matches!(self.phase, Phase::Idle | Phase::Checking)
    }
}

pub fn fmt_mb(bytes: u64) -> String {
    format!("{:.1} МБ", bytes as f64 / 1_048_576.0)
}

impl AnvilApp {
    /// Первая проверка — через несколько секунд после запуска, чтобы не мешать старту.
    pub fn update_init(&mut self) {
        self.update.method = Method::detect();
        cleanup(&self.update.method, self.args.updated);
        // снимки и сценарии не ходят в сеть — разве что им дали свой адрес релизов
        let headless = (self.args.screenshot.is_some() || !self.args.script.is_empty()) && std::env::var_os("ANVIL_UPDATE_FEED").is_none();
        if self.settings.update.check && !headless {
            self.update.next_check = Some(4.0);
        }
        if self.args.updated {
            self.toast(format!("Anvil обновлён до {}", self.update.current), ToastKind::Ok);
        }
    }

    pub fn update_tick(&mut self) {
        let msgs: Vec<Msg> = match &self.update.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for m in msgs {
            self.update.rx = None;
            match m {
                Msg::Checked(r) => self.update_checked(r),
                Msg::Fetched(r) => self.update_fetched(r),
            }
        }
        if let Some(t) = self.update.next_check {
            if self.now >= t {
                self.update.next_check = None;
                if self.settings.update.check && matches!(self.update.phase, Phase::Idle | Phase::Available | Phase::Failed(_)) {
                    self.update_check(false);
                }
            } else {
                // кадров может не быть часами — попросить один к сроку
                self.ctx.request_repaint_after(Duration::from_secs_f64((t - self.now).max(0.05)));
            }
        }
    }

    fn schedule_next_check(&mut self) {
        let h = self.settings.update.interval_hours;
        self.update.next_check = (self.settings.update.check && h > 0).then(|| self.now + f64::from(h) * 3600.0);
    }

    pub fn update_check(&mut self, manual: bool) {
        if matches!(self.update.phase, Phase::Checking | Phase::Downloading | Phase::Installing) {
            if manual {
                self.status("Обновления: уже идёт проверка или загрузка");
            }
            return;
        }
        if let Phase::Ready(_) = self.update.phase {
            if manual {
                self.update.hidden = false;
            }
            return;
        }
        self.update.manual = manual;
        let was = std::mem::replace(&mut self.update.phase, Phase::Checking);
        if was == Phase::Available {
            // плашка остаётся, пока идёт повторная проверка
            self.update.phase = Phase::Available;
        }
        if manual {
            self.status("Проверка обновлений…");
        }
        let (tx, rx) = mpsc::channel();
        self.update.rx = Some(rx);
        let url = feed_url(self.settings.update.prerelease);
        let pre = self.settings.update.prerelease;
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Checked(fetch(&url, pre)));
            wake();
        });
    }

    fn update_checked(&mut self, r: Result<Option<Release>, String>) {
        let manual = std::mem::take(&mut self.update.manual);
        self.schedule_next_check();
        match r {
            Ok(Some(rel)) if rel.version > self.update.current => {
                let ver = rel.version.to_string();
                let skipped = self.session.skipped_update.as_deref() == Some(ver.as_str());
                if skipped && !manual {
                    self.update.phase = Phase::Idle;
                    return;
                }
                let fresh = self.update.release.as_ref().is_none_or(|r| r.version != rel.version);
                self.update.release = Some(rel);
                if fresh {
                    self.update.phase = Phase::Available;
                    self.update.hidden = !self.settings.update.banner && !manual;
                    self.update.notes = false;
                    self.update.shown_at = self.now;
                    if self.settings.update.auto_download && self.update.method.asset_name().is_some() {
                        self.update_download();
                    }
                } else if self.update.phase == Phase::Checking {
                    self.update.phase = Phase::Available;
                }
                if manual {
                    self.update.hidden = false;
                }
            }
            Ok(_) => {
                if self.update.phase == Phase::Checking {
                    self.update.phase = Phase::Idle;
                }
                if manual {
                    self.toast(format!("У вас последняя версия — Anvil {}", self.update.current), ToastKind::Ok);
                }
            }
            Err(e) => {
                if self.update.phase == Phase::Checking {
                    self.update.phase = if self.update.release.is_some() { Phase::Available } else { Phase::Idle };
                }
                if manual {
                    self.toast(format!("Не удалось проверить обновления: {e}"), ToastKind::Error);
                }
            }
        }
    }

    /// «Обновить»: скачать (или открыть страницу релиза, если поставить самим нельзя).
    pub fn update_download(&mut self) {
        let Some(rel) = self.update.release.clone() else {
            self.update_check(true);
            return;
        };
        if matches!(self.update.phase, Phase::Downloading | Phase::Installing | Phase::Ready(_)) {
            return;
        }
        let asset = self.update.method.asset_name().and_then(|n| rel.asset(n)).cloned();
        let Some(asset) = asset else {
            let why = match &self.update.method {
                Method::Manual(why) => (*why).to_string(),
                _ => "в релизе нет файла для этой системы".into(),
            };
            self.toast(format!("Обновить отсюда нельзя: {why}. Открываю страницу релиза"), ToastKind::Info);
            self.open_url(&rel.page);
            return;
        };
        self.update.progress = Arc::new(Progress::default());
        self.update.progress.total.store(asset.size, AtOrd::Relaxed);
        self.update.phase = Phase::Downloading;
        let (tx, rx) = mpsc::channel();
        self.update.rx = Some(rx);
        let prog = self.update.progress.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let r = fetch_update(&asset, &prog, &*wake);
            let _ = tx.send(Msg::Fetched(r));
            wake();
        });
    }

    fn update_fetched(&mut self, r: Result<PathBuf, String>) {
        match r {
            Ok(file) => {
                self.update.phase = Phase::Ready(file);
                if self.settings.update.banner {
                    self.update.hidden = false;
                }
            }
            Err(e) if self.update.progress.cancel.load(AtOrd::Relaxed) => {
                let _ = e;
                self.update.phase = Phase::Available;
            }
            Err(e) => {
                self.update.phase = Phase::Failed(e);
                self.update.hidden = false;
            }
        }
    }

    pub fn update_cancel(&mut self) {
        if self.update.phase == Phase::Downloading {
            self.update.progress.cancel.store(true, AtOrd::Relaxed);
        }
    }

    /// Следующий шаг одной командой: скачать → перезапустить.
    pub fn update_next(&mut self) {
        match self.update.phase {
            Phase::Ready(_) => self.update_restart(),
            Phase::Downloading | Phase::Installing => self.update.hidden = false,
            _ if self.update.release.is_none() => self.update_check(true),
            _ => self.update_download(),
        }
    }

    /// «Перезапустить»: сохранить файлы, запустить установку и закрыть Anvil.
    pub fn update_restart(&mut self) {
        let Phase::Ready(file) = self.update.phase.clone() else { return };
        let untitled = self.docs.iter().filter(|d| d.path.is_none() && d.dirty()).count();
        let mut risks = Vec::new();
        if untitled > 0 {
            risks.push(format!("несохранённых безымянных вкладок: {untitled} — они пропадут"));
        }
        if self.collab.session.is_some() {
            risks.push("сессия совместной работы завершится".to_string());
        }
        let armed = self.update.armed.is_some_and(|t| self.now - t < 6.0);
        if !risks.is_empty() && !armed {
            self.update.armed = Some(self.now);
            self.toast(format!("Перед перезапуском: {}. Нажмите «Перезапустить» ещё раз", risks.join("; ")), ToastKind::Warn);
            return;
        }
        self.update.armed = None;
        // файлы с путём сохраняем сами — изменения не теряются
        for i in 0..self.docs.len() {
            if self.docs[i].path.is_some() && self.docs[i].dirty() && !self.save_doc_now(i) {
                return;
            }
        }
        self.save_session();
        match apply(&self.update.method, &file, true) {
            Ok(()) => {
                self.update.phase = Phase::Installing;
                self.ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.update.phase = Phase::Failed(e);
                self.update.hidden = false;
            }
        }
    }

    /// Выход с непоставленным обновлением — поставить тихо, без перезапуска.
    pub fn update_on_exit(&mut self) {
        if let Phase::Ready(file) = self.update.phase.clone()
            && self.settings.update.install_on_exit
        {
            self.update.phase = Phase::Installing;
            let _ = apply(&self.update.method, &file, false);
        }
        if self.update.phase == Phase::Downloading {
            self.update.progress.cancel.store(true, AtOrd::Relaxed);
        }
    }

    pub fn update_skip(&mut self) {
        let Some(rel) = self.update.release.take() else {
            self.status("Обновлений нет");
            return;
        };
        self.update_cancel();
        self.session.skipped_update = Some(rel.version.to_string());
        self.save_session();
        self.update.phase = Phase::Idle;
        self.status(format!("Версия {} пропущена — напомню о следующей", rel.version));
    }

    pub fn update_open_page(&mut self) {
        let page = self.update.release.as_ref().map(|r| r.page.clone()).unwrap_or_else(|| RELEASES_PAGE.to_string());
        self.open_url(&page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("не разобралось: {s}"))
    }

    #[test]
    fn versions_compare_like_semver() {
        assert!(v("v0.4.0") > v("0.3.1"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-beta.10") > v("1.0.0-beta.2"));
        assert!(v("1.0.0-rc.1") > v("1.0.0-beta.10"));
        assert!(v("1.0.0-beta") < v("1.0.0-beta.1"));
        assert_eq!(v("1.2"), v("1.2.0"));
        assert_eq!(v("V1.2.3+build.5").to_string(), "1.2.3");
        assert_eq!(v("1.2.3-beta.1").to_string(), "1.2.3-beta.1");
        assert!(Version::parse("").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
        assert!(Version::parse("latest").is_none());
    }

    #[test]
    fn current_version_parses() {
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    fn sample() -> Value {
        serde_json::json!([
            {
                "tag_name": "v0.5.0-beta.1", "name": "Anvil 0.5 beta", "draft": false, "prerelease": true,
                "html_url": "https://example.org/0.5", "body": "", "assets": []
            },
            {
                "tag_name": "v0.4.0", "name": "Anvil 0.4.0 — обновления", "draft": false, "prerelease": false,
                "html_url": "https://example.org/0.4", "body": "## Что нового\n- **Плашка** [обновления](https://x)",
                "assets": [
                    { "name": "anvil.exe", "browser_download_url": "https://example.org/anvil.exe", "size": 10,
                      "digest": "sha256:ABCDEF" },
                    { "name": "anvil-setup-x64.exe", "browser_download_url": "https://example.org/setup.exe", "size": 20 }
                ]
            },
            { "tag_name": "v9.0.0", "draft": true, "assets": [] },
            { "tag_name": "nightly", "draft": false, "assets": [] }
        ])
    }

    #[test]
    fn newest_skips_drafts_and_prereleases() {
        let r = newest(&sample(), false).unwrap_or_else(|| panic!("нет релиза"));
        assert_eq!(r.version, v("0.4.0"));
        assert_eq!(r.title, "Anvil 0.4.0 — обновления");
        assert_eq!(r.page, "https://example.org/0.4");
        let exe = r.asset("anvil.exe").unwrap_or_else(|| panic!("нет файла"));
        assert_eq!(exe.sha256.as_deref(), Some("abcdef"));
        assert_eq!(exe.size, 10);
        assert_eq!(r.asset("anvil-setup-x64.exe").and_then(|a| a.sha256.clone()), None);

        let pre = newest(&sample(), true).unwrap_or_else(|| panic!("нет релиза"));
        assert_eq!(pre.version.to_string(), "0.5.0-beta.1");
        // один объект, как отвечает /releases/latest
        assert_eq!(newest(&sample()[1], false).map(|r| r.version), Some(v("0.4.0")));
        assert_eq!(newest(&serde_json::json!({ "message": "Not Found" }), false), None);
    }

    #[test]
    fn notes_lose_markup() {
        let md = "## Совместная работа\n\nНажмите **«Поделиться»** — [ссылка](https://x) в `буфере`.\n\n![картинка](https://x.png)\n```sh\nanvil --join\n```\n- **Без конфликтов.** CRDT\n* второй [незакрытый\n- ключи `[update]`, [бета](https://b) и [x]\n| таблица |";
        let lines = plain_notes(md, 10);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Совместная работа",
                "Нажмите «Поделиться» — ссылка в буфере.",
                "• Без конфликтов. CRDT",
                "• второй [незакрытый",
                "• ключи [update], бета и [x]"
            ]
        );
        assert!(lines[0].heading && !lines[1].heading);
        assert_eq!(plain_notes(md, 2).len(), 2);
    }

    #[test]
    fn installer_arguments() {
        let dir = Path::new(r"C:\Users\u\AppData\Local\Programs\Anvil");
        let a = installer_args(dir, true);
        assert_eq!(a[0], "/SILENT");
        assert!(a.contains(&format!("/DIR={}", dir.display())));
        assert!(a.contains(&"/RELAUNCH=1".to_string()));
        assert!(a.contains(&"/CLOSEAPPLICATIONS".to_string()));
        let quiet = installer_args(dir, false);
        assert_eq!(quiet[0], "/VERYSILENT");
        assert!(!quiet.contains(&"/RELAUNCH=1".to_string()));
    }

    #[test]
    fn download_verifies_checksum_and_reuses_file() -> Result<(), String> {
        // локальный сервер на один-два запроса: отдаёт 100 КБ
        let body: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let served = body.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2).flatten() {
                let mut s = stream;
                let mut req = [0u8; 2048];
                let _ = s.read(&mut req);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", served.len());
                let _ = s.write_all(&served);
            }
        });
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        ctx.update(&body);
        let good = hex(ctx.finish().as_ref());
        let dir = std::env::temp_dir().join(format!("anvil-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let url = format!("http://127.0.0.1:{port}/anvil.bin");

        // подменённый файл: сумма не совпала → ошибка, ничего не осталось
        let bad = Asset { name: "anvil.bin".into(), url: url.clone(), size: body.len() as u64, sha256: Some("00".repeat(32)) };
        let err = download(&bad, &dir, &Progress::default(), &|| {}).err().unwrap_or_default();
        assert!(err.contains("контрольная сумма"), "{err}");
        assert!(!dir.join("anvil.bin").exists() && !dir.join("anvil.bin.part").exists());

        let ok = Asset { sha256: Some(good), ..bad };
        let prog = Progress::default();
        let path = download(&ok, &dir, &prog, &|| {})?;
        assert_eq!(std::fs::read(&path).map_err(|e| e.to_string())?, body);
        assert_eq!(prog.got.load(AtOrd::Relaxed), body.len() as u64);
        // сервер больше не отвечает, но целый файл уже есть — повторно не качаем
        let again = download(&ok, &dir, &Progress::default(), &|| {})?;
        assert_eq!(again, path);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn replace_keeps_running_copy_aside() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("anvil-replace-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let exe = dir.join(if cfg!(windows) { "anvil.exe" } else { "anvil" });
        let new = dir.join("new.bin");
        std::fs::write(&exe, b"old").map_err(|e| e.to_string())?;
        std::fs::write(&new, b"new").map_err(|e| e.to_string())?;
        replace_exe(&exe, &new)?;
        assert_eq!(std::fs::read(&exe).map_err(|e| e.to_string())?, b"new");
        assert_eq!(old_path(&exe).exists(), cfg!(windows));
        cleanup(&Method::Replace { exe: exe.clone() }, false);
        assert!(!old_path(&exe).exists());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
