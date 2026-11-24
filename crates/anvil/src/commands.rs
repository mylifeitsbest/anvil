//! Команды и горячие клавиши (пресеты VS Code / JetBrains / Sublime + свои привязки).

use anvil_config::{KeymapPreset, KeymapSettings};
use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pick {
    TypingEffect,
    CursorStyle,
    CursorBlink,
    Density,
    Easing,
    TabStyle,
    LineNumbers,
    FileIcons,
    AnimSpeed,
    Keymap,
    ImportVscodeTheme,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    PaletteCommands,
    PaletteFiles,
    PaletteThemes,
    PaletteAccent,
    PaletteGotoLine,
    PalettePick(Pick),
    FileNew,
    FileOpen,
    FileOpenFolder,
    FileSave,
    FileSaveAs,
    FileSaveAll,
    FileClose,
    FileCloseAll,
    FileReopen,
    FileCloseFolder,
    EditUndo,
    EditRedo,
    EditSelectAll,
    EditSelectLine,
    EditSelectNext,
    EditSelectAllOcc,
    EditCursorAbove,
    EditCursorBelow,
    EditMoveLineUp,
    EditMoveLineDown,
    EditCopyLineUp,
    EditCopyLineDown,
    EditDeleteLine,
    EditToggleComment,
    EditIndent,
    EditOutdent,
    EditInsertLineBelow,
    FindOpen,
    FindReplace,
    FindNext,
    FindPrev,
    ViewSidebar,
    ViewPanel,
    ViewZen,
    ViewZoomIn,
    ViewZoomOut,
    ViewZoomReset,
    ViewMinimap,
    ViewNextTab,
    ViewPrevTab,
    ViewExplorer,
    ViewToolchains,
    ViewSidebarSide,
    ViewFps,
    RunFile,
    RunStop,
    SettingsOpen,
    SettingsDefaults,
    SettingsReload,
    SettingsFolder,
    SettingsProject,
    ImportVscodeSettings,
    ThemeImportFile,
    ThemesFolder,
    PacksFolder,
    ToolsRescan,
    ToggleAnimations,
    ToggleCursorTrail,
    ToggleRainbowIndent,
    ToggleBrackets,
    RefreshTree,
    LspComplete,
    LspHover,
    GotoDefinition,
    FindReferences,
    RenameSymbol,
    FormatDocument,
    NextProblem,
    PrevProblem,
    LspRestart,
    ViewProblems,
    CollabShare,
    CollabJoin,
    CollabSession,
    CollabCopyInvite,
    CollabCopyReadInvite,
    CollabLeave,
    CollabRegisterLinks,
    CollabRelay,
    UpdateCheck,
    UpdateInstall,
    UpdateNotes,
    UpdateSkip,
    AppQuit,
    About,
}

pub struct CmdInfo {
    pub cmd: Cmd,
    pub id: &'static str,
    pub title: &'static str,
}

macro_rules! cmds {
    ($($cmd:expr, $id:literal, $title:literal;)*) => {
        pub const COMMANDS: &[CmdInfo] = &[$(CmdInfo { cmd: $cmd, id: $id, title: $title },)*];
    };
}

use Cmd::*;
cmds! {
    PaletteCommands, "palette.commands", "Палитра команд";
    PaletteFiles, "palette.files", "Быстрое открытие файла";
    PaletteThemes, "palette.themes", "Тема: выбрать цветовую тему";
    PaletteAccent, "palette.accent", "Тема: акцентный цвет";
    PaletteGotoLine, "palette.goto_line", "Перейти к строке…";
    PalettePick(Pick::TypingEffect), "pick.typing_effect", "Анимации: эффект при вводе (искры, вспышка…)";
    PalettePick(Pick::CursorStyle), "pick.cursor_style", "Курсор: стиль";
    PalettePick(Pick::CursorBlink), "pick.cursor_blink", "Курсор: мигание";
    PalettePick(Pick::Density), "pick.density", "Интерфейс: плотность";
    PalettePick(Pick::Easing), "pick.easing", "Анимации: кривая";
    PalettePick(Pick::AnimSpeed), "pick.anim_speed", "Анимации: скорость";
    PalettePick(Pick::TabStyle), "pick.tab_style", "Вкладки: стиль";
    PalettePick(Pick::LineNumbers), "pick.line_numbers", "Редактор: номера строк";
    PalettePick(Pick::FileIcons), "pick.file_icons", "Интерфейс: значки файлов";
    PalettePick(Pick::Keymap), "pick.keymap", "Горячие клавиши: пресет (VS Code, JetBrains, Sublime)";
    PalettePick(Pick::ImportVscodeTheme), "theme.import_vscode_installed", "Перенос из VS Code: тема из установленных расширений";
    ImportVscodeSettings, "settings.import_vscode", "Перенос из VS Code: настройки (settings.json)";
    ThemeImportFile, "theme.import_file", "Перенос из VS Code: тема из файла .json…";
    FileNew, "file.new", "Файл: новый";
    FileOpen, "file.open", "Файл: открыть…";
    FileOpenFolder, "file.open_folder", "Файл: открыть папку…";
    FileSave, "file.save", "Файл: сохранить";
    FileSaveAs, "file.save_as", "Файл: сохранить как…";
    FileSaveAll, "file.save_all", "Файл: сохранить всё";
    FileClose, "file.close", "Файл: закрыть вкладку";
    FileCloseAll, "file.close_all", "Файл: закрыть все вкладки";
    FileReopen, "file.reopen", "Файл: открыть закрытую вкладку";
    FileCloseFolder, "file.close_folder", "Файл: закрыть папку";
    EditUndo, "edit.undo", "Правка: отменить";
    EditRedo, "edit.redo", "Правка: повторить";
    EditSelectAll, "edit.select_all", "Выделение: всё";
    EditSelectLine, "edit.select_line", "Выделение: строка";
    EditSelectNext, "edit.select_next", "Выделение: следующее вхождение";
    EditSelectAllOcc, "edit.select_all_occurrences", "Выделение: все вхождения";
    EditCursorAbove, "edit.cursor_above", "Курсор: добавить выше";
    EditCursorBelow, "edit.cursor_below", "Курсор: добавить ниже";
    EditMoveLineUp, "edit.move_line_up", "Строки: переместить вверх";
    EditMoveLineDown, "edit.move_line_down", "Строки: переместить вниз";
    EditCopyLineUp, "edit.copy_line_up", "Строки: копировать вверх";
    EditCopyLineDown, "edit.copy_line_down", "Строки: копировать вниз";
    EditDeleteLine, "edit.delete_line", "Строки: удалить";
    EditToggleComment, "edit.toggle_comment", "Закомментировать / раскомментировать";
    EditIndent, "edit.indent", "Отступ: увеличить";
    EditOutdent, "edit.outdent", "Отступ: уменьшить";
    EditInsertLineBelow, "edit.insert_line_below", "Вставить строку ниже";
    FindOpen, "find.open", "Поиск";
    FindReplace, "find.replace", "Поиск и замена";
    FindNext, "find.next", "Поиск: следующее";
    FindPrev, "find.prev", "Поиск: предыдущее";
    ViewSidebar, "view.sidebar", "Вид: боковая панель";
    ViewPanel, "view.panel", "Вид: панель вывода";
    ViewZen, "view.zen", "Вид: режим «дзен»";
    ViewZoomIn, "view.zoom_in", "Вид: крупнее";
    ViewZoomOut, "view.zoom_out", "Вид: мельче";
    ViewZoomReset, "view.zoom_reset", "Вид: сбросить масштаб";
    ViewMinimap, "view.minimap", "Вид: миникарта";
    ViewNextTab, "view.next_tab", "Вид: следующая вкладка";
    ViewPrevTab, "view.prev_tab", "Вид: предыдущая вкладка";
    ViewExplorer, "view.explorer", "Вид: проводник";
    ViewToolchains, "view.toolchains", "Вид: тулчейны и паки";
    ViewSidebarSide, "view.sidebar_side", "Вид: боковая панель слева/справа";
    ViewFps, "view.fps", "Вид: счётчик кадров";
    RunFile, "run.file", "Запуск: текущий файл или проект";
    RunStop, "run.stop", "Запуск: остановить";
    SettingsOpen, "settings.open", "Настройки: открыть settings.toml";
    SettingsDefaults, "settings.defaults", "Настройки: все ключи с описанием";
    SettingsReload, "settings.reload", "Настройки: перечитать";
    SettingsFolder, "settings.folder", "Настройки: открыть папку";
    SettingsProject, "settings.project", "Настройки: файл проекта (.anvil/settings.toml)";
    ThemesFolder, "themes.folder", "Тема: открыть папку тем";
    PacksFolder, "packs.folder", "Паки: открыть папку паков";
    ToolsRescan, "tools.rescan", "Тулчейны: найти заново";
    ToggleAnimations, "toggle.animations", "Анимации: включить / выключить все";
    ToggleCursorTrail, "toggle.cursor_trail", "Курсор: шлейф вкл/выкл";
    ToggleRainbowIndent, "toggle.rainbow_indent", "Редактор: радужные отступы вкл/выкл";
    ToggleBrackets, "toggle.brackets", "Редактор: раскраска скобок вкл/выкл";
    RefreshTree, "tree.refresh", "Проводник: обновить";
    LspComplete, "lsp.complete", "Код: показать подсказки";
    LspHover, "lsp.hover", "Код: описание символа";
    GotoDefinition, "lsp.definition", "Код: перейти к определению";
    FindReferences, "lsp.references", "Код: найти все ссылки";
    RenameSymbol, "lsp.rename", "Код: переименовать символ";
    FormatDocument, "lsp.format", "Код: отформатировать документ";
    NextProblem, "lsp.next_problem", "Проблемы: следующая";
    PrevProblem, "lsp.prev_problem", "Проблемы: предыдущая";
    ViewProblems, "view.problems", "Вид: панель проблем";
    LspRestart, "lsp.restart", "Языковые серверы: перезапустить";
    CollabShare, "collab.share", "Совместная работа: поделиться проектом";
    CollabJoin, "collab.join", "Совместная работа: присоединиться по ссылке…";
    CollabSession, "collab.session", "Совместная работа: участники и приглашения";
    CollabCopyInvite, "collab.copy_invite", "Совместная работа: скопировать ссылку для редактирования";
    CollabCopyReadInvite, "collab.copy_read_invite", "Совместная работа: скопировать ссылку только для чтения";
    CollabLeave, "collab.leave", "Совместная работа: завершить или покинуть сессию";
    CollabRegisterLinks, "collab.register_links", "Совместная работа: открывать ссылки anvil:// в Anvil";
    CollabRelay, "collab.relay", "Совместная работа: свой relay-сервер (для интернета)…";
    UpdateCheck, "update.check", "Обновления: проверить";
    UpdateInstall, "update.install", "Обновления: скачать и установить (перезапуск)";
    UpdateNotes, "update.notes", "Обновления: что нового (страница релиза)";
    UpdateSkip, "update.skip", "Обновления: пропустить эту версию";
    About, "app.about", "О программе";
    AppQuit, "app.quit", "Выход";
}

pub fn by_id(id: &str) -> Option<Cmd> {
    COMMANDS.iter().find(|c| c.id == id).map(|c| c.cmd)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stroke {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Stroke {
    pub fn from_event(key: Key, m: Modifiers) -> Self {
        Self { key, ctrl: m.ctrl || m.command || m.mac_cmd, shift: m.shift, alt: m.alt }
    }

    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(key_label(self.key));
        s
    }
}

fn key_label(k: Key) -> &'static str {
    match k {
        Key::Comma => ",",
        Key::Period => ".",
        Key::Slash => "/",
        Key::Backslash => "\\",
        Key::Backtick => "`",
        Key::Minus => "-",
        Key::Equals => "=",
        Key::Plus => "+",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::Enter => "Enter",
        Key::Escape => "Esc",
        _ => k.name(),
    }
}

fn parse_key(s: &str) -> Option<Key> {
    let k = match s.to_lowercase().as_str() {
        "," => Key::Comma,
        "." => Key::Period,
        "/" => Key::Slash,
        "\\" => Key::Backslash,
        "`" => Key::Backtick,
        "-" => Key::Minus,
        "=" => Key::Equals,
        "+" => Key::Plus,
        "[" => Key::OpenBracket,
        "]" => Key::CloseBracket,
        ";" => Key::Semicolon,
        "'" => Key::Quote,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "enter" | "return" => Key::Enter,
        "esc" | "escape" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        other if other.len() == 1 && other.chars().all(|c| c.is_ascii_digit()) => Key::from_name(other).or_else(|| Key::from_name(&format!("Num{other}")))?,
        other => Key::from_name(&other.to_uppercase()).or_else(|| Key::from_name(other))?,
    };
    Some(k)
}

/// `ctrl+shift+p` или аккорд `ctrl+k ctrl+o`.
pub fn parse_binding(s: &str) -> Option<Vec<Stroke>> {
    let mut out = Vec::new();
    for part in s.split_whitespace() {
        let mut st = Stroke { key: Key::A, ctrl: false, shift: false, alt: false };
        let mut key = None;
        // «ctrl++» — плюс как клавиша
        let tokens: Vec<&str> = match part.strip_suffix("++") {
            Some(head) => head.split('+').chain(std::iter::once("+")).collect(),
            None => part.split('+').collect(),
        };
        for t in tokens {
            match t.to_lowercase().as_str() {
                "ctrl" | "cmd" | "control" | "meta" => st.ctrl = true,
                "shift" => st.shift = true,
                "alt" | "option" => st.alt = true,
                "" => {}
                k => key = Some(parse_key(k)?),
            }
        }
        st.key = key?;
        out.push(st);
    }
    (!out.is_empty()).then_some(out)
}

const VSCODE: &[(&str, Cmd)] = &[
    ("ctrl+shift+p", PaletteCommands),
    ("f1", PaletteCommands),
    ("ctrl+p", PaletteFiles),
    ("ctrl+k ctrl+t", PaletteThemes),
    ("ctrl+g", PaletteGotoLine),
    ("ctrl+n", FileNew),
    ("ctrl+o", FileOpen),
    ("ctrl+k ctrl+o", FileOpenFolder),
    ("ctrl+s", FileSave),
    ("ctrl+shift+s", FileSaveAs),
    ("ctrl+k s", FileSaveAll),
    ("ctrl+w", FileClose),
    ("ctrl+f4", FileClose),
    ("ctrl+k ctrl+w", FileCloseAll),
    ("ctrl+shift+t", FileReopen),
    ("ctrl+z", EditUndo),
    ("ctrl+y", EditRedo),
    ("ctrl+shift+z", EditRedo),
    ("ctrl+a", EditSelectAll),
    ("ctrl+l", EditSelectLine),
    ("ctrl+d", EditSelectNext),
    ("ctrl+shift+l", EditSelectAllOcc),
    ("ctrl+alt+up", EditCursorAbove),
    ("ctrl+alt+down", EditCursorBelow),
    ("alt+up", EditMoveLineUp),
    ("alt+down", EditMoveLineDown),
    ("shift+alt+up", EditCopyLineUp),
    ("shift+alt+down", EditCopyLineDown),
    ("ctrl+shift+k", EditDeleteLine),
    ("ctrl+/", EditToggleComment),
    ("ctrl+]", EditIndent),
    ("ctrl+[", EditOutdent),
    ("ctrl+enter", EditInsertLineBelow),
    ("ctrl+f", FindOpen),
    ("ctrl+h", FindReplace),
    ("f3", FindNext),
    ("shift+f3", FindPrev),
    ("ctrl+b", ViewSidebar),
    ("ctrl+j", ViewPanel),
    ("ctrl+`", ViewPanel),
    ("ctrl+k z", ViewZen),
    ("ctrl+=", ViewZoomIn),
    ("ctrl++", ViewZoomIn),
    ("ctrl+-", ViewZoomOut),
    ("ctrl+0", ViewZoomReset),
    ("ctrl+tab", ViewNextTab),
    ("ctrl+pagedown", ViewNextTab),
    ("ctrl+shift+tab", ViewPrevTab),
    ("ctrl+pageup", ViewPrevTab),
    ("ctrl+shift+e", ViewExplorer),
    ("ctrl+shift+x", ViewToolchains),
    ("f5", RunFile),
    ("ctrl+f5", RunFile),
    ("shift+f5", RunStop),
    ("ctrl+,", SettingsOpen),
    ("ctrl+space", LspComplete),
    ("ctrl+k ctrl+i", LspHover),
    ("f12", GotoDefinition),
    ("shift+f12", FindReferences),
    ("f2", RenameSymbol),
    ("shift+alt+f", FormatDocument),
    ("f8", NextProblem),
    ("shift+f8", PrevProblem),
    ("ctrl+shift+m", ViewProblems),
    ("ctrl+q", AppQuit),
];

const JETBRAINS: &[(&str, Cmd)] = &[
    ("ctrl+shift+a", PaletteCommands),
    ("ctrl+shift+n", PaletteFiles),
    ("ctrl+d", EditCopyLineDown),
    ("ctrl+y", EditDeleteLine),
    ("alt+j", EditSelectNext),
    ("ctrl+shift+alt+j", EditSelectAllOcc),
    ("ctrl+shift+up", EditMoveLineUp),
    ("ctrl+shift+down", EditMoveLineDown),
    ("ctrl+shift+z", EditRedo),
    ("ctrl+r", FindReplace),
    ("shift+f10", RunFile),
    ("ctrl+f2", RunStop),
    ("alt+1", ViewExplorer),
    ("alt+4", ViewPanel),
    ("ctrl+alt+s", SettingsOpen),
    ("ctrl+b", GotoDefinition),
    ("alt+f7", FindReferences),
    ("shift+f6", RenameSymbol),
    ("ctrl+alt+l", FormatDocument),
    ("ctrl+q", LspHover),
    ("f2", NextProblem),
    ("shift+f2", PrevProblem),
];

const SUBLIME: &[(&str, Cmd)] = &[
    ("ctrl+shift+d", EditCopyLineDown),
    ("ctrl+shift+up", EditMoveLineUp),
    ("ctrl+shift+down", EditMoveLineDown),
    ("ctrl+b", RunFile),
    ("ctrl+k ctrl+b", ViewSidebar),
    ("ctrl+r", PaletteGotoLine),
];

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    pub binds: Vec<(Vec<Stroke>, Cmd)>,
}

impl Keymap {
    pub fn build(s: &KeymapSettings) -> (Self, Vec<String>) {
        let mut km = Keymap::default();
        let mut warnings = Vec::new();
        let add = |km: &mut Keymap, warnings: &mut Vec<String>, key: &str, cmd: Option<Cmd>| match parse_binding(key) {
            Some(strokes) => {
                km.binds.retain(|(k, _)| *k != strokes);
                if let Some(c) = cmd {
                    km.binds.push((strokes, c));
                }
            }
            None => warnings.push(format!("[keymap]: не понял сочетание «{key}»")),
        };
        for (k, c) in VSCODE {
            add(&mut km, &mut warnings, k, Some(*c));
        }
        let preset: &[(&str, Cmd)] = match s.preset {
            KeymapPreset::Vscode => &[],
            KeymapPreset::Jetbrains => JETBRAINS,
            KeymapPreset::Sublime => SUBLIME,
        };
        for (k, c) in preset {
            add(&mut km, &mut warnings, k, Some(*c));
        }
        for b in &s.bindings {
            if b.command.is_empty() {
                add(&mut km, &mut warnings, &b.key, None);
            } else {
                match by_id(&b.command) {
                    Some(c) => add(&mut km, &mut warnings, &b.key, Some(c)),
                    None => warnings.push(format!("[keymap]: неизвестная команда «{}»", b.command)),
                }
            }
        }
        (km, warnings)
    }

    pub fn label(&self, cmd: Cmd) -> Option<String> {
        self.binds.iter().find(|(_, c)| *c == cmd).map(|(s, _)| s.iter().map(Stroke::label).collect::<Vec<_>>().join(" "))
    }

    /// Результат нажатия: команда, начало аккорда или ничего.
    pub fn resolve(&self, pending: Option<Stroke>, st: Stroke) -> Resolve {
        if let Some(first) = pending {
            if let Some((_, c)) = self.binds.iter().find(|(k, _)| k.len() == 2 && k[0] == first && k[1] == st) {
                return Resolve::Cmd(*c);
            }
            return Resolve::Cancel;
        }
        if self.binds.iter().any(|(k, _)| k.len() == 2 && k[0] == st) {
            return Resolve::Chord(st);
        }
        match self.binds.iter().find(|(k, _)| k.len() == 1 && k[0] == st) {
            Some((_, c)) => Resolve::Cmd(*c),
            None => Resolve::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolve {
    Cmd(Cmd),
    Chord(Stroke),
    Cancel,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bindings() {
        let b = parse_binding("ctrl+shift+p").unwrap_or_default();
        assert_eq!(b.len(), 1);
        assert!(b[0].ctrl && b[0].shift && b[0].key == Key::P);
        assert_eq!(parse_binding("ctrl+k ctrl+o").map(|v| v.len()), Some(2));
        assert_eq!(parse_binding("ctrl++").map(|v| v[0].key), Some(Key::Plus));
        assert_eq!(parse_binding("ctrl+`").map(|v| v[0].key), Some(Key::Backtick));
        assert_eq!(parse_binding("f5").map(|v| v[0].key), Some(Key::F5));
        assert_eq!(parse_binding("ctrl+0").map(|v| v[0].key), Some(Key::Num0));
        assert!(parse_binding("ctrl+wat").is_none());
    }

    #[test]
    fn default_keymap_and_chords() {
        let (km, w) = Keymap::build(&KeymapSettings::default());
        assert!(w.is_empty(), "{w:?}");
        let ctrl = |key| Stroke { key, ctrl: true, shift: false, alt: false };
        assert_eq!(km.resolve(None, ctrl(Key::S)), Resolve::Cmd(FileSave));
        assert_eq!(km.resolve(None, ctrl(Key::K)), Resolve::Chord(ctrl(Key::K)));
        assert_eq!(km.resolve(Some(ctrl(Key::K)), ctrl(Key::O)), Resolve::Cmd(FileOpenFolder));
    }

    #[test]
    fn user_bindings_override_and_unbind() {
        let s = KeymapSettings {
            preset: KeymapPreset::Jetbrains,
            bindings: vec![
                anvil_config::KeyBinding { key: "ctrl+s".into(), command: String::new() },
                anvil_config::KeyBinding { key: "f9".into(), command: "run.file".into() },
            ],
        };
        let (km, w) = Keymap::build(&s);
        assert!(w.is_empty());
        let st = |key, ctrl| Stroke { key, ctrl, shift: false, alt: false };
        assert_eq!(km.resolve(None, st(Key::S, true)), Resolve::None);
        assert_eq!(km.resolve(None, st(Key::F9, false)), Resolve::Cmd(RunFile));
        assert_eq!(km.resolve(None, st(Key::D, true)), Resolve::Cmd(EditCopyLineDown));
    }

    #[test]
    fn command_ids_are_unique() {
        let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}
