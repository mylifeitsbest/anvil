//! Ссылки-приглашения `anvil://…` по клику.
//!
//! * Система запускает `anvil.exe --join ССЫЛКА`. Если Anvil уже открыт, новый процесс передаёт
//!   ссылку ему через локальный сокет и сразу выходит — второго окна нет.
//! * Открытое окно слушает 127.0.0.1 на случайном порту; порт и случайный токен лежат в файле
//!   в папке настроек (читать его может только владелец). Без токена ссылку не передать.
//! * Регистрация схемы `anvil://`: установщик делает её сам, для портативной версии — команда палитры.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const FILE: &str = "instance.txt";
/// Длиннее приглашения не бывает; всё сверх — мусор.
const MAX_LINE: u64 = 4096;

/// Передать ссылку уже открытому Anvil. `true` — передано, этому процессу можно выходить.
pub fn forward(dir: &Path, link: &str) -> bool {
    let Ok(s) = std::fs::read_to_string(dir.join(FILE)) else { return false };
    let mut it = s.split_whitespace();
    let (Some(port), Some(token)) = (it.next().and_then(|p| p.parse::<u16>().ok()), it.next()) else { return false };
    let Ok(mut c) = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(400)) else { return false };
    let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
    if writeln!(c, "anvil-join {token} {}", link.trim()).is_err() {
        return false;
    }
    let mut line = String::new();
    BufReader::new(c.take(64)).read_line(&mut line).is_ok() && line.trim() == "ok"
}

/// Слушатель открытого окна: ссылки приходят в канал, окно будится.
pub struct Listener {
    pub rx: Receiver<String>,
    file: PathBuf,
    stamp: String,
}

impl Listener {
    pub fn start(dir: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> Option<Listener> {
        let token = anvil_collab::random_hex::<16>()?;
        let l = TcpListener::bind(("127.0.0.1", 0)).ok()?;
        let port = l.local_addr().ok()?.port();
        let stamp = format!("{port} {token}\n");
        let file = dir.join(FILE);
        anvil_relay::write_private(&file, stamp.as_bytes()).ok()?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("anvil-links".into())
            .spawn(move || {
                for conn in l.incoming() {
                    let Ok(mut c) = conn else { continue };
                    let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
                    let mut line = String::new();
                    let Ok(r) = c.try_clone() else { continue };
                    if BufReader::new(r.take(MAX_LINE)).read_line(&mut line).is_err() {
                        continue;
                    }
                    let mut parts = line.trim_end().splitn(3, ' ');
                    let ok = parts.next() == Some("anvil-join") && anvil_collab::same_secret(parts.next().unwrap_or_default(), &token);
                    let link = parts.next().unwrap_or_default().trim().to_string();
                    if !ok || link.is_empty() {
                        continue;
                    }
                    if tx.send(link).is_err() {
                        break;
                    }
                    let _ = c.write_all(b"ok\n");
                    wake();
                }
            })
            .ok()?;
        Some(Listener { rx, file, stamp })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // файл удаляем, только если он наш: другое окно могло записать свой
        if std::fs::read_to_string(&self.file).is_ok_and(|s| s == self.stamp) {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

/// Команды регистрации `anvil://` для пользователя (без прав администратора).
#[cfg(windows)]
fn register_commands(exe: &Path) -> Vec<Vec<String>> {
    let exe = exe.display().to_string();
    let key = r"HKCU\Software\Classes\anvil";
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    vec![
        s(&["add", key, "/ve", "/d", "URL:Anvil", "/f"]),
        s(&["add", key, "/v", "URL Protocol", "/d", "", "/f"]),
        s(&["add", &format!(r"{key}\DefaultIcon"), "/ve", "/d", &format!("\"{exe}\",0"), "/f"]),
        s(&["add", &format!(r"{key}\shell\open\command"), "/ve", "/d", &format!("\"{exe}\" --join \"%1\""), "/f"]),
    ]
}

/// Открывать ссылки `anvil://` этим exe. Возвращает текст для пользователя.
pub fn register() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        for args in register_commands(&exe) {
            let out = std::process::Command::new("reg").args(&args).output().map_err(|e| format!("reg: {e}"))?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
        }
        Ok(format!("Ссылки anvil:// теперь открываются в {}", exe.display()))
    }
    #[cfg(target_os = "linux")]
    {
        let apps = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok_or("не найдена домашняя папка")?
            .join("applications");
        std::fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=Anvil\nExec=\"{}\" --join %u\nMimeType=x-scheme-handler/anvil;\nNoDisplay=true\nTerminal=false\n",
            exe.display()
        );
        std::fs::write(apps.join("anvil-url.desktop"), desktop).map_err(|e| e.to_string())?;
        let ok = std::process::Command::new("xdg-mime").args(["default", "anvil-url.desktop", "x-scheme-handler/anvil"]).status().is_ok_and(|s| s.success());
        if ok {
            Ok("Ссылки anvil:// теперь открываются в Anvil".into())
        } else {
            Err("xdg-mime не сработал — установите xdg-utils".into())
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = exe;
        Err("На этой системе открытие ссылок по клику пока не поддерживается — используйте «присоединиться по ссылке»".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_reaches_listener_only_with_token() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("anvil-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let l = Listener::start(&dir, Arc::new(|| {})).ok_or("слушатель не запустился")?;
        let link = "anvil://192.168.0.5:7878/0123456789abcdef0123456789abcdef?fp=00";
        assert!(forward(&dir, link));
        assert_eq!(l.rx.recv_timeout(Duration::from_secs(3)).map_err(|e| e.to_string())?, link);

        // чужой токен — не принимается
        let port = std::fs::read_to_string(dir.join(FILE)).map_err(|e| e.to_string())?.split_whitespace().next().unwrap_or_default().to_string();
        let mut c = TcpStream::connect(format!("127.0.0.1:{port}")).map_err(|e| e.to_string())?;
        writeln!(c, "anvil-join 0000 {link}").map_err(|e| e.to_string())?;
        drop(c);
        assert!(l.rx.recv_timeout(Duration::from_millis(300)).is_err());

        // окно закрылось — файл убран, передавать некому
        drop(l);
        assert!(!dir.join(FILE).exists());
        assert!(!forward(&dir, link));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn registry_commands_point_to_exe() {
        let cmds = register_commands(Path::new(r"C:\Program Files\Anvil\anvil.exe"));
        let last = cmds.last().map(|c| c.join(" ")).unwrap_or_default();
        assert!(last.contains(r"HKCU\Software\Classes\anvil\shell\open\command"), "{last}");
        assert!(last.contains(r#""C:\Program Files\Anvil\anvil.exe" --join "%1""#), "{last}");
        assert!(cmds.iter().any(|c| c.iter().any(|x| x == "URL Protocol")));
    }
}
