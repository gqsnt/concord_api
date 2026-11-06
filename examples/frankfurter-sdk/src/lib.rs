#![doc = include_str!("../README.md")]

mod error;
mod model;

use concord::{Endpoint, Json, SendWith};

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
    /// the operations themselves declare `/v2`.
    ///
    /// Every request's `send()` returns `Result<`[`RateResponse`]`, `[`Error`]`>` (this
    /// crate's error, through [`SendPolicy`]): typed provider errors and response metadata.
    /// Two calls return [`concord::Error`] instead: `with_base_url` and `prepare()` (no
    /// request is sent). Signatures show both as a bare `Error`; this crate's [`Error`]
    /// implements `From<concord::Error>`, so `?` converts.
    pub client Frankfurter() {
        base "https://api.frankfurter.dev";
        send SendPolicy;
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

/// The send policy of [`Frankfurter`] (`send SendPolicy;`): what every generated `send()`
/// returns. Callers never name it.
///
/// `send()` executes once and returns a complete rate or this crate's [`Error`] with the
/// metadata available: any non-2xx status is [`Error::Provider`] with a bounded typed error
/// document; build, transport, success media-type and success decode failures are
/// [`Error::Concord`]. No retries or partial rate are returned. Build and pre-head transport
/// errors have no metadata.
#[derive(Default)]
pub struct SendPolicy;

impl SendWith<Rate> for SendPolicy {
    type Output = Result<RateResponse, Error>;

    async fn send<E: Endpoint<Rate> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            // Status is authoritative even when the document is absent, invalid or inconsistent.
            let (meta, document) = received.document::<Json, Problem>(ERROR_BODY_LIMIT).await;
            let document = document.map_err(ErrorBodyError::from);
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        // A success-body failure keeps the head in the Concord error (`Error::head_meta`).
        let (rate, meta) = received.decode_with_meta().await?;
        Ok(RateResponse { rate, meta })
    }
}
