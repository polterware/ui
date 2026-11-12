//! Port of `layout-base/src/util/LinkedList.js`.
//!
//! A doubly linked list whose entries are handles, as upstream's nodes are. Upstream
//! only uses it as a FIFO queue (`LGraph.updateConnected`), but the whole API is ported.
//! `get_object_at`/`set_object_at` are 1-based upstream: the walk starts at `i = 1`, so
//! index `0` and index `1` both address the head. The port keeps that. Upstream's
//! bounds check calls `this.length()` where `length` is a number, which throws, so the
//! port answers out-of-range lookups with `None`/`false` instead.

/// A handle to an entry of a [`LinkedList`], upstream's node object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LinkedListNode(usize);

struct Entry<T> {
    value: Option<T>,
    next: Option<usize>,
    prev: Option<usize>,
}

/// Upstream's `LinkedList`.
pub struct LinkedList<T> {
    entries: Vec<Entry<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    length: usize,
}

impl<T> Default for LinkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LinkedList<T> {
    /// An empty list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            head: None,
            tail: None,
            length: 0,
        }
    }

    /// A list holding `values` in order, upstream's `new LinkedList(vals)`.
    pub fn from_values(values: impl IntoIterator<Item = T>) -> Self {
        let mut list = Self::new();
        for value in values {
            list.push(value);
        }
        list
    }

    /// Upstream `size`.
    pub fn size(&self) -> usize {
        self.length
    }

    /// Upstream `length`.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Upstream's internal `nodeFrom`.
    fn node_from(&mut self, value: T) -> LinkedListNode {
        self.entries.push(Entry {
            value: Some(value),
            next: None,
            prev: None,
        });
        LinkedListNode(self.entries.len() - 1)
    }

    /// Upstream's internal `add`.
    fn add(&mut self, prev: Option<usize>, node: usize, next: Option<usize>) -> LinkedListNode {
        match prev {
            Some(prev) => self.entries[prev].next = Some(node),
            None => self.head = Some(node),
        }

        match next {
            Some(next) => self.entries[next].prev = Some(node),
            None => self.tail = Some(node),
        }

        self.entries[node].prev = prev;
        self.entries[node].next = next;

        self.length += 1;

        LinkedListNode(node)
    }

    /// Upstream's internal `remove`.
    fn remove_entry(&mut self, node: usize) -> LinkedListNode {
        let prev = self.entries[node].prev;
        let next = self.entries[node].next;

        match prev {
            Some(prev) => self.entries[prev].next = next,
            None => self.head = next,
        }

        match next {
            Some(next) => self.entries[next].prev = prev,
            None => self.tail = prev,
        }

        self.entries[node].prev = None;
        self.entries[node].next = None;

        self.length -= 1;

        LinkedListNode(node)
    }

    /// Upstream `insertBefore`.
    pub fn insert_before(&mut self, value: T, other: LinkedListNode) -> LinkedListNode {
        let node = self.node_from(value);
        let prev = self.entries[other.0].prev;
        self.add(prev, node.0, Some(other.0))
    }

    /// Upstream `insertAfter`.
    pub fn insert_after(&mut self, value: T, other: LinkedListNode) -> LinkedListNode {
        let node = self.node_from(value);
        let next = self.entries[other.0].next;
        self.add(Some(other.0), node.0, next)
    }

    /// Upstream `push`.
    pub fn push(&mut self, value: T) -> LinkedListNode {
        let node = self.node_from(value);
        let tail = self.tail;
        self.add(tail, node.0, None)
    }

    /// Upstream `unshift`.
    pub fn unshift(&mut self, value: T) -> LinkedListNode {
        let node = self.node_from(value);
        let head = self.head;
        self.add(None, node.0, head)
    }

    /// Upstream `remove`: unlinks the entry and returns its handle.
    pub fn remove(&mut self, node: LinkedListNode) -> LinkedListNode {
        self.remove_entry(node.0)
    }

    /// Upstream `pop`.
    pub fn pop(&mut self) -> Option<T> {
        let node = self.pop_node()?;
        self.take_value(node)
    }

    /// Upstream `popNode`.
    pub fn pop_node(&mut self) -> Option<LinkedListNode> {
        let tail = self.tail?;
        Some(self.remove_entry(tail))
    }

    /// Upstream `shift`.
    pub fn shift(&mut self) -> Option<T> {
        let node = self.shift_node()?;
        self.take_value(node)
    }

    /// Upstream `shiftNode`.
    pub fn shift_node(&mut self) -> Option<LinkedListNode> {
        let head = self.head?;
        Some(self.remove_entry(head))
    }

    /// Upstream `get_object_at`: 1-based, with index `0` addressing the head too.
    pub fn get_object_at(&self, index: usize) -> Option<&T> {
        if index > self.length {
            return None;
        }

        let mut current = self.head;
        let mut i = 1;
        while i < index {
            current = current.and_then(|node| self.entries[node].next);
            i += 1;
        }

        current.and_then(|node| self.entries[node].value.as_ref())
    }

    /// Upstream `set_object_at`: 1-based like [`LinkedList::get_object_at`].
    pub fn set_object_at(&mut self, index: usize, value: T) -> bool {
        if index > self.length {
            return false;
        }

        let mut current = self.head;
        let mut i = 1;
        while i < index {
            current = current.and_then(|node| self.entries[node].next);
            i += 1;
        }

        match current {
            Some(node) => {
                self.entries[node].value = Some(value);
                true
            }
            None => false,
        }
    }

    /// The value of a handle.
    pub fn value(&self, node: LinkedListNode) -> Option<&T> {
        self.entries[node.0].value.as_ref()
    }

    /// The value of a handle, mutably.
    pub fn value_mut(&mut self, node: LinkedListNode) -> Option<&mut T> {
        self.entries[node.0].value.as_mut()
    }

    /// The values in order.
    pub fn iter(&self) -> LinkedListIter<'_, T> {
        LinkedListIter {
            list: self,
            next: self.head,
        }
    }

    fn take_value(&mut self, node: LinkedListNode) -> Option<T> {
        self.entries[node.0].value.take()
    }
}

impl<'a, T> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = LinkedListIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterates a [`LinkedList`] in order.
pub struct LinkedListIter<'a, T> {
    list: &'a LinkedList<T>,
    next: Option<usize>,
}

impl<'a, T> Iterator for LinkedListIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.next?;
        self.next = self.list.entries[node].next;
        self.list.entries[node].value.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_operations() {
        let mut list = LinkedList::new();
        assert!(list.is_empty());
        list.push(1);
        list.push(2);
        list.push(3);
        assert_eq!(list.size(), 3);
        assert_eq!(list.shift(), Some(1));
        assert_eq!(list.pop(), Some(3));
        assert_eq!(list.iter().copied().collect::<Vec<_>>(), vec![2]);
        assert_eq!(list.shift(), Some(2));
        assert_eq!(list.shift(), None);
        assert!(list.is_empty());
    }

    #[test]
    fn insertion_keeps_order() {
        let mut list = LinkedList::from_values([1, 3]);
        let tail = list.iter().count();
        assert_eq!(tail, 2);

        let three = list.push(5);
        let three_before = list.insert_before(4, three);
        list.insert_after(6, three);
        assert_eq!(
            list.iter().copied().collect::<Vec<_>>(),
            vec![1, 3, 4, 5, 6]
        );
        assert_eq!(list.value(three_before), Some(&4));
        assert_eq!(list.value(three), Some(&5));

        let head = list.unshift(0);
        assert_eq!(
            list.iter().copied().collect::<Vec<_>>(),
            vec![0, 1, 3, 4, 5, 6]
        );
        list.remove(head);
        assert_eq!(list.size(), 5);
    }

    #[test]
    fn object_access_is_one_based() {
        let mut list = LinkedList::from_values([10, 20, 30]);
        assert_eq!(list.get_object_at(0), Some(&10));
        assert_eq!(list.get_object_at(1), Some(&10));
        assert_eq!(list.get_object_at(2), Some(&20));
        assert_eq!(list.get_object_at(3), Some(&30));
        assert_eq!(list.get_object_at(4), None);

        assert!(list.set_object_at(2, 21));
        assert_eq!(list.get_object_at(2), Some(&21));
        assert!(!list.set_object_at(9, 99));
    }

    #[test]
    fn shift_node_and_pop_node_return_handles() {
        let mut list = LinkedList::from_values(["a", "b"]);
        let head = list.shift_node().unwrap();
        assert_eq!(list.value(head), Some(&"a"));
        let tail = list.pop_node().unwrap();
        assert_eq!(list.value(tail), Some(&"b"));
        assert!(list.pop_node().is_none());
    }
}
