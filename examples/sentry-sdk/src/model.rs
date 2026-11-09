use crate::RateLimits;
use concord::ResponseMeta;
use concord_retry::concord::ReceiveReport;
use serde::Deserialize;
use std::fmt;

/// The stable issue fields used by this slice; extra provider fields are ignored.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct Issue {
    /// Issue identifier for subsequent reads.
    pub id: String,
    /// Human-readable identifier.
    pub short_id: String,
    /// Provider title (may contain application secrets).
    pub title: String,
    /// Open provider status vocabulary, retained as text.
    pub status: String,
}

/// Event summary or detail. `entries` is present on full event details.
#[derive(Clone, Deserialize, concord::Debug)]
#[sensitive]
pub struct Event {
    /// Storage identifier; use `event_id` to retrieve the event.
    pub id: String,
    /// Sentry event identifier, distinct from the storage `id`.
    #[serde(rename = "eventID")]
    pub event_id: String,
    /// Owning issue.
    #[serde(rename = "groupID")]
    pub group_id: String,
    /// Provider title.
    pub title: String,
    /// Full event entries; empty on summaries that omit them.
    #[serde(default)]
    pub entries: Vec<serde_json::Value>,
}

/// Fully decoded value, response head, Sentry quotas and retry accounting.
pub struct Response<T> {
    /// Explicit access to provider content.
    pub value: T,
    /// Status, headers and endpoint identity. Native header access is unredacted.
    pub meta: ResponseMeta,
    /// Continuation selected by the results flag; never a URL to follow.
    pub next_cursor: Option<String>,
    /// Separate frequency and concurrency observations.
    pub rates: RateLimits,
    /// Dispatch accounting, excluding admission hooks and body decoding.
    pub retry: ReceiveReport,
}

impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .field("has_next", &self.next_cursor.is_some())
            .field("rates", &self.rates)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}
