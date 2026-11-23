//! Правки и дерево истории.
//!
//! Undo не теряет ветки: если откатиться и написать что-то другое, старая ветка
//! остаётся в дереве (redo идёт в самую свежую ветку). Подряд идущий ввод
//! склеивается в один шаг, как в обычных редакторах.

use std::time::{Duration, Instant};

use ropey::Rope;

use crate::selection::Selections;

/// Одна замена: начиная с `start` (в символах, в состоянии документа до этой замены)
/// удалить `removed` и вставить `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

impl Change {
    pub fn apply(&self, rope: &mut Rope) {
        let end = self.start + self.removed.chars().count();
        if end > self.start {
            rope.remove(self.start..end);
        }
        if !self.inserted.is_empty() {
            rope.insert(self.start, &self.inserted);
        }
    }

    pub fn inverse(&self) -> Change {
        Change { start: self.start, removed: self.inserted.clone(), inserted: self.removed.clone() }
    }
}

/// Набор замен, применяемых по порядку, и выделения до/после.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub changes: Vec<Change>,
    pub before: Selections,
    pub after: Selections,
}

impl Transaction {
    pub fn apply(&self, rope: &mut Rope) {
        for c in &self.changes {
            c.apply(rope);
        }
    }

    pub fn revert(&self, rope: &mut Rope) {
        for c in self.changes.iter().rev() {
            c.inverse().apply(rope);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.iter().all(|c| c.removed.is_empty() && c.inserted.is_empty())
    }
}

/// Вид правки — для склейки шагов истории.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

#[derive(Debug, Clone)]
struct Node {
    parent: Option<usize>,
    /// Последний элемент — самая свежая ветка.
    children: Vec<usize>,
    tx: Option<Transaction>,
    kind: EditKind,
    time: Instant,
}

#[derive(Debug, Clone)]
pub struct History {
    nodes: Vec<Node>,
    current: usize,
    /// Окно склейки подряд идущего ввода.
    pub group_window: Duration,
}

impl Default for History {
    fn default() -> Self {
        Self {
            nodes: vec![Node { parent: None, children: Vec::new(), tx: None, kind: EditKind::Other, time: Instant::now() }],
            current: 0,
            group_window: Duration::from_millis(900),
        }
    }
}

impl History {
    /// Записать уже применённую транзакцию.
    pub fn record(&mut self, tx: Transaction, kind: EditKind, now: Instant) {
        let cur = self.current;
        let node = &mut self.nodes[cur];
        let mergeable = cur != 0
            && node.children.is_empty()
            && node.kind == kind
            && kind != EditKind::Other
            && now.duration_since(node.time) < self.group_window
            && !tx.changes.iter().any(|c| c.inserted.contains('\n'));
        if mergeable && let Some(prev) = node.tx.as_mut() {
            prev.changes.extend(tx.changes);
            prev.after = tx.after;
            node.time = now;
            return;
        }
        let id = self.nodes.len();
        self.nodes.push(Node { parent: Some(cur), children: Vec::new(), tx: Some(tx), kind, time: now });
        self.nodes[cur].children.push(id);
        self.current = id;
    }

    /// Не склеивать следующую правку с предыдущей (например, после перемещения курсора).
    pub fn seal(&mut self) {
        let cur = self.current;
        self.nodes[cur].kind = EditKind::Other;
    }

    pub fn can_undo(&self) -> bool {
        self.current != 0
    }

    pub fn can_redo(&self) -> bool {
        !self.nodes[self.current].children.is_empty()
    }

    /// Откатывает текущий шаг. Возвращает выделения, которые были до него.
    pub fn undo(&mut self, rope: &mut Rope) -> Option<Selections> {
        let node = &self.nodes[self.current];
        let tx = node.tx.as_ref()?;
        tx.revert(rope);
        let sel = tx.before.clone();
        self.current = node.parent?;
        self.seal();
        Some(sel)
    }

    /// Повторяет самую свежую ветку.
    pub fn redo(&mut self, rope: &mut Rope) -> Option<Selections> {
        let child = *self.nodes[self.current].children.last()?;
        let tx = self.nodes[child].tx.as_ref()?;
        tx.apply(rope);
        let sel = tx.after.clone();
        self.current = child;
        self.seal();
        Some(sel)
    }

    /// Сколько всего веток (листьев) в дереве — для статистики и тестов.
    pub fn branch_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.children.is_empty()).count()
    }

    /// Идентификатор текущего узла (используется, чтобы понять, сохранён ли документ).
    pub fn position(&self) -> usize {
        self.current
    }
}
