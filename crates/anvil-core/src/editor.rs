//! Операции редактирования с поддержкой мультикурсоров.
//!
//! Каждая операция строит список замен по всем выделениям, применяет их одной
//! транзакцией (один шаг undo) и вычисляет новые выделения.

use std::collections::BTreeSet;

use ropey::Rope;

use crate::buffer::Buffer;
use crate::history::{Change, EditKind, Transaction};
use crate::search::{self, SearchOptions};
use crate::selection::{Selection, Selections};
use crate::text::{char_at, char_before, display_col, leading_ws, line_end, line_len, line_text, next_word, pos_for_col, prev_word, word_at};

#[derive(Clone, Debug)]
pub struct EditConfig {
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub auto_close_brackets: bool,
    pub auto_close_quotes: bool,
    pub auto_indent: bool,
    /// Токен строчного комментария языка (`//`, `#`, `--`…).
    pub line_comment: Option<String>,
    /// После каких символов в конце строки увеличивать отступ.
    pub indent_after: Vec<char>,
}

impl Default for EditConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            auto_close_brackets: true,
            auto_close_quotes: true,
            auto_indent: true,
            line_comment: Some("//".into()),
            indent_after: vec!['{', '[', '('],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp(usize),
    PageDown(usize),
}

/// Что сделала последняя правка — интерфейс использует это для эффектов ввода.
#[derive(Clone, Debug, Default)]
pub struct EditEvent {
    pub positions: Vec<usize>,
    pub kind: Option<EditKind>,
    pub serial: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Editor {
    pub buffer: Buffer,
    pub sel: Selections,
    pub cfg: EditConfig,
    pub last: EditEvent,
}

/// Замена диапазона исходного текста; `anchor`/`head` — новое выделение относительно `start`.
struct Repl {
    start: usize,
    end: usize,
    text: String,
    anchor: usize,
    head: usize,
    primary: bool,
}

type LineEdit = (usize, usize, String);

impl Editor {
    pub fn new(buffer: Buffer, cfg: EditConfig) -> Self {
        Self { buffer, sel: Selections::single(0), cfg, last: EditEvent::default() }
    }

    pub fn from_text(text: &str) -> Self {
        Self::new(Buffer::from_text(text), EditConfig::default())
    }

    pub fn text(&self) -> String {
        self.buffer.rope.to_string()
    }

    pub fn rope(&self) -> &Rope {
        &self.buffer.rope
    }

    fn unit(&self) -> String {
        if self.cfg.insert_spaces { " ".repeat(self.cfg.tab_size.max(1)) } else { "\t".into() }
    }

    // ------------------------------------------------------------------ ядро применения правок

    fn apply_repls(&mut self, mut repls: Vec<Repl>, kind: EditKind) {
        repls.sort_by_key(|r| (r.start, r.end));
        let before = self.sel.clone();
        let mut changes = Vec::new();
        let mut list = Vec::new();
        let mut primary = None;
        let mut positions = Vec::new();
        let mut delta: isize = 0;
        let mut last_end: Option<usize> = None;
        for r in repls {
            if last_end.is_some_and(|e| r.start < e) {
                continue;
            }
            last_end = Some(r.end);
            let start = (r.start as isize + delta) as usize;
            let removed = self.buffer.rope.slice(r.start..r.end).to_string();
            let ins_len = r.text.chars().count();
            if r.primary {
                primary = Some(list.len());
            }
            list.push(Selection::new(start + r.anchor, start + r.head));
            positions.push(start + r.head);
            delta += ins_len as isize - (r.end - r.start) as isize;
            if removed != r.text {
                changes.push(Change { start, removed, inserted: r.text });
            }
        }
        let mut after = Selections { primary: primary.unwrap_or(list.len().saturating_sub(1)), list };
        after.normalize();
        if !changes.is_empty() {
            self.buffer.commit(Transaction { changes, before, after: after.clone() }, kind);
            self.last = EditEvent { positions, kind: Some(kind), serial: self.last.serial + 1 };
        }
        self.sel = after;
    }

    /// Правки в исходных координатах (по возрастанию, без пересечений) + функция новых выделений.
    fn commit_edits(&mut self, mut edits: Vec<LineEdit>, kind: EditKind, make_sel: impl FnOnce(&Rope, &[LineEdit]) -> Selections) {
        edits.sort_by_key(|e| (e.0, e.1));
        edits.dedup_by(|b, a| b.0 < a.1 || (b.0 == a.0 && b.1 == a.1 && b.2 == a.2));
        if edits.is_empty() {
            return;
        }
        let before = self.sel.clone();
        let mut changes = Vec::new();
        let mut delta: isize = 0;
        for (s, e, text) in &edits {
            let start = (*s as isize + delta) as usize;
            changes.push(Change { start, removed: self.buffer.rope.slice(*s..*e).to_string(), inserted: text.clone() });
            delta += text.chars().count() as isize - (*e - *s) as isize;
        }
        let mut rope = self.buffer.rope.clone();
        for c in &changes {
            c.apply(&mut rope);
        }
        let mut after = make_sel(&rope, &edits);
        after.clamp(rope.len_chars());
        self.buffer.commit(Transaction { changes, before, after: after.clone() }, kind);
        self.sel = after;
        self.last = EditEvent { positions: Vec::new(), kind: Some(kind), serial: self.last.serial + 1 };
    }

    /// Позиция после правок (для правок в исходных координатах).
    fn map_pos(pos: usize, edits: &[LineEdit]) -> usize {
        let mut delta: isize = 0;
        for (s, e, text) in edits {
            let ins = text.chars().count() as isize;
            if pos >= *e {
                delta += ins - (*e - *s) as isize;
            } else if pos > *s {
                return (*s as isize + delta) as usize;
            } else {
                break;
            }
        }
        (pos as isize + delta).max(0) as usize
    }

    fn mapped_selections(&self, edits: &[LineEdit]) -> Selections {
        let mut s = self.sel.clone();
        for sel in &mut s.list {
            sel.anchor = Self::map_pos(sel.anchor, edits);
            sel.head = Self::map_pos(sel.head, edits);
            sel.goal = None;
        }
        s
    }

    // ------------------------------------------------------------------ ввод

    fn pair_for(&self, c: char) -> Option<char> {
        match c {
            '(' | '[' | '{' if self.cfg.auto_close_brackets => Some(match c {
                '(' => ')',
                '[' => ']',
                _ => '}',
            }),
            '"' | '\'' | '`' if self.cfg.auto_close_quotes => Some(c),
            _ => None,
        }
    }

    fn is_closer(&self, c: char) -> bool {
        (self.cfg.auto_close_brackets && matches!(c, ')' | ']' | '}')) || (self.cfg.auto_close_quotes && matches!(c, '"' | '\'' | '`'))
    }

    /// Ввод текста во все курсоры (автоскобки, обёртывание выделения, перепечатка закрывающих).
    pub fn insert_text(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n");
        if s == "\n" {
            return self.newline();
        }
        let mut chars = s.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        let n = s.chars().count();
        let rope = &self.buffer.rope;
        let mut repls = Vec::new();
        for (i, sel) in self.sel.list.iter().enumerate() {
            let (a, b) = (sel.start(), sel.end());
            let mut r = Repl { start: a, end: b, text: s.clone(), anchor: n, head: n, primary: i == self.sel.primary };
            if let Some(c) = single {
                let next = char_at(rope, b);
                let prev = char_before(rope, a);
                if sel.is_empty() && self.is_closer(c) && next == Some(c) {
                    r.end = b + 1;
                    r.anchor = 1;
                    r.head = 1;
                } else if let Some(close) = self.pair_for(c) {
                    let quote = c == close;
                    if !sel.is_empty() {
                        let inner = rope.slice(a..b).to_string();
                        let len = inner.chars().count();
                        r.text = format!("{c}{inner}{close}");
                        r.anchor = 1;
                        r.head = 1 + len;
                    } else {
                        let next_ok = next.is_none_or(|x| x.is_whitespace() || ")]}>,;:".contains(x));
                        let prev_ok = !quote || prev.is_none_or(|p| !(crate::text::is_word(p) || p == c));
                        if next_ok && prev_ok {
                            r.text = format!("{c}{close}");
                            r.anchor = 1;
                            r.head = 1;
                        }
                    }
                }
            }
            repls.push(r);
        }
        self.apply_repls(repls, EditKind::Insert);
    }

    /// Enter с автоотступом; между парными скобками раскрывает блок.
    pub fn newline(&mut self) {
        let rope = &self.buffer.rope;
        let unit = self.unit();
        let mut repls = Vec::new();
        for (i, sel) in self.sel.list.iter().enumerate() {
            let (a, b) = (sel.start(), sel.end());
            let line = rope.char_to_line(a);
            let ls = rope.line_to_char(line);
            let mut indent = if self.cfg.auto_indent { leading_ws(rope, line) } else { String::new() };
            if indent.chars().count() > a - ls {
                indent = indent.chars().take(a - ls).collect();
            }
            let mut p = a;
            while p > ls && matches!(rope.char(p - 1), ' ' | '\t') {
                p -= 1;
            }
            let prev = char_before(rope, p).filter(|_| p > ls);
            let next = char_at(rope, b);
            let opens = self.cfg.auto_indent && prev.is_some_and(|c| self.cfg.indent_after.contains(&c));
            let between = matches!((prev, next), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')')));
            let (text, k) = if opens && between {
                let first = format!("\n{indent}{unit}");
                let k = first.chars().count();
                (format!("{first}\n{indent}"), k)
            } else if opens {
                let t = format!("\n{indent}{unit}");
                let k = t.chars().count();
                (t, k)
            } else {
                let t = format!("\n{indent}");
                let k = t.chars().count();
                (t, k)
            };
            repls.push(Repl { start: a, end: b, text, anchor: k, head: k, primary: i == self.sel.primary });
        }
        self.apply_repls(repls, EditKind::Insert);
    }

    pub fn backspace(&mut self) {
        let rope = &self.buffer.rope;
        let tab = self.cfg.tab_size.max(1);
        let mut repls = Vec::new();
        for (i, sel) in self.sel.list.iter().enumerate() {
            let primary = i == self.sel.primary;
            let (a, b) = (sel.start(), sel.end());
            let (start, end) = if !sel.is_empty() {
                (a, b)
            } else if a == 0 {
                (0, 0)
            } else {
                let prev = rope.char(a - 1);
                let next = char_at(rope, a);
                let pair = matches!((prev, next), ('(', Some(')')) | ('[', Some(']')) | ('{', Some('}'))) && self.cfg.auto_close_brackets
                    || matches!((prev, next), ('"', Some('"')) | ('\'', Some('\'')) | ('`', Some('`'))) && self.cfg.auto_close_quotes;
                if pair {
                    (a - 1, a + 1)
                } else if self.cfg.insert_spaces && prev == ' ' {
                    let ls = rope.line_to_char(rope.char_to_line(a));
                    let only_spaces = rope.slice(ls..a).chars().all(|c| c == ' ');
                    let n = if only_spaces { (a - ls - 1) % tab + 1 } else { 1 };
                    (a - n, a)
                } else {
                    (a - 1, a)
                }
            };
            repls.push(Repl { start, end, text: String::new(), anchor: 0, head: 0, primary });
        }
        self.apply_repls(repls, EditKind::Delete);
    }

    pub fn delete_forward(&mut self) {
        let n = self.buffer.rope.len_chars();
        let repls = self
            .sel
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (a, b) = if s.is_empty() { (s.head, (s.head + 1).min(n)) } else { (s.start(), s.end()) };
                Repl { start: a, end: b, text: String::new(), anchor: 0, head: 0, primary: i == self.sel.primary }
            })
            .collect();
        self.apply_repls(repls, EditKind::Delete);
    }

    pub fn delete_word(&mut self, forward: bool) {
        let rope = &self.buffer.rope;
        let repls = self
            .sel
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (a, b) = if !s.is_empty() {
                    (s.start(), s.end())
                } else if forward {
                    (s.head, next_word(rope, s.head))
                } else {
                    (prev_word(rope, s.head), s.head)
                };
                Repl { start: a, end: b, text: String::new(), anchor: 0, head: 0, primary: i == self.sel.primary }
            })
            .collect();
        self.apply_repls(repls, EditKind::Delete);
    }

    /// Tab: отступ строк, если выделено несколько строк, иначе — до следующей позиции табуляции.
    pub fn tab(&mut self) {
        let rope = &self.buffer.rope;
        let multi = self.sel.list.iter().any(|s| rope.char_to_line(s.start()) != rope.char_to_line(s.end()));
        if multi {
            return self.indent_lines(false);
        }
        let tab = self.cfg.tab_size.max(1);
        let repls = self
            .sel
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let col = display_col(rope, s.start(), tab);
                let text = if self.cfg.insert_spaces { " ".repeat(tab - col % tab) } else { "\t".into() };
                let k = text.chars().count();
                Repl { start: s.start(), end: s.end(), text, anchor: k, head: k, primary: i == self.sel.primary }
            })
            .collect();
        self.apply_repls(repls, EditKind::Insert);
    }

    /// Строки, которых касаются выделения.
    fn touched_lines(&self) -> BTreeSet<usize> {
        let rope = &self.buffer.rope;
        let mut set = BTreeSet::new();
        for s in &self.sel.list {
            let l1 = rope.char_to_line(s.start());
            let mut l2 = rope.char_to_line(s.end());
            if !s.is_empty() && l2 > l1 && rope.line_to_char(l2) == s.end() {
                l2 -= 1;
            }
            set.extend(l1..=l2);
        }
        set
    }

    pub fn indent_lines(&mut self, outdent: bool) {
        let rope = &self.buffer.rope;
        let unit = self.unit();
        let tab = self.cfg.tab_size.max(1);
        let mut edits = Vec::new();
        for l in self.touched_lines() {
            let ls = rope.line_to_char(l);
            if outdent {
                let ws = leading_ws(rope, l);
                let n = if ws.starts_with('\t') { 1 } else { ws.chars().take_while(|c| *c == ' ').count().min(tab) };
                if n > 0 {
                    edits.push((ls, ls + n, String::new()));
                }
            } else if line_len(rope, l) > 0 {
                edits.push((ls, ls, unit.clone()));
            }
        }
        let sel = self.mapped_selections(&edits);
        self.commit_edits(edits, EditKind::Other, |_, _| sel);
    }

    pub fn toggle_comment(&mut self) {
        let Some(token) = self.cfg.line_comment.clone() else { return };
        let rope = &self.buffer.rope;
        let lines: Vec<usize> = self.touched_lines().into_iter().filter(|l| !line_text(rope, *l).trim().is_empty()).collect();
        if lines.is_empty() {
            return;
        }
        let all = lines.iter().all(|l| line_text(rope, *l).trim_start().starts_with(&token));
        let mut edits = Vec::new();
        if all {
            for l in &lines {
                let t = line_text(rope, *l);
                let ws = t.chars().take_while(|c| c.is_whitespace()).count();
                let after: String = t.chars().skip(ws + token.chars().count()).take(1).collect();
                let n = token.chars().count() + usize::from(after == " ");
                let s = rope.line_to_char(*l) + ws;
                edits.push((s, s + n, String::new()));
            }
        } else {
            let min = lines.iter().map(|l| leading_ws(rope, *l).chars().count()).min().unwrap_or(0);
            for l in &lines {
                let s = rope.line_to_char(*l) + min;
                edits.push((s, s, format!("{token} ")));
            }
        }
        let sel = self.mapped_selections(&edits);
        self.commit_edits(edits, EditKind::Other, |_, _| sel);
    }

    /// Блоки строк выделений, соседние склеены.
    fn line_blocks(&self) -> Vec<(usize, usize)> {
        let rope = &self.buffer.rope;
        let mut blocks: Vec<(usize, usize)> = self
            .sel
            .list
            .iter()
            .map(|s| {
                let l1 = rope.char_to_line(s.start());
                let mut l2 = rope.char_to_line(s.end());
                if !s.is_empty() && l2 > l1 && rope.line_to_char(l2) == s.end() {
                    l2 -= 1;
                }
                (l1, l2)
            })
            .collect();
        blocks.sort();
        let mut out: Vec<(usize, usize)> = Vec::new();
        for b in blocks {
            match out.last_mut() {
                Some(last) if b.0 <= last.1 + 1 => last.1 = last.1.max(b.1),
                _ => out.push(b),
            }
        }
        out
    }

    /// (строка, колонка) для якоря и головы каждого выделения.
    fn line_cols(&self) -> Vec<(usize, usize, usize, usize)> {
        let rope = &self.buffer.rope;
        self.sel
            .list
            .iter()
            .map(|s| {
                let al = rope.char_to_line(s.anchor);
                let hl = rope.char_to_line(s.head);
                (al, s.anchor - rope.line_to_char(al), hl, s.head - rope.line_to_char(hl))
            })
            .collect()
    }

    /// Alt+↑ / Alt+↓.
    pub fn move_lines(&mut self, up: bool) {
        let rope = &self.buffer.rope;
        let blocks = self.line_blocks();
        let nl = rope.len_lines();
        let (Some(first), Some(last)) = (blocks.first(), blocks.last()) else { return };
        if (up && first.0 == 0) || (!up && last.1 + 1 >= nl) {
            return;
        }
        let join = |a: usize, b: usize| (a..=b).map(|l| line_text(rope, l)).collect::<Vec<_>>().join("\n");
        let mut edits = Vec::new();
        for &(a, b) in &blocks {
            if up {
                edits.push((rope.line_to_char(a - 1), line_end(rope, b), format!("{}\n{}", join(a, b), line_text(rope, a - 1))));
            } else {
                edits.push((rope.line_to_char(a), line_end(rope, b + 1), format!("{}\n{}", line_text(rope, b + 1), join(a, b))));
            }
        }
        let lc = self.line_cols();
        let d: isize = if up { -1 } else { 1 };
        let sel = self.sel_from_line_cols_after(lc, move |_| d);
        self.commit_edits(edits, EditKind::Other, sel);
    }

    fn sel_from_line_cols_after(
        &self,
        lc: Vec<(usize, usize, usize, usize)>,
        shift: impl Fn(usize) -> isize + 'static,
    ) -> impl FnOnce(&Rope, &[LineEdit]) -> Selections + 'static {
        let primary = self.sel.primary;
        move |rope: &Rope, _: &[LineEdit]| {
            let at = |l: usize, c: usize| {
                let l = ((l as isize + shift(l)).max(0) as usize).min(rope.len_lines().saturating_sub(1));
                rope.line_to_char(l) + c.min(line_len(rope, l))
            };
            Selections { list: lc.iter().map(|(al, ac, hl, hc)| Selection::new(at(*al, *ac), at(*hl, *hc))).collect(), primary }
        }
    }

    /// Shift+Alt+↓ / Shift+Alt+↑: копия строк ниже или выше.
    pub fn duplicate_lines(&mut self, down: bool) {
        let rope = &self.buffer.rope;
        let blocks = self.line_blocks();
        let mut edits = Vec::new();
        for &(a, b) in &blocks {
            let text = (a..=b).map(|l| line_text(rope, l)).collect::<Vec<_>>().join("\n");
            let e = line_end(rope, b);
            edits.push((e, e, format!("\n{text}")));
        }
        let lc = self.line_cols();
        let shift = move |l: usize| -> isize {
            let mut s = 0isize;
            for &(a, b) in &blocks {
                let size = (b - a + 1) as isize;
                if l > b || (l >= a && down) {
                    s += size;
                }
            }
            s
        };
        let sel = self.sel_from_line_cols_after(lc, shift);
        self.commit_edits(edits, EditKind::Other, sel);
    }

    /// Ctrl+Shift+K.
    pub fn delete_lines(&mut self) {
        let rope = &self.buffer.rope;
        let blocks = self.line_blocks();
        let nl = rope.len_lines();
        let mut edits = Vec::new();
        for &(a, b) in &blocks {
            let (mut s, e) = (rope.line_to_char(a), if b + 1 < nl { rope.line_to_char(b + 1) } else { rope.len_chars() });
            if b + 1 >= nl && a > 0 {
                s = line_end(rope, a - 1);
            }
            edits.push((s, e, String::new()));
        }
        let cols: Vec<(usize, usize)> = self.sel.list.iter().map(|s| (rope.char_to_line(s.head), display_col(rope, s.head, self.cfg.tab_size))).collect();
        let tab = self.cfg.tab_size;
        let primary = self.sel.primary;
        let blocks2 = blocks.clone();
        self.commit_edits(edits, EditKind::Delete, move |rope, _| {
            let list = cols
                .iter()
                .map(|(l, c)| {
                    let removed_before: usize = blocks2.iter().filter(|(_, b)| b < l).map(|(a, b)| b - a + 1).sum();
                    let block_start = blocks2.iter().find(|(a, b)| l >= a && l <= b).map(|(a, _)| *a);
                    let line = block_start.unwrap_or(*l).saturating_sub(removed_before).min(rope.len_lines().saturating_sub(1));
                    Selection::cursor(pos_for_col(rope, line, *c, tab))
                })
                .collect();
            Selections { list, primary }
        });
    }

    // ------------------------------------------------------------------ перемещение и выделение

    pub fn move_cursors(&mut self, m: Move, extend: bool) {
        let rope = &self.buffer.rope;
        let n = rope.len_chars();
        let tab = self.cfg.tab_size;
        let nl = rope.len_lines();
        for s in &mut self.sel.list {
            let mut head = s.head;
            let mut goal = None;
            let line = rope.char_to_line(head);
            match m {
                Move::Left => head = if !extend && !s.is_empty() { s.start() } else { head.saturating_sub(1) },
                Move::Right => head = if !extend && !s.is_empty() { s.end() } else { (head + 1).min(n) },
                Move::Up | Move::Down | Move::PageUp(_) | Move::PageDown(_) => {
                    let (up, k) = match m {
                        Move::Up => (true, 1),
                        Move::Down => (false, 1),
                        Move::PageUp(k) => (true, k.max(1)),
                        Move::PageDown(k) => (false, k.max(1)),
                        _ => (false, 1),
                    };
                    let g = s.goal.unwrap_or_else(|| display_col(rope, head, tab));
                    head = match (up, line.checked_sub(k), line + k) {
                        (true, Some(t), _) => pos_for_col(rope, t, g, tab),
                        (true, None, _) => 0,
                        (false, _, t) if t < nl => pos_for_col(rope, t, g, tab),
                        (false, _, _) => n,
                    };
                    goal = Some(g);
                }
                Move::WordLeft => head = prev_word(rope, head),
                Move::WordRight => head = next_word(rope, head),
                Move::Home => {
                    let ls = rope.line_to_char(line);
                    let first = ls + leading_ws(rope, line).chars().count();
                    head = if head == first { ls } else { first };
                }
                Move::End => head = line_end(rope, line),
                Move::DocStart => head = 0,
                Move::DocEnd => head = n,
            }
            s.head = head;
            if !extend {
                s.anchor = head;
            }
            s.goal = goal;
        }
        self.sel.normalize();
        self.buffer.history.seal();
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.sel = Selections::single(pos.min(self.buffer.rope.len_chars()));
        self.buffer.history.seal();
    }

    /// Протянуть основное выделение до позиции (Shift+клик, перетаскивание мышью).
    pub fn extend_to(&mut self, pos: usize) {
        let pos = pos.min(self.buffer.rope.len_chars());
        let p = self.sel.primary;
        if let Some(s) = self.sel.list.get_mut(p) {
            s.head = pos;
            s.goal = None;
        }
        self.sel.normalize();
    }

    /// Alt+клик: добавить курсор (или убрать, если он уже там).
    pub fn toggle_cursor(&mut self, pos: usize) {
        let pos = pos.min(self.buffer.rope.len_chars());
        if self.sel.len() > 1
            && let Some(i) = self.sel.list.iter().position(|s| s.is_empty() && s.head == pos)
        {
            self.sel.list.remove(i);
            self.sel.primary = self.sel.list.len() - 1;
            return;
        }
        self.sel.push(Selection::cursor(pos));
    }

    /// Ctrl+Alt+↑/↓: курсор строкой выше/ниже крайнего.
    pub fn add_cursor_vertical(&mut self, up: bool) {
        let rope = &self.buffer.rope;
        let tab = self.cfg.tab_size;
        let edge = if up { self.sel.list.first() } else { self.sel.list.last() };
        let Some(edge) = edge.copied() else { return };
        let line = rope.char_to_line(edge.head);
        let g = edge.goal.unwrap_or_else(|| display_col(rope, edge.head, tab));
        let target = if up { line.checked_sub(1) } else { (line + 1 < rope.len_lines()).then_some(line + 1) };
        if let Some(t) = target {
            let mut s = Selection::cursor(pos_for_col(rope, t, g, tab));
            s.goal = Some(g);
            self.sel.push(s);
        }
    }

    pub fn select_all(&mut self) {
        self.sel = Selections::from_one(Selection::new(0, self.buffer.rope.len_chars()));
    }

    pub fn select_word_at(&mut self, pos: usize) {
        let (a, b) = word_at(&self.buffer.rope, pos);
        self.sel = Selections::from_one(Selection::new(a, b));
    }

    /// Ctrl+L: выделить строку целиком (повторно — следующую).
    pub fn select_line(&mut self) {
        let rope = &self.buffer.rope;
        let n = rope.len_chars();
        let nl = rope.len_lines();
        for s in &mut self.sel.list {
            let l1 = rope.char_to_line(s.start());
            let l2 = rope.char_to_line(s.end());
            let a = rope.line_to_char(l1);
            let b = if l2 + 1 < nl { rope.line_to_char(l2 + 1) } else { n };
            *s = Selection::new(a, b);
        }
        self.sel.normalize();
    }

    /// Ctrl+D: выделить слово под курсором, затем следующее такое же вхождение.
    pub fn select_next_occurrence(&mut self) -> bool {
        let rope = &self.buffer.rope;
        let p = self.sel.primary();
        if p.is_empty() {
            let (a, b) = word_at(rope, p.head);
            if a == b {
                return false;
            }
            let i = self.sel.primary;
            self.sel.list[i] = Selection::new(a, b);
            self.sel.normalize();
            return true;
        }
        let needle = rope.slice(p.start()..p.end()).to_string();
        let opts = SearchOptions { case_sensitive: true, ..SearchOptions::default() };
        let Ok(found) = search::find_all(rope, &needle, opts, 1_000_000) else { return false };
        let from = self.sel.list.iter().map(|s| s.end()).max().unwrap_or(0);
        let taken = |r: &(usize, usize)| self.sel.list.iter().any(|s| s.start() == r.0 && s.end() == r.1);
        let next = found.iter().find(|r| r.0 >= from && !taken(r)).or_else(|| found.iter().find(|r| !taken(r)));
        match next {
            Some(&(a, b)) => {
                self.sel.push(Selection::new(a, b));
                true
            }
            None => false,
        }
    }

    /// Ctrl+Shift+L: все вхождения.
    pub fn select_all_occurrences(&mut self) {
        let rope = &self.buffer.rope;
        let p = self.sel.primary();
        let (a, b) = if p.is_empty() { word_at(rope, p.head) } else { (p.start(), p.end()) };
        if a == b {
            return;
        }
        let needle = rope.slice(a..b).to_string();
        let opts = SearchOptions { case_sensitive: true, ..SearchOptions::default() };
        if let Ok(found) = search::find_all(rope, &needle, opts, 100_000)
            && !found.is_empty()
        {
            let primary = found.iter().position(|r| r.0 == a).unwrap_or(0);
            self.sel = Selections { list: found.into_iter().map(|(s, e)| Selection::new(s, e)).collect(), primary };
            self.sel.normalize();
        }
    }

    /// Esc: убрать лишние курсоры, затем выделение.
    pub fn collapse(&mut self) {
        if self.sel.len() > 1 {
            self.sel.collapse_to_primary();
        } else if let Some(s) = self.sel.list.first_mut() {
            s.anchor = s.head;
        }
    }

    // ------------------------------------------------------------------ буфер обмена

    /// Без выделения копируется строка целиком (как в VS Code).
    pub fn copy(&self) -> String {
        let rope = &self.buffer.rope;
        if self.sel.list.iter().all(Selection::is_empty) {
            let lines: BTreeSet<usize> = self.sel.list.iter().map(|s| rope.char_to_line(s.head)).collect();
            lines.into_iter().map(|l| format!("{}\n", line_text(rope, l))).collect()
        } else {
            self.sel.list.iter().filter(|s| !s.is_empty()).map(|s| rope.slice(s.start()..s.end()).to_string()).collect::<Vec<_>>().join("\n")
        }
    }

    pub fn cut(&mut self) -> String {
        let text = self.copy();
        if self.sel.list.iter().all(Selection::is_empty) {
            self.delete_lines();
        } else {
            self.backspace();
        }
        text
    }

    /// Вставка; если строк столько же, сколько курсоров, — по строке в каждый.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n");
        let mut lines: Vec<&str> = text.split('\n').collect();
        if text.ends_with('\n') && lines.len() > 1 {
            lines.pop();
        }
        let distribute = self.sel.len() > 1 && lines.len() == self.sel.len();
        let repls = self
            .sel
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = if distribute { lines[i].to_string() } else { text.clone() };
                let k = t.chars().count();
                Repl { start: s.start(), end: s.end(), text: t, anchor: k, head: k, primary: i == self.sel.primary }
            })
            .collect();
        self.apply_repls(repls, EditKind::Other);
    }

    /// Заменить набор диапазонов (например, «заменить всё»). Курсор остаётся у основного выделения.
    pub fn replace_ranges(&mut self, ranges: Vec<(usize, usize, String)>) {
        let primary = Self::map_pos(self.sel.primary().head, &ranges);
        self.commit_edits(ranges, EditKind::Other, move |_, _| Selections::single(primary));
    }

    pub fn undo(&mut self) -> bool {
        match self.buffer.undo() {
            Some(s) => {
                self.sel = s;
                self.sel.clamp(self.buffer.rope.len_chars());
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.buffer.redo() {
            Some(s) => {
                self.sel = s;
                self.sel.clamp(self.buffer.rope.len_chars());
                true
            }
            None => false,
        }
    }

    /// Строка и колонка основного курсора (с нуля).
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let rope = &self.buffer.rope;
        let h = self.sel.primary().head.min(rope.len_chars());
        let l = rope.char_to_line(h);
        (l, display_col(rope, h, self.cfg.tab_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::from_text(text);
        e.set_cursor(cursor);
        e
    }

    fn type_str(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.insert_text(&c.to_string());
        }
    }

    #[test]
    fn autopairs_and_overtype() {
        let mut e = ed("", 0);
        type_str(&mut e, "fn main(");
        assert_eq!(e.text(), "fn main()");
        assert_eq!(e.sel.primary().head, 8);
        type_str(&mut e, ")");
        assert_eq!(e.text(), "fn main()");
        assert_eq!(e.sel.primary().head, 9);
    }

    #[test]
    fn quotes_do_not_pair_after_letters() {
        let mut e = ed("", 0);
        type_str(&mut e, "don't");
        assert_eq!(e.text(), "don't");
    }

    #[test]
    fn enter_between_braces_expands_block() {
        let mut e = ed("    fn x() {}", 12);
        e.newline();
        assert_eq!(e.text(), "    fn x() {\n        \n    }");
        assert_eq!(e.sel.primary().head, 21);
    }

    #[test]
    fn backspace_removes_pair_and_indent_unit() {
        let mut e = ed("()", 1);
        e.backspace();
        assert_eq!(e.text(), "");
        let mut e = ed("        x", 8);
        e.backspace();
        assert_eq!(e.text(), "    x");
    }

    #[test]
    fn multicursor_typing() {
        let mut e = ed("a\nb\nc", 0);
        e.add_cursor_vertical(false);
        e.add_cursor_vertical(false);
        assert_eq!(e.sel.len(), 3);
        type_str(&mut e, "> ");
        assert_eq!(e.text(), "> a\n> b\n> c");
    }

    #[test]
    fn ctrl_d_renames_all() {
        let mut e = ed("foo bar foo baz foo", 1);
        assert!(e.select_next_occurrence());
        assert!(e.select_next_occurrence());
        assert!(e.select_next_occurrence());
        assert_eq!(e.sel.len(), 3);
        type_str(&mut e, "x");
        assert_eq!(e.text(), "x bar x baz x");
        assert!(e.undo());
        assert_eq!(e.text(), "foo bar foo baz foo");
        assert_eq!(e.sel.len(), 3);
    }

    #[test]
    fn undo_tree_keeps_branches() {
        let mut e = ed("", 0);
        type_str(&mut e, "a");
        e.buffer.history.seal();
        type_str(&mut e, "b");
        assert!(e.undo());
        assert_eq!(e.text(), "a");
        e.buffer.history.seal();
        type_str(&mut e, "c");
        assert_eq!(e.text(), "ac");
        assert_eq!(e.buffer.history.branch_count(), 2);
        assert!(e.undo());
        assert!(e.redo());
        assert_eq!(e.text(), "ac");
        assert!(e.undo());
        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(!e.undo());
    }

    #[test]
    fn typing_is_grouped() {
        let mut e = ed("", 0);
        type_str(&mut e, "hello");
        assert!(e.undo());
        assert_eq!(e.text(), "");
    }

    #[test]
    fn move_and_duplicate_lines() {
        let mut e = ed("one\ntwo\nthree", 5);
        e.move_lines(true);
        assert_eq!(e.text(), "two\none\nthree");
        assert_eq!(e.sel.primary().head, 1);
        e.move_lines(false);
        assert_eq!(e.text(), "one\ntwo\nthree");
        e.duplicate_lines(true);
        assert_eq!(e.text(), "one\ntwo\ntwo\nthree");
        assert_eq!(e.cursor_line_col().0, 2);
    }

    #[test]
    fn delete_lines_keeps_column() {
        let mut e = ed("abc\ndef\nghi", 5);
        e.delete_lines();
        assert_eq!(e.text(), "abc\nghi");
        assert_eq!(e.sel.primary().head, 5);
        let mut e = ed("abc\ndef", 5);
        e.delete_lines();
        assert_eq!(e.text(), "abc");
    }

    #[test]
    fn toggle_comment_roundtrip() {
        let mut e = ed("    a\n    b", 0);
        e.select_all();
        e.toggle_comment();
        assert_eq!(e.text(), "    // a\n    // b");
        e.toggle_comment();
        assert_eq!(e.text(), "    a\n    b");
    }

    #[test]
    fn indent_and_outdent_selection() {
        let mut e = ed("a\nb", 0);
        e.select_all();
        e.tab();
        assert_eq!(e.text(), "    a\n    b");
        e.indent_lines(true);
        assert_eq!(e.text(), "a\nb");
    }

    #[test]
    fn paste_distributes_lines() {
        let mut e = ed("x\ny", 0);
        e.add_cursor_vertical(false);
        e.paste("1\n2");
        assert_eq!(e.text(), "1x\n2y");
    }

    #[test]
    fn copy_without_selection_takes_line() {
        let e = ed("abc\ndef", 5);
        assert_eq!(e.copy(), "def\n");
    }

    #[test]
    fn word_and_vertical_motion() {
        let mut e = ed("hello world\nhi", 0);
        e.move_cursors(Move::WordRight, false);
        assert_eq!(e.sel.primary().head, 5);
        e.move_cursors(Move::End, false);
        e.move_cursors(Move::Down, false);
        assert_eq!(e.sel.primary().head, 14);
        e.move_cursors(Move::Up, false);
        assert_eq!(e.sel.primary().head, 11);
    }

    #[test]
    fn replace_all_via_ranges() {
        let mut e = ed("a-a-a", 0);
        let r = search::replacements(e.rope(), "a", "bb", SearchOptions::default()).unwrap_or_default();
        e.replace_ranges(r);
        assert_eq!(e.text(), "bb-bb-bb");
    }
}
