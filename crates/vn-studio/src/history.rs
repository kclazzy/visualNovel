//! Отмена и повтор: снимки состояния. Правки, идущие подряд (набор текста, перетаскивание),
//! склеиваются в один шаг.
const LIMIT: usize = 100;
const MERGE_SECONDS: f64 = 0.8;

pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    current: T,
    last_change: f64,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(value: T) -> Self {
        Self { undo: vec![], redo: vec![], current: value, last_change: f64::NEG_INFINITY }
    }

    /// Сообщить текущее состояние. Возвращает `true`, если оно изменилось.
    pub fn observe(&mut self, value: &T, now: f64) -> bool {
        if *value == self.current {
            return false;
        }
        let previous = std::mem::replace(&mut self.current, value.clone());
        if now - self.last_change > MERGE_SECONDS || self.undo.is_empty() {
            self.undo.push(previous);
            if self.undo.len() > LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_change = now;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Option<T> {
        let v = self.undo.pop()?;
        self.redo.push(std::mem::replace(&mut self.current, v.clone()));
        self.last_change = f64::NEG_INFINITY;
        Some(v)
    }

    pub fn redo(&mut self) -> Option<T> {
        let v = self.redo.pop()?;
        self.undo.push(std::mem::replace(&mut self.current, v.clone()));
        self.last_change = f64::NEG_INFINITY;
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_walk_through_separate_edits() {
        let mut h = History::new(1);
        assert!(!h.observe(&1, 0.0));
        assert!(h.observe(&2, 10.0));
        assert!(h.observe(&3, 20.0));
        assert_eq!(h.undo(), Some(2));
        assert_eq!(h.undo(), Some(1));
        assert_eq!(h.undo(), None);
        assert_eq!(h.redo(), Some(2));
        assert_eq!(h.redo(), Some(3));
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn quick_successive_edits_are_one_step() {
        let mut h = History::new(String::new());
        for (i, text) in ["П", "Пр", "При", "Прив"].iter().enumerate() {
            h.observe(&text.to_string(), 5.0 + i as f64 * 0.2);
        }
        assert_eq!(h.undo(), Some(String::new()));
        assert!(!h.can_undo());
    }

    #[test]
    fn new_edit_after_undo_drops_redo() {
        let mut h = History::new(1);
        h.observe(&2, 10.0);
        h.undo();
        h.observe(&5, 20.0);
        assert!(!h.can_redo());
        assert_eq!(h.undo(), Some(1));
    }
}
