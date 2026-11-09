//! Rate admission for one bot: the client's send policy.
//!
//! Discord counts requests per **route bucket and top-level resource** and, separately, per bot
//! (global). `X-RateLimit-Bucket` names a bucket but "does not include top-level resources", so
//! the limiter keys its windows by `(bucket hash, channel or guild)`: two channels sharing a hash
//! never share a window. It learns the bucket of each declared route from the responses, and
//! keeps a global gate set by a global 429.
//!
//! Admission refuses, it never sleeps: a request whose scope is exhausted returns
//! [`Error::Gated`] before any I/O, with the remaining delay, and the caller decides whether to
//! wait. A 429 is returned as [`Error::RateLimited`] and recorded as a gate. Nothing is retried.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use concord::reqwest::header::HeaderMap;
use concord::reqwest::{StatusCode, Url};
use concord::{Endpoint, Operation, ResponseMeta, SendWith};

use crate::ERROR_BODY_LIMIT;
use crate::error::{
    AmbiguousWrite, ApiError, Error, GateScope, Gated, RateLimitBody, RateLimitScope, RateLimited,
    Resource,
};
use crate::model::{ChannelId, GuildId};

/// A Discord rate-limit bucket hash (`X-RateLimit-Bucket`), which excludes the top-level
/// resource.
#[derive(Clone, PartialEq, Eq, Hash, concord::Debug)]
pub struct BucketHash(Arc<str>);

impl BucketHash {
    /// The hash as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BucketHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A request's declared operation and top-level resource: what every [`Error`] reports.
#[derive(Clone, Copy, PartialEq, Eq, concord::Debug)]
pub struct Route {
    operation: Operation,
    resource: Option<Resource>,
}

impl Route {
    pub(crate) fn new(operation: Operation, resource: Option<Resource>) -> Self {
        Self {
            operation,
            resource,
        }
    }

    /// The declared endpoint (`GET Discord.channel.messages`).
    pub fn operation(&self) -> Operation {
        self.operation
    }

    /// The channel or guild the request was for.
    pub fn resource(&self) -> Option<Resource> {
        self.resource
    }

    fn key(&self) -> RouteKey {
        RouteKey {
            name: self.operation.name(),
            method: self.operation.method(),
        }
    }

    /// Use the declaration's read effect to distinguish uncertain writes from failed reads.
    fn is_write(&self) -> bool {
        !self.operation.is_read()
    }

    /// A transport failure before a response head. For a write it is ambiguous, a refused
    /// connection included: the client is the caller's, and a redirect may follow a performed
    /// write to a target that refuses (`Error::delivery()` is `Unknown` for the same reason).
    fn failed_before_head(self, error: concord::Error) -> Error {
        if self.is_write() {
            Error::AmbiguousWrite(Box::new(AmbiguousWrite {
                route: self,
                accepted: None,
                error,
            }))
        } else {
            Error::concord(error, self.resource)
        }
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.resource {
            Some(resource) => write!(f, "{} on {resource}", self.operation),
            None => self.operation.fmt(f),
        }
    }
}

/// The top-level resource of a built request: the ID after the first `channels` or `guilds`
/// path segment. The declaration's scopes are exactly `channels/{channel_id}` and
/// `guilds/{guild_id}`, and the declared base (`/api/v10`) holds neither word; a base URL
/// replaced with one that does would be misread.
fn resource_of(url: &Url) -> Option<Resource> {
    let mut segments = url.path_segments()?;
    while let Some(segment) = segments.next() {
        match segment {
            "channels" => {
                return segments
                    .next()?
                    .parse::<ChannelId>()
                    .ok()
                    .map(Resource::Channel);
            }
            "guilds" => {
                return segments
                    .next()?
                    .parse::<GuildId>()
                    .ok()
                    .map(Resource::Guild);
            }
            _ => {}
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RouteKey {
    name: &'static str,
    method: Option<&'static str>,
}

/// A window's bucket: Discord's hash, or the route itself while its hash is unknown.
#[derive(Clone, PartialEq, Eq, Hash)]
enum BucketRef {
    Hash(BucketHash),
    Route(RouteKey),
}

struct Window {
    remaining: u64,
    reset_at: Instant,
}

#[derive(Default)]
struct State {
    global_until: Option<Instant>,
    routes: HashMap<RouteKey, BucketHash>,
    windows: HashMap<(BucketRef, Option<Resource>), Window>,
}

impl State {
    fn bucket_ref(&self, route: &Route, sent: Option<&BucketHash>) -> BucketRef {
        match sent.or_else(|| self.routes.get(&route.key())) {
            Some(hash) => BucketRef::Hash(hash.clone()),
            None => BucketRef::Route(route.key()),
        }
    }
}

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// The rate state of **one bot token** and the send policy of [`Discord`](crate::Discord)
/// (`send limiter -> Result<_, Error>;`): every generated `send()` is admitted, executed once and observed by it.
///
/// Cheap to clone (an `Arc`); clones share state, so give one limiter to every handle of a bot
/// and a separate one to each other token. `Debug` prints counts only.
///
/// What it does, per request:
///
/// 1. **Admit** (no I/O): refuse with [`Error::Gated`] while a global 429's delay runs, or while
///    the window of this route's bucket for this channel/guild has `X-RateLimit-Remaining: 0`
///    and its `X-RateLimit-Reset-After` has not elapsed.
/// 2. **Execute once**, then record `X-RateLimit-Bucket`, `-Remaining` and `-Reset-After` for
///    `(bucket, resource)` and remember the route's bucket (routes can share one).
/// 3. On **429**: gate the global scope (`X-RateLimit-Scope: global`, `X-RateLimit-Global`, or
///    body `global: true`) or the `(bucket, resource)` window for the 429's delay, and return
///    [`Error::RateLimited`].
///
/// Not done: no sleeping, queueing or retry; no proactive pacing to the documented 50 requests
/// per second; concurrent requests on one window are not serialized (two admitted requests can
/// both meet a 429). `X-RateLimit-Reset` (an absolute epoch) is not used: relative
/// `Reset-After` avoids clock skew.
#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<State>>,
    clock: Clock,
}

impl RateLimiter {
    /// A limiter on the monotonic system clock.
    pub fn new() -> Self {
        Self::with_clock(Instant::now)
    }

    /// A limiter on an injected monotonic clock (tests, simulations).
    pub fn with_clock(now: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        Self {
            state: Arc::default(),
            clock: Arc::new(now),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn admit(&self, route: &Route) -> Result<(), Error> {
        let now = (self.clock)();
        let mut state = self.lock();
        if let Some(until) = state.global_until {
            if now < until {
                return Err(Error::Gated(Box::new(Gated {
                    route: *route,
                    scope: GateScope::Global,
                    retry_after: until - now,
                })));
            }
            state.global_until = None;
        }
        let bucket = state.routes.get(&route.key()).cloned();
        let key = (state.bucket_ref(route, None), route.resource);
        if let Some(window) = state.windows.get(&key)
            && window.remaining == 0
            && now < window.reset_at
        {
            return Err(Error::Gated(Box::new(Gated {
                route: *route,
                scope: GateScope::Bucket { bucket },
                retry_after: window.reset_at - now,
            })));
        }
        Ok(())
    }

    fn observe(&self, route: &Route, headers: &HeaderMap) {
        let bucket = bucket_of(headers);
        let remaining = single(headers, "x-ratelimit-remaining").and_then(unsigned);
        let reset_after = single(headers, "x-ratelimit-reset-after").and_then(decimal_seconds);
        let now = (self.clock)();
        let mut state = self.lock();
        state.windows.retain(|_, window| window.reset_at > now);
        if let Some(hash) = &bucket {
            state.routes.insert(route.key(), hash.clone());
        }
        if let (Some(remaining), Some(reset_after)) = (remaining, reset_after) {
            let key = (state.bucket_ref(route, bucket.as_ref()), route.resource);
            let reset_at = later(now, reset_after);
            // Responses overlap: one sent before an exhaustion (or a 429) can arrive after it
            // with a higher count and an earlier reset. While a window is open, keep the lower
            // count and the later reset, so a late success never reopens it early.
            let window = state.windows.entry(key).or_insert(Window {
                remaining,
                reset_at,
            });
            window.remaining = window.remaining.min(remaining);
            window.reset_at = window.reset_at.max(reset_at);
        }
    }

    fn rate_limited(
        &self,
        route: Route,
        meta: ResponseMeta,
        body: Result<RateLimitBody, concord::Error>,
    ) -> RateLimited {
        let headers = meta.headers();
        let scope_header = single(headers, "x-ratelimit-scope");
        let global = scope_header == Some("global")
            || single(headers, "x-ratelimit-global").is_some_and(|v| v == "true")
            || body.as_ref().is_ok_and(|b| b.global);
        let scope = match scope_header {
            _ if global => RateLimitScope::Global,
            Some("user") => RateLimitScope::User,
            Some("shared") => RateLimitScope::Shared,
            _ => RateLimitScope::Unspecified,
        };
        let header_delay = single(headers, "retry-after").and_then(decimal_seconds);
        let body_delay = body
            .as_ref()
            .ok()
            .and_then(|b| finite_seconds(b.retry_after));
        let retry_after = match (header_delay, body_delay) {
            (Some(header), Some(body)) => Some(header.max(body)),
            (header, body) => header.or(body),
        }
        .or_else(|| single(headers, "x-ratelimit-reset-after").and_then(decimal_seconds));
        let bucket = bucket_of(headers);
        if let Some(delay) = retry_after {
            let until = later((self.clock)(), delay);
            let mut state = self.lock();
            if global {
                state.global_until = Some(state.global_until.map_or(until, |t| t.max(until)));
            } else {
                let key = (state.bucket_ref(&route, bucket.as_ref()), route.resource);
                let window = state.windows.entry(key).or_insert(Window {
                    remaining: 0,
                    reset_at: until,
                });
                window.remaining = 0;
                window.reset_at = window.reset_at.max(until);
            }
        }
        RateLimited {
            route,
            scope,
            retry_after,
            bucket,
            meta,
            body,
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.lock();
        f.debug_struct("RateLimiter")
            .field("routes", &state.routes.len())
            .field("windows", &state.windows.len())
            .field("global_gate", &state.global_until.is_some())
            .finish()
    }
}

impl<T: Send + 'static> SendWith<T, crate::FailureDocument> for RateLimiter {
    type Output = Result<T, Error>;

    /// Admit, execute once, observe; map the status to [`Error`]. A `POST` that fails after it
    /// may have been sent is [`Error::AmbiguousWrite`], never resent.
    async fn send<E: Endpoint<T, crate::FailureDocument> + Send>(
        &self,
        endpoint: E,
    ) -> Self::Output {
        let prepared = endpoint.prepare()?;
        let route = Route {
            operation: E::operation(),
            resource: resource_of(prepared.request().url()),
        };
        self.admit(&route)?;
        let received = match prepared.receive().await {
            Ok(received) => received,
            Err(error) => return Err(route.failed_before_head(error)),
        };
        self.observe(&route, received.head().headers());
        let status = received.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let (meta, body) = received.failure(ERROR_BODY_LIMIT).await.into_document();
            return Err(Error::RateLimited(Box::new(self.rate_limited(
                route,
                meta,
                body.map(|document| match document {
                    crate::FailureDocument::RateLimited(body) => body,
                    _ => unreachable!("429 codec selected"),
                }),
            ))));
        }
        if !status.is_success() {
            let (meta, document) = received.failure(ERROR_BODY_LIMIT).await.into_document();
            return Err(Error::Api(Box::new(ApiError {
                route,
                meta,
                document: document.map(|document| match document {
                    crate::FailureDocument::Api(document) => document,
                    _ => unreachable!("API codec selected"),
                }),
            })));
        }
        match received.into_success()?.decode().await {
            Ok(value) => Ok(value),
            Err(error) if route.is_write() => {
                Err(Error::AmbiguousWrite(Box::new(AmbiguousWrite {
                    route,
                    accepted: Some(status),
                    error,
                })))
            }
            Err(error) => Err(Error::concord(error, route.resource)),
        }
    }
}

/// Exactly one field line, as text.
fn single<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    concord::field::single(headers, name).ok()??.to_str().ok()
}

fn bucket_of(headers: &HeaderMap) -> Option<BucketHash> {
    single(headers, "x-ratelimit-bucket")
        .filter(|hash| !hash.is_empty() && !hash.contains([' ', '\t']))
        .map(|hash| BucketHash(hash.into()))
}

fn unsigned(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Non-negative decimal seconds with an optional fraction (`1`, `64.57`): Discord's spelling of
/// `X-RateLimit-Reset-After` and `Retry-After`. Signs, exponents, whitespace and empty parts
/// are refused; digits beyond nanoseconds round **up**, so a minimum is never shortened.
pub(crate) fn decimal_seconds(text: &str) -> Option<Duration> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let seconds = unsigned(whole)?;
    let mut nanos = 0u64;
    if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let kept = &fraction[..fraction.len().min(9)];
        nanos = kept.parse::<u64>().ok()? * 10u64.pow(9 - kept.len() as u32);
        if fraction[kept.len()..].bytes().any(|b| b != b'0') {
            nanos += 1;
        }
    }
    Duration::from_secs(seconds).checked_add(Duration::from_nanos(nanos))
}

fn finite_seconds(value: f64) -> Option<Duration> {
    (value.is_finite() && value >= 0.0)
        .then(|| Duration::try_from_secs_f64(value).ok())
        .flatten()
}

/// `now + delay`, saturating at about ten years for a delay the clock cannot represent.
fn later(now: Instant, delay: Duration) -> Instant {
    const CAP: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);
    now.checked_add(delay)
        .or_else(|| now.checked_add(CAP))
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_seconds_accepts_discord_spellings_only() {
        assert_eq!(decimal_seconds("65"), Some(Duration::from_secs(65)));
        assert_eq!(
            decimal_seconds("64.57"),
            Some(Duration::from_millis(64_570))
        );
        assert_eq!(decimal_seconds("0.001"), Some(Duration::from_millis(1)));
        assert_eq!(
            decimal_seconds("1.0000000001"),
            Some(Duration::new(1, 1)),
            "beyond nanoseconds rounds up"
        );
        for refused in [
            "", ".5", "5.", "-1", "+1", "1e3", " 1", "1 ", "1.2.3", "NaN", "inf",
        ] {
            assert_eq!(decimal_seconds(refused), None, "{refused:?}");
        }
        assert_eq!(decimal_seconds("18446744073709551616"), None, "overflow");
    }

    #[test]
    fn resource_is_the_first_channel_or_guild_segment() {
        let url = |path: &str| Url::parse(&format!("http://h{path}")).unwrap();
        assert_eq!(
            resource_of(&url("/api/v10/guilds/290926798629997250/channels")),
            Some(Resource::Guild(GuildId::new(290926798629997250)))
        );
        assert_eq!(
            resource_of(&url("/api/v10/channels/41771983423143937/messages/1")),
            Some(Resource::Channel(ChannelId::new(41771983423143937)))
        );
        assert_eq!(resource_of(&url("/api/v10/users/@me")), None);
    }

    fn bucket_headers(remaining: &str, reset_after: &str) -> HeaderMap {
        HeaderMap::from_iter([
            (
                "x-ratelimit-bucket".parse().unwrap(),
                "shared".parse().unwrap(),
            ),
            (
                "x-ratelimit-remaining".parse().unwrap(),
                remaining.parse().unwrap(),
            ),
            (
                "x-ratelimit-reset-after".parse().unwrap(),
                reset_after.parse().unwrap(),
            ),
        ])
    }

    // Found while trying `concord-ratelimit` here (lane b12-y, Codex gpt-6.1-sol).
    #[test]
    fn overlapping_success_cannot_reopen_or_shorten_exhaustion() {
        let epoch = Instant::now();
        let offset = Arc::new(Mutex::new(Duration::ZERO));
        let clock = offset.clone();
        let limiter = RateLimiter::with_clock(move || epoch + *clock.lock().unwrap());
        let route = Route::new(
            Operation::label("messages"),
            Some(Resource::Channel(ChannelId::new(1))),
        );
        limiter.observe(&route, &bucket_headers("0", "60"));
        // A success already in flight reports an older positive count and shorter reset.
        limiter.observe(&route, &bucket_headers("4", "1"));
        *offset.lock().unwrap() = Duration::from_secs(2);
        let Err(Error::Gated(gate)) = limiter.admit(&route) else {
            panic!("an overlapping response must not reopen the exhausted window");
        };
        assert_eq!(gate.retry_after, Duration::from_secs(58));
        assert!(matches!(gate.scope, GateScope::Bucket { .. }));
        *offset.lock().unwrap() = Duration::from_secs(60);
        limiter.admit(&route).unwrap();
        limiter.observe(&route, &HeaderMap::new());
        assert!(
            limiter.lock().windows.is_empty(),
            "expired windows are pruned"
        );
    }

    #[test]
    fn zero_delay_429_keeps_prior_reset_despite_overlapping_success() {
        let now = Instant::now();
        let limiter = RateLimiter::with_clock(move || now);
        let route = Route::new(Operation::label("messages"), None);
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "4".parse().unwrap());
        headers.insert("x-ratelimit-reset-after", "60".parse().unwrap());
        limiter.observe(&route, &headers);
        limiter.admit(&route).unwrap();
        let limited = limiter.rate_limited(
            route,
            ResponseMeta::new(
                concord::ResponseHead::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    concord::reqwest::Version::HTTP_11,
                    &HeaderMap::new(),
                ),
                Some(route.operation()),
            ),
            Ok(
                serde_json::from_str(r#"{"message":"limited","retry_after":0,"global":false}"#)
                    .unwrap(),
            ),
        );
        assert_eq!(limited.retry_after, Some(Duration::ZERO));
        // A later success cannot undo that cooldown, even though the 429's own delay was zero.
        headers.insert("x-ratelimit-reset-after", "1".parse().unwrap());
        limiter.observe(&route, &headers);
        let Err(Error::Gated(gate)) = limiter.admit(&route) else {
            panic!("the original reset horizon must remain authoritative");
        };
        assert_eq!(gate.retry_after, Duration::from_secs(60));
    }
}
