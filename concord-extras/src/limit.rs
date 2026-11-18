//! Limits shared by the calls of a client: [`Limit`], a middleware layer (feature `limit`).
//!
//! A `Limit` decides when a request may leave. It never sends a request twice and
//! never changes a response. [`Limit::new`] alone makes the requests after a `429`
//! wait until its `Retry-After` has passed; it guesses no rate and no cap of its own.
//! [`max_in_flight`](Limit::max_in_flight) caps the requests in progress. A [`Bucket`]
//! adds one budget of the API, with windows known in advance or read from each response.
//!
//! Clones of a `Limit` share one state: configure it fully, then give a clone to each
//! client that spends the same budget. The state lives in one process.
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
//! # After a `429`
//!
//! `Retry-After` is read with [`concord_core::Headers::retry_after`]: seconds or an
//! HTTP date. Zero or a past date makes no request wait. A missing or unreadable value
//! makes requests wait for the time set with [`Limit::without_retry_after`], or not at
//! all when none is set. The limit
//! never resends a request. When the engine retries a call, the new attempt waits for
//! the same limits and is counted like any other request.
//!
//! # A cap on concurrency
//!
//! Add a cap when you need one. A request keeps its place until its response body has
//! been read to the end, fails or is dropped, so the place stays taken while the body is unread:
//!
//! ```no_run
//! let limit = concord::limit::Limit::new().max_in_flight(4);
//! ```
//!
//! # The longest wait
//!
//! A request waits at most 60 seconds by default before it may leave. Change that limit
//! with `max_hold`; the call timeout still applies on its own:
//!
//! ```no_run
//! let limit = concord::limit::Limit::new()
//!     .max_hold(std::time::Duration::from_secs(10));
//! ```
//!
//! # Buckets
//!
//! To enforce a rate known in advance, add a bucket with a rolling window. The key
//! function assigns each request to a budget; returning `None` leaves the request out
//! of the bucket:
//!
//! ```no_run
//! use concord::limit::{Bucket, Limit, Window};
//! use std::time::Duration;
//! let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into()))
//!     .window(Window::rolling(10, Duration::from_secs(1))));
//! ```
//!
//! When the server announces its budget, have the bucket learn it from each response
//! instead. [`counts`] reads paired headers of capacities and counts; [`remaining`] reads
//! a capacity, the units left and a reset date in Unix seconds, and [`remaining_rfc3339`]
//! the same with a reset written as an RFC 3339 date-time. Each bucket can also set its
//! own cost (one by default).
//!
//! ```no_run
//! use concord::limit::{Bucket, Limit, counts};
//! let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into()))
//!     .learn(counts("x-limit", "x-count")));
//! ```
//!
//! Several budgets can apply to one request. Here, an application bucket and a method
//! bucket on each host learn two windows each, from four headers. Each bucket owns its
//! key, windows, `learn` function, cost and `hit_when` function; the same key string
//! returned by two different buckets still names two separate budgets.
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
//! A `429` with `Retry-After` makes requests wait on each bucket that counted the
//! request and has no `hit_when`, and on each such bucket whose `hit_when` returns true.
//! If the `429` is about none of these buckets, every request through the limit waits.
//!
//! # What holds for every bucket
//!
//! A request reserves its cost in all the windows of its buckets at once, or in none.
//! Cost spent by a request that was sent is not given back after a failure or a
//! cancellation. For each key of a bucket with no known window, one request at a time
//! may leave to learn the windows. The next request of that key may leave once the
//! response head has been learned from, or once the first request fails or is dropped
//! before that, and only if the other limits allow it. It does not wait for the body.
//! Nothing can prove that this first request fits a budget not yet seen. A bucket that has
//! a cap on the calls in flight ([`Bucket::max_in_flight`]) and no `learn` function is the
//! exception: it can learn nothing, so it lets up to its cap leave at once. A fixed window
//! refills once to its whole capacity at the stated reset; it invents no other reset.
//! Once a learned fixed window's refill cannot pay a request within its capacity, and no
//! request awaiting its head reports a newer reset, that window is no longer known.
//! Its key returns to one learning request at a time; other windows still apply. A fixed
//! window set in advance stays known and needs another reset from a response before it
//! can send again.
//!
//! Missing or unreadable headers teach nothing. Learning never turns a received
//! response into a failed call: spent quota stays spent, and a valid `Retry-After`
//! still makes requests wait. A panic in a key or cost function unwinds before the
//! request is let through, without changing the shared state. If any `learn` or
//! `hit_when` function panics, nothing from that response is learned; a panicking
//! `hit_when` counts as a match. These functions run outside the lock on the state, so a
//! panic never blocks other calls or makes the limit unusable. No panic payload enters
//! diagnostics; the author's panic hook still runs. The limit starts no task.
//!
//! # Memory
//!
//! By default the limit keeps state for at most 1024 pairs of bucket and key, across
//! all buckets; `max_keys` sets that bound. When it needs room, it forgets a key that
//! holds no unexpired quota debt, no active wait after a `429` and no request in progress
//! (until the end of its body, including a first request still awaiting its response).
//! Learned windows are then learned again; configured windows still apply. Other bounds
//! are constants: 8 windows per bucket, 512 UTF-8 bytes per key, 4096 history entries
//! per window. On 64-bit Linux, one key's retained payload is at most 1,050,112 bytes:
//! 512 key bytes + 256 bytes for the map entry + 8 × (96 window bytes +
//! 4096 × 32 debit bytes). Spare slots of the hash table, allocator metadata, the shared
//! settings and the calls and bodies owned by the caller come on top of this bound.
//! A full history merges its two oldest entries at the later expiry, which errs on the
//! safe side and never fails a call. A configuration that breaks a rule takes precedence
//! over exceeding `max_keys`. Invalid learned windows teach nothing. The `learn`,
//! `hit_when`, key and cost functions bound their own temporary allocations.
//!
//! # What a limit does not cover
//!
//! Share one limit across all clients that spend the same budget: a limit does not
//! coordinate with other processes or with separate `Limit` values, and it cannot see a
//! budget that the server reports only in the body. The Tower adapter can also enforce a
//! rate on your side; its wait for readiness counts as idle time of the transport, and
//! Tower's usual concurrency layer releases its place at the response head.

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

/// What a key or cost function receives: the built request and its endpoint.
///
/// More inputs may be added later.
///
/// ```no_run
/// fn endpoint(call: &concord::limit::RequestInfo<'_>) -> &'static str { call.info().endpoint() }
/// ```
pub struct RequestInfo<'a> {
    request: &'a Request,
    info: &'a TryInfo,
}

impl RequestInfo<'_> {
    /// Returns the built request, read-only: the function cannot change it.
    ///
    /// ```no_run
    /// fn host(call: &concord::limit::RequestInfo<'_>) -> Option<String> {
    ///     Some(call.request().url().host_str()?.to_owned())
    /// }
    /// ```
    pub fn request(&self) -> &Request {
        self.request
    }

    /// Returns the endpoint's identity and the number of this attempt.
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

/// Describes a budget for each key: which requests it counts, its windows, and which `429` is about it.
///
/// A bucket combines its key function, the windows known in advance, its `learn`
/// function, its cost and its `hit_when` function. A key of `None` or a cost of zero
/// leaves the request out of this bucket entirely: the request neither waits for the
/// first response of the bucket nor waits after a `429` about the bucket. Each pair of
/// a bucket and a key has its own budget, so the same key string returned by two
/// different buckets names two separate budgets, and [`Limit::max_keys`] counts these
/// pairs.
///
/// A clone is independent: it copies the windows and shares only the key, cost, `learn`
/// and `hit_when` functions, which cannot change, so configuring a clone never changes
/// another. What a bucket has spent and learned belongs to the `Limit` it is added to.
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
    max_in_flight: Option<usize>,
}

impl Bucket {
    /// A bucket whose function names the budget of each request; `None` leaves the request out.
    ///
    /// By default the cost is one, no window is known, and every `429` is about this
    /// bucket. A panic in the key or cost function unwinds that call before the state
    /// of the limit changes; other calls can go on using the limit. While a key has no
    /// known window, one request of that key at a time may leave: the next one may leave
    /// once the response head has been learned from, or once the earlier request has
    /// failed or been dropped, and only if the other limits allow it. It does not wait for
    /// the body. If the responses teach nothing, that lasts for good. A bucket with a
    /// [`max_in_flight`](Self::max_in_flight) cap and no [`learn`](Self::learn) function is
    /// the exception: nothing can be learned, so its cap is its limit.
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
            max_in_flight: None,
        }
    }

    /// Adds a window known in advance; call it again to add more, up to eight.
    ///
    /// A window set here is where the budget of each key starts. When [`learn`](Self::learn)
    /// returns a window at the same position, the capacity that the API reports replaces
    /// this one, also upward, from that response on; what is left can still only go down.
    /// The same code can then serve keys of different sizes: Riot gives a development key
    /// 20 requests a second and a production key far more, so a bucket declared for the
    /// first learns the second from its headers.
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

    /// Allows at most `n` requests of each key of this bucket at once; a request keeps its
    /// place until its response body is read to the end, fails or is dropped.
    ///
    /// By default a bucket has no cap of its own; [`Limit::max_in_flight`] caps the whole
    /// limit, and a client can use both: five calls for each tenant, and twenty in all. They
    /// apply with the windows, all at once: a request that waits for a place holds nothing
    /// in another bucket. A bucket that has a cap and no [`learn`](Self::learn) function
    /// lets up to `n` requests of a key leave at once, from the first call; a bucket that
    /// learns still lets one request of a key leave at a time until a response has taught
    /// it a window (see [`new`](Self::new)). A request that finds no free place before
    /// `max_hold` ends fails with `MaxHold`, whose `required` is `None`; when another wait
    /// that is known, a window or a `429`, is longer than what is left of `max_hold`, the
    /// request fails at once instead, and `required` is that wait. A cap of zero fails
    /// every call with `InvalidPolicy` before any key or cost function runs.
    ///
    /// The cap counts what this process holds, and does not protect against three things.
    /// A request dropped by its caller, or by a call `timeout`, frees its place at once,
    /// even while the server still works on it. A response whose body is kept and never
    /// read holds its place for as long as it is kept. And a key with a request in progress
    /// is never forgotten, so that its count is never lost: when every key that
    /// [`Limit::max_keys`] allows has a request in progress, a request with a new key fails
    /// at once with `TooManyKeys`.
    ///
    /// ```no_run
    /// use concord::limit::{Bucket, Limit};
    /// concord::api! {
    ///     client Books {
    ///         base "https://books.example.test";
    ///         GET invoices(tenant: String) -> String {
    ///             path "invoices";
    ///             headers { "tenant-id" = tenant; }
    ///             read concord::Text;
    ///         }
    ///     }
    /// }
    /// async fn invoices(tenant: String) -> Result<String, concord::Error> {
    ///     // Five calls in progress at once for each tenant, whatever the other tenants do.
    ///     let per_tenant = Bucket::new(|call| {
    ///         Some(call.request().headers().get("tenant-id")?.to_str().ok()?.to_owned())
    ///     })
    ///     .max_in_flight(5);
    ///     Books::new().with_middleware(Limit::new().bucket(per_tenant)).invoices(tenant).await
    /// }
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_in_flight(self, n: usize) -> Self {
        Self {
            max_in_flight: Some(n),
            ..self
        }
    }

    /// Sets the function that reads the windows from each response.
    ///
    /// By default a bucket learns nothing. An empty list teaches nothing.
    ///
    /// The list describes every window of the bucket, in order: a window's position
    /// identifies it across responses and the windows set with [`window`](Self::window).
    /// The header helpers keep that order by sorting rolling periods. A list that is
    /// incompatible, too large or invalid is ignored as a whole. What is learned can
    /// only tighten an active window, taking into account the cost of other requests in
    /// progress; an older reset is ignored. No result fails the request that received
    /// the response. A learned fixed window whose refill cannot pay a request within
    /// its capacity and has no newer reset becomes unknown again once the outstanding
    /// heads have arrived, or their requests failed or were dropped; its key returns to
    /// one learning request at a time (see [`Window`]).
    /// For a window without `remaining`,
    /// the cost of the request that received the response is counted against the capacity.
    ///
    /// If any `learn` or `hit_when` function panics, everything learned from that
    /// response is discarded, the response is still returned, and other calls go on.
    /// The author's panic hook still runs. While a key has no known window, the next
    /// request of that key may leave once the head has been learned from, without waiting
    /// for the body, or after a failure or drop before the head; the other limits still
    /// apply.
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

    /// Sets the function that computes this bucket's cost for a request.
    ///
    /// The cost is one by default; a cost of zero leaves the request out of the bucket.
    /// Two buckets can count the same request differently, one in points and the other
    /// in requests. A panic in the function unwinds that call before the request is let
    /// through, without blocking other calls.
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

    /// Says which `429` is about this bucket; without it, every `429` is.
    ///
    /// The wait that a `429` imposes (its `Retry-After`, or the wait set by
    /// [`Limit::without_retry_after`]) applies to this bucket when the function returns
    /// true. If the `429` is about none of the buckets that counted the request, every
    /// request through the limit waits instead. A panic in the function counts as a
    /// match, to be safe; it discards everything learned from that response, and the
    /// response is still returned. Other calls go on.
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

/// One window of a budget: at most a number of units in a period, rolling or fixed.
///
/// A rolling window keeps every unit spent, counted here or reported by the server,
/// for a full period. A fixed window refills once to its whole capacity at its stated
/// reset; it invents no repeating period. A learned fixed window becomes unknown again
/// once its refill cannot pay a request within its capacity and no outstanding head
/// reports a newer reset.
/// Its key returns to one learning request at a time, under the other windows, and can
/// be forgotten when it holds nothing. A fixed window set with [`Bucket::window`] stays
/// known: once its refill is spent, it needs another reset from a response. Without one,
/// each further request fails with `MaxHold` after waiting `max_hold`.
///
/// An invalid window added with [`Bucket::window`] fails the request before it is sent;
/// an invalid window returned by `learn` teaches nothing. `Debug` prints no settings.
///
/// A clone is independent: it copies the window's settings, so configuring a clone
/// never changes the original.
///
/// A rate known in advance needs no function to read the response headers:
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
    // The reset of a fixed window as the server dated it, when a reader of dates knows it.
    reset_at: Option<SystemTime>,
}

#[derive(Clone)]
enum Kind {
    Rolling(Duration),
    Fixed(Option<Instant>),
}

impl Window {
    /// A rolling window: at most `capacity` units in any `period`. Both must be positive.
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
            reset_at: None,
        }
    }

    /// A fixed window: a budget of `capacity` units that refills once, `resets_in` from now.
    ///
    /// At that reset the whole capacity is available again. No further reset is invented.
    /// A learned window becomes unknown again once that refill cannot pay a request within
    /// its capacity and no outstanding head reports a newer reset; a window set with
    /// [`Bucket::window`] stays known and needs another reset from a response (see
    /// [`Window`]). The capacity must be positive; a `resets_in` of zero refills at once.
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
            reset_at: None,
        }
    }

    /// Sets the units that the server reports as left after the request that received
    /// the response.
    ///
    /// A value above the capacity is invalid, and a reported value never gives back
    /// units already spent.
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

/// What a `learn` or `hit_when` function receives: the status and headers of one response.
///
/// Each bucket that counted the request receives its own `Seen`. More context may be
/// added later. Only the methods return the values: `Debug` prints neither the key nor
/// any header value.
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
    /// Returns the key that this bucket's key function returned for the request.
    ///
    /// ```no_run
    /// fn key(seen: &concord::limit::Seen<'_>) -> String { seen.key().to_owned() }
    /// ```
    pub fn key(&self) -> &str {
        self.key
    }

    /// Returns the response status. The body is not read.
    ///
    /// ```no_run
    /// fn refused(seen: &concord::limit::Seen<'_>) -> bool { seen.status() == 429 }
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers.
    ///
    /// ```no_run
    /// fn names(seen: &concord::limit::Seen<'_>) -> usize { seen.headers().len() }
    /// ```
    pub fn headers(&self) -> &HeaderMap {
        self.headers
    }

    /// Returns a header's value if it appears exactly once and holds only visible ASCII,
    /// spaces and tabs.
    ///
    /// A repeated header returns `None`, so it teaches nothing. This differs from
    /// [`Headers::get`](concord_core::Headers::get), which returns the first value of a
    /// repeated header.
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

    /// Returns the wall-clock time read when the response was received.
    ///
    /// ```no_run
    /// fn now(seen: &concord::limit::Seen<'_>) -> std::time::SystemTime { seen.now() }
    /// ```
    pub fn now(&self) -> SystemTime {
        self.now
    }

    /// Returns the time from `now()` until a date in Unix seconds.
    ///
    /// Returns zero if the date is past, and `None` if the date cannot be represented.
    /// The limit itself waits on its internal timer, not on the wall clock.
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

    /// Returns the time until an RFC 3339 date-time, such as `2026-11-17T12:00:30Z`, counted
    /// on the server's clock when the response gives it.
    ///
    /// The time is counted from the response's `Date` header when it appears once, holds at
    /// most 4096 bytes and is an HTTP date in any of its three forms, read as `Retry-After` is
    /// read (`Sun, 06 Nov 1994 08:49:37 GMT`, and the two obsolete forms), so that a wrong clock
    /// in this process does not move the reset. As that grammar allows, `23:59:60` is read as
    /// the next second on any day, also on one when UTC had no leap second, and a two-digit year
    /// takes its century from this process's clock. Without such a header it is counted from
    /// [`now()`](Self::now), and a wrong clock then costs this much: a clock that is fast by
    /// some minutes makes each reset that much nearer, so a window refills that much early,
    /// and a reset nearer than the error looks past and teaches nothing; a clock that is
    /// slow makes requests wait that much longer than the server needs. The limit then waits
    /// on its internal timer, not on the wall clock. [`until_unix`](Self::until_unix) always
    /// counts from `now()`, and has no bound of fifty years.
    ///
    /// Accepted: a full date, then `T`, `t` or one space, a full time with seconds, an
    /// optional fraction, then `Z`, `z` or an offset such as `+01:00`; spaces and tabs around
    /// the text are ignored. A leap second, `23:59:60` UTC on the last day of a month, counts
    /// as the next second. A fraction finer than a nanosecond is rounded up, never down.
    /// Returns zero if the date is past. Returns `None` for any other separator, a date that
    /// does not exist, a malformed offset, a leap second at another time, a text over 4096
    /// bytes, a date that cannot be represented, or a reset more than fifty years (of 365.2425
    /// days) ahead, as an HTTP date's two-digit year is never read further ahead.
    ///
    /// ```no_run
    /// fn reset(seen: &concord::limit::Seen<'_>) -> Option<std::time::Duration> {
    ///     seen.until_rfc3339(seen.header("x-reset")?)
    /// }
    /// ```
    pub fn until_rfc3339(&self, text: &str) -> Option<Duration> {
        headers::until(headers::rfc3339(text)?, self.server_now())
    }

    /// The response's `Date` when it is one valid HTTP date; otherwise the local clock.
    fn server_now(&self) -> SystemTime {
        self.header("date")
            .and_then(|date| headers::http_date(date, self.now))
            .unwrap_or(self.now)
    }
}

impl fmt::Debug for Seen<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seen { .. }")
    }
}

/// Why a `Limit` refused to send a request.
///
/// It carries no request, key or header text. Each reason comes from the settings or
/// from the waits, with no lower-level cause, so `source()` is `None`. The middleware
/// keeps this typed error as the source of the call's error. `MaxHold` does not set the
/// engine's `retry_after()`, which comes from a response. Once a response is received,
/// a `Limit` returns no error for learning or for reading headers.
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
    /// The configuration is invalid: a bound, a key size, a window or a setting,
    /// including a cap of zero on concurrency. Check the bounds, the keys and the windows
    /// of the configuration.
    ///
    /// It takes precedence when the request would also exceed `max_keys`.
    InvalidPolicy,
    /// The keys already kept, across all buckets, plus the new keys of the request would exceed
    /// `max_keys`, even after forgetting the keys that hold nothing. A request can be refused
    /// with fewer than `max_keys` keys of its own, when other keys are still in use. Raise the
    /// bound with [`Limit::max_keys`].
    TooManyKeys,
    /// The bucket's cost is larger than the capacity of at least one known window.
    CostExceedsCapacity,
    /// The request would wait longer than `max_hold` before it could leave.
    ///
    /// For an operation that is safe to repeat, wait for the known delay still needed,
    /// then call again:
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
        /// The wait still needed when the request was refused, not the original wait;
        /// `None` if unknown.
        required: Option<Duration>,
        /// The original `Retry-After` from the server, for a `429` that contributed to this wait.
        retry_after: Option<Duration>,
        /// The time already spent waiting before the request was refused.
        waited: Duration,
    },
}

impl LimitError {
    /// Finds a `LimitError` in this error or its `source()` chain.
    ///
    /// Returns `None` when there is none, also when the chain is cyclic.
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
            Self::InvalidPolicy => "the limit policy is invalid",
            Self::TooManyKeys => "limit keys exceed max_keys",
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

/// A middleware layer that makes requests wait for the limits of an API.
///
/// By default it only makes requests wait after a `429`; each bucket you add describes
/// the budget of each key that its key function returns. The [module example](self) shows an application
/// limit and a method limit read from four headers.
///
/// Clones share one state and one configuration: a setter called on any clone changes
/// every clone. Configure it fully, then clone.
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
    /// A limit with no bucket and no cap: after a `429` with `Retry-After`, the next requests wait.
    ///
    /// Defaults: no cap on concurrency, a longest wait of 60 seconds, 1024 keys across
    /// all buckets, and no budget set or guessed in advance. It reads `Retry-After` as
    /// `Headers::retry_after()` does: seconds or an HTTP date; zero or a past date makes
    /// no request wait, and so does a missing or unreadable value unless
    /// [`without_retry_after`](Self::without_retry_after) is set. When the engine retries
    /// a call, the new attempt waits for the same limits and is counted like any other
    /// request.
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

    /// Sets the wait after a `429` with no readable `Retry-After`; by default there is none.
    ///
    /// The wait applies to buckets through the same `hit_when` functions as a readable
    /// header, and to every request when the `429` is about no bucket. A readable zero
    /// or past date still makes no request wait. For this configured wait,
    /// `MaxHold.retry_after` stays `None`; `required` reports the wait.
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

    /// Allows at most `n` requests at once; a request keeps its place until its response
    /// body is read to the end, fails or is dropped.
    ///
    /// By default there is no cap. A cap of zero fails the first call with `InvalidPolicy`.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_in_flight(4);
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_in_flight(self, n: usize) -> Self {
        self.inner.lock().policy.max_in_flight = Some(n);
        self
    }

    /// Sets the longest total wait of a request: 60 seconds by default.
    ///
    /// It bounds waiting: a request that needs no wait leaves, for every valid value of this
    /// setting. A value that no deadline can represent, such as `Duration::MAX`, is invalid:
    /// every call fails with `InvalidPolicy`, even one that needs no wait. A request whose known wait is longer fails at once; a request whose wait is
    /// unknown fails when this limit is reached. Zero lets a request leave only if it needs
    /// no wait. The error is `MaxHold`: `required` is the wait still needed when
    /// the request was refused, and `retry_after` keeps the server's original value for
    /// a `429` that contributed to the wait.
    ///
    /// The time counts from when the limit receives the request, the key and cost functions
    /// of the buckets included: a request that must wait may wait until this limit has
    /// passed since then, and fails with `MaxHold` if its wait would end later. A request
    /// that needs no wait leaves whatever those functions took: their time is the author's.
    /// They run to their end, so a call that fails can return later than this limit by the
    /// time they take, with nothing sent. The limit is checked when the waiting task runs
    /// again, and Tokio's timer wakes with a granularity of about a millisecond: a request
    /// whose wait and `max_hold` are both shorter than that can be refused after waiting.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_hold(std::time::Duration::from_secs(10));
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_hold(self, longest: Duration) -> Self {
        self.inner.lock().policy.max_hold = longest;
        self
    }

    /// Bounds the pairs of bucket and key kept in memory, across all buckets: 1024 by default.
    ///
    /// Zero is `InvalidPolicy`. When room is needed, a key is forgotten only if it holds
    /// no unexpired quota debt, no active wait after a `429` and no request in progress,
    /// including one awaiting its first response or with an unread body. Otherwise the
    /// request fails with `TooManyKeys`. If the request also breaks a rule of the
    /// configuration, `InvalidPolicy` takes precedence. The [module documentation](self)
    /// computes the most memory kept for each key.
    ///
    /// ```no_run
    /// let limit = concord::limit::Limit::new().max_keys(64);
    /// ```
    #[must_use = "attach the configured value to its bucket or client"]
    pub fn max_keys(self, n: usize) -> Self {
        self.inner.lock().policy.max_keys = n;
        self
    }

    /// Adds a bucket with its whole definition; call it again to add more.
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
        // `max_hold` counts from here: the key and cost functions take part of it.
        let start = Instant::now();
        let selected = self
            .inner
            .select(&request, &info)
            .map_err(|error| TransportError::blocked().with_source(error))?;
        let (reservation, permit) = self
            .inner
            .reserve(selected, start)
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

/// Returns a `learn` function for paired headers that list `capacity:seconds` and `used:seconds`.
///
/// Each header may be at most 4096 bytes. It reads up to eight windows, sorted by
/// period so that their order is stable. Both lists must have the same distinct
/// positive periods, and the capacities must be positive; missing, malformed,
/// unmatched or oversized input teaches nothing. The units left are the capacity minus
/// the units used, saturating at zero. Every unit used is counted for a full period,
/// because the phase of the window is unknown. The [module example](self) attaches two
/// of these functions, for four headers.
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

/// Returns a `learn` function for headers of a capacity, the units left and a reset in Unix seconds.
///
/// Each header may be at most 4096 bytes. All three must be readable nonnegative
/// integers; the capacity must be positive, the units left must not exceed it, and the
/// reset must be representable and in the future. Otherwise nothing is learned. The
/// result is one current fixed window that refills once, to its whole capacity, at that
/// reset: not a guessed repeating period, nor a continuous rate of refill.
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

/// Returns a `learn` function for headers of a capacity, the units left and a reset as an RFC 3339 date-time.
///
/// It reads as [`remaining`] does, with the reset written as a date-time such as
/// `2026-11-17T12:00:30Z`, read as [`Seen::until_rfc3339`] reads it: counted from the
/// response's `Date` header when it has a valid one, otherwise from the local clock. Each
/// header may be at most 4096 bytes. The capacity and the units left must be readable
/// nonnegative integers, written with digits only; the capacity must be positive, the units
/// left must not exceed it, and the reset must be in the future. Otherwise nothing is
/// learned. The result is one current fixed window that refills once, to its whole
/// capacity, at that reset. A later response that reports less left, for the same reset or a
/// later one, lowers what is left at once, and the later of the two deadlines is kept, even
/// when its own `Date` makes its reset look nearer. A later response that names the same reset
/// again, after that refill, teaches nothing, however late it arrives, also once the key has
/// returned to learning. The key remembers the last reset it consumed, at each position, for as
/// long as the limit keeps the key, also when a response gives it fewer windows or a window
/// without a date; a key that is forgotten to make room forgets it too. This assumes that the
/// resets an API gives for one key move forward: two servers whose clocks differ by a second
/// can still name one refill twice.
///
/// ```no_run
/// use concord::limit::{remaining_rfc3339, Bucket, Limit};
/// concord::api! {
///     client Messages {
///         base "https://messages.example.test";
///         POST create() -> String { path "messages"; read concord::Text; }
///     }
/// }
/// async fn create() -> Result<String, concord::Error> {
///     let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into())).learn(remaining_rfc3339(
///         "x-requests-limit",
///         "x-requests-remaining",
///         "x-requests-reset",
///     )));
///     Messages::new().with_middleware(limit).create().await
/// }
/// ```
#[must_use = "attach the configured value to its bucket or client"]
// PANIC-SAFETY: This lint exception keeps the public callback type explicit; it permits no panic.
#[allow(clippy::type_complexity)]
pub fn remaining_rfc3339(
    limit: impl Into<String>,
    remaining: impl Into<String>,
    reset: impl Into<String>,
) -> Box<dyn Fn(&Seen<'_>) -> Vec<Window> + Send + Sync> {
    headers::remaining_rfc3339(limit.into(), remaining.into(), reset.into())
}
