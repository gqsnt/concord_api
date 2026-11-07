//! A bounded FRED v1 slice. All examples and tests use local fixtures.
//!
//! Navigate [`Fred::series`] for metadata, XML/JSON observations and zipped CSV.
//! Generated `send()` uses [`Policy`]; native `prepare()` exits bypass that policy.
//! See the package README for the exact limits and exclusions.

mod codecs;
mod error;
mod models;
mod options;
mod pagination;

pub use concord;
pub use concord_pagination;
pub use concord_retry;
pub use error::{Error, HttpError, Problem};
pub use models::{CsvExport, Metadata, Observation, Observations, SearchPage, Series};
pub use options::{AggregationMethod, Frequency, Units};
pub use pagination::{OffsetEnvelope, pages};

use codecs::{ProblemDocument, Xml, ZippedCsv};
use concord::{Endpoint, SendWith};

/// Maximum HTTP success body, including a compressed ZIP: 1 MiB.
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Maximum combined uncompressed ZIP payload: 4 MiB.
pub const EXTRACTED_LIMIT: usize = 4 * 1024 * 1024;
/// Maximum entries in a ZIP (including README).
pub const ENTRY_LIMIT: usize = 8;
/// Maximum provider error document: 16 KiB.
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

/// One attempt per call, bounded bodies, provider errors, and no automatic retry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy;

impl<T: Send + 'static> SendWith<T> for Policy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let retry_after = concord_retry::http::retry_after(
                received
                    .head()
                    .headers()
                    .get_all("retry-after")
                    .iter()
                    .map(|v| v.as_bytes()),
                std::time::SystemTime::now(),
            );
            let (meta, document) = received
                .document::<ProblemDocument, Problem>(ERROR_BODY_LIMIT)
                .await;
            return Err(Error::Http(Box::new(HttpError {
                meta,
                document,
                retry_after,
            })));
        }
        Ok(received.decode().await?)
    }
}

/// Transport suitable for a query credential: no redirects or referrer, 30-second timeout.
/// Applications may pass their own transport with equivalent restrictions.
pub fn http_client() -> Result<concord::reqwest::Client, concord::reqwest::Error> {
    concord::reqwest::Client::builder()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .referer(false)
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

concord::api! {
    /// FRED v1 public data, authenticated with a query API key.
    pub client Fred(#[sensitive] query api_key: String) {
        base "https://api.stlouisfed.org";
        path "fred";
        response Xml;
        limit BODY_LIMIT;
        send Policy;

        /// Shared offset parameter for pageable operations.
        params OffsetOptions(query offset: u64 = 0);

        /// Validated observation controls, omitted until explicitly set.
        /// FRED documents frequency aggregation for XML/JSON only; CSV forwards
        /// these same controls without promising provider-side aggregation.
        params ObservationOptions(
            query units?: Units,
            query frequency?: Frequency,
            query aggregation_method?: AggregationMethod,
        );

        /// Search series using FRED's default XML representation.
        GET search(query search_text: String, query limit: SearchLimit = SearchLimit::default(), ..OffsetOptions) -> SearchPage {
            path "series/search";
        }
        /// Search series as JSON.
        GET search_json(query search_text: String, query limit: SearchLimit = SearchLimit::default(), ..OffsetOptions) -> SearchPage {
            path "series/search";
            query { "file_type" = "json"; }
            response concord::Json;
        }

        /// One selected series, identified by its query parameter.
        scope series(query series_id: String) {
            path "series";
            /// Retrieve the series metadata as XML.
            GET metadata() -> Metadata;
            /// Retrieve the series metadata as JSON.
            GET metadata_json() -> Metadata {
                query { "file_type" = "json"; }
                response concord::Json;
            }
            /// Read observations as XML (no file_type parameter means XML).
            GET observations(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> Observations {
                path "observations";
            }
            /// Read observations as JSON, preserving every value as a string.
            GET observations_json(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> Observations {
                path "observations";
                query { "file_type" = "json"; }
                response concord::Json;
            }
            /// Export a ZIP with one CSV and an optional text README, without CSV coercion.
            /// Observation controls are forwarded; FRED documents frequency
            /// aggregation only for XML/JSON, not for CSV exports.
            GET export_csv(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> CsvExport {
                path "observations";
                query { "file_type" = "csv"; }
                response ZippedCsv;
            }
        }
    }
}

macro_rules! limit {
    ($name:ident, $max:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name(u32);
        impl $name {
            /// Validate the provider's inclusive range; nothing is sent on error.
            pub fn new(value: u32) -> Result<Self, Error> {
                if (1..=$max).contains(&value) {
                    Ok(Self(value))
                } else {
                    Err(Error::InvalidLimit { maximum: $max })
                }
            }
            /// The validated wire value.
            pub const fn get(self) -> u32 {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self($max)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
limit!(SearchLimit, 1000, "A search page size in 1..=1000.");
limit!(
    ObservationLimit,
    100000,
    "An observations page size in 1..=100000."
);

/// Compiles the public README examples without making provider requests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeExamples;
