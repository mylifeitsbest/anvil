//! Сессия: последняя папка, открытые файлы, недавние проекты (`state.toml`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub root: Option<PathBuf>,
    pub open: Vec<PathBuf>,
    pub active: usize,
    pub recent: Vec<PathBuf>,
    pub panel_open: bool,
    /// Версия, о которой не напоминать («Обновления: пропустить эту версию»).
    pub skipped_update: Option<String>,
}

impl Session {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn remember_root(&mut self, root: &Path) {
        self.recent.retain(|r| r != root);
        self.recent.insert(0, root.to_path_buf());
        self.recent.truncate(10);
    }
}
