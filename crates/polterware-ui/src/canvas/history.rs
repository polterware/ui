//! Undo and redo over whole document snapshots (`history/history.ts`).
//!
//! A change commits `Immediately` (one undo step) or `Eventually` (a typed edit: the first
//! one keeps the document from before it, and the step lands when the next immediate change
//! commits). A step that changes nothing is dropped. At most `limit` steps are kept.

/// How a change enters the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Immediately,
    Eventually,
}

#[derive(Clone, Debug)]
struct Step<T> {
    before: T,
    after: T,
}

#[derive(Clone, Debug)]
pub struct History<T> {
    undo: Vec<Step<T>>,
    redo: Vec<Step<T>>,
    limit: usize,
    pending: Option<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self::new(100)
    }
}

impl<T> History<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
            pending: None,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Whether a typed edit waits to become a step.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl<T: Clone + PartialEq> History<T> {
    /// Records the change from `before` to `after`.
    pub fn commit(&mut self, before: &T, after: &T, capture: Capture) {
        match capture {
            Capture::Eventually => {
                if self.pending.is_none() {
                    self.pending = Some(before.clone());
                }
            }
            Capture::Immediately => {
                let before = self.pending.take().unwrap_or_else(|| before.clone());
                if before == *after {
                    return;
                }
                self.undo.push(Step {
                    before,
                    after: after.clone(),
                });
                if self.undo.len() > self.limit {
                    self.undo.remove(0);
                }
                self.redo.clear();
            }
        }
    }

    /// The document one step back, or `None` with nothing to undo.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        let step = self.undo.pop()?;
        self.pending = None;
        let before = step.before.clone();
        self.redo.push(Step {
            before: step.before,
            after: current.clone(),
        });
        Some(before)
    }

    /// The document one step forward, or `None` with nothing to redo.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        let step = self.redo.pop()?;
        self.pending = None;
        let after = step.after.clone();
        self.undo.push(Step {
            before: current.clone(),
            after: step.after,
        });
        Some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_edits_land_as_one_step() {
        let mut history = History::new(100);
        history.commit(&"a", &"ab", Capture::Eventually);
        history.commit(&"ab", &"abc", Capture::Eventually);
        assert!(!history.can_undo());
        history.commit(&"abc", &"abc", Capture::Immediately);
        assert_eq!(history.undo(&"abc"), Some("a"));
        assert_eq!(history.redo(&"a"), Some("abc"));
    }

    #[test]
    fn a_change_to_nothing_is_dropped_and_the_limit_holds() {
        let mut history = History::new(2);
        history.commit(&1, &1, Capture::Immediately);
        assert!(!history.can_undo());
        for n in 0..5 {
            history.commit(&n, &(n + 1), Capture::Immediately);
        }
        assert_eq!(history.undo(&5), Some(4));
        assert_eq!(history.undo(&4), Some(3));
        assert_eq!(history.undo(&3), None);
    }
}
