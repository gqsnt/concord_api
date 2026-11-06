//! A bounded NASA NeoWs feed, browse and lookup client.
//!
//! Generated operations use one request and limit successful JSON bodies to [`BODY_LIMIT`].
//! See the package README for examples, supported fields, and disclosure boundaries.

use concord::{Endpoint, Json, SendWith};
use serde::Deserialize;
use std::{collections::BTreeMap, fmt};

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

/// Safe-to-display failure. It deliberately retains no provider body, URL, headers, or source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Concord failure stage.
    pub kind: concord::ErrorKind,
    /// HTTP status if a response was received.
    pub status: Option<concord::reqwest::StatusCode>,
}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self {
            kind: error.kind(),
            status: error.head().map(|head| head.status()),
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
        Ok(endpoint
            .prepare()?
            .receive()
            .await?
            .decode_limited(BODY_LIMIT)
            .await?)
    }
}

/// Compiles the public usage example.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
