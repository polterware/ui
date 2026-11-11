//! Port of `graphlib/lib/data/priority-queue.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt). The heap layout is ported exactly, including its child indices, because the
//! order in which equal priorities are removed decides which shortest path wins.

/// A min-priority queue over string keys.
#[derive(Debug, Default)]
pub(crate) struct PriorityQueue {
    entries: Vec<Entry>,
    key_indices: std::collections::HashMap<String, usize>,
}

#[derive(Debug)]
struct Entry {
    key: String,
    priority: f64,
}

impl PriorityQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn size(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.key.clone()).collect()
    }

    #[cfg(test)]
    pub(crate) fn has(&self, key: &str) -> bool {
        self.key_indices.contains_key(key)
    }

    pub(crate) fn priority(&self, key: &str) -> Option<f64> {
        let index = *self.key_indices.get(key)?;
        Some(self.entries[index].priority)
    }

    #[cfg(test)]
    pub(crate) fn min(&self) -> &str {
        assert!(self.size() > 0, "Queue underflow");
        &self.entries[0].key
    }

    pub(crate) fn add(&mut self, key: &str, priority: f64) -> bool {
        if self.key_indices.contains_key(key) {
            return false;
        }
        let index = self.entries.len();
        self.key_indices.insert(key.to_string(), index);
        self.entries.push(Entry {
            key: key.to_string(),
            priority,
        });
        self.decrease_index(index);
        true
    }

    pub(crate) fn remove_min(&mut self) -> String {
        assert!(self.size() > 0, "Queue underflow");
        let last = self.entries.len() - 1;
        self.swap(0, last);
        let min = self.entries.pop().expect("the queue is not empty");
        self.key_indices.remove(&min.key);
        self.heapify(0);
        min.key
    }

    pub(crate) fn decrease(&mut self, key: &str, priority: f64) {
        let Some(&index) = self.key_indices.get(key) else {
            panic!("Key not found: {key}");
        };
        let current = self.entries[index].priority;
        assert!(
            priority <= current,
            "New priority is greater than current priority. Key: {key} Old: {current} New: {priority}"
        );
        self.entries[index].priority = priority;
        self.decrease_index(index);
    }

    fn heapify(&mut self, index: usize) {
        let left = 2 * index;
        let right = left + 1;
        let mut largest = index;
        if left < self.entries.len() {
            if self.entries[left].priority < self.entries[largest].priority {
                largest = left;
            }
            if right < self.entries.len()
                && self.entries[right].priority < self.entries[largest].priority
            {
                largest = right;
            }
            if largest != index {
                self.swap(index, largest);
                self.heapify(largest);
            }
        }
    }

    fn decrease_index(&mut self, mut index: usize) {
        let priority = self.entries[index].priority;
        while index != 0 {
            let parent = index >> 1;
            if self.entries[parent].priority < priority {
                break;
            }
            self.swap(index, parent);
            index = parent;
        }
    }

    fn swap(&mut self, first: usize, second: usize) {
        self.entries.swap(first, second);
        let first_key = self.entries[first].key.clone();
        let second_key = self.entries[second].key.clone();
        self.key_indices.insert(first_key, first);
        self.key_indices.insert(second_key, second);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::PriorityQueue;

    #[test]
    fn size_is_empty_then_counts_entries() {
        let mut queue = PriorityQueue::new();
        assert_eq!(queue.size(), 0);
        queue.add("a", 1.0);
        assert_eq!(queue.size(), 1);
        queue.add("b", 2.0);
        assert_eq!(queue.size(), 2);
    }

    #[test]
    fn keys_lists_every_key() {
        let mut queue = PriorityQueue::new();
        for (index, key) in ["a", "1", "false", "undefined", "null"].iter().enumerate() {
            queue.add(key, index as f64);
        }
        let mut keys = queue.keys();
        keys.sort();
        assert_eq!(keys, ["1", "a", "false", "null", "undefined"]);
    }

    #[test]
    fn has_reports_membership() {
        let mut queue = PriorityQueue::new();
        queue.add("a", 1.0);
        assert!(queue.has("a"));
        assert!(!queue.has("foo"));
    }

    #[test]
    fn priority_returns_the_current_priority() {
        let mut queue = PriorityQueue::new();
        queue.add("a", 1.0);
        queue.add("b", 2.0);
        assert_eq!(queue.priority("a"), Some(1.0));
        assert_eq!(queue.priority("b"), Some(2.0));
        assert_eq!(queue.priority("foo"), None);
    }

    #[test]
    #[should_panic(expected = "Queue underflow")]
    fn min_panics_on_an_empty_queue() {
        PriorityQueue::new().min();
    }

    #[test]
    fn min_returns_the_smallest_key_without_removing_it() {
        let mut queue = PriorityQueue::new();
        queue.add("b", 2.0);
        queue.add("a", 1.0);
        assert_eq!(queue.min(), "a");
        assert_eq!(queue.size(), 2);
    }

    #[test]
    fn add_returns_whether_the_key_was_added() {
        let mut queue = PriorityQueue::new();
        assert!(queue.add("a", 1.0));
        assert!(!queue.add("a", 1.0));
        assert_eq!(queue.keys(), ["a"]);
    }

    #[test]
    fn remove_min_returns_entries_in_priority_order() {
        let mut queue = PriorityQueue::new();
        for (key, priority) in [("b", 2.0), ("a", 1.0), ("c", 3.0), ("e", 5.0), ("d", 4.0)] {
            queue.add(key, priority);
        }
        assert_eq!(queue.remove_min(), "a");
        assert_eq!(queue.remove_min(), "b");
        assert_eq!(queue.remove_min(), "c");
        assert_eq!(queue.remove_min(), "d");
        assert_eq!(queue.remove_min(), "e");
    }

    #[test]
    #[should_panic(expected = "Queue underflow")]
    fn remove_min_panics_on_an_empty_queue() {
        PriorityQueue::new().remove_min();
    }

    #[test]
    fn decrease_lowers_the_priority() {
        let mut queue = PriorityQueue::new();
        queue.add("a", 1.0);
        queue.decrease("a", -1.0);
        assert_eq!(queue.priority("a"), Some(-1.0));
    }

    #[test]
    #[should_panic(expected = "Key not found: a")]
    fn decrease_panics_for_an_unknown_key() {
        PriorityQueue::new().decrease("a", -1.0);
    }

    #[test]
    #[should_panic(expected = "New priority is greater than current priority")]
    fn decrease_panics_when_the_priority_grows() {
        let mut queue = PriorityQueue::new();
        queue.add("a", 1.0);
        queue.decrease("a", 2.0);
    }
}
