//! Текстовый буфер: rope + история + сведения о файле.

use std::io::Read;
use std::path::Path;
use std::time::Instant;

use ropey::Rope;

use crate::history::{Change, EditKind, History, Transaction};
use crate::selection::Selections;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub rope: Rope,
    pub history: History,
    /// Растёт на каждой правке — по нему кэшируется подсветка.
    pub version: u64,
    pub line_ending: LineEnding,
    pub bom: bool,
    /// Файл был не в UTF-8 и прочитан с заменой символов.
    pub lossy: bool,
    /// Журнал своих правок для совместной работы (`None` — не ведётся и ничего не стоит).
    pub journal: Option<Vec<Change>>,
    saved_at: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::from_text("")
    }
}

impl Buffer {
    /// Создаёт буфер; `\r\n` нормализуется в `\n` (и возвращается при сохранении).
    pub fn from_text(text: &str) -> Self {
        let (text, bom) = match text.strip_prefix('\u{feff}') {
            Some(t) => (t, true),
            None => (text, false),
        };
        let crlf = text.contains("\r\n");
        let rope = if text.contains('\r') { Rope::from_str(&text.replace("\r\n", "\n").replace('\r', "\n")) } else { Rope::from_str(text) };
        Self {
            rope,
            history: History::default(),
            version: 0,
            line_ending: if crlf { LineEnding::CrLf } else { LineEnding::Lf },
            bom,
            lossy: false,
            journal: None,
            saved_at: 0,
        }
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut bytes)?;
        Ok(match String::from_utf8(bytes) {
            Ok(s) => Self::from_text(&s),
            Err(e) => {
                let mut b = Self::from_text(&String::from_utf8_lossy(e.as_bytes()));
                b.lossy = true;
                b
            }
        })
    }

    /// Текст для записи на диск.
    pub fn text_for_save(&self, trim_trailing: bool, final_newline: bool) -> String {
        let mut s = String::with_capacity(self.rope.len_bytes() + 16);
        if self.bom {
            s.push('\u{feff}');
        }
        let nl = match self.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        };
        let n = self.rope.len_lines();
        for (i, line) in self.rope.lines().enumerate() {
            let mut l: String = line.chars().filter(|c| *c != '\n').collect();
            if trim_trailing {
                l.truncate(l.trim_end_matches([' ', '\t']).len());
            }
            s.push_str(&l);
            if i + 1 < n {
                s.push_str(nl);
            }
        }
        if final_newline && self.rope.len_chars() > 0 && self.rope.char(self.rope.len_chars() - 1) != '\n' {
            s.push_str(nl);
        }
        s
    }

    pub fn save(&mut self, path: &Path, trim_trailing: bool, final_newline: bool) -> std::io::Result<()> {
        std::fs::write(path, self.text_for_save(trim_trailing, final_newline))?;
        self.mark_saved();
        Ok(())
    }

    pub fn mark_saved(&mut self) {
        self.saved_at = self.history.position();
    }

    pub fn is_dirty(&self) -> bool {
        self.history.position() != self.saved_at
    }

    /// Применить транзакцию и записать её в историю.
    pub fn commit(&mut self, tx: Transaction, kind: EditKind) {
        if tx.is_empty() {
            return;
        }
        tx.apply(&mut self.rope);
        if let Some(j) = self.journal.as_mut() {
            j.extend(tx.changes.iter().cloned());
        }
        self.history.record(tx, kind, Instant::now());
        self.version += 1;
    }

    /// Применить правки со стороны (другого участника): в историю попадают, в журнал — нет.
    pub fn apply_external(&mut self, tx: Transaction) {
        if tx.is_empty() {
            return;
        }
        tx.apply(&mut self.rope);
        self.history.record(tx, EditKind::Other, Instant::now());
        self.history.seal();
        self.version += 1;
    }

    pub fn undo(&mut self) -> Option<Selections> {
        let changes = if self.journal.is_some() { self.history.undo_changes() } else { None };
        let s = self.history.undo(&mut self.rope)?;
        if let (Some(j), Some(c)) = (self.journal.as_mut(), changes) {
            j.extend(c);
        }
        self.version += 1;
        Some(s)
    }

    pub fn redo(&mut self) -> Option<Selections> {
        let changes = if self.journal.is_some() { self.history.redo_changes() } else { None };
        let s = self.history.redo(&mut self.rope)?;
        if let (Some(j), Some(c)) = (self.journal.as_mut(), changes) {
            j.extend(c);
        }
        self.version += 1;
        Some(s)
    }

    /// Забрать накопленный журнал своих правок.
    pub fn take_journal(&mut self) -> Vec<Change> {
        self.journal.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_roundtrip_and_bom() {
        let b = Buffer::from_text("\u{feff}a\r\nb\r\n");
        assert_eq!(b.rope.to_string(), "a\nb\n");
        assert_eq!(b.line_ending, LineEnding::CrLf);
        assert_eq!(b.text_for_save(false, true), "\u{feff}a\r\nb\r\n");
    }

    #[test]
    fn trims_and_adds_final_newline() {
        let b = Buffer::from_text("x  \ny\t");
        assert_eq!(b.text_for_save(true, true), "x\ny\n");
        assert_eq!(b.text_for_save(false, false), "x  \ny\t");
    }
}
