#![doc = include_str!("../README.md")]

mod error;
mod model;

use concord::{Decode, Endpoint, Json};

pub use bigdecimal::BigDecimal as Decimal;
pub use chrono::NaiveDate as Date;
pub use error::{Error, ErrorBodyError, Problem, ProviderError, ResponseMeta};
pub use model::Rate;

/// OpenAPI `info.version` inspected on 2026-11-06; the route family is v2.
pub const API_VERSION: &str = "2.1.1";

/// Maximum bytes retained while reading a non-2xx error document (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

concord::api! {
    /// Unauthenticated Frankfurter v2 pair-rate client.
    ///
    /// Construct with `Frankfurter::new(http)`. Transport timeouts, redirects and default
    /// headers belong to the supplied `concord::reqwest::Client`. No config record or token
    /// is needed. `with_base_url(origin)` replaces the origin, retaining any path prefix;
    /// the operations themselves declare `/v2`. Use `fetch()` for typed provider errors
    /// and response metadata; generated `send()` returns just `Rate` and Concord errors.
    pub client Frankfurter() {
        base "https://api.frankfurter.dev";
        format Json;

        /// Latest available observation for one pair, not necessarily today's date.
        ///
        /// Currency strings are sent as supplied, each as one encoded path segment.
        /// `set_providers("ECB,TCMB")` optionally selects provider keys; availability and
        /// validation belong to the provider. Omission leaves the provider default intact.
        GET latest_pair(base: String, quote: String, query providers?: String) -> Rate {
            path "v2/rate/{base}/{quote}";
        }

        /// Observation for the requested calendar date; inspect the returned observation date.
        ///
        /// `date` is required and sent as `YYYY-MM-DD`. The optional `providers` filter has
        /// the same spelling as on `latest_pair` and requires no change to existing callers.
        GET historical_pair(base: String, quote: String, query date: Date, query providers?: String) -> Rate {
            path "v2/rate/{base}/{quote}";
        }
    }
}

/// One fully decoded rate and the head of the response that supplied it.
#[derive(Clone, Debug)]
pub struct RateResponse {
    /// Provider observation, with exact decimal value.
    pub rate: Rate,
    /// Actual status, HTTP version, headers and declared operation identity.
    pub meta: ResponseMeta,
}

impl FrankfurterLatestPair {
    /// Executes once, returning a complete rate or a typed error with available metadata.
    ///
    /// No retries or partial rate are returned. See [`Error`] and the crate documentation
    /// for body/completion semantics. Build and pre-head transport errors have no metadata.
    pub async fn fetch(self) -> Result<RateResponse, Error> {
        fetch(self).await
    }
}

impl FrankfurterHistoricalPair {
    /// Executes once, returning a complete historical rate or a typed error.
    ///
    /// Same error and completion semantics as [`FrankfurterLatestPair::fetch`].
    pub async fn fetch(self) -> Result<RateResponse, Error> {
        fetch(self).await
    }
}

async fn fetch(endpoint: impl Endpoint<Rate>) -> Result<RateResponse, Error> {
    let received = endpoint.prepare()?.receive().await?;
    let meta = ResponseMeta::capture(&received.head(), received.operation());
    if !received.status().is_success() {
        // Status is authoritative even when the document is absent, invalid or inconsistent.
        let document = match <Json as Decode<Problem>>::check_content_type(&received.head()) {
            Err(error) => Err(ErrorBodyError::Metadata(error)),
            Ok(()) => match received.bytes_limited(ERROR_BODY_LIMIT).await {
                Err(error) => Err(ErrorBodyError::Read(error)),
                Ok(bytes) => <Json as Decode<Problem>>::decode_body((), bytes)
                    .map_err(ErrorBodyError::Decode),
            },
        };
        return Err(Error::Provider(Box::new(ProviderError { meta, document })));
    }
    match received.decode().await {
        Ok(rate) => Ok(RateResponse { rate, meta }),
        Err(source) => Err(Error::Concord {
            source,
            meta: Some(Box::new(meta)),
        }),
    }
}
