//! Общий текст на CRDT (`yrs`).
//!
//! Редактор работает в символах (`ropey`), `yrs` — в байтах UTF-8. Переводит между ними
//! «зеркало» — копия текста в rope, которая всегда совпадает с содержимым CRDT. Порядок важен:
//! сначала в документ попадают свои правки ([`SharedText::apply_local`]), потом чужие
//! ([`SharedText::apply_remote`]) — тогда зеркало и буфер редактора совпадают на каждом шаге.

use std::cell::RefCell;
use std::rc::Rc;

use ropey::Rope;
use yrs::types::Delta;
use yrs::undo::UndoManager;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Any, Assoc, Doc, IndexedSequence, Observable, OffsetKind, Options, Origin, Out, ReadTxn, StateVector, StickyIndex, Text, TextRef, Transact, Update};

const LOCAL: &str = "anvil:local";
const REMOTE: &str = "anvil:remote";

/// Правка в символах: начиная с `start` удалить `remove` символов и вставить `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub remove: usize,
    pub insert: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("повреждённое обновление документа: {0}")]
    Decode(String),
    #[error("обновление не применилось: {0}")]
    Apply(String),
}

enum Op {
    Retain(u32),
    Insert(String),
    Delete(u32),
}

pub struct SharedText {
    doc: Doc,
    text: TextRef,
    mirror: Rope,
    undo: UndoManager<()>,
    /// Изменения из чужих транзакций и отмены (по транзакции на элемент), в байтах.
    inbox: Rc<RefCell<Vec<Vec<Op>>>>,
}

impl SharedText {
    /// Новый документ. `client` — уникальный номер участника; `initial` — начальный текст
    /// (у хозяина — содержимое файла, у гостя — пусто, текст придёт с синхронизацией).
    pub fn new(client: u64, initial: &str) -> Self {
        let mut opts = Options::with_client_id(yrs::ClientID::new(client & ((1 << 53) - 1)));
        opts.offset_kind = OffsetKind::Bytes;
        let doc = Doc::with_options(opts);
        let text = doc.get_or_insert_text("text");
        if !initial.is_empty() {
            let mut txn = doc.transact_mut();
            text.insert(&mut txn, 0, initial);
        }
        let inbox: Rc<RefCell<Vec<Vec<Op>>>> = Rc::default();
        let sink = inbox.clone();
        let local = Origin::from(LOCAL);
        text.observe("anvil", move |txn, e| {
            if txn.origin() == Some(&local) {
                return;
            }
            let ops = e
                .delta(txn)
                .iter()
                .filter_map(|d| match d {
                    Delta::Retain(n, _) => Some(Op::Retain(*n)),
                    Delta::Deleted(n) => Some(Op::Delete(*n)),
                    Delta::Inserted(Out::Any(Any::String(s)), _) => Some(Op::Insert(s.to_string())),
                    // в общем тексте бывают только строки
                    Delta::Inserted(..) => None,
                })
                .collect();
            sink.borrow_mut().push(ops);
        });
        let mut undo = UndoManager::with_options(yrs::undo::Options { capture_timeout_millis: 700, ..Default::default() });
        undo.expand_scope(&doc, &text);
        undo.include_origin(LOCAL);
        Self { doc, text, mirror: Rope::from_str(initial), undo, inbox }
    }

    pub fn text(&self) -> String {
        self.mirror.to_string()
    }

    pub fn len_chars(&self) -> usize {
        self.mirror.len_chars()
    }

    /// Свои правки (уже применённые в редакторе) → обновление для остальных.
    /// Правки идут по порядку, каждая — в координатах после предыдущей.
    pub fn apply_local(&mut self, edits: &[Edit]) -> Option<Vec<u8>> {
        if edits.iter().all(|e| e.remove == 0 && e.insert.is_empty()) {
            return None;
        }
        let mut txn = self.doc.transact_mut_with(LOCAL);
        for e in edits {
            let len = self.mirror.len_chars();
            let start = e.start.min(len);
            let end = (start + e.remove).min(len);
            let b0 = self.mirror.char_to_byte(start);
            if end > start {
                let b1 = self.mirror.char_to_byte(end);
                self.text.remove_range(&mut txn, b0 as u32, (b1 - b0) as u32);
                self.mirror.remove(start..end);
            }
            if !e.insert.is_empty() {
                self.text.insert(&mut txn, b0 as u32, &e.insert);
                self.mirror.insert(start, &e.insert);
            }
        }
        let update = txn.encode_update_v1();
        drop(txn);
        Some(update)
    }

    /// Чужое обновление → правки для редактора (в символах, по порядку).
    pub fn apply_remote(&mut self, update: &[u8]) -> Result<Vec<Edit>, DocError> {
        let u = Update::decode_v1(update).map_err(|e| DocError::Decode(e.to_string()))?;
        {
            let mut txn = self.doc.transact_mut_with(REMOTE);
            txn.apply_update(u).map_err(|e| DocError::Apply(e.to_string()))?;
        }
        Ok(self.drain())
    }

    /// Вектор состояния: что у нас уже есть (для запроса недостающего).
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// Всё, чего нет у участника с вектором `sv`.
    pub fn diff(&self, sv: &[u8]) -> Result<Vec<u8>, DocError> {
        let sv = StateVector::decode_v1(sv).map_err(|e| DocError::Decode(e.to_string()))?;
        Ok(self.doc.transact().encode_state_as_update_v1(&sv))
    }

    /// Отменить свою последнюю правку (чужие не трогаются).
    /// Возвращает правки для редактора и обновление для остальных.
    pub fn undo(&mut self) -> (Vec<Edit>, Option<Vec<u8>>) {
        self.step(true)
    }

    pub fn redo(&mut self) -> (Vec<Edit>, Option<Vec<u8>>) {
        self.step(false)
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    /// Следующая правка начнёт новый шаг отмены.
    pub fn seal(&mut self) {
        self.undo.reset();
    }

    fn step(&mut self, undo: bool) -> (Vec<Edit>, Option<Vec<u8>>) {
        let before = self.doc.transact().state_vector();
        let done = if undo { self.undo.undo_blocking() } else { self.undo.redo_blocking() };
        let edits = self.drain();
        if !done {
            return (edits, None);
        }
        let update = self.doc.transact().encode_state_as_update_v1(&before);
        (edits, Some(update))
    }

    /// «Липкая» позиция: остаётся у того же символа, как бы ни менялся текст вокруг.
    pub fn sticky(&self, pos: usize) -> Vec<u8> {
        let b = self.mirror.char_to_byte(pos.min(self.mirror.len_chars())) as u32;
        let txn = self.doc.transact();
        self.text.sticky_index(&txn, b, Assoc::After).map(|s| s.encode_v1()).unwrap_or_default()
    }

    /// Текущая позиция (в символах) для «липкой».
    pub fn resolve(&self, sticky: &[u8]) -> Option<usize> {
        if sticky.is_empty() {
            return None;
        }
        let s = StickyIndex::decode_v1(sticky).ok()?;
        let txn = self.doc.transact();
        let off = s.get_offset(&txn)?;
        let b = (off.index as usize).min(self.mirror.len_bytes());
        Some(self.mirror.byte_to_char(b))
    }

    /// Разобрать накопленные изменения в правки по символам, обновляя зеркало.
    fn drain(&mut self) -> Vec<Edit> {
        let batches = std::mem::take(&mut *self.inbox.borrow_mut());
        let mut out: Vec<Edit> = Vec::new();
        for ops in batches {
            let mut byte = 0usize;
            for op in ops {
                match op {
                    Op::Retain(n) => byte += n as usize,
                    Op::Insert(s) => {
                        let at = self.mirror.byte_to_char(byte.min(self.mirror.len_bytes()));
                        self.mirror.insert(at, &s);
                        byte += s.len();
                        // удаление сразу перед вставкой в том же месте — одна замена
                        match out.last_mut() {
                            Some(prev) if prev.insert.is_empty() && prev.start == at => prev.insert = s,
                            _ => out.push(Edit { start: at, remove: 0, insert: s }),
                        }
                    }
                    Op::Delete(n) => {
                        let len_b = self.mirror.len_bytes();
                        let a = self.mirror.byte_to_char(byte.min(len_b));
                        let b = self.mirror.byte_to_char((byte + n as usize).min(len_b));
                        if b > a {
                            self.mirror.remove(a..b);
                            out.push(Edit { start: a, remove: b - a, insert: String::new() });
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &mut String, edits: &[Edit]) {
        for e in edits {
            let a: usize = text.char_indices().nth(e.start).map(|x| x.0).unwrap_or(text.len());
            let b: usize = text.char_indices().nth(e.start + e.remove).map(|x| x.0).unwrap_or(text.len());
            text.replace_range(a..b, &e.insert);
        }
    }

    fn ins(start: usize, s: &str) -> Edit {
        Edit { start, remove: 0, insert: s.into() }
    }

    fn del(start: usize, n: usize) -> Edit {
        Edit { start, remove: n, insert: String::new() }
    }

    /// Хозяин и гость: начальная синхронизация, правки в обе стороны, кириллица и эмодзи.
    #[test]
    fn sync_both_ways() -> Result<(), DocError> {
        let mut host = SharedText::new(1, "fn main() {}\n");
        let mut guest = SharedText::new(2, "");
        let mut guest_buf = String::new();
        let init = host.diff(&guest.state_vector())?;
        let e = guest.apply_remote(&init)?;
        apply(&mut guest_buf, &e);
        assert_eq!(guest_buf, "fn main() {}\n");

        // гость пишет кириллицу внутрь скобок
        guest_buf.replace_range(11..11, " привет 🦀 ");
        let u = guest.apply_local(&[ins(11, " привет 🦀 ")]).ok_or(DocError::Apply("пусто".into()))?;
        let mut host_buf = host.text();
        let e = host.apply_remote(&u)?;
        apply(&mut host_buf, &e);
        assert_eq!(host_buf, "fn main() { привет 🦀 }\n");
        assert_eq!(host.text(), host_buf);

        // хозяин удаляет «привет » — по символам, не по байтам
        let u = host.apply_local(&[del(12, 7)]).ok_or(DocError::Apply("пусто".into()))?;
        let e = guest.apply_remote(&u)?;
        apply(&mut guest_buf, &e);
        assert_eq!(guest_buf, "fn main() { 🦀 }\n");
        assert_eq!(e, vec![del(12, 7)]);
        Ok(())
    }

    /// Одновременные правки в одном месте сходятся к одному тексту у обоих.
    #[test]
    fn concurrent_edits_converge() -> Result<(), DocError> {
        let mut a = SharedText::new(10, "");
        let mut b = SharedText::new(20, "");
        let base = a.apply_local(&[ins(0, "hello world")]).ok_or(DocError::Apply("пусто".into()))?;
        b.apply_remote(&base)?;
        let (mut ta, mut tb) = (a.text(), b.text());

        let ua = a.apply_local(&[ins(5, ",")]).ok_or(DocError::Apply("пусто".into()))?;
        apply(&mut ta, &[ins(5, ",")]);
        let ub1 = b.apply_local(&[del(0, 5)]).ok_or(DocError::Apply("пусто".into()))?;
        let ub2 = b.apply_local(&[ins(0, "привет")]).ok_or(DocError::Apply("пусто".into()))?;
        apply(&mut tb, &[del(0, 5), ins(0, "привет")]);

        let e = a.apply_remote(&ub1)?;
        apply(&mut ta, &e);
        let e = a.apply_remote(&ub2)?;
        apply(&mut ta, &e);
        let e = b.apply_remote(&ua)?;
        apply(&mut tb, &e);

        assert_eq!(ta, tb);
        assert_eq!(ta, a.text());
        assert_eq!(tb, b.text());
        // «hello» удалено, обе вставки на месте (порядок при одной позиции решает CRDT, у всех одинаково)
        assert!(!ta.contains("hello") && ta.contains("привет") && ta.contains(',') && ta.ends_with(" world"), "{ta}");
        Ok(())
    }

    /// Обновления, пришедшие не по порядку, применяются, когда приходит недостающее.
    #[test]
    fn out_of_order_updates() -> Result<(), DocError> {
        let mut a = SharedText::new(1, "");
        let mut b = SharedText::new(2, "");
        let u1 = a.apply_local(&[ins(0, "abc")]).ok_or(DocError::Apply("пусто".into()))?;
        let u2 = a.apply_local(&[ins(3, "def")]).ok_or(DocError::Apply("пусто".into()))?;
        let mut t = String::new();
        let e = b.apply_remote(&u2)?;
        apply(&mut t, &e);
        assert_eq!(t, "");
        let e = b.apply_remote(&u1)?;
        apply(&mut t, &e);
        assert_eq!(t, "abcdef");
        Ok(())
    }

    /// Отмена убирает только свою правку, чужая остаётся.
    #[test]
    fn undo_is_local() -> Result<(), DocError> {
        let mut a = SharedText::new(1, "x\n");
        let mut b = SharedText::new(2, "");
        b.apply_remote(&a.diff(&b.state_vector())?)?;
        let ua = a.apply_local(&[ins(0, "A")]).ok_or(DocError::Apply("пусто".into()))?;
        b.apply_remote(&ua)?;
        let ub = b.apply_local(&[ins(3, "B")]).ok_or(DocError::Apply("пусто".into()))?;
        let mut ta = a.text();
        let e = a.apply_remote(&ub)?;
        apply(&mut ta, &e);
        assert_eq!(ta, "Ax\nB");

        let (edits, update) = a.undo();
        apply(&mut ta, &edits);
        assert_eq!(ta, "x\nB");
        assert_eq!(a.text(), ta);
        let mut tb = b.text();
        let e = b.apply_remote(&update.ok_or(DocError::Apply("нет обновления".into()))?)?;
        apply(&mut tb, &e);
        assert_eq!(tb, "x\nB");

        let (edits, _) = a.redo();
        apply(&mut ta, &edits);
        assert_eq!(ta, "Ax\nB");
        Ok(())
    }

    /// Липкая позиция курсора едет вместе с текстом.
    #[test]
    fn sticky_cursor_follows_text() -> Result<(), DocError> {
        let mut a = SharedText::new(1, "hello");
        let mut b = SharedText::new(2, "");
        b.apply_remote(&a.diff(&b.state_vector())?)?;
        let s = b.sticky(4); // перед «o»
        let u = a.apply_local(&[ins(0, "ну, ")]).ok_or(DocError::Apply("пусто".into()))?;
        b.apply_remote(&u)?;
        assert_eq!(a.resolve(&s), Some(8));
        assert_eq!(b.resolve(&s), Some(8));
        assert_eq!(b.resolve(&b.sticky(b.len_chars())), Some(b.len_chars()));
        Ok(())
    }
}
