use std::{collections::BTreeMap, fmt};

use concord::{QueryPairs, QueryPairsWriter, ResponseMeta};
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;

use crate::ApiProblem;

/// An indivisible continuation object. All returned fields are sent on the next request.
///
/// String and integer values are accepted, including numeric search offsets. The generic
/// `continue` string is mandatory. Fixed query inputs cannot be overwritten. Fields can be
/// inspected explicitly; Debug/Display print no values. Keep the same wiki, query and page size.
#[derive(Clone, PartialEq, Eq)]
pub struct Continuation(BTreeMap<String, String>);

impl Continuation {
    /// The complete, unredacted set of query parameters to replay.
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Continuation {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let fields = BTreeMap::<String, Value>::deserialize(de)?;
        if !fields.get("continue").is_some_and(Value::is_string) {
            return Err(D::Error::custom(
                "continuation needs its generic continue string",
            ));
        }
        let mut pairs = BTreeMap::new();
        for (key, value) in fields {
            if key.is_empty()
                || matches!(
                    key.as_str(),
                    "action"
                        | "format"
                        | "formatversion"
                        | "list"
                        | "prop"
                        | "srsearch"
                        | "srlimit"
                        | "srprop"
                        | "pageids"
                        | "rvprop"
                        | "rvslots"
                        | "rvlimit"
                        | "cmtitle"
                        | "cmlimit"
                )
            {
                return Err(D::Error::custom(
                    "continuation collides with a fixed query input",
                ));
            }
            let text = match value {
                Value::String(text) => text,
                Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
                _ => {
                    return Err(D::Error::custom(
                        "continuation values must be strings or integers",
                    ));
                }
            };
            pairs.insert(key, text);
        }
        Ok(Self(pairs))
    }
}

impl QueryPairs for Continuation {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        for (key, value) in &self.0 {
            out.pair(key, value);
        }
    }
}

impl fmt::Debug for Continuation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Continuation")
            .field("fields", &self.0.len())
            .finish()
    }
}

impl fmt::Display for Continuation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<continuation>")
    }
}

/// The wire envelope decoded by Concord. Use generated `send()` to apply provider semantics.
#[derive(Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
pub struct Envelope<T> {
    pub(crate) query: Option<T>,
    #[serde(
        default,
        rename = "continue",
        deserialize_with = "present_continuation"
    )]
    pub(crate) continuation: Option<Continuation>,
    pub(crate) error: Option<ApiProblem>,
    pub(crate) warnings: Option<Value>,
}

// Absence ends traversal; a present null is malformed, never an exhaustion signal.
fn present_continuation<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Option<Continuation>, D::Error> {
    Continuation::deserialize(de).map(Some)
}

/// A query after the SDK has checked its error/warning envelope.
#[derive(Debug)]
pub struct QueryResponse<T> {
    /// The endpoint's query data.
    pub data: T,
    /// Every next-page parameter; absence alone indicates exhaustion.
    pub continuation: Option<Continuation>,
    /// HTTP status, headers and operation. Explicit header access is unredacted.
    pub meta: ResponseMeta,
}

/// Page identity returned by search or category membership.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct PageIdentity {
    /// Stable ID within this wiki.
    pub pageid: u64,
    /// Namespace number (not necessarily the main namespace).
    pub ns: i32,
    /// Provider title, treated as data.
    pub title: String,
}

/// One page of search hits, in provider order.
#[derive(Debug, Deserialize)]
pub struct SearchResults {
    /// Matching page identities.
    pub search: Vec<PageIdentity>,
}

/// One page of category members.
#[derive(Debug, Deserialize)]
pub struct CategoryResults {
    /// Members, including subcategories or files when returned by the wiki.
    pub categorymembers: Vec<PageIdentity>,
}

/// The content-query wire data. Prefer `MediaWikiQuery::fetch_page` for a checked projection.
#[derive(Deserialize)]
pub struct ContentResults {
    pub(crate) pages: Vec<ContentPage>,
}

#[derive(Deserialize)]
pub(crate) struct ContentPage {
    pub pageid: Option<u64>,
    pub title: String,
    #[serde(default)]
    pub missing: bool,
    #[serde(default)]
    pub invalid: bool,
    #[serde(default)]
    pub revisions: Vec<Revision>,
}

#[derive(Deserialize)]
pub(crate) struct Revision {
    pub revid: Option<u64>,
    pub slots: Option<Slots>,
}

#[derive(Deserialize)]
pub(crate) struct Slots {
    pub main: Option<Slot>,
}

#[derive(Deserialize)]
pub(crate) struct Slot {
    pub content: Option<String>,
}

/// The selected page's raw content; no wikitext parsing or HTML rendering is performed.
#[derive(Debug, PartialEq, Eq)]
pub struct PageContent {
    /// The requested wiki-local page ID.
    pub pageid: u64,
    /// Current provider title.
    pub title: String,
    /// Revision whose main slot supplied the content.
    pub revision: u64,
    /// Raw main-slot text, possibly empty.
    pub content: String,
}
