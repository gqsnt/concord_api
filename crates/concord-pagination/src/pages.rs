//! The ordinary facade over a dependent chain: an owned, nameable traversal whose end is
//! explicit.
//!
//! [`Pages`] and [`PageItems`] run the [`Sequential`] engine with its factory and future types
//! erased, so an SDK can return them from a method. Their contract differs from the engine's in
//! one point: the end is typed. `Ok(None)` means the provider's last page was read; reaching a
//! configured cap while the provider has a next page, and a provider failure, are errors
//! ([`PagesError`]), so a plain `while let Some(..) = ..?` loop cannot read a silent prefix.

use std::convert::Infallible;
use std::fmt;
use std::future::{Future, poll_fn};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

// `futures_util::stream::Stream` is `futures_core::Stream` (a re-export): implementing it adds no
// dependency.
use futures_util::stream::{FusedStream, Stream};

use crate::sequential::{
    ItemCheckpoint, ItemsProgress, ItemsReport, Sequential, SequentialReport, sequential,
};
use crate::types::{Budget, Outcome, Page, Progress, Resume};

/// What the engine's `poll_page` returns.
type Polled<T, K, M, E> = Poll<Option<Result<Page<T, K, M>, E>>>;

/// What the facade's polls return.
type Next<V, E, K> = Poll<Result<Option<V>, PagesError<E, K>>>;

/// A [`Sequential`] chain with its factory and future types erased.
trait Chain<T, K, M, E>: Send {
    fn poll(&mut self, cx: &mut Context<'_>) -> Polled<T, K, M, E>;
    fn position(&self) -> Option<&Resume<K>>;
    fn counters(&self) -> Progress;
    fn limit(&mut self, pages: u64);
    fn into_items(self: Box<Self>) -> Box<dyn ItemChain<T, K, E>>;
    fn report(self: Box<Self>) -> SequentialReport<K>;
}

/// An [`Items`](crate::Items) consumer with its factory and future types erased.
trait ItemChain<T, K, E>: Send {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T, E>>>;
    /// The underlying chain's page-level position.
    fn page(&self) -> Option<&Resume<K>>;
    fn position(&self) -> Option<&ItemCheckpoint<K>>;
    fn counters(&self) -> ItemsProgress;
    fn limit(&mut self, pages: u64);
    fn report(self: Box<Self>) -> ItemsReport<K, T>;
}

impl<K, T, M, E, F, Fut> Chain<T, K, M, E> for Sequential<K, T, M, E, F, Fut>
where
    K: Clone + Send + 'static,
    T: Send + 'static,
    M: 'static,
    E: 'static,
    F: FnMut(Option<K>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Page<T, K, M>, E>> + Send + 'static,
{
    fn poll(&mut self, cx: &mut Context<'_>) -> Polled<T, K, M, E> {
        self.poll_page(cx)
    }

    fn position(&self) -> Option<&Resume<K>> {
        self.checkpoint()
    }

    fn counters(&self) -> Progress {
        self.progress()
    }

    fn limit(&mut self, pages: u64) {
        self.set_budget(pages);
    }

    fn into_items(self: Box<Self>) -> Box<dyn ItemChain<T, K, E>> {
        Box::new((*self).items())
    }

    fn report(self: Box<Self>) -> SequentialReport<K> {
        (*self).finish()
    }
}

impl<K, T, M, E, F, Fut> ItemChain<T, K, E> for crate::Items<K, T, M, E, F, Fut>
where
    K: Clone + Send + 'static,
    T: Send + 'static,
    M: 'static,
    E: 'static,
    F: FnMut(Option<K>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Page<T, K, M>, E>> + Send + 'static,
{
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T, E>>> {
        self.poll_item(cx)
    }

    fn page(&self) -> Option<&Resume<K>> {
        self.chain().checkpoint()
    }

    fn position(&self) -> Option<&ItemCheckpoint<K>> {
        self.checkpoint()
    }

    fn counters(&self) -> ItemsProgress {
        self.progress()
    }

    fn limit(&mut self, pages: u64) {
        self.set_budget(pages);
    }

    fn report(self: Box<Self>) -> ItemsReport<K, T> {
        (*self).finish()
    }
}

/// Turns one engine step into the facade's contract and records a terminal outcome.
///
/// `page` is the chain's page-level position after the step: the failed page after a failure,
/// the first page beyond the cap after a bounded stop, `None` once the provider's last page was
/// emitted.
fn step<V, E, K: Clone>(
    ended: &mut Option<Outcome>,
    polled: Option<Result<V, E>>,
    page: Option<&Resume<K>>,
    progress: Progress,
) -> Result<Option<V>, PagesError<E, K>> {
    let (outcome, result) = match (polled, page) {
        (Some(Ok(value)), _) => return Ok(Some(value)),
        // The engine checks for the provider's last page before its budget: a cap reached on
        // the last page is exhaustion.
        (None, None) => (Outcome::Exhausted, Ok(None)),
        (None, Some(resume)) => (
            Outcome::Bounded,
            Err(PagesError::Bounded {
                resume: resume.clone(),
                progress,
            }),
        ),
        (Some(Err(error)), Some(failed)) => (
            Outcome::Failed,
            Err(PagesError::Provider {
                error,
                failed: failed.clone(),
                progress,
            }),
        ),
        (Some(Err(_)), None) => unreachable!(
            "a page is requested only from a known position, and only a success moves it"
        ),
    };
    *ended = Some(outcome);
    result
}

/// What a call after the end returns: `Ok(None)` again after exhaustion, else
/// [`PagesError::Ended`].
fn after_end<V, E, K: Clone>(
    outcome: Outcome,
    resume: Option<&Resume<K>>,
    progress: Progress,
) -> Result<Option<V>, PagesError<E, K>> {
    match outcome {
        Outcome::Exhausted => Ok(None),
        outcome => Err(PagesError::Ended {
            outcome,
            resume: resume.cloned(),
            progress,
        }),
    }
}

/// An owned, nameable dependent page chain: the ordinary way to read a cursor-paginated
/// listing.
///
/// The factory receives the key of the page to fetch (`None` for the provider's first page)
/// and returns an owned future of that [`Page`], exactly as for [`sequential`]: it is called
/// one page at a time, only once the previous page returned its next key, never after a
/// failure, never beyond a cap and never after the traversal is dropped or finished. Building a
/// `Pages` calls nothing. Because the factory and future types are erased, an SDK can name the
/// traversal in a signature (`fn invoices(&self) -> Pages<Invoice, Cursor, Meta, Error>`); the
/// price is that the factory and its futures must be `Send + 'static`, and the items `Send`.
/// Each page request costs one heap allocation for its future (the engine's), plus one for the
/// traversal when it is built and one when [`items`](Self::items) converts it.
///
/// # The end is explicit
///
/// [`next_page`](Self::next_page) returns:
///
/// - `Ok(Some(page))` for each page, including an empty page that has a next key;
/// - `Ok(None)` only once the provider's last page (the one without a next key) was returned,
///   and again on every later call;
/// - `Err(`[`PagesError::Bounded`]`)` when the cap of [`max_pages`](Self::max_pages) is
///   reached while the provider has a next page (a cap reached exactly on the last page is
///   exhaustion, `Ok(None)`);
/// - `Err(`[`PagesError::Provider`]`)` with the factory's own error, moved and typed, when a
///   page fails; a dependent chain cannot continue after it;
/// - after a `Bounded` or `Provider` error, `Err(`[`PagesError::Ended`]`)` on every later call,
///   never `Ok(None)`.
///
/// There is no cap unless you set one. Every error carries the counters and the page to
/// continue from.
///
/// [`next_page`](Self::next_page) is cancellation-safe: the page in flight belongs to the
/// traversal, so dropping a pending call loses nothing and the next call resumes the same
/// request. Dropping the traversal drops it; nothing continues locally.
///
/// For the engine's report (outcome, counters, resume and failed pages), use
/// [`finish`](Self::finish). `Debug` prints counters and outcomes, never a key, item or
/// metadata value.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{Page, Pages, PagesError, Resume};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// #[derive(Debug)]
/// struct ApiError;
///
/// // An SDK names the traversal in its signature.
/// fn invoices() -> Pages<u32, String, (), ApiError> {
///     Pages::new(Resume::Start, |cursor: Option<String>| async move {
///         Ok(match cursor.as_deref() {
///             None => Page::new(vec![1, 2], Some("c1".to_owned())),
///             Some("c1") => Page::new(vec![], Some("c2".to_owned())), // empty, not the end
///             _ => Page::new(vec![3], None),
///         })
///     })
/// }
///
/// let mut pages = invoices();
/// let mut items = Vec::new();
/// while let Some(page) = pages.next_page().await.unwrap() {
///     items.extend(page.items);
/// }
/// assert_eq!(items, [1, 2, 3]); // `Ok(None)`: the provider's last page was read
///
/// // A cap is opt-in, and reaching it while the provider has more is an error.
/// let mut pages = invoices().max_pages(NonZeroUsize::new(1).unwrap());
/// assert_eq!(pages.next_page().await.unwrap().unwrap().items, [1, 2]);
/// let Err(PagesError::Bounded { resume, .. }) = pages.next_page().await else {
///     panic!("a cap with a next page is not the end");
/// };
/// assert_eq!(resume, Resume::At("c1".to_owned())); // continue from here later
///
/// // Or collect, with the prefix kept on an incomplete result.
/// let incomplete = invoices()
///     .max_pages(NonZeroUsize::new(2).unwrap())
///     .items()
///     .collect_complete()
///     .await
///     .unwrap_err();
/// assert_eq!(incomplete.collected, [1, 2]);
/// assert_eq!(incomplete.error.resume(), Some(&Resume::At("c2".to_owned())));
/// # }
/// ```
pub struct Pages<T, K, M = (), E = Infallible> {
    chain: Box<dyn Chain<T, K, M, E>>,
    ended: Option<Outcome>,
}

impl<T, K, M, E> Pages<T, K, M, E>
where
    T: Send + 'static,
    K: Clone + Send + 'static,
    M: 'static,
    E: 'static,
{
    /// A chain starting at `from`, without a cap, whose pages `fetch` requests.
    pub fn new<F, Fut>(from: Resume<K>, fetch: F) -> Self
    where
        F: FnMut(Option<K>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Page<T, K, M>, E>> + Send + 'static,
    {
        let mut chain = sequential(from, Budget::new(NonZeroUsize::MAX), fetch);
        // No cap: the engine's counter is a `u64`.
        chain.set_budget(u64::MAX);
        Self {
            chain: Box::new(chain),
            ended: None,
        }
    }
}

impl<T, K: Clone, M, E> Pages<T, K, M, E> {
    /// At most `max` page requests by this traversal, counting those already made, failed ones
    /// included. Reaching it while the provider has a next page ends the traversal with
    /// [`PagesError::Bounded`]. Set it before reading: a traversal that already ended stays
    /// ended. A [`Budget`] converts into the cap (`max_pages(budget)`).
    pub fn max_pages(mut self, max: impl Into<NonZeroUsize>) -> Self {
        let max: NonZeroUsize = max.into();
        self.chain.limit(max.get() as u64);
        self
    }

    /// The next page; `Ok(None)` only after the provider's last page. See the
    /// [type documentation](Self) for every result.
    pub async fn next_page(&mut self) -> Result<Option<Page<T, K, M>>, PagesError<E, K>> {
        poll_fn(|cx| self.poll_page(cx)).await
    }

    fn poll_page(&mut self, cx: &mut Context<'_>) -> Next<Page<T, K, M>, E, K> {
        if let Some(outcome) = self.ended {
            return Poll::Ready(after_end(
                outcome,
                self.chain.position(),
                self.chain.counters(),
            ));
        }
        let polled = ready!(self.chain.poll(cx));
        Poll::Ready(step(
            &mut self.ended,
            polled,
            self.chain.position(),
            self.chain.counters(),
        ))
    }

    /// Where to resume to continue after the last returned page: the page in flight, the
    /// failed page, or the first page beyond the cap. `None` once the provider's last page was
    /// returned.
    pub fn checkpoint(&self) -> Option<&Resume<K>> {
        self.chain.position()
    }

    /// Counters: pages requested, in flight, returned and failed.
    pub fn progress(&self) -> Progress {
        self.chain.counters()
    }

    /// Flattens the chain into items; see [`PageItems`]. The cap and an earlier end carry over.
    pub fn items(self) -> PageItems<T, K, E> {
        PageItems {
            chain: self.chain.into_items(),
            ended: self.ended,
        }
    }

    /// Reads every page. `Ok` only after the provider's last page; otherwise the pages already
    /// read come back with the error in [`Incomplete`].
    pub async fn collect_complete(
        mut self,
    ) -> Result<Vec<Page<T, K, M>>, Incomplete<Page<T, K, M>, E, K>> {
        let mut collected = Vec::new();
        loop {
            match self.next_page().await {
                Ok(Some(page)) => collected.push(page),
                Ok(None) => return Ok(collected),
                Err(error) => return Err(Incomplete { collected, error }),
            }
        }
    }

    /// Ends the traversal and returns the engine's report: outcome, counters, resume point and
    /// failed page. A page in flight is dropped (outcome [`Outcome::Stopped`]).
    pub fn finish(self) -> SequentialReport<K> {
        self.chain.report()
    }
}

impl<T, K, M, E> fmt::Debug for Pages<T, K, M, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pages")
            .field("progress", &self.chain.counters())
            .field("checkpoint", &self.chain.position())
            .field("ended", &self.ended)
            .finish()
    }
}

/// The items of a [`Pages`] chain, one at a time ([`Pages::items`]).
///
/// [`next`](Self::next) follows the rules of [`Pages::next_page`]: `Ok(None)` only once every
/// item of the provider's last page was returned; empty pages with a next key are passed
/// through; a cap reached while the provider has more is [`PagesError::Bounded`], a failed page
/// [`PagesError::Provider`], and every call after either is [`PagesError::Ended`]. A page is
/// fetched only once the previous page's items were all returned (no read-ahead), and a pending
/// call can be dropped without losing the page in flight.
///
/// [`checkpoint`](Self::checkpoint) is the engine's item-level [`ItemCheckpoint`] (the page
/// being consumed and how many of its items were returned), [`progress`](Self::progress) its
/// [`ItemsProgress`], and [`finish`](Self::finish) its [`ItemsReport`], including the fetched
/// items never returned. The errors' page positions are page boundaries: a `PageItems` starts at
/// one, and fetches a page only after the previous page's last item.
pub struct PageItems<T, K, E = Infallible> {
    chain: Box<dyn ItemChain<T, K, E>>,
    ended: Option<Outcome>,
}

impl<T, K: Clone, E> PageItems<T, K, E> {
    /// See [`Pages::max_pages`].
    pub fn max_pages(mut self, max: impl Into<NonZeroUsize>) -> Self {
        let max: NonZeroUsize = max.into();
        self.chain.limit(max.get() as u64);
        self
    }

    /// The next item; `Ok(None)` only after the provider's last item.
    pub async fn next(&mut self) -> Result<Option<T>, PagesError<E, K>> {
        poll_fn(|cx| self.poll_item(cx)).await
    }

    fn poll_item(&mut self, cx: &mut Context<'_>) -> Next<T, E, K> {
        if let Some(outcome) = self.ended {
            return Poll::Ready(after_end(
                outcome,
                self.chain.page(),
                self.chain.counters().pages,
            ));
        }
        let polled = ready!(self.chain.poll(cx));
        Poll::Ready(step(
            &mut self.ended,
            polled,
            self.chain.page(),
            self.chain.counters().pages,
        ))
    }

    /// Where to resume to continue after the last returned item; `None` once every item of the
    /// provider's last page was returned. Resume with [`resume_items`](crate::resume_items).
    pub fn checkpoint(&self) -> Option<&ItemCheckpoint<K>> {
        self.chain.position()
    }

    /// Counters: items returned, pages consumed, and the page chain's counters.
    pub fn progress(&self) -> ItemsProgress {
        self.chain.counters()
    }

    /// An owning [`Stream`] of the items; see [`ItemStream`].
    pub fn into_stream(self) -> ItemStream<T, K, E> {
        ItemStream {
            items: self,
            done: false,
        }
    }

    /// Reads every item. `Ok` only after the provider's last item; otherwise the items already
    /// read come back with the error in [`Incomplete`].
    pub async fn collect_complete(mut self) -> Result<Vec<T>, Incomplete<T, E, K>> {
        let mut collected = Vec::new();
        loop {
            match self.next().await {
                Ok(Some(item)) => collected.push(item),
                Ok(None) => return Ok(collected),
                Err(error) => return Err(Incomplete { collected, error }),
            }
        }
    }

    /// Ends consumption and returns the engine's report: outcome, counters, item-level resume
    /// point, the current page's items never returned, and the failed page.
    pub fn finish(self) -> ItemsReport<K, T> {
        self.chain.report()
    }
}

impl<T, K, E> fmt::Debug for PageItems<T, K, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageItems")
            .field("progress", &self.chain.counters())
            .field("checkpoint", &self.chain.position())
            .field("ended", &self.ended)
            .finish()
    }
}

/// An owning [`Stream`] (`futures_core::Stream`) of a chain's items
/// ([`PageItems::into_stream`]).
///
/// It yields `Ok(item)` for each item. At provider exhaustion it ends (`None`). When the cap
/// is reached while the provider has more, or a page fails, it yields that typed `Err`
/// ([`PagesError::Bounded`] or [`PagesError::Provider`]) and then ends: a `None` after an `Err`
/// does **not** prove the listing complete. Only a stream that ended without an `Err` read
/// everything. After its end it keeps returning `None` ([`FusedStream`]).
///
/// It polls the chain on the caller's task: no spawned task, no read-ahead (a page is fetched
/// only when an item is asked for and the previous page's items were all yielded). The page in
/// flight belongs to the stream, so dropping a pending `next()` future loses nothing and the
/// next poll resumes the same request. [`into_inner`](Self::into_inner) gives the
/// [`PageItems`] back, for its checkpoint, counters and report.
pub struct ItemStream<T, K, E = Infallible> {
    items: PageItems<T, K, E>,
    done: bool,
}

impl<T, K, E> ItemStream<T, K, E> {
    /// The items consumer, with its checkpoint, counters and report.
    pub fn into_inner(self) -> PageItems<T, K, E> {
        self.items
    }
}

impl<T, K: Clone, E> Stream for ItemStream<T, K, E> {
    type Item = Result<T, PagesError<E, K>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        Poll::Ready(match ready!(this.items.poll_item(cx)) {
            Ok(Some(item)) => Some(Ok(item)),
            Ok(None) => {
                this.done = true;
                None
            }
            Err(error) => {
                this.done = true;
                Some(Err(error))
            }
        })
    }
}

impl<T, K: Clone, E> FusedStream for ItemStream<T, K, E> {
    fn is_terminated(&self) -> bool {
        self.done
    }
}

impl<T, K, E> fmt::Debug for ItemStream<T, K, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemStream")
            .field("items", &self.items)
            .field("done", &self.done)
            .finish()
    }
}

/// Why a [`Pages`] or [`PageItems`] call did not return a page or an item.
///
/// Every variant carries the page chain's counters and the page to continue from: pass it as
/// `from` to a new traversal over the same listing. `Debug` prints variants, positions as
/// `Start` or `At(..)`, and counters: never a key or the provider's error (reach it through the
/// field). `Display` does not include the provider's error either (it may quote a URL or
/// content): it says which stage failed and how many pages were emitted. Log the provider error
/// through the `error` field or [`source()`](std::error::Error::source), deliberately.
pub enum PagesError<E, K> {
    /// The factory failed on a page; the traversal is over.
    #[non_exhaustive]
    Provider {
        /// The factory's own error, moved.
        error: E,
        /// The page whose request failed: the one to retry.
        failed: Resume<K>,
        /// The page chain's counters.
        progress: Progress,
    },
    /// The cap of [`Pages::max_pages`] was reached while the provider had a next page; the
    /// traversal is over.
    #[non_exhaustive]
    Bounded {
        /// The first page not read.
        resume: Resume<K>,
        /// The page chain's counters.
        progress: Progress,
    },
    /// A call after a `Provider` or `Bounded` error: the traversal had already ended, and this
    /// is not the end of the listing.
    #[non_exhaustive]
    Ended {
        /// How it ended: [`Outcome::Failed`] or [`Outcome::Bounded`].
        outcome: Outcome,
        /// The page to continue from (the failed page, or the first page not read).
        resume: Option<Resume<K>>,
        /// The page chain's counters.
        progress: Progress,
    },
}

impl<E, K> PagesError<E, K> {
    /// How the traversal ended: [`Outcome::Failed`] for `Provider`, [`Outcome::Bounded`] for
    /// `Bounded`, the earlier outcome for `Ended`.
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Provider { .. } => Outcome::Failed,
            Self::Bounded { .. } => Outcome::Bounded,
            Self::Ended { outcome, .. } => *outcome,
        }
    }

    /// The page to continue from: the failed page (retry it) or the first page not read.
    pub fn resume(&self) -> Option<&Resume<K>> {
        match self {
            Self::Provider { failed, .. } => Some(failed),
            Self::Bounded { resume, .. } => Some(resume),
            Self::Ended { resume, .. } => resume.as_ref(),
        }
    }

    /// The page chain's counters when the error was returned.
    pub fn progress(&self) -> Progress {
        match self {
            Self::Provider { progress, .. }
            | Self::Bounded { progress, .. }
            | Self::Ended { progress, .. } => *progress,
        }
    }
}

impl<E, K> fmt::Debug for PagesError<E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider {
                failed, progress, ..
            } => f
                .debug_struct("Provider")
                .field("failed", failed)
                .field("progress", progress)
                .finish_non_exhaustive(),
            Self::Bounded { resume, progress } => f
                .debug_struct("Bounded")
                .field("resume", resume)
                .field("progress", progress)
                .finish(),
            Self::Ended {
                outcome,
                resume,
                progress,
            } => f
                .debug_struct("Ended")
                .field("outcome", outcome)
                .field("resume", resume)
                .field("progress", progress)
                .finish(),
        }
    }
}

impl<E, K> fmt::Display for PagesError<E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The provider's error may quote a URL or content: it is reachable through the
            // `error` field and `source()`, never formatted here.
            Self::Provider { progress, .. } => write!(
                f,
                "a page request failed (after {} pages emitted)",
                progress.emitted
            ),
            Self::Bounded { .. } => {
                f.write_str("page cap reached while the provider had more pages")
            }
            Self::Ended { outcome, .. } => write!(f, "the traversal had already ended: {outcome}"),
        }
    }
}

impl<E: std::error::Error + 'static, K> std::error::Error for PagesError<E, K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider { error, .. } => Some(error),
            Self::Bounded { .. } | Self::Ended { .. } => None,
        }
    }
}

/// An incomplete `collect_complete` ([`Pages::collect_complete`],
/// [`PageItems::collect_complete`]): what was read before the traversal stopped, and why it
/// stopped. It is never a successful result.
///
/// `Debug` prints the number of collected values and the error's `Debug`.
#[non_exhaustive]
pub struct Incomplete<V, E, K> {
    /// The pages or items read, in order: a prefix of the listing.
    pub collected: Vec<V>,
    /// Why the traversal stopped, with the page to continue from.
    pub error: PagesError<E, K>,
}

impl<V, E, K> fmt::Debug for Incomplete<V, E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incomplete")
            .field("collected", &self.collected.len())
            .field("error", &self.error)
            .finish()
    }
}

impl<V, E, K> fmt::Display for Incomplete<V, E, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "incomplete after {} collected: {}",
            self.collected.len(),
            self.error
        )
    }
}

impl<V, E: std::error::Error + 'static, K: 'static> std::error::Error for Incomplete<V, E, K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
