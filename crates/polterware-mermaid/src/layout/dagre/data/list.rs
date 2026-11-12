//! Port of `dagre-d3-es/src/dagre/data/list.js` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! A doubly linked FIFO list. Entries are handles (upstream's linked objects), so
//! re-enqueueing one, even on another list, first unlinks it from wherever it is.
//! The linked lists form `Rc` cycles while a list lives; [`List`]'s drop breaks them.

use std::cell::RefCell;
use std::rc::Rc;

/// One entry of a [`List`]. Create it with [`ListNode::new`] and hand the handle to
/// [`List::enqueue`].
#[derive(Debug)]
pub struct ListNode<T> {
    value: Option<T>,
    next: RefCell<Option<Rc<ListNode<T>>>>,
    prev: RefCell<Option<Rc<ListNode<T>>>>,
}

impl<T> ListNode<T> {
    /// A detached entry holding `value`.
    pub fn new(value: T) -> Rc<Self> {
        Rc::new(Self {
            value: Some(value),
            next: RefCell::new(None),
            prev: RefCell::new(None),
        })
    }

    /// The value, or `None` for a list's sentinel.
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

/// A FIFO list of entry handles.
#[derive(Debug)]
pub struct List<T> {
    sentinel: Rc<ListNode<T>>,
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> List<T> {
    /// An empty list.
    pub fn new() -> Self {
        let sentinel = Rc::new(ListNode {
            value: None,
            next: RefCell::new(None),
            prev: RefCell::new(None),
        });
        *sentinel.next.borrow_mut() = Some(sentinel.clone());
        *sentinel.prev.borrow_mut() = Some(sentinel.clone());
        Self { sentinel }
    }

    /// Removes and returns the first entry, upstream's `dequeue`.
    pub fn dequeue(&self) -> Option<Rc<ListNode<T>>> {
        let entry = self.sentinel.prev.borrow().clone()?;
        if Rc::ptr_eq(&entry, &self.sentinel) {
            return None;
        }
        unlink(&entry);
        Some(entry)
    }

    /// Adds `entry` at the back of the FIFO, unlinking it from any list first.
    pub fn enqueue(&self, entry: Rc<ListNode<T>>) {
        let has_links = entry.prev.borrow().is_some() && entry.next.borrow().is_some();
        if has_links {
            unlink(&entry);
        }
        let front = self.sentinel.next.borrow().clone();
        *entry.next.borrow_mut() = front.clone();
        if let Some(front) = front {
            *front.prev.borrow_mut() = Some(entry.clone());
        }
        *self.sentinel.next.borrow_mut() = Some(entry.clone());
        *entry.prev.borrow_mut() = Some(self.sentinel.clone());
    }

    /// Whether the list has no entries.
    pub fn is_empty(&self) -> bool {
        let prev = self.sentinel.prev.borrow();
        prev.as_ref()
            .is_some_and(|entry| Rc::ptr_eq(entry, &self.sentinel))
    }

    /// Entries from the oldest to the newest.
    pub fn iter_entries(&self) -> Vec<Rc<ListNode<T>>> {
        let mut entries = Vec::new();
        let mut current = self.sentinel.prev.borrow().clone();
        while let Some(entry) = current {
            if Rc::ptr_eq(&entry, &self.sentinel) {
                break;
            }
            current = entry.prev.borrow().clone();
            entries.push(entry);
        }
        entries
    }
}

impl<T: serde::Serialize> List<T> {
    /// Upstream's `toString`: the entries as a JSON array, oldest first.
    pub fn to_json_string(&self) -> String {
        let entries: Vec<String> = self
            .iter_entries()
            .into_iter()
            .map(|entry| {
                entry
                    .value()
                    .map(|value| serde_json::to_string(value).unwrap_or_default())
                    .unwrap_or_default()
            })
            .collect();
        format!("[{}]", entries.join(", "))
    }
}

impl<T> Drop for List<T> {
    fn drop(&mut self) {
        let mut current = self.sentinel.next.borrow_mut().take();
        while let Some(entry) = current {
            let next = entry.next.borrow_mut().take();
            *entry.prev.borrow_mut() = None;
            current = next;
        }
        *self.sentinel.prev.borrow_mut() = None;
    }
}

fn unlink<T>(entry: &Rc<ListNode<T>>) {
    let prev = entry.prev.borrow().clone();
    let next = entry.next.borrow().clone();
    if let Some(prev) = &prev {
        *prev.next.borrow_mut() = next.clone();
    }
    if let Some(next) = &next {
        *next.prev.borrow_mut() = prev;
    }
    *entry.next.borrow_mut() = None;
    *entry.prev.borrow_mut() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dequeue_returns_none_when_empty() {
        let list: List<i32> = List::new();
        assert!(list.dequeue().is_none());
        assert!(list.is_empty());
    }

    #[test]
    fn dequeue_unlinks_and_returns_the_first_entry() {
        let list = List::new();
        let entry = ListNode::new(1);
        list.enqueue(entry.clone());
        assert!(Rc::ptr_eq(&list.dequeue().unwrap(), &entry));
        assert!(list.is_empty());
    }

    #[test]
    fn dequeue_returns_entries_in_fifo_order() {
        let list = List::new();
        let first = ListNode::new(1);
        let second = ListNode::new(2);
        list.enqueue(first.clone());
        list.enqueue(second.clone());
        assert!(Rc::ptr_eq(&list.dequeue().unwrap(), &first));
        assert!(Rc::ptr_eq(&list.dequeue().unwrap(), &second));
    }

    #[test]
    fn re_enqueueing_relinks_an_entry() {
        let list = List::new();
        let first = ListNode::new(1);
        let second = ListNode::new(2);
        list.enqueue(first.clone());
        list.enqueue(second.clone());
        list.enqueue(first.clone());
        assert!(Rc::ptr_eq(&list.dequeue().unwrap(), &second));
        assert!(Rc::ptr_eq(&list.dequeue().unwrap(), &first));
    }

    #[test]
    fn enqueueing_on_another_list_unlinks_from_the_first() {
        let first = List::new();
        let second = List::new();
        let entry = ListNode::new(1);
        first.enqueue(entry.clone());
        second.enqueue(entry.clone());
        assert!(first.dequeue().is_none());
        assert!(Rc::ptr_eq(&second.dequeue().unwrap(), &entry));
    }

    #[test]
    fn to_json_string_lists_entries_oldest_first() {
        let list = List::new();
        list.enqueue(ListNode::new(json!({"entry": 1})));
        list.enqueue(ListNode::new(json!({"entry": 2})));
        assert_eq!(list.to_json_string(), "[{\"entry\":1}, {\"entry\":2}]");
    }
}
