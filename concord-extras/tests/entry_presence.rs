#![cfg(feature = "json")]
//! Reader regressions for deferred, non-consuming entry presence checks.
use concord_core::body::Entries;
use serde::{Serialize, Serializer};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

// Test-only helper: a serialization or byte-body failure must fail the test.
#[allow(clippy::unwrap_used)]
fn json(entries: &Entries<'_>) -> Vec<u8> {
    concord_extras::body::json(entries)
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec()
}

// IntoIterator can own the conversion, and may move data out of its source.
// The None probe must not call it merely to discover that this is a collection.
struct TakenSeq(RefCell<Vec<u8>>, Arc<AtomicUsize>);
impl IntoIterator for &TakenSeq {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;
    fn into_iter(self) -> Self::IntoIter {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.take().into_iter()
    }
}
impl Serialize for TakenSeq {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self)
    }
}

#[test]
fn none_probe_collect_seq_must_not_take_the_values() {
    let calls = Arc::new(AtomicUsize::new(0));
    let value = TakenSeq(RefCell::new(vec![7, 8]), calls.clone());
    let mut entries = Entries::new();
    entries.__push("rows", &value);
    let after_insert = calls.load(Ordering::SeqCst);
    let wire = json(&entries);
    println!(
        "collect_seq: after_insert={after_insert}, wire={}",
        String::from_utf8_lossy(&wire)
    );
    assert_eq!(after_insert, 0, "presence probe invoked IntoIterator");
    assert_eq!(wire, br#"{"rows":[7,8]}"#);
}

struct TakenMap(RefCell<Vec<(&'static str, u8)>>, Arc<AtomicUsize>);
impl IntoIterator for &TakenMap {
    type Item = (&'static str, u8);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.take().into_iter()
    }
}
impl Serialize for TakenMap {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(self)
    }
}

#[test]
fn none_probe_collect_map_must_not_take_the_values() {
    let calls = Arc::new(AtomicUsize::new(0));
    let value = TakenMap(RefCell::new(vec![("a", 7), ("b", 8)]), calls.clone());
    let mut entries = Entries::new();
    entries.__push("rows", &value);
    let after_insert = calls.load(Ordering::SeqCst);
    let wire = json(&entries);
    println!(
        "collect_map: after_insert={after_insert}, wire={}",
        String::from_utf8_lossy(&wire)
    );
    assert_eq!(after_insert, 0, "presence probe invoked IntoIterator");
    assert_eq!(wire, br#"{"rows":{"a":7,"b":8}}"#);
}

struct HintIter<T> {
    next: Option<T>,
    hints: Arc<AtomicUsize>,
}
impl<T> Iterator for HintIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.next.take()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.hints.fetch_add(1, Ordering::SeqCst);
        let len = usize::from(self.next.is_some());
        (len, Some(len))
    }
}
struct Hints {
    map: bool,
    hints: Arc<AtomicUsize>,
}
impl Serialize for Hints {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.map {
            s.collect_map(HintIter {
                next: Some(("a", 1)),
                hints: self.hints.clone(),
            })
        } else {
            s.collect_seq(HintIter {
                next: Some(1),
                hints: self.hints.clone(),
            })
        }
    }
}
#[test]
fn none_probe_collectors_must_not_call_size_hint() {
    let mut observed = Vec::new();
    for map in [false, true] {
        let hints = Arc::new(AtomicUsize::new(0));
        let mut e = Entries::new();
        e.__push(
            "x",
            Hints {
                map,
                hints: hints.clone(),
            },
        );
        assert_eq!(hints.load(Ordering::SeqCst), 0);
        let wire = json(&e);
        assert_eq!(
            wire,
            if map {
                br#"{"x":{"a":1}}"#.as_slice()
            } else {
                br#"{"x":[1]}"#.as_slice()
            }
        );
        observed.push((map, hints.load(Ordering::SeqCst)));
    }
    assert_eq!(observed, [(false, 1), (true, 1)]);
}

#[test]
fn entries_any_serde_writer_promise_must_respect_binary_presence() {
    struct HumanNone;
    impl Serialize for HumanNone {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() {
                s.serialize_none()
            } else {
                s.serialize_u8(7)
            }
        }
    }
    let value = HumanNone;
    let direct = std::collections::BTreeMap::from([("x", &value)]);
    let expected = bincode::serialize(&direct).unwrap();
    let mut e = Entries::new();
    e.__push("x", &value);
    assert_eq!(json(&e), b"{}");
    let actual = bincode::serialize(&e).unwrap();
    let decoded: std::collections::BTreeMap<String, u8> = bincode::deserialize(&actual).unwrap();
    println!("binary map after Entries: {decoded:?}; expected x=7");
    assert_eq!(
        actual, expected,
        "the human-readable probe silently discarded a binary value"
    );
}

#[test]
fn entries_binary_none_must_be_omitted_in_binary_writer() {
    struct BinaryNone;
    impl Serialize for BinaryNone {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() {
                s.serialize_u8(7)
            } else {
                s.serialize_none()
            }
        }
    }
    let mut e = Entries::new();
    e.__push("x", BinaryNone);
    assert_eq!(json(&e), br#"{"x":7}"#);
    let encoded = bincode::serialize(&e).unwrap();
    let decoded: std::collections::BTreeMap<String, Option<u8>> =
        bincode::deserialize(&encoded).unwrap();
    assert!(
        decoded.is_empty(),
        "a binary top-level None was kept: {decoded:?}"
    );
}
