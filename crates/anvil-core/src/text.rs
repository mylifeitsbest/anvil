//! Вспомогательные функции над rope: строки, колонки, слова.

use ropey::Rope;

/// Длина строки в символах без перевода строки.
pub fn line_len(rope: &Rope, line: usize) -> usize {
    if line >= rope.len_lines() {
        return 0;
    }
    let s = rope.line(line);
    let n = s.len_chars();
    if n > 0 && s.char(n - 1) == '\n' { n - 1 } else { n }
}

/// Позиция конца строки (перед `\n`).
pub fn line_end(rope: &Rope, line: usize) -> usize {
    rope.line_to_char(line) + line_len(rope, line)
}

pub fn line_text(rope: &Rope, line: usize) -> String {
    let start = rope.line_to_char(line);
    rope.slice(start..start + line_len(rope, line)).to_string()
}

/// Ведущие пробелы и табы строки.
pub fn leading_ws(rope: &Rope, line: usize) -> String {
    let start = rope.line_to_char(line);
    let len = line_len(rope, line);
    rope.slice(start..start + len).chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Экранная колонка позиции с учётом табов.
pub fn display_col(rope: &Rope, pos: usize, tab: usize) -> usize {
    let pos = pos.min(rope.len_chars());
    let line = rope.char_to_line(pos);
    let start = rope.line_to_char(line);
    let tab = tab.max(1);
    let mut col = 0;
    for c in rope.slice(start..pos).chars() {
        col = if c == '\t' { col + tab - col % tab } else { col + 1 };
    }
    col
}

/// Позиция в строке, ближайшая к экранной колонке `col`.
pub fn pos_for_col(rope: &Rope, line: usize, col: usize, tab: usize) -> usize {
    let line = line.min(rope.len_lines().saturating_sub(1));
    let start = rope.line_to_char(line);
    let len = line_len(rope, line);
    let tab = tab.max(1);
    let mut c_col = 0;
    for (i, c) in rope.slice(start..start + len).chars().enumerate() {
        let w = if c == '\t' { tab - c_col % tab } else { 1 };
        if c_col + w > col {
            // ближе к началу или к концу символа
            return if col - c_col < w.div_ceil(2) { start + i } else { start + i + 1 };
        }
        c_col += w;
    }
    start + len
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Class {
    Space,
    Word,
    Punct,
    Newline,
}

fn class(c: char) -> Class {
    if c == '\n' {
        Class::Newline
    } else if c.is_whitespace() {
        Class::Space
    } else if is_word(c) {
        Class::Word
    } else {
        Class::Punct
    }
}

/// Следующая граница слова (Ctrl+→).
pub fn next_word(rope: &Rope, pos: usize) -> usize {
    let n = rope.len_chars();
    let mut i = pos;
    if i >= n {
        return n;
    }
    if rope.char(i) == '\n' {
        return i + 1;
    }
    while i < n && class(rope.char(i)) == Class::Space {
        i += 1;
    }
    if i < n {
        let k = class(rope.char(i));
        if k == Class::Newline {
            return i;
        }
        while i < n && class(rope.char(i)) == k {
            i += 1;
        }
    }
    i
}

/// Предыдущая граница слова (Ctrl+←).
pub fn prev_word(rope: &Rope, pos: usize) -> usize {
    let mut i = pos.min(rope.len_chars());
    if i == 0 {
        return 0;
    }
    if rope.char(i - 1) == '\n' {
        return i - 1;
    }
    while i > 0 && class(rope.char(i - 1)) == Class::Space {
        i -= 1;
    }
    if i > 0 {
        let k = class(rope.char(i - 1));
        if k == Class::Newline {
            return i;
        }
        while i > 0 && class(rope.char(i - 1)) == k {
            i -= 1;
        }
    }
    i
}

/// Слово под позицией (или пустой диапазон).
pub fn word_at(rope: &Rope, pos: usize) -> (usize, usize) {
    let n = rope.len_chars();
    let pos = pos.min(n);
    let mut a = pos;
    while a > 0 && is_word(rope.char(a - 1)) {
        a -= 1;
    }
    let mut b = pos;
    while b < n && is_word(rope.char(b)) {
        b += 1;
    }
    (a, b)
}

pub fn char_at(rope: &Rope, pos: usize) -> Option<char> {
    (pos < rope.len_chars()).then(|| rope.char(pos))
}

pub fn char_before(rope: &Rope, pos: usize) -> Option<char> {
    (pos > 0 && pos <= rope.len_chars()).then(|| rope.char(pos - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_with_tabs() {
        let r = Rope::from_str("\tab\n  x");
        assert_eq!(display_col(&r, 1, 4), 4);
        assert_eq!(display_col(&r, 3, 4), 6);
        assert_eq!(pos_for_col(&r, 0, 4, 4), 1);
        assert_eq!(pos_for_col(&r, 0, 1, 4), 0);
        assert_eq!(pos_for_col(&r, 1, 99, 4), 7);
    }

    #[test]
    fn word_jumps() {
        let r = Rope::from_str("let foo_bar = x.y;");
        assert_eq!(next_word(&r, 0), 3);
        assert_eq!(next_word(&r, 3), 11);
        assert_eq!(prev_word(&r, 11), 4);
        assert_eq!(word_at(&r, 6), (4, 11));
    }
}
