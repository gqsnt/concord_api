//! Request inputs: the bounded time range, sort order, page size, cursor and the search body.

use std::fmt;
use std::num::NonZeroU32;

use concord::{QueryPairs, QueryPairsWriter};
use serde::{Serialize, Serializer};

/// A closed time range in milliseconds since the Unix epoch, `from <= to`.
///
/// On `GET /api/v2/events` it is sent as `filter[from]` and `filter[to]` (both or neither: the
/// one value writes both keys); in a search body as `filter.from` and `filter.to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeRange {
    from: u64,
    to: u64,
}

impl TimeRange {
    /// The range `from..=to`, in milliseconds since the Unix epoch.
    ///
    /// # Errors
    ///
    /// [`InputError::InvertedRange`] when `from > to` (two same-typed arguments are easy to
    /// swap; an inverted range would select nothing).
    pub fn from_millis(from: u64, to: u64) -> Result<Self, InputError> {
        if from > to {
            return Err(InputError::InvertedRange);
        }
        Ok(Self { from, to })
    }

    /// The lower bound, in milliseconds.
    pub fn from_ms(&self) -> u64 {
        self.from
    }

    /// The upper bound, in milliseconds.
    pub fn to_ms(&self) -> u64 {
        self.to
    }
}

impl QueryPairs for TimeRange {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        out.pair("filter[from]", self.from);
        out.pair("filter[to]", self.to);
    }
}

/// The order of events in results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// `timestamp`: oldest first.
    Ascending,
    /// `-timestamp`: newest first.
    Descending,
}

impl Sort {
    fn wire(self) -> &'static str {
        match self {
            Sort::Ascending => "timestamp",
            Sort::Descending => "-timestamp",
        }
    }
}

serde_plain::derive_display_from_serialize!(Sort);

impl Serialize for Sort {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire())
    }
}

/// The maximum number of events per response (`page[limit]`, `page.limit`; provider default
/// 10). Search documents it as an `int32`, so it is `1..=i32::MAX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PageLimit(NonZeroU32);

impl PageLimit {
    /// A page size of `limit` events.
    ///
    /// # Errors
    ///
    /// [`InputError::PageLimit`] for 0 or a value above `i32::MAX`.
    pub fn new(limit: u32) -> Result<Self, InputError> {
        NonZeroU32::new(limit)
            .filter(|n| i32::try_from(n.get()).is_ok())
            .map(Self)
            .ok_or(InputError::PageLimit)
    }

    /// The number of events.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for PageLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// An opaque continuation cursor (`meta.page.after`), sent back as `page[cursor]` or
/// `page.cursor`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Cursor(String);

impl Cursor {
    /// A cursor with this text, exactly as the provider sent it.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The cursor's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

serde_plain::derive_display_from_serialize!(Cursor);

/// The body of `POST /api/v2/events/search`: a filter, options, page settings and a sort.
///
/// Fields left unset are not sent, so the provider's defaults apply (`filter.query` `*`,
/// `filter.from` `now-15m`, `filter.to` `now`, `options.timezone` `UTC`, `page.limit` 10).
///
/// ```
/// use concord_sdk_datadog_events::{PageLimit, SearchRequest, Sort};
///
/// // The documentation's first example body.
/// let request = SearchRequest::new()
///     .query("datadog-agent")
///     .from("2020-09-17T11:48:36+01:00")
///     .to("2020-09-17T12:48:36+01:00")
///     .sort(Sort::Ascending)
///     .limit(PageLimit::new(5).unwrap());
/// # let _ = request;
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SearchRequest {
    #[serde(skip_serializing_if = "SearchFilter::is_empty")]
    filter: SearchFilter,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<SearchOptions>,
    #[serde(skip_serializing_if = "PageOptions::is_empty")]
    page: PageOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Sort>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct SearchFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

impl SearchFilter {
    fn is_empty(&self) -> bool {
        self.query.is_none() && self.from.is_none() && self.to.is_none()
    }
}

/// "Either provide a timezone or a time offset but not both, otherwise the query fails":
/// one value, so both cannot be sent.
#[derive(Clone, Debug, PartialEq, Serialize)]
enum SearchOptions {
    #[serde(rename = "timezone")]
    Timezone(String),
    #[serde(rename = "timeOffset")]
    TimeOffset(i64),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct PageOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<Cursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<PageLimit>,
}

impl PageOptions {
    fn is_empty(&self) -> bool {
        self.cursor.is_none() && self.limit.is_none()
    }
}

impl SearchRequest {
    /// An empty search: every field at the provider's default.
    pub fn new() -> Self {
        Self::default()
    }

    /// `filter.query`: a search query in Datadog's event search syntax, sent as written.
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.filter.query = Some(query.into());
        self
    }

    /// `filter.from`: date math (`now-15m`), a timestamp in milliseconds, or a date-time,
    /// sent as written.
    pub fn from(mut self, from: impl Into<String>) -> Self {
        self.filter.from = Some(from.into());
        self
    }

    /// `filter.to`, like [`from`](Self::from).
    pub fn to(mut self, to: impl Into<String>) -> Self {
        self.filter.to = Some(to.into());
        self
    }

    /// `filter.from` and `filter.to` from a range in milliseconds.
    pub fn range(self, range: TimeRange) -> Self {
        self.from(range.from.to_string()).to(range.to.to_string())
    }

    /// `options.timezone` (`GMT`, `UTC+1`, `America/New_York`); replaces a time offset.
    pub fn timezone(mut self, timezone: impl Into<String>) -> Self {
        self.options = Some(SearchOptions::Timezone(timezone.into()));
        self
    }

    /// `options.timeOffset` in seconds; replaces a timezone.
    pub fn time_offset(mut self, seconds: i64) -> Self {
        self.options = Some(SearchOptions::TimeOffset(seconds));
        self
    }

    /// `sort`.
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = Some(sort);
        self
    }

    /// `page.limit`.
    pub fn limit(mut self, limit: PageLimit) -> Self {
        self.page.limit = Some(limit);
        self
    }

    /// `page.cursor`: continue a previous search (the cursor came from its `meta.page.after`).
    /// The pagination adapter sets it itself.
    pub fn cursor(self, cursor: Cursor) -> Self {
        self.maybe_cursor(Some(cursor))
    }

    pub(crate) fn maybe_cursor(mut self, cursor: Option<Cursor>) -> Self {
        self.page.cursor = cursor;
        self
    }
}

/// An input the SDK refuses before building a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_input_error)]
pub enum InputError {
    /// A time range whose lower bound is after its upper bound.
    InvertedRange,
    /// A page size of 0 or above `i32::MAX`.
    PageLimit,
    /// Event text exceeds the v1 limit of 4,000 characters.
    EventTextTooLong,
    /// Event aggregation key exceeds the v1 limit of 100 characters.
    AggregationKeyTooLong,
}

fn display_input_error(value: &InputError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match value {
        InputError::InvertedRange => "the time range's lower bound is after its upper bound",
        InputError::PageLimit => "the page limit must be between 1 and 2147483647",
        InputError::EventTextTooLong => "event text must not exceed 4000 characters",
        InputError::AggregationKeyTooLong => "event aggregation key must not exceed 100 characters",
    })
}
