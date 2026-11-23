//! Реестр языковых серверов: встроенный `servers.toml` + пользовательские `[[server]]` из папки паков.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const BUILTIN: &str = include_str!("../servers.toml");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Install {
    pub windows: Option<String>,
    pub macos: Option<String>,
    pub linux: Option<String>,
}

impl Install {
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
pub struct ServerDef {
    pub id: String,
    pub name: String,
    pub languages: Vec<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub probe: Vec<String>,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub install: Install,
}

#[derive(Deserialize)]
struct ServerFile {
    #[serde(default)]
    server: Vec<ServerDef>,
}

#[derive(Debug, Clone, Default)]
pub struct ServerRegistry {
    pub servers: Vec<ServerDef>,
    pub warnings: Vec<String>,
}

impl ServerRegistry {
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut reg = Self::default();
        match toml::from_str::<ServerFile>(BUILTIN) {
            Ok(f) => reg.servers = f.server,
            Err(e) => reg.warnings.push(format!("встроенные серверы: {e}")),
        }
        if let Some(dir) = user_dir
            && let Ok(rd) = std::fs::read_dir(dir)
        {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                if !text.contains("[[server]]") {
                    continue;
                }
                match toml::from_str::<ServerFile>(&text) {
                    Ok(sf) => {
                        for s in sf.server {
                            reg.servers.retain(|x| x.id != s.id);
                            reg.servers.push(s);
                        }
                    }
                    Err(e) => reg.warnings.push(format!("{}: {e}", f.display())),
                }
            }
        }
        reg.servers.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
        reg
    }

    pub fn for_language<'a>(&'a self, lang: &'a str) -> impl Iterator<Item = &'a ServerDef> + 'a {
        self.servers.iter().filter(move |s| s.languages.iter().any(|l| l == lang))
    }

    pub fn get(&self, id: &str) -> Option<&ServerDef> {
        self.servers.iter().find(|s| s.id == id)
    }
}

/// Первый найденный исполняемый файл сервера.
pub fn locate(def: &ServerDef) -> Option<PathBuf> {
    def.command.iter().find_map(|c| which::which(c).ok())
}

/// Проверка, что сервер действительно установлен (а не, например, прокси rustup без компонента).
pub fn probe(def: &ServerDef, exe: &Path) -> bool {
    if def.probe.is_empty() {
        return true;
    }
    let mut cmd = Command::new(exe);
    cmd.args(&def.probe).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::client::hide_console(&mut cmd);
    let Ok(mut child) = cmd.spawn() else { return false };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) if start.elapsed() > Duration::from_secs(6) => {
                let _ = child.kill();
                return false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
}

/// Корень проекта для файла: ближайшая папка с признаком, иначе корень рабочей папки или папка файла.
pub fn project_root(def: &ServerDef, file: &Path, workspace: Option<&Path>) -> PathBuf {
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut cur = Some(dir);
    while let Some(d) = cur {
        let hit = def.roots.iter().any(|m| match m.strip_prefix('*') {
            Some(suffix) => std::fs::read_dir(d).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().ends_with(suffix))).unwrap_or(false),
            None => d.join(m).exists(),
        });
        if hit {
            return d.to_path_buf();
        }
        if workspace.is_some_and(|w| w == d) {
            break;
        }
        cur = d.parent();
    }
    match workspace {
        Some(w) if file.starts_with(w) => w.to_path_buf(),
        _ => dir.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_servers_parse() {
        let reg = ServerRegistry::load(None);
        assert!(reg.warnings.is_empty(), "{:?}", reg.warnings);
        assert!(reg.servers.len() >= 25);
        assert_eq!(reg.for_language("python").next().map(|s| s.id.as_str()), Some("pyright"));
        assert!(reg.for_language("rust").any(|s| s.id == "rust-analyzer"));
        for s in &reg.servers {
            assert!(!s.command.is_empty(), "{}", s.id);
        }
    }

    #[test]
    fn root_prefers_marker() -> std::io::Result<()> {
        let base = std::env::temp_dir().join(format!("anvil-lsp-root-{}", std::process::id()));
        let src = base.join("crate").join("src");
        std::fs::create_dir_all(&src)?;
        std::fs::write(base.join("crate").join("Cargo.toml"), "")?;
        let reg = ServerRegistry::load(None);
        let ra = reg.get("rust-analyzer").cloned();
        if let Some(ra) = ra {
            assert_eq!(project_root(&ra, &src.join("main.rs"), Some(&base)), base.join("crate"));
        }
        std::fs::remove_dir_all(&base)?;
        Ok(())
    }
}
