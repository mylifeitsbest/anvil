//! Выделения и мультикурсоры. Позиции — индексы символов в rope.

/// Одно выделение: `anchor` — где начали, `head` — где курсор.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
    /// Желаемая колонка при движении вверх/вниз.
    pub goal: Option<usize>,
}

impl Selection {
    pub fn cursor(pos: usize) -> Self {
        Self { anchor: pos, head: pos, goal: None }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head, goal: None }
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn len(&self) -> usize {
        self.end() - self.start()
    }
}

/// Список выделений, отсортированный и без пересечений.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selections {
    pub list: Vec<Selection>,
    pub primary: usize,
}

impl Default for Selections {
    fn default() -> Self {
        Self::single(0)
    }
}

impl Selections {
    pub fn single(pos: usize) -> Self {
        Self { list: vec![Selection::cursor(pos)], primary: 0 }
    }

    pub fn from_one(sel: Selection) -> Self {
        Self { list: vec![sel], primary: 0 }
    }

    pub fn primary(&self) -> Selection {
        self.list.get(self.primary).copied().unwrap_or(Selection::cursor(0))
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn push(&mut self, sel: Selection) {
        self.list.push(sel);
        self.primary = self.list.len() - 1;
        self.normalize();
    }

    /// Сортирует и сливает пересекающиеся выделения, сохраняя основное.
    pub fn normalize(&mut self) {
        if self.list.is_empty() {
            self.list.push(Selection::cursor(0));
            self.primary = 0;
            return;
        }
        let primary = self.primary();
        let mut v = std::mem::take(&mut self.list);
        v.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Selection> = Vec::with_capacity(v.len());
        for s in v {
            if let Some(last) = out.last_mut() {
                let overlaps = s.start() < last.end() || (s.start() == last.start() && s.end() == last.end());
                if overlaps {
                    let start = last.start().min(s.start());
                    let end = last.end().max(s.end());
                    let forward = last.head >= last.anchor;
                    *last = if forward { Selection::new(start, end) } else { Selection::new(end, start) };
                    continue;
                }
            }
            out.push(s);
        }
        self.primary = out.iter().position(|s| s.start() <= primary.start() && primary.end() <= s.end()).unwrap_or(out.len() - 1);
        self.list = out;
    }

    /// Оставить только основное выделение (Esc).
    pub fn collapse_to_primary(&mut self) {
        let p = self.primary();
        self.list = vec![p];
        self.primary = 0;
    }

    /// Сдвинуть все позиции, чтобы они не выходили за `max`.
    pub fn clamp(&mut self, max: usize) {
        for s in &mut self.list {
            s.anchor = s.anchor.min(max);
            s.head = s.head.min(max);
        }
        self.normalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_overlaps_and_keeps_primary() {
        let mut s = Selections { list: vec![Selection::new(10, 15), Selection::new(0, 3), Selection::new(12, 20)], primary: 2 };
        s.normalize();
        assert_eq!(s.list, vec![Selection::new(0, 3), Selection::new(10, 20)]);
        assert_eq!(s.primary, 1);
    }

    #[test]
    fn identical_cursors_merge() {
        let mut s = Selections { list: vec![Selection::cursor(5), Selection::cursor(5), Selection::cursor(1)], primary: 0 };
        s.normalize();
        assert_eq!(s.list, vec![Selection::cursor(1), Selection::cursor(5)]);
    }
}
