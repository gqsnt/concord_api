use crate::{Error, ProviderError, RetryAfterError, SMALL_BODY_LIMIT, error::Problem};
use concord::{Delivery, Endpoint, Json, Prepared, SendWith, reqwest::StatusCode};
use std::{
    fmt,
    time::{Duration, SystemTime},
};

/// A strict numeric header observation; invalid and absent remain distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderNumber {
    /// Header absent.
    Missing,
    /// Exactly one unsigned integer field.
    Value(u64),
    /// Malformed, repeated or overflowing field.
    Invalid,
}

/// Response metadata. Raw header values require explicit access.
#[derive(Debug)]
pub struct Metadata(pub concord::ResponseMeta);

impl Metadata {
    /// Authoritative HTTP status.
    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    /// Mailgun's total calls per window, if present and valid.
    pub fn rate_limit(&self) -> HeaderNumber {
        self.number("x-ratelimit-limit")
    }

    /// Mailgun's remaining calls in the window.
    pub fn rate_remaining(&self) -> HeaderNumber {
        self.number("x-ratelimit-remaining")
    }

    /// Provider reset header as Unix milliseconds, without a guessed delay or clock conversion.
    pub fn rate_reset_millis(&self) -> HeaderNumber {
        self.number("x-ratelimit-reset")
    }

    /// Optional standard Retry-After hint. This never schedules or authorizes a retry.
    pub fn retry_after(&self, now: SystemTime) -> Result<Option<Duration>, RetryAfterError> {
        concord_retry::http::retry_after(
            self.0
                .headers()
                .get_all("retry-after")
                .iter()
                .map(|v| v.as_bytes()),
            now,
        )
    }

    fn number(&self, name: &str) -> HeaderNumber {
        match concord::field::unsigned(self.0.headers(), name) {
            Ok(None) => HeaderNumber::Missing,
            Ok(Some(value)) => HeaderNumber::Value(value),
            Err(_) => HeaderNumber::Invalid,
        }
    }
}

/// Typed response with its retained head. Debug withholds the payload.
pub struct Response<T> {
    /// Provider data; a send response establishes acceptance only.
    pub value: T,
    /// HTTP status, operation identity and headers.
    pub meta: Metadata,
}

impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// One execution, bounded decoding, no retry or auth-refresh loop.
#[derive(Clone, Copy, Debug, Default)]
pub struct SendPolicy;

impl<T: Send> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        Self::execute(endpoint.prepare()?).await
    }
}

impl SendPolicy {
    pub(crate) async fn execute<T>(prepared: Prepared<T>) -> Result<Response<T>, Error> {
        let write = prepared.request().method() == concord::reqwest::Method::POST;
        let failure = |e: concord::Error| {
            if write && e.delivery() != Delivery::NotSent {
                Error::AmbiguousSend(Box::new(e))
            } else {
                Error::from(e)
            }
        };
        let received = prepared.receive().await.map_err(failure)?;
        if !received.status().is_success() {
            let (meta, document) = received.document::<Json, Problem>(SMALL_BODY_LIMIT).await;
            return Err(Error::Provider(Box::new(ProviderError {
                meta: Metadata(meta),
                document: document.map_err(Box::new),
            })));
        }
        let (value, meta) = received.decode_with_meta().await.map_err(failure)?;
        Ok(Response {
            value,
            meta: Metadata(meta),
        })
    }
}
