//! Тулчейны: десятки компиляторов и интерпретаторов без единой строки кода под каждый.
//!
//! Пак ([`Pack`]) — декларативное TOML-описание: как найти инструмент, как запустить
//! файл или проект, как установить. Встроенные паки лежат в `packs/builtin.toml`,
//! свои — в папке `packs/` настроек. Ничего не висит в памяти и не запускается,
//! пока пользователь сам не нажал «Запустить» или «Установить».

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const BUILTIN: &str = include_str!("../packs/builtin.toml");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Detect {
    /// Исполняемые файлы по порядку предпочтения; найденный подставляется как `{cmd}`.
    pub commands: Vec<String>,
    #[serde(default)]
    pub version: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    pub build: Option<String>,
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSpec {
    /// Файл-признак проекта (`Cargo.toml`, `go.mod`, `*.csproj`…).
    pub marker: String,
    pub build: Option<String>,
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Install {
    pub windows: Option<String>,
    pub macos: Option<String>,
    pub linux: Option<String>,
}

impl Install {
    /// Команда установки для текущей ОС.
    pub fn current(&self) -> Option<&str> {
        if cfg!(windows) {
            self.windows.as_deref()
        } else if cfg!(target_os = "macos") {
            self.macos.as_deref()
        } else {
            self.linux.as_deref()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub languages: Vec<String>,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub priority: i32,
    pub detect: Detect,
    pub run: Option<RunSpec>,
    pub project: Option<ProjectSpec>,
    #[serde(default)]
    pub install: Install,
    #[serde(skip)]
    pub builtin: bool,
}

#[derive(Deserialize)]
struct PackFile {
    #[serde(default)]
    pack: Vec<Pack>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("для языка «{0}» нет ни одного пака")]
    NoPack(String),
    #[error("не найден ни один инструмент для «{lang}». Установите: {hint}")]
    NotInstalled { lang: String, hint: String },
    #[error("пак «{0}» не умеет запускать файлы")]
    NoRun(String),
    #[error("пустая команда в паке «{0}»")]
    EmptyCommand(String),
}

/// Все паки: встроенные + пользовательские (пользовательский пак с тем же `id` заменяет встроенный).
#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub packs: Vec<Pack>,
    pub warnings: Vec<String>,
}

impl Registry {
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut reg = Self::default();
        match toml::from_str::<PackFile>(BUILTIN) {
            Ok(f) => reg.packs.extend(f.pack.into_iter().map(|mut p| {
                p.builtin = true;
                p
            })),
            Err(e) => reg.warnings.push(format!("встроенные паки: {e}")),
        }
        if let Some(dir) = user_dir
            && let Ok(rd) = std::fs::read_dir(dir)
        {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
            files.sort();
            for f in files {
                match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|s| toml::from_str::<PackFile>(&s).map_err(|e| e.to_string())) {
                    Ok(pf) => {
                        for p in pf.pack {
                            reg.packs.retain(|x| x.id != p.id);
                            reg.packs.push(p);
                        }
                    }
                    Err(e) => reg.warnings.push(format!("{}: {e}", f.display())),
                }
            }
        }
        reg.packs.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
        reg
    }

    pub fn for_language<'a>(&'a self, lang: &'a str) -> impl Iterator<Item = &'a Pack> + 'a {
        self.packs.iter().filter(move |p| p.languages.iter().any(|l| l == lang))
    }

    pub fn get(&self, id: &str) -> Option<&Pack> {
        self.packs.iter().find(|p| p.id == id)
    }

    /// Сколько языков покрыто паками.
    pub fn language_count(&self) -> usize {
        let mut v: Vec<&str> = self.packs.iter().flat_map(|p| p.languages.iter().map(String::as_str)).collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    }
}

/// Найденный инструмент.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub command: String,
    pub path: PathBuf,
}

/// Ищет первую доступную команду пака в PATH. Быстро: процессы не запускаются.
pub fn detect(pack: &Pack) -> Option<Found> {
    for cmd in &pack.detect.commands {
        if let Ok(path) = which::which(cmd) {
            // заглушки Microsoft Store вместо настоящего Python
            let stub = cmd.starts_with("python") && path.to_string_lossy().contains("WindowsApps");
            if !stub {
                return Some(Found { command: cmd.clone(), path });
            }
        }
    }
    None
}

/// Версия инструмента (запускает процесс, поэтому вызывать в фоне).
pub fn version(pack: &Pack, found: &Found) -> Option<String> {
    let parts: Vec<String> = pack.detect.version.iter().map(|a| a.replace("{cmd}", &found.command)).collect();
    let (prog, args) = parts.split_first()?;
    let prog = which::which(prog).ok()?;
    let mut cmd = Command::new(prog);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(4) => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    let mut out = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    if out.trim().is_empty()
        && let Some(mut s) = child.stderr.take()
    {
        let _ = s.read_to_string(&mut out);
    }
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(72).collect())
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

/// Разбивает команду на аргументы с учётом кавычек.
pub fn split_command(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub program: String,
    pub args: Vec<String>,
    pub display: String,
}

/// Что и где запускать.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub title: String,
    pub cwd: PathBuf,
    pub steps: Vec<Step>,
}

impl Plan {
    /// Команда через системную оболочку (для установки: там бывают `&&` и `|`).
    pub fn shell(title: &str, command: &str, cwd: &Path) -> Self {
        let step = if cfg!(windows) {
            let mut args = vec!["/C".to_string()];
            args.extend(split_command(command));
            Step { program: "cmd".into(), args, display: command.to_string() }
        } else {
            Step { program: "sh".into(), args: vec!["-c".into(), command.to_string()], display: command.to_string() }
        };
        Self { title: title.to_string(), cwd: cwd.to_path_buf(), steps: vec![step] }
    }
}

fn find_marker(file_dir: &Path, root: Option<&Path>, marker: &str) -> Option<PathBuf> {
    let mut dir = Some(file_dir);
    while let Some(d) = dir {
        let hit = if let Some(suffix) = marker.strip_prefix('*') {
            std::fs::read_dir(d).ok()?.flatten().any(|e| e.file_name().to_string_lossy().ends_with(suffix))
        } else {
            d.join(marker).exists()
        };
        if hit {
            return Some(d.to_path_buf());
        }
        if root.is_some_and(|r| r == d) {
            break;
        }
        dir = d.parent();
    }
    None
}

/// Пользовательские команды для языка из `[toolchains.overrides]`.
#[derive(Debug, Clone, Default)]
pub struct Override {
    pub build: Option<String>,
    pub run: Option<String>,
}

/// Составляет план запуска файла: проект (если найден признак) или одиночный файл.
pub fn plan_run(reg: &Registry, lang: &str, file: &Path, root: Option<&Path>, overrides: &BTreeMap<String, Override>) -> Result<Plan, ToolError> {
    let packs: Vec<&Pack> = reg.for_language(lang).collect();
    let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "main".into());
    let out_dir = std::env::temp_dir().join("anvil-run");
    let _ = std::fs::create_dir_all(&out_dir);
    let out = out_dir.join(if cfg!(windows) { format!("{stem}.exe") } else { stem.clone() });

    let ov = overrides.get(lang);
    let found_pack = packs.iter().find_map(|p| detect(p).map(|f| (*p, f)));
    let (pack, found) = match (found_pack, ov) {
        (Some(x), _) => (Some(x.0), Some(x.1)),
        (None, Some(o)) if o.run.is_some() => (None, None),
        (None, _) => {
            let first = packs.first().ok_or_else(|| ToolError::NoPack(lang.to_string()))?;
            let hint = first.install.current().map(str::to_string).unwrap_or_else(|| first.homepage.clone());
            return Err(ToolError::NotInstalled { lang: lang.to_string(), hint });
        }
    };

    let mut cwd = dir.clone();
    let (mut build, mut run, title) = match pack {
        Some(p) => {
            let project = p.project.as_ref().and_then(|pr| find_marker(&dir, root, &pr.marker).map(|d| (pr, d)));
            match (project, &p.run) {
                (Some((pr, d)), _) => {
                    cwd = d;
                    (pr.build.clone(), pr.run.clone(), format!("{} · проект", p.name))
                }
                (None, Some(r)) => (r.build.clone(), r.run.clone(), p.name.clone()),
                (None, None) => return Err(ToolError::NoRun(p.id.clone())),
            }
        }
        None => (None, String::new(), format!("{lang} · свои команды")),
    };
    if let Some(o) = ov {
        if o.build.is_some() {
            build = o.build.clone();
        }
        if let Some(r) = &o.run {
            run = r.clone();
        }
    }
    let cmd = found.as_ref().map(|f| f.command.clone()).unwrap_or_default();
    let vars = [
        ("{file}", file.to_string_lossy().to_string()),
        ("{dir}", dir.to_string_lossy().to_string()),
        ("{stem}", stem),
        ("{out}", out.to_string_lossy().to_string()),
        ("{root}", root.map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| cwd.to_string_lossy().to_string())),
        ("{cmd}", cmd),
    ];
    let mk = |template: &str| -> Result<Step, ToolError> {
        let parts: Vec<String> = split_command(template)
            .into_iter()
            .map(|mut a| {
                for (k, v) in &vars {
                    a = a.replace(k, v);
                }
                a
            })
            .collect();
        let (program, args) = parts.split_first().ok_or_else(|| ToolError::EmptyCommand(lang.to_string()))?;
        Ok(Step { program: program.clone(), args: args.to_vec(), display: parts.join(" ") })
    };
    let mut steps = Vec::new();
    if let Some(b) = build.as_deref().filter(|b| !b.trim().is_empty()) {
        steps.push(mk(b)?);
    }
    steps.push(mk(&run)?);
    Ok(Plan { title, cwd, steps })
}

// ---------------------------------------------------------------------------- запуск

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    Step(String),
    Out { text: String, err: bool },
    Exit { code: Option<i32>, millis: u128 },
    Failed(String),
}

/// Запущенный процесс: события приходят в канал, процесс можно остановить.
pub struct RunHandle {
    pub title: String,
    pub rx: Receiver<RunEvent>,
    kill: Arc<AtomicBool>,
}

impl RunHandle {
    pub fn kill(&self) {
        self.kill.store(true, Ordering::Relaxed);
    }
}

fn pump(mut r: impl Read + Send + 'static, err: bool, tx: Sender<RunEvent>, wake: Arc<dyn Fn() + Send + Sync>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    while let Some(i) = pending.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = pending.drain(..=i).collect();
                        let text = String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string();
                        let _ = tx.send(RunEvent::Out { text, err });
                    }
                    wake();
                }
            }
        }
        if !pending.is_empty() {
            let _ = tx.send(RunEvent::Out { text: String::from_utf8_lossy(&pending).trim_end().to_string(), err });
            wake();
        }
    })
}

/// Запускает план в фоновом потоке. `wake` вызывается при каждом новом событии (перерисовать окно).
pub fn spawn(plan: Plan, wake: Arc<dyn Fn() + Send + Sync>) -> RunHandle {
    let (tx, rx) = mpsc::channel();
    let kill = Arc::new(AtomicBool::new(false));
    let k = kill.clone();
    let title = plan.title.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut last_code = Some(0);
        for step in &plan.steps {
            let _ = tx.send(RunEvent::Step(step.display.clone()));
            wake();
            let program = which::which(&step.program).unwrap_or_else(|_| PathBuf::from(&step.program));
            let mut cmd = Command::new(&program);
            cmd.args(&step.args).current_dir(&plan.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd.env("CLICOLOR_FORCE", "0").env("NO_COLOR", "1");
            hide_console(&mut cmd);
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(RunEvent::Failed(format!("не удалось запустить «{}»: {e}", step.program)));
                    wake();
                    return;
                }
            };
            let mut readers = Vec::new();
            if let Some(o) = child.stdout.take() {
                readers.push(pump(o, false, tx.clone(), wake.clone()));
            }
            if let Some(e) = child.stderr.take() {
                readers.push(pump(e, true, tx.clone(), wake.clone()));
            }
            let status = loop {
                if k.load(Ordering::Relaxed) {
                    let _ = child.kill();
                }
                match child.try_wait() {
                    Ok(Some(s)) => break Some(s),
                    Ok(None) => std::thread::sleep(Duration::from_millis(15)),
                    Err(_) => break None,
                }
            };
            for r in readers {
                let _ = r.join();
            }
            last_code = status.and_then(|s| s.code());
            if last_code != Some(0) || k.load(Ordering::Relaxed) {
                break;
            }
        }
        let _ = tx.send(RunEvent::Exit { code: last_code, millis: started.elapsed().as_millis() });
        wake();
    });
    RunHandle { title, rx, kill }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_packs_parse() {
        let reg = Registry::load(None);
        assert!(reg.warnings.is_empty(), "{:?}", reg.warnings);
        assert!(reg.packs.len() >= 40, "паков: {}", reg.packs.len());
        assert!(reg.language_count() >= 35, "языков: {}", reg.language_count());
        for p in &reg.packs {
            assert!(!p.detect.commands.is_empty(), "{}", p.id);
            assert!(p.run.is_some() || p.project.is_some(), "{}", p.id);
        }
    }

    #[test]
    fn splits_quoted_commands() {
        assert_eq!(split_command(r#"gcc "my file.c" -o 'out put' x"#), vec!["gcc", "my file.c", "-o", "out put", "x"]);
        assert_eq!(split_command(r#"a "" b"#), vec!["a", "", "b"]);
    }

    #[test]
    fn override_used_when_nothing_installed() {
        let reg = Registry { packs: Vec::new(), warnings: Vec::new() };
        let mut ov = BTreeMap::new();
        ov.insert("brainfuck".to_string(), Override { build: None, run: Some("bf {file}".into()) });
        let plan = plan_run(&reg, "brainfuck", Path::new("/tmp/a.bf"), None, &ov);
        assert!(matches!(plan, Ok(ref p) if p.steps.len() == 1 && p.steps[0].program == "bf"));
        assert!(matches!(plan_run(&reg, "nope", Path::new("x"), None, &BTreeMap::new()), Err(ToolError::NoPack(_))));
    }

    #[test]
    fn user_pack_replaces_builtin() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("anvil-packs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("my.toml"),
            "[[pack]]\nid = \"go\"\nname = \"Мой Go\"\nlanguages = [\"go\"]\ndetect = { commands = [\"go\"] }\nrun = { run = \"go run -race {file}\" }\n",
        )?;
        let reg = Registry::load(Some(&dir));
        assert_eq!(reg.get("go").map(|p| p.name.as_str()), Some("Мой Go"));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn runs_a_real_process() {
        let plan = Plan::shell("echo", "echo привет", &std::env::temp_dir());
        let h = spawn(plan, Arc::new(|| {}));
        let mut out = Vec::new();
        while let Ok(ev) = h.rx.recv_timeout(Duration::from_secs(10)) {
            let done = matches!(ev, RunEvent::Exit { .. } | RunEvent::Failed(_));
            out.push(ev);
            if done {
                break;
            }
        }
        assert!(out.iter().any(|e| matches!(e, RunEvent::Exit { code: Some(0), .. })), "{out:?}");
    }
}
