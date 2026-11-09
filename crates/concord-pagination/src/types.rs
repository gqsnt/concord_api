//! Vocabulary shared by every traversal: pages, resume points, policies, budgets, keyed
//! outcomes, progress and outcomes.

use std::fmt;
use std::num::NonZeroUsize;

/// One page as a traversal sees it: the items an adapter moved out of the provider's envelope, the
/// continuation it selected from the full response (body field, header or link), and any metadata
/// the caller wants to keep.
///
/// The adapter builds it from the whole typed response *before* the item projection consumes it,
/// so a continuation in a header and items in a nested field fit in one value. An envelope that
/// contradicts itself is the adapter's typed error, not an empty page.
///
/// `Debug` prints the number of items and whether a next key exists, never the items, the key or
/// the metadata.
pub struct Page<T, K, M = ()> {
    /// The page's items, moved (never cloned) out of the response.
    pub items: Vec<T>,
    /// The key of the following page; `None` on the provider's last page. For a sequential chain
    /// this is the only source of the next request: the traversal never guesses one.
    pub next: Option<K>,
    /// Response metadata the caller keeps with the page (`()` when none).
    pub meta: M,
}

impl<T, K> Page<T, K> {
    /// A page without metadata.
    pub fn new(items: Vec<T>, next: Option<K>) -> Self {
        Self {
            items,
            next,
            meta: (),
        }
    }
}

impl<T, K, M> fmt::Debug for Page<T, K, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("items", &self.items.len())
            .field("next", &self.next.as_ref().map(|_| Redacted))
            .finish_non_exhaustive()
    }
}

/// A manifest of a discovery chain: descriptors of independent jobs, and the key of the next
/// manifest (`None` on the last one). It is an ordinary [`Page`] whose items are job
/// descriptors.
pub type Manifest<D, MK> = Page<D, MK>;

/// Where a traversal starts: the provider's first page, or the page a key designates.
///
/// `Debug` prints `Start` or `At(..)`, never the key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Resume<K> {
    /// The first page (the request factory receives `None`).
    Start,
    /// The page this key designates (the request factory receives `Some(key)`).
    At(K),
}

impl<K> Resume<K> {
    /// The key, if this is not the start.
    pub fn key(&self) -> Option<&K> {
        match self {
            Self::Start => None,
            Self::At(key) => Some(key),
        }
    }

    /// The factory argument for this position.
    pub(crate) fn to_request(&self) -> Option<K>
    where
        K: Clone,
    {
        self.key().cloned()
    }
}

impl<K> fmt::Debug for Resume<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("Start"),
            Self::At(_) => f.write_str("At(..)"),
        }
    }
}

/// The order in which independent outcomes are returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Order {
    /// Plan (or discovery) order. A completed result whose predecessors are still running is
    /// retained, and retained results occupy window slots. The default.
    #[default]
    Ordered,
    /// The order in which the scheduler observes completions. Nothing is retained for emission.
    Completion,
}

/// What an independent traversal does when a page or job fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OnError {
    /// The first failure the scheduler observes stops admission at once and drops every other
    /// owned future. The default.
    #[default]
    Stop,
    /// Every page or job gets its own keyed outcome; failed keys are retained (see
    /// [`Options::failure_limit`]). A traversal with failures never reports
    /// [`Outcome::Exhausted`].
    Continue,
}

/// A finite count of factory invocations (data pages, jobs or discovery calls).
///
/// A call is charged when it is admitted (its future constructed), not when it succeeds, so
/// failures cannot bypass the budget. It counts *logical* calls: a factory that retries, refreshes
/// credentials or sends subrequests internally makes more HTTP attempts than its budget.
///
/// Without an admission guard, the engine does not compare keys, so the budget is its only
/// protection against a key chain that cycles through more than one page (A → B → A).
/// [`Pages::refuse_repeated_keys`](crate::Pages::refuse_repeated_keys) adds a shared guard that
/// refuses any repeated key before invoking the factory. An adapter can also refuse a key that
/// does not advance (a next key equal to the one just requested) with its own error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Budget(NonZeroUsize);

impl Budget {
    /// At most `max` factory invocations.
    pub const fn new(max: NonZeroUsize) -> Self {
        Self(max)
    }

    /// The number of invocations allowed.
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// A budget as a page cap ([`Pages::max_pages`](crate::Pages::max_pages)).
impl From<Budget> for NonZeroUsize {
    fn from(budget: Budget) -> Self {
        budget.0
    }
}

impl From<NonZeroUsize> for Budget {
    fn from(max: NonZeroUsize) -> Self {
        Self(max)
    }
}

/// Settings of an independent traversal ([`independent`](crate::independent),
/// [`first_page_then`](crate::first_page_then)) and of the job side of
/// [`manifest_jobs`](crate::manifest_jobs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub(crate) budget: Budget,
    pub(crate) window: NonZeroUsize,
    pub(crate) order: Order,
    pub(crate) on_error: OnError,
    pub(crate) failure_limit: Option<NonZeroUsize>,
}

impl Options {
    /// At most `budget` factory invocations, a window of one, [`Order::Ordered`] and
    /// [`OnError::Stop`].
    pub const fn new(budget: Budget) -> Self {
        Self {
            budget,
            window: NonZeroUsize::MIN,
            order: Order::Ordered,
            on_error: OnError::Stop,
            failure_limit: None,
        }
    }

    /// The execution window: futures in flight plus completed results retained for ordered
    /// emission never exceed it.
    pub const fn window(mut self, window: NonZeroUsize) -> Self {
        self.window = window;
        self
    }

    /// The output order.
    pub const fn order(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    /// The failure policy.
    pub const fn on_error(mut self, on_error: OnError) -> Self {
        self.on_error = on_error;
        self
    }

    /// With [`OnError::Continue`], the number of failed keys the traversal retains before it
    /// stops with [`Outcome::FailureLimit`]: the failure that reaches the limit stops admission
    /// like [`OnError::Stop`]. Without a limit the retained failed keys are bounded only by the
    /// budget. Ignored with [`OnError::Stop`].
    pub const fn failure_limit(mut self, limit: NonZeroUsize) -> Self {
        self.failure_limit = Some(limit);
        self
    }
}

/// An outcome tied to the key that produced it and to its position in the plan (or in discovery
/// order), counting from 0 within this traversal.
///
/// `Debug` prints the index only. `Display` (when `V: Display`, typically an error) prints the
/// index and the value, never the key.
pub struct Keyed<K, V> {
    /// Position in the plan or discovery order of this traversal, from 0.
    pub index: u64,
    /// The key the factory received.
    pub key: K,
    /// The page or job result, error or state.
    pub value: V,
}

impl<K, V> fmt::Debug for Keyed<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keyed")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl<K, V: fmt::Display> fmt::Display for Keyed<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}: {}", self.index, self.value)
    }
}

impl<K, E: std::error::Error + 'static> std::error::Error for Keyed<K, E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.value)
    }
}

/// Why an admitted key has no successful emitted outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unfinished {
    /// Its future is running (snapshots only).
    InFlight,
    /// It completed successfully but waits behind an earlier key for ordered emission. A report
    /// hands its result back.
    Retained,
    /// Its page or job failed (whether or not the error was emitted).
    Failed,
    /// Its future was dropped when the traversal stopped.
    Cancelled,
}

/// How a traversal ended.
///
/// Which variants a traversal can produce is documented on its report type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Outcome {
    /// The provider or plan has no more work and every page or job succeeded.
    Exhausted,
    /// The plan has no more work and every admitted key was attempted, but some failed
    /// ([`OnError::Continue`]). Not a complete successful traversal.
    ExhaustedWithFailures,
    /// The page or job [`Budget`] stopped admission while more work remained.
    Bounded,
    /// An observed failure stopped the traversal ([`OnError::Stop`], or any failure of a
    /// sequential chain).
    Failed,
    /// A guarded dependent chain reached a key it had already requested.
    Repeated,
    /// [`OnError::Continue`] reached its [`Options::failure_limit`].
    FailureLimit,
    /// The caller ended the traversal with `finish` while work remained.
    Stopped,
    /// Discovery failed (the first page or its plan, or a manifest): the remaining work is
    /// unknown.
    DiscoveryFailed,
    /// The discovery budget stopped a manifest chain that had a next manifest: the remaining work
    /// is unknown.
    DiscoveryBounded,
}

impl Outcome {
    /// `true` only for [`Outcome::Exhausted`]: everything the provider had was fetched without a
    /// failure.
    pub fn is_success(self) -> bool {
        self == Self::Exhausted
    }

    /// `true` when work may remain undiscovered ([`Outcome::DiscoveryFailed`],
    /// [`Outcome::DiscoveryBounded`]).
    pub fn is_discovery_incomplete(self) -> bool {
        matches!(self, Self::DiscoveryFailed | Self::DiscoveryBounded)
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Exhausted => "exhausted: every page succeeded",
            Self::ExhaustedWithFailures => "exhausted with failures",
            Self::Bounded => "bounded by the budget",
            Self::Failed => "stopped by a failure",
            Self::Repeated => "stopped at a repeated page key",
            Self::FailureLimit => "stopped by the failure limit",
            Self::Stopped => "stopped by the caller",
            Self::DiscoveryFailed => "discovery failed: remaining work unknown",
            Self::DiscoveryBounded => "discovery bounded: remaining work unknown",
        })
    }
}

/// Counters of a traversal at one moment: cheap, `Copy`, no keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Progress {
    /// Factory invocations so far: what the budget has been charged.
    pub admitted: u64,
    /// Futures currently owned and running.
    pub in_flight: usize,
    /// Completed results held for ordered emission.
    pub retained: usize,
    /// Outcomes returned by `next` (successes and emitted failures).
    pub emitted: u64,
    /// Successful outcomes returned by `next`.
    pub succeeded: u64,
    /// Failures observed (emitted or not).
    pub failed: u64,
    /// Futures dropped when the traversal stopped.
    pub cancelled: u64,
}

/// A snapshot of an independent traversal's sparse progress.
///
/// With the plan's keys in order, `admitted` and `unfinished` describe everything: every key of
/// the plan's first `admitted` keys that is not listed in `unfinished` was emitted successfully;
/// keys after them were never started. To resume, run `unfinished` keys (minus those whose
/// retained results you keep) followed by the plan without its first `admitted` keys.
///
/// It clones the unfinished keys: its size is the window plus the retained failed keys.
#[derive(Clone)]
#[non_exhaustive]
pub struct Checkpoint<K> {
    /// Keys taken from the plan (or discovered descriptors admitted) by this traversal.
    pub admitted: u64,
    /// Admitted keys without a successful emitted outcome, in plan order.
    pub unfinished: Vec<Keyed<K, Unfinished>>,
}

impl<K> Checkpoint<K> {
    /// The first key in plan order that was not emitted successfully: the ordered restart point.
    /// `None` when every admitted key succeeded (continue with the plan after `admitted`).
    pub fn restart(&self) -> Option<&K> {
        self.unfinished.first().map(|entry| &entry.key)
    }
}

impl<K> fmt::Debug for Checkpoint<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Checkpoint")
            .field("admitted", &self.admitted)
            .field("unfinished", &self.unfinished.len())
            .finish()
    }
}

impl<K: Clone, V: Clone> Clone for Keyed<K, V> {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }
}

impl<K: PartialEq, V: PartialEq> PartialEq for Keyed<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.key == other.key && self.value == other.value
    }
}

impl<K: Eq, V: Eq> Eq for Keyed<K, V> {}

/// Placeholder printed instead of a key.
pub(crate) struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("..")
    }
}
