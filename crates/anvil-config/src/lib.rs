//! Конфигурация Anvil.
//!
//! * `settings.toml` пользователя + `.anvil/settings.toml` проекта (глубокое слияние таблиц);
//! * неизвестные ключи не ломают загрузку, а возвращаются как предупреждения;
//! * темы — TOML-файлы, плюс импорт тем VS Code.

pub mod color;
pub mod settings;
pub mod theme;
pub mod vscode;
pub mod vscode_settings;

use std::path::{Path, PathBuf};

pub use color::Rgba;
pub use settings::*;
pub use theme::{Palette, ResolvedTheme, SYNTAX_KEYS, SyntaxStyle, ThemeDef, ThemeRegistry};

/// Полный, подробно прокомментированный `settings.toml` со значениями по умолчанию.
pub const DEFAULT_SETTINGS_TOML: &str = include_str!("../default-settings.toml");

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("ввод-вывод: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Где лежат файлы пользователя.
#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
    pub settings: PathBuf,
    pub themes: PathBuf,
    pub packs: PathBuf,
    pub state: PathBuf,
    /// Портативный режим: папка `anvil-data` рядом с исполняемым файлом.
    pub portable: bool,
}

impl Paths {
    /// `ANVIL_CONFIG_DIR` → портативная `anvil-data/` рядом с exe → системная папка настроек.
    pub fn detect() -> Self {
        if let Some(dir) = std::env::var_os("ANVIL_CONFIG_DIR") {
            return Self::at(PathBuf::from(dir), false);
        }
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("anvil-data")))
            && dir.is_dir()
        {
            return Self::at(dir, true);
        }
        let dir = directories::ProjectDirs::from("", "", "Anvil").map(|p| p.config_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from(".anvil-config"));
        Self::at(dir, false)
    }

    pub fn at(dir: PathBuf, portable: bool) -> Self {
        Self { settings: dir.join("settings.toml"), themes: dir.join("themes"), packs: dir.join("packs"), state: dir.join("state.toml"), dir, portable }
    }

    /// Создаёт папки и пишет шаблон настроек, если файла ещё нет.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.themes)?;
        std::fs::create_dir_all(&self.packs)?;
        if !self.settings.exists() {
            std::fs::write(&self.settings, DEFAULT_SETTINGS_TOML)?;
        }
        Ok(())
    }
}

/// Рекурсивное слияние: значения `over` заменяют `base`, вложенные таблицы сливаются.
pub fn merge_tables(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge_tables(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// Разбирает таблицу настроек; неизвестные ключи попадают в предупреждения.
pub fn settings_from_table(table: toml::Table) -> Result<(Settings, Vec<String>), ConfigError> {
    let mut unknown = Vec::new();
    let settings: Settings = serde_ignored::deserialize(toml::Value::Table(table), |path| unknown.push(format!("неизвестный ключ «{path}»")))?;
    Ok((settings, unknown))
}

pub fn parse_settings(text: &str) -> Result<(Settings, Vec<String>), ConfigError> {
    settings_from_table(text.parse::<toml::Table>()?)
}

/// Результат загрузки: настройки и всё, что стоит показать пользователю.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    pub warnings: Vec<String>,
    /// Была ли ошибка разбора (тогда `settings` — значения по умолчанию).
    pub failed: bool,
}

pub fn load_settings(user: &Path, project: Option<&Path>) -> Loaded {
    let mut warnings = Vec::new();
    let mut failed = false;
    let mut table = toml::Table::new();
    let mut read = |path: &Path, table: &mut toml::Table| match std::fs::read_to_string(path) {
        Ok(text) => match text.parse::<toml::Table>() {
            Ok(t) => merge_tables(table, t),
            Err(e) => {
                warnings.push(format!("{}: {e}", path.display()));
                failed = true;
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warnings.push(format!("{}: {e}", path.display())),
    };
    read(user, &mut table);
    if let Some(p) = project {
        read(p, &mut table);
    }
    match settings_from_table(table) {
        Ok((settings, mut w)) => {
            warnings.append(&mut w);
            Loaded { settings, warnings, failed }
        }
        Err(e) => {
            warnings.push(e.to_string());
            Loaded { settings: Settings::default(), warnings, failed: true }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_matches_defaults_exactly() -> Result<(), ConfigError> {
        let (s, unknown) = parse_settings(DEFAULT_SETTINGS_TOML)?;
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(s, Settings::default());
        Ok(())
    }

    #[test]
    fn partial_file_keeps_other_defaults() -> Result<(), ConfigError> {
        let (s, unknown) = parse_settings("[editor]\nfont_size = 18\n[cursor]\nstyle = \"block\"\nwat = 1\n")?;
        assert_eq!(s.editor.font_size, 18.0);
        assert_eq!(s.cursor.style, CursorStyle::Block);
        assert_eq!(s.editor.tab_size, 4);
        assert_eq!(unknown.len(), 1);
        Ok(())
    }

    #[test]
    fn project_overrides_user() -> Result<(), ConfigError> {
        let mut base: toml::Table = "[theme]\nname = \"midnight\"\nradius = 4\n".parse()?;
        merge_tables(&mut base, "[theme]\ntint = \"#ff0000\"\n".parse()?);
        let (s, _) = settings_from_table(base)?;
        assert_eq!(s.theme.name, "midnight");
        assert_eq!(s.theme.radius, 4.0);
        assert_eq!(s.theme.tint, Rgba::parse("#ff0000"));
        Ok(())
    }

    #[test]
    fn bad_color_is_an_error() {
        assert!(parse_settings("[theme]\naccent = \"red\"\n").is_err());
    }
}
