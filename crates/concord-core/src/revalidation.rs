//! Private, bounded entity-tag storage. No provider or request data is formatted.
use std::collections::{VecDeque, hash_map::RandomState};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::Mutex;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Version};
use url::Url;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheKey([u64; 4]);

#[derive(Clone)]
pub(crate) struct Entity {
    pub(crate) tag: HeaderValue,
    pub(crate) body: Bytes,
    pub(crate) headers: HeaderMap,
    pub(crate) version: Version,
    pub(crate) content_length: Option<u64>,
}

pub(crate) struct RevalidationCache {
    hashes: [RandomState; 4],
    entries: usize,
    bytes: usize,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    lru: VecDeque<(CacheKey, Entity)>,
    bytes: usize,
}

impl RevalidationCache {
    pub(crate) fn new(entries: usize, bytes: usize) -> Self {
        Self {
            hashes: std::array::from_fn(|_| RandomState::new()),
            entries,
            bytes,
            state: Mutex::new(State::default()),
        }
    }

    pub(crate) fn key(
        &self,
        endpoint: &str,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
    ) -> CacheKey {
        // Header insertion order does not change the request identity. Preserve
        // the order of repeated values for each name.
        let mut names: Vec<_> = headers.keys().collect();
        names.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        CacheKey(std::array::from_fn(|index| {
            let mut hash = self.hashes[index].build_hasher();
            endpoint.hash(&mut hash);
            method.hash(&mut hash);
            url.as_str().hash(&mut hash);
            for name in &names {
                name.as_str().hash(&mut hash);
                for value in headers.get_all(*name) {
                    value.as_bytes().hash(&mut hash);
                }
            }
            hash.finish()
        }))
    }

    pub(crate) fn get(&self, key: CacheKey) -> Option<Entity> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let index = state.lru.iter().position(|(stored, _)| *stored == key)?;
        let entry = state.lru.remove(index)?;
        let result = entry.1.clone();
        state.lru.push_back(entry);
        Some(result)
    }

    pub(crate) fn store(&self, key: CacheKey, entity: Option<Entity>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = state.lru.iter().position(|(stored, _)| *stored == key) {
            if let Some((_, removed)) = state.lru.remove(index) {
                state.bytes -= removed.body.len();
            }
        }
        let Some(entity) = entity else { return };
        if self.entries == 0 || self.bytes == 0 || entity.body.len() > self.bytes {
            return;
        }
        // Evict before adding, avoiding arithmetic overflow even for a usize::MAX bound.
        while state.lru.len() >= self.entries || state.bytes > self.bytes - entity.body.len() {
            let Some((_, removed)) = state.lru.pop_front() else {
                break;
            };
            state.bytes -= removed.body.len();
        }
        state.bytes += entity.body.len();
        state.lru.push_back((key, entity));
    }
}
