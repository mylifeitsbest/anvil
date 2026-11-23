//! Открытый документ: ядро редактора + язык + кэш подсветки + состояние вида.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anvil_config::Settings;
use anvil_core::{Buffer, EditConfig, Editor, Selections};
use anvil_syntax::{Bracket, Highlighter, Language, Span};
use eframe::egui::Pos2;

/// Анимация одного курсора (в координатах содержимого: x — пиксели от начала строки, y — от верха файла).
#[derive(Clone, Copy, Debug)]
pub struct CaretAnim {
    pub from: Pos2,
    pub to: Pos2,
    pub start: f64,
    pub trail_from: Pos2,
    pub trail_start: f64,
}

#[derive(Debug, Default)]
pub struct View {
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub disp_y: f32,
    pub disp_x: f32,
    pub carets: Vec<CaretAnim>,
    /// Последнее движение/правка — от него считается мигание.
    pub activity: f64,
    pub sel_changed: f64,
    pub last_sel: Option<Selections>,
    pub last_serial: u64,
    /// Прокрутить к курсору на следующем кадре.
    pub reveal: bool,
    pub center: bool,
    pub find_matches: Vec<(usize, usize)>,
    pub find_key: String,
    pub scrolled_at: f64,
}

pub struct Document {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub name: String,
    pub lang: &'static Language,
    pub ed: Editor,
    pub spans: Vec<Span>,
    pub brackets: Vec<Bracket>,
    pub hl_version: u64,
    pub large: bool,
    pub view: View,
    pub disk_mtime: Option<SystemTime>,
}

pub fn edit_config(lang: &Language, s: &Settings) -> EditConfig {
    EditConfig {
        tab_size: s.editor.tab_size.clamp(1, 16) as usize,
        insert_spaces: s.editor.insert_spaces,
        auto_close_brackets: s.editor.auto_close_brackets,
        auto_close_quotes: s.editor.auto_close_quotes && !matches!(lang.id, "text" | "markdown"),
        auto_indent: s.editor.auto_indent,
        line_comment: lang.line_comment.map(str::to_string),
        indent_after: lang.indent_after.to_vec(),
    }
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Document {
    pub fn open(path: &Path, id: u64, s: &Settings, now: f64) -> std::io::Result<Self> {
        let buffer = Buffer::load(path)?;
        let lang = anvil_syntax::detect(path);
        let large = buffer.rope.len_bytes() as u64 > s.files.large_file_mb as u64 * 1024 * 1024;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
        Ok(Self {
            id,
            path: Some(path.to_path_buf()),
            name,
            lang,
            ed: Editor::new(buffer, edit_config(lang, s)),
            spans: Vec::new(),
            brackets: Vec::new(),
            hl_version: u64::MAX,
            large,
            view: View { reveal: true, activity: now, ..View::default() },
            disk_mtime: mtime(path),
        })
    }

    pub fn untitled(n: usize, id: u64, s: &Settings, now: f64) -> Self {
        let lang = anvil_syntax::by_id("text").unwrap_or(&anvil_syntax::LANGUAGES[0]);
        Self {
            id,
            path: None,
            name: if n <= 1 { "Без названия".into() } else { format!("Без названия {n}") },
            lang,
            ed: Editor::new(Buffer::default(), edit_config(lang, s)),
            spans: Vec::new(),
            brackets: Vec::new(),
            hl_version: u64::MAX,
            large: false,
            view: View { activity: now, ..View::default() },
            disk_mtime: None,
        }
    }

    pub fn dirty(&self) -> bool {
        self.ed.buffer.is_dirty()
    }

    pub fn set_path(&mut self, path: &Path, s: &Settings) {
        self.path = Some(path.to_path_buf());
        self.name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.lang = anvil_syntax::detect(path);
        self.ed.cfg = edit_config(self.lang, s);
        self.hl_version = u64::MAX;
    }

    pub fn save(&mut self, s: &Settings) -> std::io::Result<()> {
        let Some(path) = self.path.clone() else { return Err(std::io::Error::other("у файла нет пути")) };
        self.ed.buffer.save(&path, s.files.trim_trailing_whitespace, s.files.insert_final_newline)?;
        self.disk_mtime = mtime(&path);
        Ok(())
    }

    /// Файл изменился на диске, а у нас нет несохранённых правок — перечитать.
    pub fn reload_if_changed(&mut self, s: &Settings) -> bool {
        let Some(path) = self.path.clone() else { return false };
        let now = mtime(&path);
        if now.is_none() || now == self.disk_mtime || self.dirty() {
            return false;
        }
        match Buffer::load(&path) {
            Ok(b) => {
                let head = self.ed.sel.primary().head;
                self.ed.buffer = b;
                self.ed.cfg = edit_config(self.lang, s);
                self.ed.set_cursor(head.min(self.ed.buffer.rope.len_chars()));
                self.disk_mtime = now;
                self.hl_version = u64::MAX;
                true
            }
            Err(_) => false,
        }
    }

    /// Обновить подсветку, если текст поменялся.
    pub fn ensure_highlight(&mut self, hl: &mut Highlighter, enabled: bool) {
        let v = self.ed.buffer.version;
        if self.hl_version == v {
            return;
        }
        self.hl_version = v;
        if self.large || !enabled {
            self.spans.clear();
            self.brackets.clear();
            return;
        }
        let text = self.ed.buffer.rope.to_string();
        self.spans = hl.highlight(self.lang, &text);
        self.brackets = anvil_syntax::brackets(&text, &self.spans);
    }
}
