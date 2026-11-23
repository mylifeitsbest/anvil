//! Процесс языкового сервера и JSON-RPC поверх его stdin/stdout.
//!
//! Поток чтения разбирает сообщения, сам отвечает на служебные запросы сервера
//! (`workspace/configuration`, регистрация возможностей, прогресс) и складывает
//! ответы и уведомления в канал. Интерфейс забирает их раз в кадр.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::transport::{read_message, write_message};

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Response { id: i64, result: Result<Value, String> },
    Notification { method: String, params: Value },
    Exited { stderr: String },
}

#[cfg(windows)]
pub fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_cmd: &mut Command) {}

pub struct Client {
    pub server_id: String,
    pub root: PathBuf,
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    next_id: AtomicI64,
    pub rx: Receiver<Incoming>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

/// Журнал обмена для отладки: `ANVIL_LSP_LOG=путь/к/файлу`.
fn log(dir: &str, v: &Value) {
    let Some(path) = std::env::var_os("ANVIL_LSP_LOG") else { return };
    let mut line = v.to_string();
    if line.len() > 2000 {
        line.truncate(line.char_indices().nth(2000).map(|(i, _)| i).unwrap_or(line.len()));
        line.push('…');
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(f, "{dir} {line}");
    }
}

fn send(stdin: &Mutex<ChildStdin>, v: &Value) {
    log("→", v);
    if let Ok(mut w) = stdin.lock() {
        let _ = write_message(&mut *w, v);
    }
}

/// Ответ на запрос сервера к клиенту.
fn auto_reply(method: &str, params: &Value, root: &Path) -> Result<Value, (i64, String)> {
    match method {
        "workspace/configuration" => {
            let n = params.get("items").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            Ok(Value::Array(vec![Value::Null; n]))
        }
        "workspace/workspaceFolders" => {
            Ok(json!([{ "uri": crate::uri::from_path(root), "name": root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() }]))
        }
        "window/workDoneProgress/create"
        | "client/registerCapability"
        | "client/unregisterCapability"
        | "window/showMessageRequest"
        | "window/showDocument"
        | "workspace/diagnostic/refresh"
        | "workspace/semanticTokens/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/codeLens/refresh"
        | "workspace/inlineValue/refresh" => Ok(Value::Null),
        "workspace/applyEdit" => Ok(json!({ "applied": false })),
        _ => Err((-32601, format!("Anvil пока не поддерживает {method}"))),
    }
}

impl Client {
    pub fn spawn(server_id: &str, exe: &Path, args: &[String], root: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        let mut cmd = Command::new(exe);
        cmd.args(args).current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        hide_console(&mut cmd);
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("нет stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("нет stdout"))?;
        let stderr = child.stderr.take();
        let stdin = Arc::new(Mutex::new(stdin));
        let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx): (Sender<Incoming>, Receiver<Incoming>) = mpsc::channel();

        if let Some(err) = stderr {
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    if let Ok(mut t) = tail.lock() {
                        t.push_back(line);
                        if t.len() > 40 {
                            t.pop_front();
                        }
                    }
                }
            });
        }

        {
            let stdin = stdin.clone();
            let root = root.to_path_buf();
            let tail = tail.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stdout);
                while let Ok(Some(msg)) = read_message(&mut r) {
                    log("←", &msg);
                    let id = msg.get("id").cloned();
                    let method = msg.get("method").and_then(Value::as_str).map(str::to_string);
                    match (id, method) {
                        (Some(id), Some(method)) => {
                            let params = msg.get("params").cloned().unwrap_or(Value::Null);
                            let reply = match auto_reply(&method, &params, &root) {
                                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                                Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
                            };
                            send(&stdin, &reply);
                        }
                        (Some(id), None) => {
                            let Some(id) = id.as_i64() else { continue };
                            let result = match msg.get("error") {
                                Some(e) => Err(e.get("message").and_then(Value::as_str).unwrap_or("ошибка сервера").to_string()),
                                None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                            };
                            let _ = tx.send(Incoming::Response { id, result });
                            wake();
                        }
                        (None, Some(method)) => {
                            let params = msg.get("params").cloned().unwrap_or(Value::Null);
                            let _ = tx.send(Incoming::Notification { method, params });
                            wake();
                        }
                        (None, None) => {}
                    }
                }
                // дать потоку stderr дочитать причину падения
                std::thread::sleep(Duration::from_millis(80));
                let stderr = tail.lock().map(|t| t.iter().cloned().collect::<Vec<_>>().join("\n")).unwrap_or_default();
                let _ = tx.send(Incoming::Exited { stderr });
                wake();
            });
        }

        Ok(Self {
            server_id: server_id.to_string(),
            root: root.to_path_buf(),
            stdin,
            child: Arc::new(Mutex::new(child)),
            next_id: AtomicI64::new(1),
            rx,
            stderr: tail,
        })
    }

    pub fn request(&self, method: &str, params: Value) -> i64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        send(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    pub fn notify(&self, method: &str, params: Value) {
        send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr.lock().map(|t| t.iter().cloned().collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }

    /// Запрос `initialize` с возможностями клиента.
    pub fn initialize(&self) -> i64 {
        let uri = crate::uri::from_path(&self.root);
        let name = self.root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "Anvil", "version": env!("CARGO_PKG_VERSION") },
            "rootUri": uri,
            "rootPath": self.root.to_string_lossy(),
            "workspaceFolders": [{ "uri": uri, "name": name }],
            "capabilities": {
                "general": { "positionEncodings": ["utf-8", "utf-32", "utf-16"] },
                "textDocument": {
                    "synchronization": { "didSave": true, "dynamicRegistration": false },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": true,
                            "documentationFormat": ["markdown", "plaintext"],
                            "deprecatedSupport": true,
                            "labelDetailsSupport": true,
                            "insertReplaceSupport": false
                        },
                        "contextSupport": true
                    },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "linkSupport": true },
                    "references": {},
                    "rename": { "prepareSupport": false },
                    "formatting": {},
                    "publishDiagnostics": { "relatedInformation": false, "versionSupport": true }
                },
                "window": { "workDoneProgress": true },
                "workspace": { "workspaceFolders": true, "configuration": true, "applyEdit": false }
            }
        });
        self.request("initialize", params)
    }

    /// Вежливо остановить сервер, а через секунду — принудительно.
    pub fn shutdown(&self) {
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        let child = self.child.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1200));
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
        });
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock()
            && matches!(c.try_wait(), Ok(None))
        {
            let _ = c.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_replies() {
        let root = Path::new("/tmp/x");
        assert_eq!(auto_reply("workspace/configuration", &json!({"items": [{}, {}]}), root), Ok(json!([null, null])));
        assert_eq!(auto_reply("client/registerCapability", &Value::Null, root), Ok(Value::Null));
        assert!(auto_reply("weird/method", &Value::Null, root).is_err());
    }
}
