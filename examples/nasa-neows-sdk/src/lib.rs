//! A bounded NASA NeoWs feed, browse and lookup client.
//!
//! Generated operations use one request and limit successful JSON bodies to [`BODY_LIMIT`].
//! Every result reports the per-key quota NASA sent ([`RateLimit`]): failures in
//! [`Error::rate_limit`], successes through [`SendMetered::send_metered`].
//! See the package README for examples, supported fields, and disclosure boundaries.

use concord::reqwest::header::{HeaderMap, HeaderName};
use concord::{Endpoint, Json, SendWith, Sends};
use serde::Deserialize;
use std::{collections::BTreeMap, fmt, future::Future};

/// Maximum decoded response body size in bytes, after HTTP decompression.
pub const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// The subset of a near-Earth object needed to identify and inspect it.
/// Additional provider fields are ignored, including links that can contain an API key.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Asteroid {
    /// NeoWs identifier, suitable for lookup.
    pub id: String,
    /// Human-readable designation.
    pub name: String,
    /// NASA's potentially hazardous classification, when supplied.
    pub is_potentially_hazardous_asteroid: Option<bool>,
}

/// Asteroids grouped by close-approach date.
#[derive(Clone, Debug, Deserialize)]
pub struct Feed {
    /// Number of objects reported by the service.
    pub element_count: u64,
    /// Objects keyed by date (`YYYY-MM-DD`).
    pub near_earth_objects: BTreeMap<String, Vec<Asteroid>>,
}

/// A single browse result page.
#[derive(Clone, Debug, Deserialize)]
pub struct BrowsePage {
    /// Objects on this page.
    pub near_earth_objects: Vec<Asteroid>,
    /// Provider pagination metadata.
    pub page: PageInfo,
}

/// Browse page counters reported by NeoWs.
#[derive(Clone, Debug, Deserialize)]
pub struct PageInfo {
    /// Requested page size.
    pub size: u64,
    /// Total number of objects.
    pub total_elements: u64,
    /// Total number of pages.
    pub total_pages: u64,
    /// Zero-based page number.
    pub number: u64,
}

concord::api! {
    /// NASA NeoWs client. Supply your API key in [`NeoWsConfig`].
    pub client NeoWs(#[sensitive] api_key: String) {
        base "https://api.nasa.gov/neo/rest/v1";
        query { "api_key" = api_key; }
        format Json;
        send Bounded;
        /// The supported NeoWs operations.
        scope neows {
            /// Query objects approaching Earth between two dates, inclusive as interpreted by NASA.
            GET feed(query start_date: String, query end_date: String) -> Feed { path "feed"; }
            /// Look up a returned NeoWs identifier.
            GET lookup(id: String) -> Asteroid { path "neo/{id}"; }
            /// Fetch a browse page; omit options for server defaults.
            GET browse(query page?: u64, query size?: u64) -> BrowsePage { path "neo/browse"; }
        }
    }
}

const X_RATELIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");
const X_RATELIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");

/// One quota header of one response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reported {
    /// The response had no such header, or no response arrived (then [`Error::status`] is
    /// `None`).
    #[default]
    Absent,
    /// The header was present but was not a single header field of ASCII decimal digits that
    /// fits in a `u64` (repeated, empty, signed, fractional, non-ASCII or too large). Its text
    /// is not kept.
    Unparsable,
    /// The number NASA reported.
    Value(u64),
}

impl Reported {
    /// The reported number, if there is one.
    pub fn value(self) -> Option<u64> {
        match self {
            Self::Value(value) => Some(value),
            Self::Absent | Self::Unparsable => None,
        }
    }

    fn read(headers: &HeaderMap, name: &HeaderName) -> Self {
        let mut fields = headers.get_all(name).iter();
        let Some(field) = fields.next() else {
            return Self::Absent;
        };
        if fields.next().is_some() {
            return Self::Unparsable;
        }
        match field.to_str().map(|text| text.trim_matches([' ', '\t'])) {
            Ok(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                text.parse().map_or(Self::Unparsable, Self::Value)
            }
            _ => Self::Unparsable,
        }
    }
}

/// The per-key quota api.nasa.gov reports on a response in `X-RateLimit-Limit` and
/// `X-RateLimit-Remaining` ("Web Service Rate Limits", <https://api.nasa.gov/>).
///
/// These are the numbers of this one response, each parsed on its own; no header text is kept,
/// so a header can never carry the API key into this value or its `Debug`. NASA documents only
/// these two headers, counters that reset on a rolling hourly basis and limits that vary by
/// service: nothing here is a default, a reset time or a pacing policy. `DEMO_KEY` is limited
/// per IP address, more strictly than a registered key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateLimit {
    /// `X-RateLimit-Limit`: requests allowed in the window.
    pub limit: Reported,
    /// `X-RateLimit-Remaining`: requests left in the window.
    pub remaining: Reported,
}

impl RateLimit {
    /// Reads both headers of a response head (also usable on a response obtained natively).
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            limit: Reported::read(headers, &X_RATELIMIT_LIMIT),
            remaining: Reported::read(headers, &X_RATELIMIT_REMAINING),
        }
    }
}

/// A successful value with the quota reported on its response.
#[derive(Clone, Debug)]
pub struct Metered<T> {
    /// The decoded value, as `send()` returns it.
    pub value: T,
    /// The response's quota headers.
    pub rate_limit: RateLimit,
}

/// Safe-to-display failure. It retains no provider body, URL, header text, or source; only the
/// two parsed quota numbers of the response, if one arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Concord failure stage.
    pub kind: concord::ErrorKind,
    /// HTTP status if a response was received.
    pub status: Option<concord::reqwest::StatusCode>,
    /// The quota headers of the failed response, a key or quota refusal included, whatever
    /// stage failed after it arrived. Both [`Reported::Absent`] when no response arrived.
    pub rate_limit: RateLimit,
}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        let head = error.head();
        Self {
            kind: error.kind(),
            status: head.map(|head| head.status()),
            rate_limit: head.map_or_else(RateLimit::default, |head| {
                RateLimit::from_headers(head.headers())
            }),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NeoWs {:?} failure", self.kind)?;
        if let Some(status) = self.status {
            write!(f, " (HTTP {})", status.as_u16())?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

/// Send policy: execute once, enforce the body limit, and discard sensitive diagnostics.
#[derive(Default)]
pub struct Bounded;
impl<T: Send> SendWith<T> for Bounded {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        metered(endpoint).await.map(|metered| metered.value)
    }
}

/// `send()` that also returns the quota of the successful response. Implemented for every
/// operation whose generated `send()` returns this crate's `Result<T, Error>`: every NeoWs
/// endpoint, with no per-endpoint code. Same single request, body limit and errors as `send()`.
///
/// Another Concord client's endpoints do not get it:
///
/// ```compile_fail,E0599
/// use concord_sdk_nasa_neows::SendMetered;
/// concord::api! {
///     pub client Other { base "http://other.test"; GET item() -> String { path "x"; response concord::Text; } }
/// }
/// async fn run(other: Other) { let _ = other.item().send_metered().await; }
/// ```
pub trait SendMetered<T>: Sized {
    /// Sends once; on success, the value and its response's [`RateLimit`].
    ///
    /// # Errors
    ///
    /// As `send()`; the [`Error`] carries the failed response's quota.
    fn send_metered(self) -> impl Future<Output = Result<Metered<T>, Error>> + Send;
}
impl<T, E> SendMetered<T> for E
where
    T: Send,
    E: Endpoint<T> + Sends<Result<T, Error>> + Send,
{
    fn send_metered(self) -> impl Future<Output = Result<Metered<T>, Error>> + Send {
        metered(self)
    }
}

// The one send path: the quota is copied from the head before the body is read; a failure
// after the head arrived carries it through `From<concord::Error>`.
async fn metered<T: Send, E: Endpoint<T> + Send>(endpoint: E) -> Result<Metered<T>, Error> {
    let received = endpoint.prepare()?.receive().await?;
    let rate_limit = RateLimit::from_headers(received.head().headers());
    let value = received.decode_limited(BODY_LIMIT).await?;
    Ok(Metered { value, rate_limit })
}

/// Compiles the public usage example.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
