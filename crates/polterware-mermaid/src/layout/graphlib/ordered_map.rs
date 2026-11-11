//! A string-keyed map that iterates in insertion order.
//!
//! Graphlib stores nodes and edges as plain JavaScript objects, so iteration follows the
//! key insertion order. `std::collections::HashMap` does not, and dagre's layout depends on
//! this order, so the graph keeps its maps here. One deliberate difference: JavaScript
//! orders integer-like keys before all others; this map keeps true insertion order.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) struct OrderedMap<V> {
    entries: Vec<(String, V)>,
    indices: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
        }
    }
}

impl<V> OrderedMap<V> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.indices.contains_key(key)
    }

    pub(crate) fn get(&self, key: &str) -> Option<&V> {
        self.indices.get(key).map(|&index| &self.entries[index].1)
    }

    pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        let index = *self.indices.get(key)?;
        Some(&mut self.entries[index].1)
    }

    /// Inserts `key`, replacing the value in place when it already exists. JavaScript's
    /// `object[key] = value` keeps the original position, and so does this.
    pub(crate) fn insert(&mut self, key: String, value: V) -> Option<V> {
        match self.indices.get(&key) {
            Some(&index) => Some(std::mem::replace(&mut self.entries[index].1, value)),
            None => {
                self.indices.insert(key.clone(), self.entries.len());
                self.entries.push((key, value));
                None
            }
        }
    }

    pub(crate) fn remove(&mut self, key: &str) -> Option<V> {
        let index = self.indices.remove(key)?;
        let (_, value) = self.entries.remove(index);
        for (offset, (moved, _)) in self.entries[index..].iter().enumerate() {
            self.indices.insert(moved.clone(), index + offset);
        }
        Some(value)
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub(crate) fn keys_vec(&self) -> Vec<String> {
        self.keys().map(str::to_string).collect()
    }
}
