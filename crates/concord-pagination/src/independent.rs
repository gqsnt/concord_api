//! Independent pages: a known plan of keys, or a plan derived from the first page.

use std::fmt;
use std::future::{Future, poll_fn};
use std::iter::Peekable;
use std::task::{Context, Poll};

use crate::types::{Checkpoint, Keyed, Options, Outcome, Progress, Unfinished};
use crate::window::{Emitted, Halt, Step, Window};

/// Runs a known plan of independent keys with a bounded window.
///
/// `keys` is consumed lazily, one key per admission (plus at most one key of look-ahead to tell
/// [`Outcome::Bounded`] from [`Outcome::Exhausted`]); a huge or endless plan allocates nothing up
/// front. `fetch` constructs an owned future per key; the futures are polled concurrently on the
/// caller's task (no spawned task, no `Send` or `'static` requirement). That the provider permits
/// independent, concurrent addressing of these keys is the adapter's knowledge, not something a
/// count implies.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{independent, Budget, Options, Outcome};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// async fn page(number: u32) -> Result<Vec<u32>, std::io::Error> {
///     Ok(vec![number * 10, number * 10 + 1])
/// }
///
/// let options = Options::new(Budget::new(NonZeroUsize::new(100).unwrap()))
///     .window(NonZeroUsize::new(3).unwrap());
/// let mut pages = independent(1..=4, options, page);
/// let mut items = Vec::new();
/// while let Some(outcome) = pages.next().await {
///     let page = outcome.map_err(|failed| failed.value).unwrap();
///     items.extend(page.value); // plan order (the default)
/// }
/// assert_eq!(items, [10, 11, 20, 21, 30, 31, 40, 41]);
/// assert_eq!(pages.finish().outcome, Outcome::Exhausted);
/// # }
/// ```
pub fn independent<K, I, R, E, F, Fut>(
    keys: I,
    options: Options,
    fetch: F,
) -> Independent<K, I, R, E, F, Fut>
where
    K: Clone,
    I: IntoIterator<Item = K>,
    F: FnMut(K) -> Fut,
    Fut: Future<Output = Result<R, E>>,
{
    Independent {
        fetch,
        window: Window::new(&options),
        budget: options.budget.get() as u64,
        plan: Plan::Ready(keys.into_iter().peekable()),
        first: None,
        ended: None,
    }
}

/// Fetches `first` once, derives the remaining keys from its result, then runs them like
/// [`independent`].
///
/// The first page is admitted alone, charged once against the budget, and emitted once (index 0).
/// `plan` sees the first result before it is emitted and returns the remaining keys as a lazy
/// iterable: validate the advertised total there and return a range or another lazy iterator, not
/// a collected `Vec`, so that a huge total allocates nothing and the budget and window still bound
/// dispatch. A `plan` error means the discovery envelope is inconsistent: the first page is then a
/// failure (not emitted as a success), there is no remaining plan, and the outcome is
/// [`Outcome::DiscoveryFailed`]. The same holds when the first page itself fails, in either
/// error mode.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{first_page_then, Budget, Options, Outcome};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// struct Listing { total_pages: u64, items: Vec<u64> }
/// #[derive(Debug)]
/// struct Protocol(&'static str);
///
/// async fn page(number: u64) -> Result<Listing, Protocol> {
///     Ok(Listing { total_pages: 3, items: vec![number] })
/// }
///
/// let options = Options::new(Budget::new(NonZeroUsize::new(1000).unwrap()))
///     .window(NonZeroUsize::new(2).unwrap());
/// let mut pages = first_page_then(1, options, page, |first: &Listing| {
///     if first.total_pages == 0 {
///         return Err(Protocol("a listing with items has at least one page"));
///     }
///     Ok(2..=first.total_pages) // lazy: nothing allocated for a huge total
/// });
/// let mut items = Vec::new();
/// while let Some(outcome) = pages.next().await {
///     items.extend(outcome.unwrap().value.items);
/// }
/// assert_eq!(items, [1, 2, 3]);
/// let report = pages.finish();
/// assert_eq!(report.outcome, Outcome::Exhausted);
/// assert_eq!(report.progress.admitted, 3); // the first page counted once
/// # }
/// ```
pub fn first_page_then<K, I, R, E, F, Fut, P>(
    first: K,
    options: Options,
    fetch: F,
    plan: P,
) -> Independent<K, I, R, E, F, Fut, P>
where
    K: Clone,
    I: IntoIterator<Item = K>,
    F: FnMut(K) -> Fut,
    Fut: Future<Output = Result<R, E>>,
    P: FnOnce(&R) -> Result<I, E>,
{
    Independent {
        fetch,
        window: Window::new(&options),
        budget: options.budget.get() as u64,
        plan: Plan::Waiting(plan),
        first: Some(first),
        ended: None,
    }
}

enum Plan<I: Iterator, P> {
    /// First-page discovery: the plan is built from the first result.
    Waiting(P),
    Ready(Peekable<I>),
    /// The first page or its plan failed: no remaining plan.
    Missing,
}

/// A bounded traversal of independent keys; see [`independent`] and [`first_page_then`].
///
/// # Window, order and errors
///
/// Futures in flight plus completed results retained for ordered emission never exceed
/// [`Options::window`]. With [`Order::Ordered`](crate::Order::Ordered) a stalled early key keeps
/// at most `window - 1` later results waiting behind it, and admission pauses until it completes.
///
/// With [`OnError::Stop`](crate::OnError::Stop), the first failure the scheduler *observes*
/// (whatever its plan position) is returned at once, even while an earlier key is stalled; no
/// key is admitted after it and every other future is dropped. The report then names the failed
/// key, the ordered restart point (the first key not emitted) and hands back the completed
/// results that were retained but never emitted. With
/// [`OnError::Continue`](crate::OnError::Continue), every key gets a keyed outcome, in plan order
/// or completion order, and failed keys are retained apart from the emission cursor.
///
/// # Storage
///
/// Besides the window, the traversal retains the key of every failed page until it ends (the
/// failure history is library-owned): at most one key with `Stop`, at most
/// [`Options::failure_limit`] (else the budget) with `Continue`. Successful outcomes are handed to
/// the caller and not remembered.
///
/// # Cancellation
///
/// [`next`](Self::next) is cancellation-safe: futures in flight and retained results belong to the
/// traversal. Dropping the traversal drops them; no request is admitted afterwards and nothing
/// continues locally (a request already transmitted may still run on the server).
pub struct Independent<K, I, R, E, F, Fut, P = fn(&R) -> Result<I, E>>
where
    I: IntoIterator<Item = K>,
{
    fetch: F,
    window: Window<K, R, E, Fut>,
    budget: u64,
    plan: Plan<I::IntoIter, P>,
    first: Option<K>,
    ended: Option<Outcome>,
}

impl<K, I, R, E, F, Fut, P> Independent<K, I, R, E, F, Fut, P>
where
    K: Clone,
    I: IntoIterator<Item = K>,
    F: FnMut(K) -> Fut,
    Fut: Future<Output = Result<R, E>>,
    P: FnOnce(&R) -> Result<I, E>,
{
    /// The next keyed outcome, or `None` when the traversal ended.
    ///
    /// With [`OnError::Stop`](crate::OnError::Stop) an `Err` ends the traversal; with
    /// [`OnError::Continue`](crate::OnError::Continue) it is one key's outcome among others.
    pub async fn next(&mut self) -> Option<Result<Keyed<K, R>, Keyed<K, E>>> {
        poll_fn(|cx| self.poll_outcome(cx)).await
    }

    fn poll_outcome(&mut self, cx: &mut Context<'_>) -> Poll<Option<Emitted<K, R, E>>> {
        loop {
            if self.ended.is_some() {
                return Poll::Ready(None);
            }
            if let Some(outcome) = self.window.take_ready() {
                return Poll::Ready(Some(self.discovered(outcome)));
            }
            self.admit();
            match self.window.poll_active(cx) {
                Step::Emit(outcome) => return Poll::Ready(Some(self.discovered(outcome))),
                Step::Again => {}
                Step::Pending => return Poll::Pending,
                Step::Idle => {
                    self.ended = Some(self.natural_outcome());
                    return Poll::Ready(None);
                }
            }
        }
    }

    /// Admits keys while the window has room and the budget allows.
    fn admit(&mut self) {
        while self.window.has_room() && self.window.admitted() < self.budget {
            let key = match &mut self.plan {
                Plan::Ready(keys) => keys.next(),
                Plan::Waiting(_) => self.first.take(),
                Plan::Missing => None,
            };
            let Some(key) = key else {
                break;
            };
            let future = (self.fetch)(key.clone());
            self.window.admit(key, future);
        }
    }

    /// Builds the plan from the first page's result before it is emitted.
    fn discovered(&mut self, outcome: Emitted<K, R, E>) -> Emitted<K, R, E> {
        if !matches!(self.plan, Plan::Waiting(_)) {
            return outcome;
        }
        let Plan::Waiting(plan) = std::mem::replace(&mut self.plan, Plan::Missing) else {
            return outcome;
        };
        match outcome {
            Ok(page) => match plan(&page.value) {
                Ok(keys) => {
                    self.plan = Plan::Ready(keys.into_iter().peekable());
                    Ok(page)
                }
                Err(value) => {
                    self.window.reclassify_failed(page.index, &page.key);
                    Err(Keyed {
                        index: page.index,
                        key: page.key,
                        value,
                    })
                }
            },
            Err(failed) => Err(failed),
        }
    }

    fn natural_outcome(&mut self) -> Outcome {
        match self.window.halted() {
            Some(Halt::Stopped) => return Outcome::Stopped,
            _ if matches!(self.plan, Plan::Missing) => return Outcome::DiscoveryFailed,
            Some(Halt::Failure) => return Outcome::Failed,
            Some(Halt::FailureLimit) => return Outcome::FailureLimit,
            None => {}
        }
        let more = match &mut self.plan {
            Plan::Ready(keys) => keys.peek().is_some(),
            Plan::Waiting(_) => true,
            Plan::Missing => false,
        };
        if more {
            Outcome::Bounded
        } else if self.window.failures() > 0 {
            Outcome::ExhaustedWithFailures
        } else {
            Outcome::Exhausted
        }
    }

    /// Whether another key could be admitted now (ignoring the window's room).
    fn can_admit(&mut self) -> bool {
        if self.window.halted().is_some() || self.window.admitted() >= self.budget {
            return false;
        }
        match &mut self.plan {
            Plan::Ready(keys) => keys.peek().is_some(),
            Plan::Waiting(_) => self.first.is_some(),
            Plan::Missing => false,
        }
    }

    /// Counters, including the window's in-flight and retained occupancy.
    pub fn progress(&self) -> Progress {
        self.window.progress()
    }

    /// A snapshot of sparse progress: keys admitted, and every admitted key without a successful
    /// emitted outcome (in flight, retained, failed). Clones those keys.
    pub fn checkpoint(&self) -> Checkpoint<K> {
        self.window.checkpoint()
    }

    /// Ends the traversal and reports how. Futures in flight are dropped (outcome
    /// [`Outcome::Stopped`] unless the traversal had already ended).
    pub fn finish(mut self) -> IndependentReport<K, I, R, E> {
        let outcome = if let Some(outcome) = self.ended {
            outcome
        } else if self.window.is_settled() && !self.can_admit() {
            self.natural_outcome()
        } else {
            self.window.halt(Halt::Stopped);
            Outcome::Stopped
        };
        let progress = self.window.progress();
        let settled = self.window.settle();
        IndependentReport {
            outcome,
            progress,
            unfinished: settled.unfinished,
            retained: settled.retained,
            unemitted_errors: settled.unemitted_errors,
            rest: match self.plan {
                Plan::Ready(keys) => Some(keys),
                Plan::Waiting(_) | Plan::Missing => None,
            },
        }
    }
}

impl<K, I, R, E, F, Fut, P> fmt::Debug for Independent<K, I, R, E, F, Fut, P>
where
    K: Clone,
    I: IntoIterator<Item = K>,
    Fut: Future<Output = Result<R, E>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Independent")
            .field("progress", &self.window.progress())
            .field("budget", &self.budget)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

/// How an independent traversal ended ([`Independent::finish`]).
///
/// Outcomes: [`Outcome::Exhausted`], [`Outcome::ExhaustedWithFailures`], [`Outcome::Bounded`],
/// [`Outcome::Failed`], [`Outcome::FailureLimit`], [`Outcome::Stopped`] and, for
/// [`first_page_then`], [`Outcome::DiscoveryFailed`].
///
/// `unfinished` lists, in plan order, every admitted key without a successful *emitted* outcome:
/// failed keys (emitted or not), keys whose futures were dropped, and retained keys (whose results
/// are in `retained`). With `rest`, it describes the remaining work exactly.
#[non_exhaustive]
pub struct IndependentReport<K, I: IntoIterator<Item = K>, R, E> {
    /// How the traversal ended.
    pub outcome: Outcome,
    /// Final counters.
    pub progress: Progress,
    /// Admitted keys without a successful emitted outcome, in plan order. Never
    /// [`Unfinished::InFlight`].
    pub unfinished: Vec<Keyed<K, Unfinished>>,
    /// Successful results completed but never emitted (they waited behind an earlier key), in plan
    /// order: reuse them, or drop them and fetch their keys again.
    pub retained: Vec<Keyed<K, R>>,
    /// Errors of failed keys that were never emitted (ordered `Continue` stopped behind an earlier
    /// key), in plan order.
    pub unemitted_errors: Vec<Keyed<K, E>>,
    /// The plan's keys never admitted (including a key peeked to detect the bound). `None` when
    /// there is no plan (first-page discovery failed or never ran).
    pub rest: Option<Peekable<I::IntoIter>>,
}

impl<K, I: IntoIterator<Item = K>, R, E> IndependentReport<K, I, R, E> {
    /// `true` only when the plan was exhausted without any failure.
    pub fn is_success(&self) -> bool {
        self.outcome.is_success()
    }

    /// The first key in plan order not emitted successfully: the ordered restart point. `None`
    /// when every admitted key succeeded.
    pub fn restart(&self) -> Option<&K> {
        self.unfinished.first().map(|entry| &entry.key)
    }

    /// Failed keys, in plan order (kept apart from the restart point).
    pub fn failed(&self) -> impl Iterator<Item = &K> {
        self.unfinished
            .iter()
            .filter(|entry| entry.value == Unfinished::Failed)
            .map(|entry| &entry.key)
    }

    /// Splits the report into the retained results and the keys still to run: unfinished keys
    /// that were not retained, then the plan's rest. Pass the keys as a new plan to finish the
    /// work while reusing the retained results.
    pub fn into_rerun(self) -> (Vec<Keyed<K, R>>, impl Iterator<Item = K>) {
        let keys = self
            .unfinished
            .into_iter()
            .filter(|entry| entry.value != Unfinished::Retained)
            .map(|entry| entry.key);
        (self.retained, keys.chain(self.rest.into_iter().flatten()))
    }
}

impl<K, I: IntoIterator<Item = K>, R, E> fmt::Debug for IndependentReport<K, I, R, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndependentReport")
            .field("outcome", &self.outcome)
            .field("progress", &self.progress)
            .field("unfinished", &self.unfinished.len())
            .field("retained", &self.retained.len())
            .field("unemitted_errors", &self.unemitted_errors.len())
            .field("rest", &self.rest.is_some())
            .finish()
    }
}
