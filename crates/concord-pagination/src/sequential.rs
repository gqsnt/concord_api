//! Dependent chains: each request needs the key the previous page returned.

use std::fmt;
use std::future::{Future, poll_fn};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::types::{Budget, Outcome, Page, Progress, Resume};

/// Marks the item, metadata and error types without owning them.
type Types<T, M, E> = fn() -> (T, M, E);

/// What a chain's `poll` returns.
type Polled<T, K, M, E> = Poll<Option<Result<Page<T, K, M>, E>>>;

/// Starts a dependent chain at `from`, admitting at most `budget` pages.
///
/// `fetch` receives the key of the page to fetch (`None` for the provider's first page) and returns
/// an owned future of that [`Page`]. It is called only once the previous page has returned its
/// `next` key, one page at a time, never after a failure, never beyond the budget and never after
/// the traversal is dropped or finished. Constructing the traversal calls nothing.
///
/// A page with no items but a next key continues the chain. A chain cannot continue after a
/// failure: there is no next key to continue from, and none is guessed.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{sequential, Budget, Outcome, Page, Resume};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // A provider with three pages linked by opaque cursors.
/// async fn list(cursor: Option<String>) -> Result<Page<u32, String>, std::io::Error> {
///     Ok(match cursor.as_deref() {
///         None => Page::new(vec![1, 2], Some("c1".into())),
///         Some("c1") => Page::new(vec![], Some("c2".into())), // empty, not the end
///         _ => Page::new(vec![3], None),
///     })
/// }
///
/// let budget = Budget::new(NonZeroUsize::new(10).unwrap());
/// let mut pages = sequential(Resume::Start, budget, list);
/// let mut items = Vec::new();
/// while let Some(page) = pages.next().await {
///     items.extend(page.unwrap().items);
/// }
/// assert_eq!(items, [1, 2, 3]);
/// let report = pages.finish();
/// assert_eq!(report.outcome, Outcome::Exhausted);
/// assert!(report.resume.is_none());
/// # }
/// ```
pub fn sequential<K, T, M, E, F, Fut>(
    from: Resume<K>,
    budget: Budget,
    fetch: F,
) -> Sequential<K, T, M, E, F, Fut>
where
    K: Clone,
    F: FnMut(Option<K>) -> Fut,
    Fut: Future<Output = Result<Page<T, K, M>, E>>,
{
    Sequential {
        fetch,
        budget: budget.get() as u64,
        admitted: 0,
        emitted: 0,
        position: Some(from),
        in_flight: None,
        ended: None,
        failed: None,
        _types: PhantomData,
    }
}

/// A dependent page chain; see [`sequential`].
///
/// [`next`](Self::next) is cancellation-safe: the page future in flight belongs to the traversal,
/// so dropping a pending `next` future loses nothing and the following call resumes polling the
/// same page. Dropping the traversal (or calling [`finish`](Self::finish)) drops that future; a
/// request already transmitted may still run on the server, but nothing continues locally.
pub struct Sequential<K, T, M, E, F, Fut> {
    fetch: F,
    budget: u64,
    admitted: u64,
    emitted: u64,
    /// The next page not yet emitted; `None` once the provider's last page was emitted.
    position: Option<Resume<K>>,
    in_flight: Option<Pin<Box<Fut>>>,
    ended: Option<Outcome>,
    failed: Option<Resume<K>>,
    _types: PhantomData<Types<T, M, E>>,
}

impl<K, T, M, E, F, Fut> Sequential<K, T, M, E, F, Fut>
where
    K: Clone,
    F: FnMut(Option<K>) -> Fut,
    Fut: Future<Output = Result<Page<T, K, M>, E>>,
{
    /// The next page, or `None` when the chain ended (exhausted, bounded, failed or finished).
    ///
    /// An `Err` is the factory's own error for the page at [`checkpoint`](Self::checkpoint); the
    /// chain is then over.
    pub async fn next(&mut self) -> Option<Result<Page<T, K, M>, E>> {
        poll_fn(|cx| self.poll_page(cx)).await
    }

    fn poll_page(&mut self, cx: &mut Context<'_>) -> Polled<T, K, M, E> {
        if self.ended.is_some() {
            return Poll::Ready(None);
        }
        if self.in_flight.is_none() {
            let Some(position) = &self.position else {
                self.ended = Some(Outcome::Exhausted);
                return Poll::Ready(None);
            };
            if self.admitted >= self.budget {
                self.ended = Some(Outcome::Bounded);
                return Poll::Ready(None);
            }
            // Charged at admission: a failing page still counts.
            self.admitted += 1;
            let request = position.to_request();
            self.in_flight = Some(Box::pin((self.fetch)(request)));
        }
        let Some(future) = self.in_flight.as_mut() else {
            return Poll::Ready(None);
        };
        let result = match future.as_mut().poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(result) => result,
        };
        self.in_flight = None;
        match result {
            Ok(page) => {
                self.emitted += 1;
                self.position = page.next.clone().map(Resume::At);
                Poll::Ready(Some(Ok(page)))
            }
            Err(error) => {
                self.failed = self.position.clone();
                self.ended = Some(Outcome::Failed);
                Poll::Ready(Some(Err(error)))
            }
        }
    }

    /// Where to resume to continue after the last emitted page: the key of the first page not yet
    /// emitted (the failed page after a failure, the page in flight while one is). `None` once
    /// the provider's last page was emitted.
    pub fn checkpoint(&self) -> Option<&Resume<K>> {
        self.position.as_ref()
    }

    /// Counters (pages admitted, emitted, failed).
    pub fn progress(&self) -> Progress {
        Progress {
            admitted: self.admitted,
            in_flight: usize::from(self.in_flight.is_some()),
            retained: 0,
            emitted: self.emitted,
            succeeded: self.emitted,
            failed: u64::from(self.failed.is_some()),
            cancelled: 0,
        }
    }

    /// The outcome the chain would report now without dropping anything, or `None` while it can
    /// still make progress.
    fn natural_outcome(&self) -> Option<Outcome> {
        if let Some(outcome) = self.ended {
            return Some(outcome);
        }
        if self.in_flight.is_some() {
            return None;
        }
        match self.position {
            None => Some(Outcome::Exhausted),
            Some(_) if self.admitted >= self.budget => Some(Outcome::Bounded),
            Some(_) => None,
        }
    }

    /// Ends the chain and reports how. A page in flight is dropped (the outcome is then
    /// [`Outcome::Stopped`] and the dropped page is the resume point).
    pub fn finish(mut self) -> SequentialReport<K> {
        let outcome = self.natural_outcome().unwrap_or(Outcome::Stopped);
        let progress = self.progress();
        self.in_flight = None;
        SequentialReport {
            outcome,
            progress: Progress {
                in_flight: 0,
                cancelled: progress.in_flight as u64,
                ..progress
            },
            resume: self.position,
            failed: self.failed,
        }
    }

    /// Flattens the chain into items with item-level checkpoints; see [`Items`].
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    /// use concord_pagination::{sequential, Budget, ItemCheckpoint, Page, Resume};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// async fn list(page: Option<u32>) -> Result<Page<char, u32>, std::io::Error> {
    ///     Ok(match page {
    ///         None => Page::new(vec!['a', 'b'], Some(1)),
    ///         _ => Page::new(vec!['c'], None),
    ///     })
    /// }
    /// let budget = Budget::new(NonZeroUsize::new(10).unwrap());
    /// let mut items = sequential(Resume::Start, budget, list).items();
    ///
    /// assert_eq!(items.next().await.unwrap().unwrap(), 'a');
    /// // Page 0 is fetched, but its checkpoint does not move past it before 'b' is emitted.
    /// assert_eq!(items.checkpoint(), Some(&ItemCheckpoint { page: Resume::Start, skip: 1 }));
    /// assert_eq!(items.next().await.unwrap().unwrap(), 'b');
    /// assert_eq!(items.checkpoint(), Some(&ItemCheckpoint { page: Resume::At(1), skip: 0 }));
    /// assert_eq!(items.next().await.unwrap().unwrap(), 'c');
    /// assert_eq!(items.checkpoint(), None); // everything emitted
    /// # }
    /// ```
    pub fn items(self) -> Items<K, T, M, E, F, Fut> {
        Items::new(self, 0)
    }
}

impl<K, T, M, E, F, Fut> fmt::Debug for Sequential<K, T, M, E, F, Fut> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sequential")
            .field("admitted", &self.admitted)
            .field("emitted", &self.emitted)
            .field("budget", &self.budget)
            .field("in_flight", &self.in_flight.is_some())
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

/// How a sequential chain ended ([`Sequential::finish`]).
///
/// Outcomes: [`Outcome::Exhausted`] (the last page had no next key), [`Outcome::Bounded`] (the
/// budget was used while a next key existed), [`Outcome::Failed`] (a page failed) or
/// [`Outcome::Stopped`] (finished early).
#[non_exhaustive]
pub struct SequentialReport<K> {
    /// How the chain ended.
    pub outcome: Outcome,
    /// Final counters.
    pub progress: Progress,
    /// Pass as `from` to continue with the first page not emitted (the failed page after a
    /// failure). `None` when the provider is exhausted.
    pub resume: Option<Resume<K>>,
    /// The page whose request failed, kept apart from `resume` (they designate the same page in
    /// a chain, but have different meanings). The error itself was returned by `next`.
    pub failed: Option<Resume<K>>,
}

impl<K> fmt::Debug for SequentialReport<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SequentialReport")
            .field("outcome", &self.outcome)
            .field("progress", &self.progress)
            .field("resume", &self.resume)
            .field("failed", &self.failed)
            .finish()
    }
}

/// An item-level position: the page being consumed and how many of its items were already
/// emitted.
///
/// A checkpoint with `skip == 0` is a page boundary. `Debug` never prints the key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ItemCheckpoint<K> {
    /// The page to fetch again.
    pub page: Resume<K>,
    /// Items of that page already emitted, to skip when it is fetched again.
    pub skip: usize,
}

impl<K> fmt::Debug for ItemCheckpoint<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemCheckpoint")
            .field("page", &self.page)
            .field("skip", &self.skip)
            .finish()
    }
}

/// Resumes item consumption at an [`ItemCheckpoint`]: fetches `at.page` again (with a fresh
/// `budget`) and skips its first `at.skip` items.
///
/// This *replays* the checkpoint page. It is exact only if the provider returns the same items in
/// the same order for that key (a stable page: same tenant, query, filter, sort, page size and
/// snapshot). The crate cannot check that; an adapter can, for example with a snapshot token
/// kept in the page metadata. If the refetched page has fewer than `skip` items, all of them are
/// skipped. An expired cursor is the factory's error; nothing restarts from the first page.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{resume_items, sequential, Budget, Page, Resume};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// async fn list(page: Option<u32>) -> Result<Page<String, u32>, std::io::Error> {
///     let n = page.unwrap_or(0);
///     let items = (0..3).map(|i| format!("p{n}i{i}")).collect();
///     Ok(Page::new(items, (n < 2).then_some(n + 1)))
/// }
/// let budget = Budget::new(NonZeroUsize::new(10).unwrap());
///
/// // Consume four items, keep the checkpoint, stop.
/// let mut items = sequential(Resume::Start, budget, list).items();
/// for _ in 0..4 {
///     items.next().await.unwrap().unwrap();
/// }
/// let report = items.finish();
/// let at = report.resume.unwrap();
/// assert_eq!((at.page.key(), at.skip), (Some(&1), 1));
///
/// // Later: resume exactly after the fourth item.
/// let mut items = resume_items(at, budget, list);
/// assert_eq!(items.next().await.unwrap().unwrap(), "p1i1");
/// # }
/// ```
pub fn resume_items<K, T, M, E, F, Fut>(
    at: ItemCheckpoint<K>,
    budget: Budget,
    fetch: F,
) -> Items<K, T, M, E, F, Fut>
where
    K: Clone,
    F: FnMut(Option<K>) -> Fut,
    Fut: Future<Output = Result<Page<T, K, M>, E>>,
{
    Items::new(sequential(at.page, budget, fetch), at.skip)
}

/// A dependent chain flattened into items ([`Sequential::items`], [`resume_items`]).
///
/// Its [`checkpoint`](Self::checkpoint) is item-level: it names the page being consumed and how
/// many of its items were emitted. It moves to the following page only after that page's last
/// item was emitted; an empty page (with a next key) moves it in one explicit transition, without
/// an item. An item counts as emitted when `next` returned it: take the checkpoint after you
/// processed an item, and it resumes after that item. Emitted is not committed: acknowledging
/// a sink or a transaction remains the caller's.
///
/// [`finish`](Self::finish) in the middle of a page returns the page's checkpoint with the number
/// of emitted items *and* the items not yet emitted; it never returns the following page's key
/// as if the page were done. A plain `drop` returns nothing, so keep a checkpoint first.
pub struct Items<K, T, M, E, F, Fut> {
    pages: Sequential<K, T, M, E, F, Fut>,
    buffer: std::vec::IntoIter<T>,
    /// The page whose items are buffered or about to be fetched; `None` once everything was
    /// emitted from the provider's last page.
    checkpoint: Option<ItemCheckpoint<K>>,
    /// The next key of the buffered page.
    after: Option<Option<K>>,
    items: u64,
    pages_consumed: u64,
}

impl<K, T, M, E, F, Fut> Items<K, T, M, E, F, Fut>
where
    K: Clone,
    F: FnMut(Option<K>) -> Fut,
    Fut: Future<Output = Result<Page<T, K, M>, E>>,
{
    fn new(pages: Sequential<K, T, M, E, F, Fut>, skip: usize) -> Self {
        let checkpoint = pages
            .checkpoint()
            .cloned()
            .map(|page| ItemCheckpoint { page, skip });
        Self {
            pages,
            buffer: Vec::new().into_iter(),
            checkpoint,
            after: None,
            items: 0,
            pages_consumed: 0,
        }
    }

    /// The next item, or `None` when the chain ended. An `Err` is the page's failure; the
    /// checkpoint still names that page.
    pub async fn next(&mut self) -> Option<Result<T, E>> {
        poll_fn(|cx| self.poll_item(cx)).await
    }

    fn poll_item(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T, E>>> {
        loop {
            if let Some(item) = self.buffer.next() {
                self.items += 1;
                if let Some(checkpoint) = &mut self.checkpoint {
                    checkpoint.skip += 1;
                }
                if self.buffer.as_slice().is_empty() {
                    self.page_consumed();
                }
                return Poll::Ready(Some(Ok(item)));
            }
            let page = match self.pages.poll_page(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(Some(Ok(page))) => page,
            };
            let skip = self
                .checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.skip);
            let mut items = page.items;
            items.drain(..skip.min(items.len()));
            self.after = Some(page.next);
            self.buffer = items.into_iter();
            if self.buffer.as_slice().is_empty() {
                // Empty (or fully skipped) page: the explicit transition to the next page.
                self.page_consumed();
            }
        }
    }

    fn page_consumed(&mut self) {
        self.pages_consumed += 1;
        let next = self.after.take().flatten();
        self.checkpoint = next.map(|key| ItemCheckpoint {
            page: Resume::At(key),
            skip: 0,
        });
    }

    /// Where to resume to continue after the last emitted item; `None` once every item of the
    /// provider's last page was emitted. Resume with [`resume_items`].
    pub fn checkpoint(&self) -> Option<&ItemCheckpoint<K>> {
        self.checkpoint.as_ref()
    }

    /// Counters: items emitted, pages fully consumed, and the page chain's counters.
    pub fn progress(&self) -> ItemsProgress {
        ItemsProgress {
            items: self.items,
            pages_consumed: self.pages_consumed,
            pages: self.pages.progress(),
        }
    }

    /// Ends consumption and reports how, with the item-level resume point and the items of the
    /// current page that were never emitted.
    pub fn finish(self) -> ItemsReport<K, T> {
        let remainder: Vec<T> = self.buffer.collect();
        let after_remainder = if remainder.is_empty() {
            self.checkpoint.clone()
        } else {
            self.after.flatten().map(|key| ItemCheckpoint {
                page: Resume::At(key),
                skip: 0,
            })
        };
        let progress = ItemsProgress {
            items: self.items,
            pages_consumed: self.pages_consumed,
            pages: self.pages.progress(),
        };
        let pages = self.pages.finish();
        let outcome = if remainder.is_empty() {
            pages.outcome
        } else {
            Outcome::Stopped
        };
        ItemsReport {
            outcome,
            progress: ItemsProgress {
                pages: pages.progress,
                ..progress
            },
            resume: self.checkpoint,
            remainder,
            after_remainder,
            failed: pages.failed,
        }
    }
}

impl<K, T, M, E, F, Fut> fmt::Debug for Items<K, T, M, E, F, Fut> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Items")
            .field("items", &self.items)
            .field("pages_consumed", &self.pages_consumed)
            .field("buffered", &self.buffer.len())
            .field("checkpoint", &self.checkpoint)
            .field("pages", &self.pages)
            .finish()
    }
}

/// Counters of an [`Items`] consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ItemsProgress {
    /// Items returned by `next`.
    pub items: u64,
    /// Pages whose last item was emitted, plus empty pages passed through.
    pub pages_consumed: u64,
    /// The underlying chain's counters.
    pub pages: Progress,
}

/// How item consumption ended ([`Items::finish`]).
///
/// Outcomes as for [`SequentialReport`]; a finish with unemitted items of the current page is
/// [`Outcome::Stopped`] even if that page was the provider's last.
#[non_exhaustive]
pub struct ItemsReport<K, T> {
    /// How consumption ended.
    pub outcome: Outcome,
    /// Final counters.
    pub progress: ItemsProgress,
    /// Where to resume with [`resume_items`]; `None` when every item was emitted from the
    /// provider's last page.
    pub resume: Option<ItemCheckpoint<K>>,
    /// The current page's items that were fetched but never emitted, in order.
    pub remainder: Vec<T>,
    /// Where to resume *if you process `remainder` yourself*: the page after the current one
    /// (`None` when the current page was the provider's last). Equal to `resume` when
    /// `remainder` is empty. Resuming here without processing `remainder` loses those items.
    pub after_remainder: Option<ItemCheckpoint<K>>,
    /// The page whose request failed, if one did.
    pub failed: Option<Resume<K>>,
}

impl<K, T> fmt::Debug for ItemsReport<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemsReport")
            .field("outcome", &self.outcome)
            .field("progress", &self.progress)
            .field("resume", &self.resume)
            .field("remainder", &self.remainder.len())
            .field("after_remainder", &self.after_remainder)
            .field("failed", &self.failed)
            .finish()
    }
}
