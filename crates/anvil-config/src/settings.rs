//! Схема `settings.toml`. Каждое поле имеет значение по умолчанию, поэтому
//! в файле достаточно указать только то, что хочется поменять.
//!
//! Многое из того, что в VS Code ставится отдельными расширениями
//! (Peacock, indent-rainbow, Bracket Pair Colorizer, Better Comments, Power Mode,
//! smooth caret, GlassIt, Customize UI…), здесь — обычные ключи настроек:
//! они ничего не стоят, пока выключены, и не запускают сторонний код.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::color::Rgba;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeSettings,
    /// Точечная перекраска интерфейса: `bg = "#101014"`, `accent = "#8b7bff"`, …
    pub colors: BTreeMap<String, Rgba>,
    /// Точечная перекраска подсветки: `keyword = "#ff8800"` или `comment = { color = "#666", italic = true }`.
    pub syntax: BTreeMap<String, SyntaxOverride>,
    pub editor: EditorSettings,
    pub cursor: CursorSettings,
    pub ui: UiSettings,
    pub animations: AnimationSettings,
    pub window: WindowSettings,
    pub files: FileSettings,
    pub keymap: KeymapSettings,
    pub toolchains: ToolchainSettings,
    pub lsp: LspSettings,
    pub collab: CollabSettings,
    pub update: UpdateSettings,
}

// ---------------------------------------------------------------- тема

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    /// Встроенная (`anvil-dark`, `anvil-light`, `midnight`, `paper`, `graphite`, `oled`) или своя из папки `themes/`.
    pub name: String,
    /// Акцентный цвет. Если не задан — берётся из темы.
    pub accent: Option<Rgba>,
    /// Скругление углов интерфейса, px.
    pub radius: f32,
    /// Подкрашивание заголовка и статус-бара (как Peacock). Удобно задавать в `.anvil/settings.toml` проекта.
    pub tint: Option<Rgba>,
    /// Сила подкрашивания 0..1.
    pub tint_strength: f32,
    /// Множитель контраста текста 0.5..1.5.
    pub contrast: f32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self { name: "anvil-dark".into(), accent: None, radius: 8.0, tint: None, tint_strength: 0.18, contrast: 1.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SyntaxOverride {
    Color(Rgba),
    Style(SyntaxStyleSpec),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SyntaxStyleSpec {
    pub color: Option<Rgba>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
}

// ---------------------------------------------------------------- редактор

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineNumbers {
    On,
    Relative,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurrentLine {
    Fill,
    Border,
    Gutter,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Whitespace {
    None,
    Boundary,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    Always,
    Hover,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    /// Имя системного шрифта, путь к .ttf/.otf или `auto`.
    pub font: String,
    pub font_size: f32,
    /// Межстрочный интервал как множитель размера шрифта.
    pub line_height: f32,
    pub letter_spacing: f32,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub line_numbers: LineNumbers,
    pub current_line: CurrentLine,
    pub indent_guides: bool,
    /// Разноцветные уровни отступов (как indent-rainbow).
    pub rainbow_indent: bool,
    pub rainbow_indent_colors: Vec<Rgba>,
    /// Раскраска парных скобок по глубине (как Bracket Pair Colorizer).
    pub bracket_colorization: bool,
    pub bracket_colors: Vec<Rgba>,
    pub auto_close_brackets: bool,
    pub auto_close_quotes: bool,
    pub auto_indent: bool,
    pub render_whitespace: Whitespace,
    pub scroll_beyond_last_line: bool,
    pub scroll_sensitivity: f32,
    pub padding_top: f32,
    /// Ширина колонки текста в символах; 0 — во всю ширину. Текст центрируется (режим «дзен»).
    pub max_width: u32,
    /// Подсветка TODO/FIXME/NOTE/! в комментариях (как Better Comments).
    pub comment_tags: bool,
    pub comment_tag_colors: BTreeMap<String, Rgba>,
    pub highlight_matching_brackets: bool,
    pub highlight_selection_matches: bool,
    pub minimap: MinimapSettings,
}

impl Default for EditorSettings {
    fn default() -> Self {
        let tags = [("TODO", "#f5c04a"), ("FIXME", "#ff5c5c"), ("NOTE", "#4cc9f0"), ("HACK", "#c77dff"), ("!", "#ff5c5c"), ("?", "#4cc9f0")];
        Self {
            font: "auto".into(),
            font_size: 14.0,
            line_height: 1.6,
            letter_spacing: 0.0,
            tab_size: 4,
            insert_spaces: true,
            line_numbers: LineNumbers::On,
            current_line: CurrentLine::Fill,
            indent_guides: true,
            rainbow_indent: false,
            rainbow_indent_colors: ["#ffd16614", "#c77dff14", "#4cc9f014", "#06d6a014"].iter().filter_map(|c| Rgba::parse(c)).collect(),
            bracket_colorization: true,
            bracket_colors: ["#ffd166", "#c77dff", "#4cc9f0"].iter().filter_map(|c| Rgba::parse(c)).collect(),
            auto_close_brackets: true,
            auto_close_quotes: true,
            auto_indent: true,
            render_whitespace: Whitespace::None,
            scroll_beyond_last_line: true,
            scroll_sensitivity: 1.0,
            padding_top: 8.0,
            max_width: 0,
            comment_tags: true,
            comment_tag_colors: tags.iter().filter_map(|(k, v)| Some(((*k).to_string(), Rgba::parse(v)?))).collect(),
            highlight_matching_brackets: true,
            highlight_selection_matches: true,
            minimap: MinimapSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MinimapSettings {
    pub enabled: bool,
    pub width: f32,
    /// Высота строки в миникарте, px.
    pub line_px: f32,
    pub slider: Visibility,
    pub opacity: f32,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self { enabled: true, width: 72.0, line_px: 3.0, slider: Visibility::Always, opacity: 0.8 }
    }
}

// ---------------------------------------------------------------- курсор

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorStyle {
    Line,
    Block,
    Underline,
    Hollow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorBlink {
    /// Плавное затухание.
    Smooth,
    /// Классическое вкл/выкл.
    Blink,
    /// Мягкая «фаза», как в VS Code `phase`.
    Phase,
    /// Курсор «дышит» по высоте.
    Expand,
    /// Не мигает.
    Solid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorSettings {
    pub style: CursorStyle,
    pub width: f32,
    pub blink: CursorBlink,
    pub blink_interval_ms: u32,
    /// Через сколько секунд простоя перестать мигать (0 — никогда). Экономит CPU.
    pub blink_timeout_s: u32,
    /// Плавное перемещение курсора.
    pub smooth: bool,
    pub smooth_ms: u32,
    /// Шлейф за курсором при прыжках (как в Neovide).
    pub trail: bool,
    pub trail_ms: u32,
    pub glow: bool,
    pub glow_radius: f32,
    /// Свой цвет курсора; по умолчанию — акцент.
    pub color: Option<Rgba>,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            style: CursorStyle::Line,
            width: 2.0,
            blink: CursorBlink::Smooth,
            blink_interval_ms: 1060,
            blink_timeout_s: 12,
            smooth: true,
            smooth_ms: 90,
            trail: true,
            trail_ms: 140,
            glow: true,
            glow_radius: 8.0,
            color: None,
        }
    }
}

// ---------------------------------------------------------------- интерфейс

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Compact,
    Comfortable,
    Spacious,
}

impl Density {
    pub fn factor(self) -> f32 {
        match self {
            Density::Compact => 0.82,
            Density::Comfortable => 1.0,
            Density::Spacious => 1.18,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TabStyle {
    Underline,
    Pill,
    Classic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileIcons {
    Dots,
    Letters,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub font: String,
    pub font_size: f32,
    /// Масштаб всего интерфейса.
    pub scale: f32,
    pub density: Density,
    pub sidebar: Side,
    pub sidebar_width: f32,
    pub sidebar_visible: bool,
    pub titlebar_height: f32,
    pub tab_height: f32,
    pub tab_style: TabStyle,
    pub tab_close_button: Visibility,
    pub statusbar: bool,
    pub statusbar_height: f32,
    pub breadcrumbs: bool,
    pub file_icons: FileIcons,
    pub panel_height: f32,
    pub shadows: bool,
    pub show_fps: bool,
    /// Режим «дзен»: скрыть всё, кроме текста.
    pub zen: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            font: "auto".into(),
            font_size: 13.0,
            scale: 1.0,
            density: Density::Comfortable,
            sidebar: Side::Left,
            sidebar_width: 240.0,
            sidebar_visible: true,
            titlebar_height: 38.0,
            tab_height: 34.0,
            tab_style: TabStyle::Underline,
            tab_close_button: Visibility::Hover,
            statusbar: true,
            statusbar_height: 24.0,
            breadcrumbs: true,
            file_icons: FileIcons::Dots,
            panel_height: 220.0,
            shadows: true,
            show_fps: false,
            zen: false,
        }
    }
}

// ---------------------------------------------------------------- анимации

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    Linear,
    OutCubic,
    OutQuint,
    InOutCubic,
    OutBack,
    Spring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Motion {
    Slide,
    Fade,
    Scale,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypingEffect {
    None,
    /// Символ «выпрыгивает» при вводе.
    Pop,
    /// Искры из-под курсора (как Power Mode).
    Sparks,
    /// Вспышка вокруг курсора.
    Glow,
}

/// Одна анимация: включена ли, сколько длится, как выглядит.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anim {
    pub enabled: bool,
    pub duration_ms: u32,
    pub style: Motion,
    pub easing: Option<Easing>,
}

impl Anim {
    pub const fn new(duration_ms: u32, style: Motion) -> Self {
        Self { enabled: true, duration_ms, style, easing: None }
    }
}

impl Default for Anim {
    fn default() -> Self {
        Self::new(180, Motion::Slide)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TypingSettings {
    pub effect: TypingEffect,
    pub intensity: f32,
    /// Лёгкая тряска экрана при вводе.
    pub shake: bool,
}

impl Default for TypingSettings {
    fn default() -> Self {
        Self { effect: TypingEffect::None, intensity: 1.0, shake: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationSettings {
    /// Главный выключатель всех анимаций.
    pub enabled: bool,
    /// Множитель скорости: 2.0 — вдвое быстрее, 0.5 — вдвое медленнее.
    pub speed: f32,
    pub easing: Easing,
    pub smooth_scroll: bool,
    pub scroll_smoothing_ms: u32,
    pub launch: Anim,
    pub tabs: Anim,
    pub tab_indicator: Anim,
    pub panels: Anim,
    pub palette: Anim,
    pub theme: Anim,
    pub hover: Anim,
    pub tree: Anim,
    pub tree_stagger_ms: u32,
    pub selection: Anim,
    pub current_line: Anim,
    pub toasts: Anim,
    pub typing: TypingSettings,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.0,
            easing: Easing::OutCubic,
            smooth_scroll: true,
            scroll_smoothing_ms: 110,
            launch: Anim::new(380, Motion::Scale),
            tabs: Anim::new(180, Motion::Slide),
            tab_indicator: Anim::new(220, Motion::Slide),
            panels: Anim::new(220, Motion::Slide),
            palette: Anim::new(170, Motion::Scale),
            theme: Anim::new(380, Motion::Fade),
            hover: Anim::new(110, Motion::Fade),
            tree: Anim::new(160, Motion::Slide),
            tree_stagger_ms: 14,
            selection: Anim::new(90, Motion::Fade),
            current_line: Anim::new(110, Motion::Slide),
            toasts: Anim::new(240, Motion::Slide),
            typing: TypingSettings::default(),
        }
    }
}

// ---------------------------------------------------------------- окно, файлы, клавиши, тулчейны

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    /// Прозрачность окна 0.3..1 (как GlassIt). Применяется после перезапуска.
    pub opacity: f32,
    pub custom_titlebar: bool,
    pub width: f32,
    pub height: f32,
    pub start_maximized: bool,
    pub restore_session: bool,
    pub vsync: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self { opacity: 1.0, custom_titlebar: true, width: 1280.0, height: 800.0, start_maximized: false, restore_session: true, vsync: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoSave {
    Off,
    Delay,
    Focus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSettings {
    pub auto_save: AutoSave,
    pub auto_save_delay_ms: u32,
    pub exclude: Vec<String>,
    pub show_hidden: bool,
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
    /// Файлы больше этого размера открываются без подсветки (МБ).
    pub large_file_mb: u32,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            auto_save: AutoSave::Off,
            auto_save_delay_ms: 1000,
            exclude: ["target", "node_modules", ".git", "dist", "build", "__pycache__", ".venv", ".idea", ".vs"].map(String::from).to_vec(),
            show_hidden: false,
            trim_trailing_whitespace: false,
            insert_final_newline: true,
            large_file_mb: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeymapPreset {
    Vscode,
    Jetbrains,
    Sublime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    /// Например `ctrl+shift+p`, `alt+up`, `f5`.
    pub key: String,
    /// Идентификатор команды, например `palette.commands`. Пустая строка — снять привязку.
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapSettings {
    pub preset: KeymapPreset,
    pub bindings: Vec<KeyBinding>,
}

impl Default for KeymapSettings {
    fn default() -> Self {
        Self { preset: KeymapPreset::Vscode, bindings: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ToolchainOverride {
    pub build: Option<String>,
    pub run: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolchainSettings {
    pub auto_detect: bool,
    /// Свои команды для языка: `[toolchains.overrides.rust] run = "cargo run --release"`.
    pub overrides: BTreeMap<String, ToolchainOverride>,
    /// Сохранять файл перед запуском.
    pub save_before_run: bool,
}

impl Default for ToolchainSettings {
    fn default() -> Self {
        Self { auto_detect: true, overrides: BTreeMap::new(), save_before_run: true }
    }
}

// ---------------------------------------------------------------- языковые серверы

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InlineDiagnostics {
    Off,
    Errors,
    All,
}

/// Свой сервер или аргументы для языка: `[lsp.servers.python] command = "pylsp"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LspServerOverride {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LspSettings {
    /// Главный выключатель языковых серверов.
    pub enabled: bool,
    pub completion: bool,
    /// Показывать подсказки во время ввода (иначе только по Ctrl+Space).
    pub completion_auto: bool,
    pub hover: bool,
    pub hover_delay_ms: u32,
    /// Волнистое подчёркивание ошибок.
    pub squiggles: bool,
    /// Текст ошибки в конце строки (как Error Lens): off · errors · all.
    pub inline_diagnostics: InlineDiagnostics,
    pub format_on_save: bool,
    pub servers: BTreeMap<String, LspServerOverride>,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            completion: true,
            completion_auto: true,
            hover: true,
            hover_delay_ms: 450,
            squiggles: true,
            inline_diagnostics: InlineDiagnostics::Errors,
            format_on_save: false,
            servers: BTreeMap::new(),
        }
    }
}

// ---------------------------------------------------------------- совместная работа

/// Подписи с именами над чужими курсорами.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorLabels {
    Always,
    /// Несколько секунд после движения курсора.
    Activity,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CollabSettings {
    /// Имя, которое видят участники (пусто — имя пользователя системы).
    pub name: String,
    /// Цвет своего курсора у других (`None` — подобрать по имени).
    pub color: Option<Rgba>,
    /// Адрес relay (`anvil://хост:порт?fp=…&key=…`); пусто — встроенный relay для локальной сети.
    pub relay: String,
    /// Порт встроенного relay.
    pub port: u16,
    /// На каком адресе слушает встроенный relay: `0.0.0.0` — вся локальная сеть, `127.0.0.1` — только этот компьютер.
    pub listen: String,
    /// Адрес для ссылок встроенного relay (пусто — найти IP в локальной сети).
    pub public_host: String,
    pub labels: CursorLabels,
    /// Сколько секунд подпись видна после движения (для `labels = "activity"`).
    pub label_seconds: f32,
    /// Подсвечивать чужие выделения.
    pub selections: bool,
    /// Полоски на полях у строк, где стоят чужие курсоры.
    pub gutter_marks: bool,
    /// Кнопка «Поделиться» в заголовке окна.
    pub share_button: bool,
    /// Ссылка для редактирования сразу копируется в буфер обмена.
    pub copy_invite: bool,
}

impl Default for CollabSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: None,
            relay: String::new(),
            port: 7878,
            listen: "0.0.0.0".into(),
            public_host: String::new(),
            labels: CursorLabels::Activity,
            label_seconds: 3.0,
            selections: true,
            gutter_marks: true,
            share_button: true,
            copy_invite: true,
        }
    }
}

// ---------------------------------------------------------------- обновления

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Проверять новые версии на GitHub (через несколько секунд после запуска и потом по таймеру).
    pub check: bool,
    /// Как часто проверять, пока Anvil открыт, в часах (0 — только при запуске).
    pub interval_hours: u32,
    /// Сразу скачивать новую версию в фоне — останется нажать «Перезапустить».
    pub auto_download: bool,
    /// Если обновление скачано, но не установлено — поставить его при выходе.
    pub install_on_exit: bool,
    /// Предлагать и предварительные версии (beta, rc).
    pub prerelease: bool,
    /// Плашка в углу окна; `false` — только значок в строке состояния.
    pub banner: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { check: true, interval_hours: 6, auto_download: false, install_on_exit: true, prerelease: false, banner: true }
    }
}
