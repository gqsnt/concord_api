//! Process-local limits shared by the calls of a client (feature `limit`).
//!
//! Configure it fully, then clone; clones share one admission state. Start with
//! [`Limit::new`] to hold later calls after a 429 with `Retry-After`. No local rate
//! or concurrency cap is guessed, and the response is returned unchanged.
//!
//! ```no_run
//! use concord::limit::Limit;
//! concord::api! {
//!     client Data {
//!         base "https://data.example.test";
//!         GET get() -> String { path "data"; read concord::Text; }
//!     }
//! }
//! async fn get() -> Result<String, concord::Error> {
//!     Data::new().with_middleware(Limit::new()).get().await
//! }
//! ```
//!
//! `Retry-After` is read with [`concord_core::Headers::retry_after`]: seconds or an
//! HTTP date; zero, a past date or unreadable text holds nothing. The middleware
//! never resends. An attempt repeated by the engine waits behind the same gates
//! and is charged like any other call.
//!
//! Add a concurrency cap when needed. Its permit lasts until response-body EOF,
//! error or drop, including time when the body is unread:
//!
//! ```no_run
//! let limit = concord::limit::Limit::new().max_in_flight(4);
//! ```
//!
//! Admission waits last at most 60 seconds by default. Change that allowance with
//! `max_hold`; the call timeout remains an independent bound:
//!
//! ```no_run
//! let limit = concord::limit::Limit::new()
//!     .max_hold(std::time::Duration::from_secs(10));
//! ```
//!
//! To enforce a known local rate, add a bucket with a rolling window. The key
//! function puts each call in an allowance; returning `None` skips the bucket:
//!
//! ```no_run
//! use concord::limit::{Bucket, Limit, Window};
//! use std::time::Duration;
//! let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into()))
//!     .window(Window::rolling(10, Duration::from_secs(1))));
//! ```
//!
//! When the server announces its budget, add a learner instead. [`counts`] reads
//! paired capacity/count headers; [`remaining`] reads a capacity, units left and
//! a Unix reset date. Each bucket can also set its own cost (one by default).
//!
//! ```no_run
//! use concord::limit::{Bucket, Limit, counts};
//! let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into()))
//!     .learn(counts("x-limit", "x-count")));
//! ```
//!
//! Several allowances can apply to one call. Here application and method buckets
//! on each host learn two windows each from four headers. Each definition owns its
//! key, windows, learning, cost and 429 selector; equal key text in different
//! definitions still identifies separate allowances.
//!
//! ```no_run
//! use concord::limit::{Bucket, Limit, counts};
//! concord::api! {
//!     client Matches {
//!         base "https://matches.example.test";
//!         GET recent() -> String { path "matches"; read concord::Text; }
//!     }
//! }
//! async fn recent() -> Result<String, concord::Error> {
//!     let application = Bucket::new(|call| Some(format!("{} application", call.request().url().host_str()?)))
//!         .learn(counts("x-app-rate-limit", "x-app-rate-limit-count"))
//!         .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application"));
//!     let method = Bucket::new(|call| Some(format!("{} {}", call.request().url().host_str()?, call.info().endpoint())))
//!         .learn(counts("x-method-rate-limit", "x-method-rate-limit-count"))
//!         .hit_when(|seen| seen.header("x-rate-limit-type") != Some("application"));
//!     let limit = Limit::new()
//!         // Allow the longest announced window: the default hold is only 60 s.
//!         .max_hold(std::time::Duration::from_secs(120))
//!         // Service-limit 429s can omit Retry-After; hold those for one second.
//!         .without_retry_after(std::time::Duration::from_secs(1))
//!         .bucket(application)
//!         .bucket(method);
//!     let api = Matches::new().with_middleware(limit);
//!     api.recent().await
//! }
//! ```
//!
//! A 429 with `Retry-After` holds every selected bucket without `hit_when` and
//! every selected bucket whose `hit_when` returns true. If it holds no bucket,
//! every call through the limit waits behind a global gate.
//!
//! All selected windows reserve their bucket's cost atomically or none do. No
//! dispatched cost is refunded after a failure or cancellation. A bucket with no
//! known window admits one discovery call at a time, releasing the next after
//! learning from the response head, or on failure or drop before that; it does not
//! wait for the body. The first discovery cannot be proved to fit an unseen budget.
//! A fixed window refills once to its whole capacity at the stated reset and stays
//! known; it does not return to discovery or invent another reset.
//!
//! Missing or unreadable headers teach nothing. Learning never turns a received
//! response into a failed call: spent quota stays spent and valid `Retry-After`
//! still gates. Key and cost panics unwind before admission without changing shared
//! state. If any learner or selector panics, nothing from that response is learned;
//! a panicking 429 selector counts as a hit. Callbacks run outside the state lock,
//! so a panic never strands other calls or makes the limit unusable. No panic
//! payload enters diagnostics; the author's panic hook still runs. No task is started.
//!
//! Retained state defaults to 1024 bucket-definition/key pairs across all buckets,
//! bounded by `max_keys`. When room is needed, a key holding no unexpired quota
//! debt, no active 429 hold and no call in flight (through body end, including
//! discovery awaiting its first response) is forgotten. Learned windows are then
//! rediscovered; configured windows still apply. Other bounds are constants:
//! 8 windows per bucket, 512 UTF-8 bytes per key, 4096 history entries per window.
//! On 64-bit Linux, one key's retained payload is at most 1,049,984 bytes:
//! 512 key bytes + 128 bytes for the map entry + 8 × (96 window bytes +
//! 4096 × 32 debit bytes). Hash-table spare slots, allocator metadata, shared
//! policy and caller-owned calls/bodies are additional to this payload bound.
//! Full histories merge their two oldest entries at the later expiry, conservatively, and never fail calls.
//! A static policy violation takes precedence over exceeding `max_keys`. Invalid
//! learned windows teach nothing. Callbacks bound their own temporary allocations.
//!
//! Share one limit for the same local allowance. Other processes and independent
//! values are not coordinated, and body-only budgets are unseen. The Tower adapter
//! can also enforce a local rate; its readiness wait counts as transport idle time,
//! and its usual concurrency layer ends at the response head.

use concord_core::http::{HeaderMap, StatusCode};
use concord_core::reqwest::{Request, Response};
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{Middleware, Next};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::Instant;

mod body;
mod engine;
mod headers;

type Key = dyn Fn(&RequestInfo<'_>) -> Option<String> + Send + Sync;
type Cost = dyn Fn(&RequestInfo<'_>) -> u64 + Send + Sync;
type Learn = dyn Fn(&Seen<'_>) -> Vec<Window> + Send + Sync;
type Hit = dyn Fn(&Seen<'_>) -> bool + Send + Sync;

/// Request context for key and cost functions, with room for additional inputs.
///
/// ```no_run
/// fn endpoint(call: &concord::limit::RequestInfo<'_>) -> &'static str { call.info().endpoint() }
/// ```
pub struct RequestInfo<'a> {
    request: &'a Request,
    info: &'a TryInfo,
}

impl RequestInfo<'_> {
    /// Borrows the built request; the callback cannot edit it.
    ///
    /// ```no_run
    /// fn host(call: &concord::limit::RequestInfo<'_>) -> Option<String> {
    ///     Some(call.request().url().host_str()?.to_owned())
    /// }
    /// ```
    pub fn request(&self) -> &Request {
        self.request
    }

    /// Borrows the endpoint identity and attempt number.
    ///
    /// ```no_run
    /// fn attempt(call: &concord::limit::RequestInfo<'_>) -> u32 { call.info().number() }
    /// ```
    pub fn info(&self) -> &TryInfo {
        self.info
    }
}

impl fmt::Debug for RequestInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestInfo { .. }")
    }
}

/// Configure it fully, then clone; clones share one bucket definition.
///
/// A definition combines its key, initial windows, learning, cost and 429 selector.
/// Admission state belongs to its containing `Limit`. The same text returned by
/// different definitions identifies separate allowances. `None` or a zero cost
/// skips this bucket entirely, including discovery and per-bucket gating.
///
/// ```no_run
/// use concord::limit::{Bucket, Window};
/// let bucket = Bucket::new(|call| Some(call.info().endpoint().into()))
///     .window(Window::rolling(10, std::time::Duration::from_secs(1)));
/// let same_definition = bucket.clone();
/// ```
#[derive(Clone)]
#[must_use = "configure the bucket and attach it with Limit::bucket"]
pub struct Bucket {
    key: Arc<Key>,
    cost: Arc<Cost>,
    learn: Option<Arc<Learn>>,
    hit: Option<Arc<Hit>>,
    windows: Vec<Window>,
}

impl Bucket {
    /// Selects a request's key; `None` means this bucket does not charge it.
    /// Default cost is one, initial windows are unknown, and every 429 selects it.
    /// A key or cost panic unwinds this call before changing admission state;
    /// other calls can continue using the limit.
    /// With no known window, the next discovery starts after the response head
    /// has been learned, or after an earlier failure or drop, without waiting for its body.
    ///
    /// ```no_run
    /// let bucket = concord::limit::Bucket::new(|call| Some(call.info().endpoint().into()));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn new(key: impl Fn(&RequestInfo<'_>) -> Option<String> + Send + Sync + 'static) -> Self {
        Self {
            key: Arc::new(key),
            cost: Arc::new(|_| 1),
            learn: None,
            hit: None,
            windows: Vec::new(),
        }
    }

    /// Adds an initial window; repeated calls add more, up to eight.
    ///
    /// ```no_run
    /// let bucket = concord::limit::Bucket::new(|_| Some("requests".into()))
    ///     .window(concord::limit::Window::rolling(10, std::time::Duration::from_secs(1)));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn window(mut self, window: Window) -> Self {
        self.windows.push(window);
        self
    }

    /// Interprets a response for this bucket. An empty result teaches nothing.
    ///
    /// Results describe a complete ordered set of windows; stable positions identify
    /// windows across responses and initial configuration. Header helpers preserve
    /// that order by sorting rolling periods. Incompatible, oversized or invalid
    /// results are ignored as a whole. Learned availability can only tighten an
    /// active window, accounting for other in-flight costs; older resets are ignored.
    /// No learning result fails the responding call or permanently blocks a key.
    /// Without `remaining`, the responding call's cost is counted against capacity.
    /// If any learner or selector panics, all learning from that response is
    /// discarded, the response is returned, and other calls can continue.
    /// Discovery is released after learning the head, without waiting for its body;
    /// failure or drop before the head also releases it. The author's panic hook still runs.
    ///
    /// ```no_run
    /// use concord::limit::{Bucket, Window};
    /// let bucket = Bucket::new(|_| Some("requests".into())).learn(|seen| {
    ///     let Some(left) = seen.header("x-left").and_then(|v| v.parse().ok()) else { return vec![]; };
    ///     vec![Window::rolling(10, std::time::Duration::from_secs(1)).remaining(left)]
    /// });
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn learn(self, learn: impl Fn(&Seen<'_>) -> Vec<Window> + Send + Sync + 'static) -> Self {
        Self {
            learn: Some(Arc::new(learn)),
            ..self
        }
    }

    /// Computes this bucket's cost; zero skips the bucket, and the default is one.
    /// Different buckets can count the same call as points and requests respectively.
    /// A cost panic unwinds this call before admission without stranding other calls.
    ///
    /// ```no_run
    /// let bucket = concord::limit::Bucket::new(|_| Some("points".into())).cost(|call| {
    ///     if call.info().endpoint().ends_with(".search") { 20 } else { 1 }
    /// });
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn cost(self, cost: impl Fn(&RequestInfo<'_>) -> u64 + Send + Sync + 'static) -> Self {
        Self {
            cost: Arc::new(cost),
            ..self
        }
    }

    /// Selects this bucket for a 429/`Retry-After` hold. With no selector it is selected.
    /// If no selected bucket is hit, the limit applies a global hold instead.
    /// A selector panic conservatively counts as a hit, discards all learning
    /// from that response, and still returns the response; other calls can continue.
    ///
    /// ```no_run
    /// let bucket = concord::limit::Bucket::new(|_| Some("application".into()))
    ///     .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application"));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn hit_when(self, hit: impl Fn(&Seen<'_>) -> bool + Send + Sync + 'static) -> Self {
        Self {
            hit: Some(Arc::new(hit)),
            ..self
        }
    }
}

impl fmt::Debug for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Bucket { .. }")
    }
}

/// Configure it fully, then clone; clones share this window's policy.
///
/// A rolling window retains every local or observed debit for a full period. A
/// fixed window refills once to its whole capacity at its stated reset and stays
/// known, with no invented repeating period or return to serial discovery.
/// If that refilled budget is spent before another reset is learned, its wait is
/// unknown and bounded by `max_hold`.
/// `Debug` prints no settings. Initial invalid windows fail before dispatch;
/// invalid windows returned by learning teach nothing.
///
/// A known fixed rate needs no response-header reader:
///
/// ```no_run
/// use concord::limit::{Bucket, Limit, Window};
/// use std::time::Duration;
/// concord::api! {
///     client Search {
///         base "https://search.example.test";
///         GET find() -> String { path "find"; read concord::Text; }
///     }
/// }
/// async fn find() -> Result<String, concord::Error> {
///     let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into()))
///         .window(Window::rolling(10, Duration::from_secs(1))));
///     Search::new().with_middleware(limit).find().await
/// }
/// ```
#[derive(Clone)]
#[must_use = "attach the window to a bucket or return it from learn"]
pub struct Window {
    capacity: u64,
    kind: Kind,
    remaining: Option<u64>,
}

#[derive(Clone)]
enum Kind {
    Rolling(Duration),
    Fixed(Option<Instant>),
}

impl Window {
    /// Allows at most `capacity` units in any `period`. Both must be positive.
    ///
    /// ```no_run
    /// let window = concord::limit::Window::rolling(10, std::time::Duration::from_secs(1));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn rolling(capacity: u64, period: Duration) -> Self {
        Self {
            capacity,
            kind: Kind::Rolling(period),
            remaining: None,
        }
    }

    /// Describes a fixed budget that refills once, `resets_in` from now.
    /// At that reset the whole capacity is available again and the window stays
    /// known. No further reset is invented; once that budget is spent, the next
    /// wait is unknown until a response teaches another reset. Capacity must be
    /// positive; a zero reset time refills immediately.
    ///
    /// ```no_run
    /// let window = concord::limit::Window::fixed(5000, std::time::Duration::from_secs(60));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn fixed(capacity: u64, resets_in: Duration) -> Self {
        Self {
            capacity,
            kind: Kind::Fixed(Instant::now().checked_add(resets_in)),
            remaining: None,
        }
    }

    /// Sets the server's remaining units after the responding call; values above
    /// capacity are invalid, and an observation never refunds spent units.
    ///
    /// ```no_run
    /// let window = concord::limit::Window::fixed(5000, std::time::Duration::from_secs(60)).remaining(4200);
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn remaining(self, n: u64) -> Self {
        Self {
            remaining: Some(n),
            ..self
        }
    }
}

impl fmt::Debug for Window {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Window { .. }")
    }
}

/// One response seen by one selected bucket, with room for more context.
///
/// Accessors return values deliberately; `Debug` omits key and header values.
///
/// ```no_run
/// fn left(seen: &concord::limit::Seen<'_>) -> Option<u64> { seen.header("x-left")?.parse().ok() }
/// ```
pub struct Seen<'a> {
    key: &'a str,
    headers: &'a HeaderMap,
    status: StatusCode,
    now: SystemTime,
}

impl Seen<'_> {
    /// Returns the key selected for this bucket.
    ///
    /// ```no_run
    /// fn key(seen: &concord::limit::Seen<'_>) -> String { seen.key().to_owned() }
    /// ```
    pub fn key(&self) -> &str {
        self.key
    }

    /// Returns the response status, without consuming its body.
    ///
    /// ```no_run
    /// fn refused(seen: &concord::limit::Seen<'_>) -> bool { seen.status() == 429 }
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Borrows the response headers.
    ///
    /// ```no_run
    /// fn names(seen: &concord::limit::Seen<'_>) -> usize { seen.headers().len() }
    /// ```
    pub fn headers(&self) -> &HeaderMap {
        self.headers
    }

    /// Returns a single header's text if present and visible ASCII; duplicates teach nothing.
    ///
    /// ```no_run
    /// fn method(seen: &concord::limit::Seen<'_>) -> bool { seen.header("x-rate-limit-type") == Some("method") }
    /// ```
    pub fn header(&self, name: &str) -> Option<&str> {
        let mut values = self.headers.get_all(name).iter();
        let value = values.next()?;
        if values.next().is_some() {
            return None;
        }
        value.to_str().ok()
    }

    /// Returns the sampled wall time of this observation.
    ///
    /// ```no_run
    /// fn now(seen: &concord::limit::Seen<'_>) -> std::time::SystemTime { seen.now() }
    /// ```
    pub fn now(&self) -> SystemTime {
        self.now
    }

    /// Returns time until a Unix-second date against `now()`, zero if past, or
    /// `None` for unrepresentable arithmetic. Waiting uses the internal timer.
    ///
    /// ```no_run
    /// fn reset(seen: &concord::limit::Seen<'_>) -> Option<std::time::Duration> {
    ///     seen.until_unix(seen.header("x-reset")?.parse().ok()?)
    /// }
    /// ```
    pub fn until_unix(&self, seconds: u64) -> Option<Duration> {
        let date = std::time::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))?;
        Some(date.duration_since(self.now).unwrap_or_default())
    }
}

impl fmt::Debug for Seen<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seen { .. }")
    }
}

/// An admission failure before sending, with no request, key or header text.
///
/// These policy/scheduling reasons have no lower-level cause, so `source()` is
/// `None`. The middleware preserves the typed error as the call error's source;
/// `MaxHold` does not populate the engine's response-derived `retry_after()`.
/// No learning or header-parse failure is returned after receiving a response.
///
/// ```no_run
/// use concord::limit::LimitError;
/// fn inspect<P: 'static>(err: &concord::Error<P>) {
///     let limit = LimitError::find(err);
///     if let Some(LimitError::MaxHold { required, retry_after, waited, .. }) = limit {
///         let given_wait = (required, retry_after, waited);
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone)]
pub enum LimitError {
    /// Invalid static bounds, key size, window or policy, including a zero concurrency cap.
    /// Takes precedence when the call would also exceed `max_keys`.
    InvalidPolicy,
    /// The selected keys exceed `max_keys` even after forgetting keys that hold nothing.
    TooManyKeys,
    /// This bucket's cost exceeds at least one known window's capacity.
    CostExceedsCapacity,
    /// Admission would exceed the call's maximum hold.
    ///
    /// For a repeatable operation, wait the remaining known delay and make a new call:
    ///
    /// ```no_run
    /// use concord::limit::LimitError;
    /// concord::api! {
    ///     client Data {
    ///         base "https://data.example.test";
    ///         GET get() -> String { path "data"; read concord::Text; }
    ///     }
    /// }
    /// async fn get(api: &Data) -> Result<String, concord::Error> {
    ///     match api.get().await {
    ///         Err(error) => {
    ///             if let Some(LimitError::MaxHold { required: Some(wait), .. }) = LimitError::find(&error) {
    ///                 tokio::time::sleep(*wait).await;
    ///                 api.get().await
    ///             } else {
    ///                 Err(error)
    ///             }
    ///         }
    ///         reply => reply,
    ///     }
    /// }
    /// ```
    #[non_exhaustive]
    MaxHold {
        /// Delay still required at rejection, not the original delay; `None` if unknown.
        required: Option<Duration>,
        /// Original server-given `Retry-After` for a 429 gate contributing to this wait.
        retry_after: Option<Duration>,
        /// Time already spent waiting for admission.
        waited: Duration,
    },
}

impl LimitError {
    /// Finds a limit failure in this error or its `source()` chain.
    /// A cyclic source chain without a limit error returns `None`.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) {
    ///     if let Some(concord::limit::LimitError::MaxHold { required, .. }) =
    ///         concord::limit::LimitError::find(error)
    ///     {
    ///         let still_to_wait = required;
    ///     }
    /// }
    /// ```
    #[must_use = "inspect the matching admission failure"]
    // PANIC-SAFETY: Explicit lifetimes show that the returned error borrows the input; no panic lint is relaxed.
    #[allow(clippy::needless_lifetimes)]
    pub fn find<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a Self> {
        let mut current = Some(error);
        let mut fast = Some(error);
        while let Some(error) = current {
            if let Some(found) = error.downcast_ref::<Self>() {
                return Some(found);
            }
            current = error.source();
            fast = fast
                .and_then(|error| error.source())
                .and_then(|error| error.source());
            if let (Some(current), Some(fast)) = (current, fast) {
                if std::ptr::eq(current, fast) {
                    return None;
                }
            }
        }
        None
    }
}

impl fmt::Debug for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPolicy => "LimitError::InvalidPolicy",
            Self::TooManyKeys => "LimitError::TooManyKeys",
            Self::CostExceedsCapacity => "LimitError::CostExceedsCapacity",
            Self::MaxHold { .. } => "LimitError::MaxHold",
        })
    }
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPolicy => "limit policy is invalid; check its bounds, keys and windows",
            Self::TooManyKeys => "limit key bound reached; configure max_keys",
            Self::CostExceedsCapacity => "limit cost exceeds window capacity",
            Self::MaxHold { .. } => "limit wait exceeds max_hold",
        })
    }
}

impl std::error::Error for LimitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// Configure it fully, then clone; clones share one admission state.
///
/// Reactive gating is the default; each explicitly added bucket describes its
/// entire policy. The [module example](self) shows four-header application/method limits.
///
/// ```no_run
/// let limit = concord::limit::Limit::new().max_in_flight(4).max_keys(64);
/// let shared = limit.clone();
/// ```
#[derive(Clone)]
#[must_use = "configure the limit and attach it as middleware"]
pub struct Limit {
    inner: Arc<engine::Inner>,
}

impl Limit {
    /// Creates only a shared reactive gate. Defaults: no concurrency cap, 60 second
    /// maximum hold, 1024 keys across all buckets, no proactive or guessed allowance.
    /// Reads `Retry-After` as `Headers::retry_after()` does: seconds or an HTTP date;
    /// zero, a past date or unreadable text holds nothing. An engine retry waits
    /// behind the same gates and is charged like any other call.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new();
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(engine::Inner::new()),
        }
    }

    /// Sets the hold for a 429 with no readable `Retry-After`; the default adds no hold.
    /// Uses the same bucket selectors and global fallback as a readable header.
    /// A readable zero or past date still holds nothing. `MaxHold.retry_after`
    /// remains `None` for this configured fallback; `required` reports the wait.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new()
    ///     .without_retry_after(std::time::Duration::from_secs(1));
    /// ```
    #[must_use = "attach the configured limit to the client"]
    pub fn without_retry_after(self, wait: Duration) -> Self {
        self.inner.lock().policy.without_retry_after = Some(wait);
        self
    }

    /// Adds a body-held concurrency cap. Zero fails the first call with `InvalidPolicy`.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_in_flight(4);
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_in_flight(self, n: usize) -> Self {
        self.inner.lock().policy.max_in_flight = Some(n);
        self
    }

    /// Sets the total admission allowance. Known excessive waits fail immediately;
    /// unknown waits expire at the allowance. Zero permits only immediate admission.
    /// `MaxHold.required` is the delay still needed at rejection;
    /// `retry_after` keeps the server's original value for a contributing 429 gate.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_hold(std::time::Duration::from_secs(10));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_hold(self, longest: Duration) -> Self {
        self.inner.lock().policy.max_hold = longest;
        self
    }

    /// Bounds the total retained bucket-definition/key pairs across all buckets.
    /// Default: 1024. Zero is `InvalidPolicy`. When room is needed, a key is forgotten
    /// only if it has no unexpired quota debt, active 429 hold or call in flight,
    /// including a first response or unread body. Otherwise `TooManyKeys` is returned.
    /// If a call also breaks a static policy rule, `InvalidPolicy` takes precedence.
    /// The [module documentation](self) computes the maximum retained payload per key.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_keys(64);
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_keys(self, n: usize) -> Self {
        self.inner.lock().policy.max_keys = n;
        self
    }

    /// Adds a complete bucket definition. May be called more than once.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new()
    ///     .bucket(concord::limit::Bucket::new(|call| Some(call.info().endpoint().into())));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn bucket(self, bucket: Bucket) -> Self {
        self.inner.lock().policy.buckets.push(bucket);
        self
    }
}

impl Default for Limit {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Limit { .. }")
    }
}

impl Middleware for Limit {
    async fn handle(
        &self,
        request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        let selected = self
            .inner
            .select(&request, &info)
            .map_err(|error| TransportError::blocked().with_source(error))?;
        let (reservation, permit) = self
            .inner
            .reserve(selected)
            .await
            .map_err(|error| TransportError::blocked().with_source(error))?;
        let response = next.send(request, info).await?;
        self.inner.observe(&reservation, &response);
        drop(reservation);
        Ok(match permit {
            Some(permit) => body::hold(response, permit),
            None => response,
        })
    }
}

/// Supplies a learner for paired `capacity:seconds` and `used:seconds` headers.
///
/// Each input header is limited to 4096 bytes. Parses up to eight windows,
/// sorted by period for stable ordering. Both lists
/// must have the same distinct positive periods and positive capacities; missing,
/// malformed, unmatched or oversized input teaches nothing. Remaining is capacity
/// minus used, saturating at zero. All debt lasts a full period because phase is unknown.
/// The [module example](self) attaches two of these learners for four headers.
///
/// ```no_run
/// let bucket = concord::limit::Bucket::new(|_| Some("application".into()))
///     .learn(concord::limit::counts("x-limit", "x-count"));
/// ```
#[must_use = "attach the configured value to its bucket or client"]
// PANIC-SAFETY: This lint exception keeps the public callback type explicit; it permits no panic.
#[allow(clippy::type_complexity)]
pub fn counts(
    limits: impl Into<String>,
    counts: impl Into<String>,
) -> Box<dyn Fn(&Seen<'_>) -> Vec<Window> + Send + Sync> {
    headers::counts(limits.into(), counts.into())
}

/// Supplies a learner for capacity, remaining units and a Unix-second reset.
///
/// Each input header is limited to 4096 bytes. All three must be readable
/// nonnegative integers; capacity must be
/// positive, remaining must not exceed it, and reset must be representable and
/// future. Otherwise nothing is learned. It describes one current fixed window,
/// with one full-capacity refill at that reset, not a guessed repeating period
/// or continuous replenishment rate.
///
/// ```no_run
/// use concord::limit::{Bucket, Limit, remaining};
/// concord::api! {
///     client Queries {
///         base "https://queries.example.test";
///         GET search() -> String { path "search"; read concord::Text; }
///     }
/// }
/// async fn search() -> Result<String, concord::Error> {
///     let limit = Limit::new().bucket(Bucket::new(|_| Some("points".into()))
///         .cost(|_| 20)
///         .learn(remaining("x-ratelimit-limit", "x-ratelimit-remaining", "x-ratelimit-reset")));
///     Queries::new().with_middleware(limit).search().await
/// }
/// ```
#[must_use = "attach the configured value to its bucket or client"]
// PANIC-SAFETY: This lint exception keeps the public callback type explicit; it permits no panic.
#[allow(clippy::type_complexity)]
pub fn remaining(
    limit: impl Into<String>,
    remaining: impl Into<String>,
    reset: impl Into<String>,
) -> Box<dyn Fn(&Seen<'_>) -> Vec<Window> + Send + Sync> {
    headers::remaining(limit.into(), remaining.into(), reset.into())
}
