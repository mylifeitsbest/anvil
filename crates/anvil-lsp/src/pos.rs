//! Позиции: rope считает символы, LSP — единицы UTF-16 (или UTF-8/32 по договорённости).

use lsp_types::{Position, Range};
use ropey::Rope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enc {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

impl Enc {
    pub fn from_name(s: &str) -> Self {
        match s {
            "utf-8" => Enc::Utf8,
            "utf-32" => Enc::Utf32,
            _ => Enc::Utf16,
        }
    }

    fn width(self, c: char) -> usize {
        match self {
            Enc::Utf8 => c.len_utf8(),
            Enc::Utf16 => c.len_utf16(),
            Enc::Utf32 => 1,
        }
    }
}

fn line_len(rope: &Rope, line: usize) -> usize {
    let s = rope.line(line);
    let n = s.len_chars();
    if n > 0 && s.char(n - 1) == '\n' { n - 1 } else { n }
}

pub fn to_lsp(rope: &Rope, pos: usize, enc: Enc) -> Position {
    let pos = pos.min(rope.len_chars());
    let line = rope.char_to_line(pos);
    let ls = rope.line_to_char(line);
    let col: usize = match enc {
        Enc::Utf32 => pos - ls,
        _ => rope.slice(ls..pos).chars().map(|c| enc.width(c)).sum(),
    };
    Position::new(line as u32, col as u32)
}

pub fn from_lsp(rope: &Rope, p: Position, enc: Enc) -> usize {
    let line = p.line as usize;
    if line >= rope.len_lines() {
        return rope.len_chars();
    }
    let ls = rope.line_to_char(line);
    let len = line_len(rope, line);
    let want = p.character as usize;
    let mut units = 0;
    for (i, c) in rope.slice(ls..ls + len).chars().enumerate() {
        if units >= want {
            return ls + i;
        }
        units += enc.width(c);
    }
    ls + len
}

pub fn range_to_chars(rope: &Rope, r: Range, enc: Enc) -> (usize, usize) {
    let a = from_lsp(rope, r.start, enc);
    let b = from_lsp(rope, r.end, enc);
    (a.min(b), a.max(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_surrogates_and_cyrillic() {
        let r = Rope::from_str("aя😀b\nx");
        // 😀 — две единицы UTF-16
        assert_eq!(to_lsp(&r, 3, Enc::Utf16), Position::new(0, 4));
        assert_eq!(from_lsp(&r, Position::new(0, 4), Enc::Utf16), 3);
        assert_eq!(to_lsp(&r, 3, Enc::Utf8), Position::new(0, 7));
        assert_eq!(to_lsp(&r, 3, Enc::Utf32), Position::new(0, 3));
        assert_eq!(from_lsp(&r, Position::new(1, 99), Enc::Utf16), 6);
        assert_eq!(from_lsp(&r, Position::new(9, 0), Enc::Utf16), r.len_chars());
    }
}
