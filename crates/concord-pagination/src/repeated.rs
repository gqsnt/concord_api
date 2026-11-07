//! Optional key history without imposing equality or hashing on ordinary traversals.

use std::collections::HashSet;
use std::hash::Hash;

pub(crate) struct KeyHistory<K> {
    keys: HashSet<K>,
    insert: fn(&mut HashSet<K>, &K) -> bool,
}

impl<K> KeyHistory<K> {
    pub(crate) fn new() -> Self
    where
        K: Clone + Eq + Hash,
    {
        Self {
            keys: HashSet::new(),
            insert: |keys, key| !keys.contains(key) && keys.insert(key.clone()),
        }
    }

    pub(crate) fn admit(&mut self, key: &K) -> bool {
        (self.insert)(&mut self.keys, key)
    }
}
