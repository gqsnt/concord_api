//! Deterministic serde payloads of controlled serialized size.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// The tracker's response element (only used as a declared output type).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    /// Identifier.
    pub id: u64,
    /// Title.
    pub title: String,
}

/// One record of a [`Payload`]; about 100 bytes of JSON.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    /// Identifier.
    pub id: u64,
    /// Name.
    pub name: String,
    /// E-mail address.
    pub email: String,
    /// A float, so number formatting and parsing are exercised.
    pub score: f64,
    /// A flag.
    pub active: bool,
    /// Tags.
    pub tags: Vec<String>,
}

/// A JSON body: a list of records.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    /// Records.
    pub items: Vec<Record>,
}

/// The body sizes used by the encoding, decoding and round-trip groups.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    /// Stable label used in benchmark IDs.
    pub label: &'static str,
    /// Approximate serialized size in bytes.
    pub target: usize,
}

/// ~100 B.
pub const SMALL: Size = Size {
    label: "100B",
    target: 100,
};
/// ~10 KB.
pub const MEDIUM: Size = Size {
    label: "10KB",
    target: 10 * 1024,
};
/// ~1 MB.
pub const LARGE: Size = Size {
    label: "1MB",
    target: 1024 * 1024,
};
/// Every body size, smallest first.
pub const SIZES: [Size; 3] = [SMALL, MEDIUM, LARGE];

fn record(i: u64) -> Record {
    Record {
        id: i,
        name: format!("n{i}"),
        email: format!("u{i}@example.com"),
        score: i as f64 + 0.5,
        active: i % 2 == 0,
        tags: vec!["t".to_owned()],
    }
}

/// A payload whose JSON is the smallest list of records at least `target` bytes long (one record
/// minimum).
pub fn payload(target: usize) -> Payload {
    let mut items = Vec::new();
    // `{"items":[` + `]}` plus one comma per additional record.
    let mut len = 12;
    while items.is_empty() || len < target {
        let r = record(items.len() as u64);
        len += serde_json::to_vec(&r).expect("serializable").len() + usize::from(!items.is_empty());
        items.push(r);
    }
    Payload { items }
}

/// UTF-8 text of exactly `target` bytes.
pub fn text(target: usize) -> String {
    const WORDS: &str = "lorem ipsum dolor sit amet consectetur adipiscing elit ";
    WORDS.chars().cycle().take(target).collect()
}

/// `target` bytes of binary data.
pub fn binary(target: usize) -> Bytes {
    (0..target)
        .map(|i| (i % 251) as u8)
        .collect::<Vec<u8>>()
        .into()
}

/// The JSON encoding of `payload(target)`.
pub fn json_bytes(target: usize) -> Bytes {
    serde_json::to_vec(&payload(target))
        .expect("serializable")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_close_to_their_labels() {
        for size in SIZES {
            let len = json_bytes(size.target).len();
            assert!(len >= size.target.min(90), "{}: {len}", size.label);
            assert!(len < size.target + 200, "{}: {len}", size.label);
            assert_eq!(text(size.target).len(), size.target);
            assert_eq!(binary(size.target).len(), size.target);
        }
    }
}
