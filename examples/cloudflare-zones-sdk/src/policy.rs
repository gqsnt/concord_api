//! The client's send policy: what every generated `send()` does.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use concord::reqwest::StatusCode;
use concord::{Endpoint, Json, Prepared, Received, SendWith};
use concord_retry::concord::ReplayMode;
use concord_retry::{Decision, Replay, Retry};

use crate::ERROR_BODY_LIMIT;
use crate::error::{EnvelopeError, EnvelopeProblem, Error, ProviderError, ResponseMeta};
use crate::ratelimit::RetryAfter;
use crate::wire::{ErrorEnvelope, PageInfo, Paged, ResponseInfo, Single};

/// A successful single-object call (Zone Details, DNS Record Details).
#[derive(Debug)]
#[non_exhaustive]
pub struct Response<R> {
    /// `result`.
    pub result: R,
    /// `messages`.
    pub messages: Vec<ResponseInfo>,
    /// `errors` sent beside `success: true`; `success` is authoritative, so they do not fail
    /// the call.
    pub errors: Vec<ResponseInfo>,
    /// The response head (status, rate-limit headers).
    pub meta: ResponseMeta,
}

/// A successful page of a listing (List Zones, List DNS Records).
#[derive(Debug)]
#[non_exhaustive]
pub struct PageResponse<R> {
    /// `result`: this page's items.
    pub result: Vec<R>,
    /// `result_info`, as sent (`None` when absent). The page walkers require it.
    pub result_info: Option<PageInfo>,
    /// `messages`.
    pub messages: Vec<ResponseInfo>,
    /// `errors` sent beside `success: true`.
    pub errors: Vec<ResponseInfo>,
    /// The response head (status, rate-limit headers).
    pub meta: ResponseMeta,
}

/// What generated `send()` does: one request, then Cloudflare's envelope rules.
///
/// - A non-2xx status is [`Error::RateLimited`] (429) or [`Error::Provider`], with the error
///   envelope read up to [`ERROR_BODY_LIMIT`](crate::ERROR_BODY_LIMIT) when it is JSON.
/// - A 2xx is decoded as the envelope; `success: false` is [`Error::Provider`] with
///   Cloudflare's `errors`, never an empty result; `success: true` without `result` is
///   [`Error::Envelope`].
/// - By default a request is sent exactly once. With
///   [`retry_rate_limited`](Self::retry_rate_limited), a `GET` answered 429 with a valid
///   `Retry-After` within the configured maximum is sent again after that delay. No other
///   outcome is repeated, no local backoff is added, and no client-side quota is assumed:
///   Cloudflare's global and per-service limits are only observed
///   ([`ResponseMeta::rate_limits`]).
///
/// The policy is the client value `policy` (`send policy;`), replaced with
/// [`Cloudflare::set_policy`](crate::Cloudflare::set_policy). `prepare()` bypasses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendPolicy {
    attempts: NonZeroUsize,
    max_retry_after: Duration,
}

impl Default for SendPolicy {
    /// One attempt per call.
    fn default() -> Self {
        Self {
            attempts: NonZeroUsize::MIN,
            max_retry_after: Duration::ZERO,
        }
    }
}

impl SendPolicy {
    /// Honour `Retry-After` on a 429 for reads: at most `attempts` requests per call (the first
    /// included), each wait exactly the server's delay and at most `max_retry_after`. A longer,
    /// missing or invalid `Retry-After` returns the 429 at once.
    #[must_use]
    pub fn retry_rate_limited(mut self, attempts: NonZeroUsize, max_retry_after: Duration) -> Self {
        self.attempts = attempts;
        self.max_retry_after = max_retry_after;
        self
    }

    /// Requests per call, the first included.
    pub fn attempts(&self) -> NonZeroUsize {
        self.attempts
    }

    /// One call: the request (repeated only as documented), then the status rule.
    async fn receive<T: Send + 'static>(
        &self,
        prepared: Prepared<T>,
        read: bool,
    ) -> Result<(Received<T>, usize), Error> {
        let (outcome, attempts) = if self.attempts.get() == 1 || !read {
            (prepared.receive().await, 1)
        } else {
            let max = self.max_retry_after;
            let epoch = Instant::now();
            let run = Retry::new(concord_retry::Policy::new(self.attempts), Replay::Permitted)
                .receive(
                    prepared,
                    ReplayMode::BestEffort,
                    |_, _| {}, // the bearer token is fixed for the client handle
                    |outcome| classify(outcome, max),
                    move || epoch.elapsed(),
                    tokio::time::sleep,
                    |_, ceiling| ceiling, // zero: the server's delay is the only wait
                )
                .await;
            let attempts = run.report.attempts;
            (run.outcome.ok_or(Error::NoAttempt)?, attempts)
        };
        let received = outcome?;
        let status = received.status();
        if status.is_success() {
            return Ok((received, attempts));
        }
        let (meta, document) = received
            .document::<Json, ErrorEnvelope>(ERROR_BODY_LIMIT)
            .await;
        let error = Box::new(ProviderError::new(meta.into(), document, attempts));
        Err(if status == StatusCode::TOO_MANY_REQUESTS {
            Error::RateLimited(error)
        } else {
            Error::Provider(error)
        })
    }
}

/// A read is repeated only for a 429 whose `Retry-After` is valid and within `max`.
fn classify<T>(outcome: &Result<Received<T>, concord::Error>, max: Duration) -> Decision {
    match outcome {
        Ok(received) if received.status() == StatusCode::TOO_MANY_REQUESTS => {
            match RetryAfter::from_headers(received.head().headers()) {
                RetryAfter::After(delay) if delay <= max => Decision::RetryAfter(Some(delay)),
                _ => Decision::Stop,
            }
        }
        _ => Decision::Done,
    }
}

fn is_read<E: Endpoint<T>, T>() -> bool {
    matches!(E::operation().method(), Some("GET" | "HEAD"))
}

impl<R: Send + 'static> SendWith<Single<R>> for SendPolicy {
    type Output = Result<Response<R>, Error>;

    async fn send<E: Endpoint<Single<R>> + Send>(&self, endpoint: E) -> Self::Output {
        let read = is_read::<E, Single<R>>();
        let (received, attempts) = self.receive(endpoint.prepare()?, read).await?;
        let (envelope, meta) = received.decode_with_meta().await?;
        let meta = ResponseMeta::from(meta);
        match envelope {
            Single::Failure(document) => Err(Error::Provider(Box::new(ProviderError::new(
                meta,
                Ok(document),
                attempts,
            )))),
            Single::Success {
                result: Some(result),
                errors,
                messages,
            } => Ok(Response {
                result,
                messages,
                errors,
                meta,
            }),
            Single::Success { result: None, .. } => Err(Error::Envelope(Box::new(
                EnvelopeError::new(meta, EnvelopeProblem::MissingResult),
            ))),
        }
    }
}

impl<R: Send + 'static> SendWith<Paged<R>> for SendPolicy {
    type Output = Result<PageResponse<R>, Error>;

    async fn send<E: Endpoint<Paged<R>> + Send>(&self, endpoint: E) -> Self::Output {
        let read = is_read::<E, Paged<R>>();
        let (received, attempts) = self.receive(endpoint.prepare()?, read).await?;
        let (envelope, meta) = received.decode_with_meta().await?;
        let meta = ResponseMeta::from(meta);
        match envelope {
            Paged::Failure(document) => Err(Error::Provider(Box::new(ProviderError::new(
                meta,
                Ok(document),
                attempts,
            )))),
            Paged::Success {
                result: Some(result),
                result_info,
                errors,
                messages,
            } => Ok(PageResponse {
                result,
                result_info,
                messages,
                errors,
                meta,
            }),
            Paged::Success { result: None, .. } => Err(Error::Envelope(Box::new(
                EnvelopeError::new(meta, EnvelopeProblem::MissingResult),
            ))),
        }
    }
}
