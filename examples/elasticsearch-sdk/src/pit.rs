//! A point in time (PIT) and `search_after` paging over it.
//!
//! The pagination guide's sequence: open a PIT on an index (`POST /<index>/_pit?keep_alive=1m`),
//! search with `pit.id` and a `sort` and **without** an index in the path, take the last hit's
//! `sort` values as the next request's `search_after` and "use the latest PIT ID" each response
//! returns, keep `query` and `sort` unchanged, and delete the PIT when finished
//! (`DELETE /_pit` with `{"id": ..}`).

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use serde::de::{Deserializer, IgnoredAny};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::error::{Error, Invalid, Protocol};
use crate::model::{IndexName, KeepAlive};

/// The largest page size this SDK requests (the default `index.max_result_window` the guide
/// names is 10,000 hits; the guide's PIT examples use `"size": 10000`).
pub const MAX_PAGE_SIZE: u32 = 10_000;

/// A point-in-time id: opaque text Elasticsearch returns and expects back. `Debug` shows its
/// length only.
#[derive(Clone, PartialEq, Eq)]
pub struct PitId(Arc<str>);

impl PitId {
    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PitId(<{} bytes>)", self.0.len())
    }
}

impl Serialize for PitId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PitId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|id| Self(id.into()))
    }
}

/// An open point in time on one index.
///
/// It tracks the **latest** PIT id: every search page through
/// [`Elasticsearch::search_after`](crate::Elasticsearch::search_after) replaces it with the
/// `pit_id` its response returned, and
/// [`Elasticsearch::close_point_in_time`](crate::Elasticsearch::close_point_in_time) deletes
/// that id. Dropping it does not close it (there is no asynchronous drop): Elasticsearch then
/// frees it when its keep-alive expires.
pub struct PointInTime {
    index: IndexName,
    keep_alive: KeepAlive,
    latest: Arc<Mutex<PitId>>,
}

impl PointInTime {
    pub(crate) fn new(index: IndexName, keep_alive: KeepAlive, id: PitId) -> Self {
        Self {
            index,
            keep_alive,
            latest: Arc::new(Mutex::new(id)),
        }
    }

    /// A handle on the same PIT state (crate-private: callers hold one owner, which closes it).
    pub(crate) fn share(&self) -> Self {
        Self {
            index: self.index.clone(),
            keep_alive: self.keep_alive,
            latest: Arc::clone(&self.latest),
        }
    }

    /// The index it was opened on.
    pub fn index(&self) -> &IndexName {
        &self.index
    }

    /// The keep-alive sent with every request that names it.
    pub fn keep_alive(&self) -> KeepAlive {
        self.keep_alive
    }

    /// The latest PIT id.
    pub fn id(&self) -> PitId {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn replace(&self, id: PitId) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = id;
    }
}

impl fmt::Debug for PointInTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointInTime")
            .field("index", &self.index)
            .field("keep_alive", &self.keep_alive)
            .field("id", &self.id())
            .finish()
    }
}

/// One sort clause: `{"<field>": "asc"}`, `{"<field>": "desc"}`, or a one-field object of the
/// caller's (`{"@timestamp": {"order": "asc", "format": ..}}`).
#[derive(Clone)]
pub struct Sort(Value);

impl Sort {
    /// `{"<field>": "asc"}`.
    pub fn asc(field: &str) -> Self {
        Self(Value::Object(
            [(field.to_owned(), Value::from("asc"))]
                .into_iter()
                .collect(),
        ))
    }

    /// `{"<field>": "desc"}`.
    pub fn desc(field: &str) -> Self {
        Self(Value::Object(
            [(field.to_owned(), Value::from("desc"))]
                .into_iter()
                .collect(),
        ))
    }

    /// A clause written by the caller: a JSON object with exactly one field.
    pub fn clause(clause: Value) -> Result<Self, Error> {
        match &clause {
            Value::Object(fields) if fields.len() == 1 => Ok(Self(clause)),
            _ => Err(Error::Invalid(Invalid::Sort)),
        }
    }
}

impl Serialize for Sort {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl fmt::Debug for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sort(..)")
    }
}

/// What a `search_after` traversal asks for on every page: the same `query` and `sort` (the guide
/// requires them unchanged), and a bounded page size.
#[derive(Clone)]
pub struct PitSearch {
    query: Option<Value>,
    sort: Vec<Sort>,
    size: u32,
}

impl PitSearch {
    /// At least one sort clause and a page size of 1..=[`MAX_PAGE_SIZE`]. A PIT search adds the
    /// `_shard_doc` tiebreaker itself.
    pub fn new(sort: Vec<Sort>, page_size: u32) -> Result<Self, Error> {
        if sort.is_empty() {
            return Err(Error::Invalid(Invalid::Sort));
        }
        if page_size == 0 || page_size > MAX_PAGE_SIZE {
            return Err(Error::Invalid(Invalid::PageSize));
        }
        Ok(Self {
            query: None,
            sort,
            size: page_size,
        })
    }

    /// The query (a JSON object, sent as written); without one Elasticsearch matches all.
    pub fn with_query(mut self, query: Value) -> Result<Self, Error> {
        if !query.is_object() {
            return Err(Error::Invalid(Invalid::Query));
        }
        self.query = Some(query);
        Ok(self)
    }

    /// The page size.
    pub fn page_size(&self) -> u32 {
        self.size
    }

    /// The body of the page after `after` (the first page for `None`), naming the PIT's latest
    /// id, for the generated `search` endpoint.
    pub fn body(&self, pit: &PointInTime, after: Option<SortValues>) -> SearchBody {
        SearchBody {
            size: self.size,
            query: self.query.clone(),
            pit: PitRef {
                id: pit.id(),
                keep_alive: pit.keep_alive,
            },
            sort: self.sort.clone(),
            search_after: after,
            track_total_hits: false,
        }
    }
}

impl fmt::Debug for PitSearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PitSearch")
            .field("query", &self.query.as_ref().map(|_| ".."))
            .field("sort", &self.sort.len())
            .field("page_size", &self.size)
            .finish()
    }
}

/// The `sort` values of a hit, as the exact JSON array text Elasticsearch returned: the next
/// request's `search_after` is these bytes, never a re-encoding. `Debug` shows the length only.
#[derive(Clone)]
pub struct SortValues(Arc<RawValue>);

impl SortValues {
    /// The JSON array text.
    pub fn as_str(&self) -> &str {
        self.0.get()
    }

    fn is_nonempty_array(&self) -> bool {
        serde_json::from_str::<Vec<IgnoredAny>>(self.0.get()).is_ok_and(|v| !v.is_empty())
    }
}

impl PartialEq for SortValues {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl fmt::Debug for SortValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SortValues(<{} bytes>)", self.as_str().len())
    }
}

impl Serialize for SortValues {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SortValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Box::<RawValue>::deserialize(deserializer).map(|raw| Self(Arc::from(raw)))
    }
}

/// The body of a PIT search request (the generated `search` endpoint's input).
#[derive(Serialize)]
pub struct SearchBody {
    size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<Value>,
    pit: PitRef,
    sort: Vec<Sort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_after: Option<SortValues>,
    track_total_hits: bool,
}

impl fmt::Debug for SearchBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchBody")
            .field("size", &self.size)
            .field("pit", &self.pit)
            .field("search_after", &self.search_after)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Serialize)]
struct PitRef {
    id: PitId,
    keep_alive: KeepAlive,
}

/// The open-PIT response: `{"id": .., "_shards": ..}`.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct OpenPitResponse {
    /// The PIT id.
    pub id: PitId,
}

/// The close-PIT request body: `{"id": ..}`.
#[derive(Debug, Serialize)]
pub struct ClosePitBody {
    id: PitId,
}

impl ClosePitBody {
    pub(crate) fn new(id: PitId) -> Self {
        Self { id }
    }
}

/// The close-PIT response. The pagination guide does not show it; both fields are read when
/// present and are `None` otherwise.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct ClosePitResponse {
    /// `succeeded`.
    #[serde(default)]
    pub succeeded: Option<bool>,
    /// `num_freed`.
    #[serde(default)]
    pub num_freed: Option<u64>,
}

/// One search response page (the generated `search` endpoint's output).
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct SearchResponse {
    /// The updated PIT id.
    #[serde(default)]
    pub pit_id: Option<PitId>,
    /// Milliseconds.
    pub took: u64,
    /// Whether the search timed out (its hits may then be incomplete).
    pub timed_out: bool,
    /// Shard counts.
    #[serde(rename = "_shards", default)]
    pub shards: Option<Shards>,
    /// The hits envelope.
    pub hits: Hits,
}

/// A response's `_shards`.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Shards {
    /// `total`.
    #[serde(default)]
    pub total: u64,
    /// `successful`.
    #[serde(default)]
    pub successful: u64,
    /// `failed`.
    #[serde(default)]
    pub failed: u64,
}

/// A response's `hits`.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Hits {
    /// The page's hits, in sort order.
    pub hits: Vec<Hit>,
}

/// One hit. `Debug` shows its index, id and score, never its source or sort values.
#[derive(Deserialize)]
pub struct Hit {
    #[serde(rename = "_index")]
    index: String,
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "_score", default)]
    score: Option<f64>,
    #[serde(rename = "_source", default)]
    source: Option<Box<RawValue>>,
    #[serde(default)]
    sort: Option<SortValues>,
}

impl Hit {
    /// `_index`.
    pub fn index(&self) -> &str {
        &self.index
    }

    /// `_id`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// `_score` (`null` when sorting by fields).
    pub fn score(&self) -> Option<f64> {
        self.score
    }

    /// `_source` as JSON text.
    pub fn source_json(&self) -> Option<&str> {
        self.source.as_deref().map(RawValue::get)
    }

    /// `_source` decoded as `T`. The error is serde_json's: it can quote the source.
    pub fn source<T: serde::de::DeserializeOwned>(&self) -> Option<Result<T, serde_json::Error>> {
        self.source
            .as_deref()
            .map(|raw| serde_json::from_str(raw.get()))
    }

    /// The hit's `sort` values.
    pub fn sort(&self) -> Option<&SortValues> {
        self.sort.as_ref()
    }
}

impl fmt::Debug for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hit")
            .field("index", &self.index)
            .field("id", &self.id)
            .field("score", &self.score)
            .finish_non_exhaustive()
    }
}

/// Checks one page and turns it into the traversal's page: the latest PIT id first (so that the
/// PIT can be closed by it even if the page is then refused), then completeness, the page size,
/// and each hit's sort values. The next key is the last hit's sort values; an empty page ends the
/// traversal.
pub(crate) fn page(
    response: SearchResponse,
    search: &PitSearch,
    pit: &PointInTime,
    after: Option<&SortValues>,
) -> Result<concord_pagination::Page<Hit, SortValues>, Error> {
    match response.pit_id {
        Some(id) if !id.as_str().is_empty() => pit.replace(id),
        _ => return Err(Error::Protocol(Protocol::MissingPitId)),
    }
    let shards_failed = response.shards.as_ref().map_or(0, |s| s.failed);
    if response.timed_out || shards_failed > 0 {
        return Err(Error::PartialSearch {
            shards_failed,
            timed_out: response.timed_out,
        });
    }
    let hits = response.hits.hits;
    if hits.len() > search.size as usize {
        return Err(Error::Protocol(Protocol::TooManyHits {
            requested: search.size,
            returned: hits.len(),
        }));
    }
    if let Some(position) = hits
        .iter()
        .position(|hit| !hit.sort.as_ref().is_some_and(SortValues::is_nonempty_array))
    {
        return Err(Error::Protocol(Protocol::MissingSort { position }));
    }
    let next = hits.last().and_then(|hit| hit.sort.clone());
    if next.is_some() && next.as_ref() == after {
        return Err(Error::Protocol(Protocol::StalledCursor));
    }
    Ok(concord_pagination::Page::new(hits, next))
}
