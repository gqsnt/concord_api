//! Lazy, prefix and complete traversal terminals.

use std::fmt;
use std::future::{Future, IntoFuture};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use crate::body::PageRequestRepresentation;
use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
};
use crate::error::ApiError;
use crate::hooks::BoxFuture;
use crate::request::{Request, RequestOptions};
use crate::response::{Buffered, ExecutionMeta, ResponseRepresentation};

use super::bridge::{PageItem, PageableEndpoint};
use super::engine::{DropReport, ErasedTraversal, Fetched, Traversal, TraversalMode};
use super::summary::{StopReason, TraversalSummary};

/// One delivered page: its zero-based index, its complete item batch and a
/// fixed metadata snapshot.
///
/// A page is delivered only after it was completely acquired and decoded,
/// its controller decision succeeded, its batch fit the item ceiling, and any
/// proposed continuation was validated. `Debug` prints the index, item count
/// and metadata, never the items.
pub struct Page<Item> {
    index: u32,
    items: Vec<Item>,
    metadata: ExecutionMeta,
}

impl<Item> Page<Item> {
    pub(super) const fn new(index: u32, items: Vec<Item>, metadata: ExecutionMeta) -> Self {
        Self {
            index,
            items,
            metadata,
        }
    }

    /// Returns the zero-based page index within its traversal.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Borrows this page's complete item batch.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Extracts this page's complete item batch.
    #[must_use]
    pub fn into_items(self) -> Vec<Item> {
        self.items
    }

    /// Returns the execution metadata fixed when this page's acquisition and
    /// validation completed. Later pages never change it; the traversal's own
    /// [`TraversalSummary`] reports later progress.
    #[must_use]
    pub const fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }
}

impl<Item> fmt::Debug for Page<Item> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Page")
            .field("index", &self.index)
            .field("item_count", &self.items.len())
            .field("page_index_metadata", &self.metadata.page_index())
            .field("attempts_started", &self.metadata.attempts_started())
            .finish()
    }
}

/// The explicit result of `collect_prefix`: the delivered items and the
/// summary that says why collection stopped.
///
/// A prefix that stopped at [`StopReason::PageBound`] is **not** evidence of
/// exhaustion: check [`TraversalSummary::stop_reason`] and
/// [`TraversalSummary::continuation`].
pub struct Prefix<Item> {
    items: Vec<Item>,
    summary: TraversalSummary,
}

impl<Item> Prefix<Item> {
    /// Borrows the delivered items in page order.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Extracts the delivered items in page order.
    #[must_use]
    pub fn into_items(self) -> Vec<Item> {
        self.items
    }

    /// Returns the traversal summary fixed when collection stopped.
    #[must_use]
    pub const fn summary(&self) -> &TraversalSummary {
        &self.summary
    }
}

impl<Item> fmt::Debug for Prefix<Item> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Prefix")
            .field("item_count", &self.items.len())
            .field("summary", &self.summary)
            .finish()
    }
}

/// A caller-driven lazy traversal returned by `request.pages(maximum_pages)`.
///
/// Construction is inert. The first polled [`next_page`](Self::next_page)
/// future of a nonzero traversal allocates the aggregate operation identity
/// and starts **one** absolute deadline covering controller start, every page
/// and attempt, and all time the application spends between polls; it is never
/// reset per page. There is no background prefetch and at most one page fetch
/// is active.
///
/// `Pages` does not implement [`RequestControls`] and has no timeout or
/// cancellation setters: apply controls to the originating request before
/// calling `.pages()`. The captured cancellation tokens can still be cancelled
/// externally after the traversal started.
///
/// # Cancellation
///
/// * Dropping a `next_page` future that was never polled changes nothing.
/// * Dropping a `next_page` future after it was polled cancels the whole
///   traversal through a synchronous drop guard: its in-flight work is dropped
///   and the summary records [`StopReason::Cancelled`]. Later `next_page` calls
///   return `Ok(None)`; budgets are never replenished.
/// * An attached cancellation signal observed before or during a fetch
///   returns one `Pagination`/`Cancelled` error, then `Ok(None)`.
/// * A fetch error is returned exactly once; later calls return `Ok(None)` and
///   the summary retains [`StopReason::Failed`].
/// * Dropping a started traversal records cancellation, never EOF or rollback.
///
/// A completed fetch failure retains its pending position for
/// [`checkpoint`](Self::checkpoint), so a new traversal can retry that page.
/// Dropping an in-flight fetch cancels this traversal; save a checkpoint before
/// starting the fetch when recovery from cancellation is needed.
pub struct Pages<Item> {
    traversal: Option<Box<dyn ErasedTraversal<Item>>>,
    settled: TraversalSummary,
    drop_report: DropReport,
}

/// Constructs an inert lazy traversal.
pub(super) fn new_pages<E>(request: Request<E>, maximum_pages: u32) -> Pages<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    let drop_report: DropReport = Arc::new(Mutex::new(None));
    let mut traversal = Traversal::new(
        request,
        maximum_pages,
        TraversalMode::Incremental,
        Arc::clone(&drop_report),
    );
    if maximum_pages == 0 {
        traversal.zero_bound();
    }
    let settled = ErasedTraversal::summary(&traversal);
    Pages {
        traversal: Some(Box::new(traversal)),
        settled,
        drop_report,
    }
}

pub(super) fn new_resumed_pages<E>(
    request: Request<E>,
    checkpoint: &super::Checkpoint,
    maximum_pages: u32,
) -> Result<Pages<PageItem<E>>, super::CheckpointError>
where
    E: PageableEndpoint,
    E::Controller: super::ResumableController<E::Output>,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    let drop_report: DropReport = Arc::new(Mutex::new(None));
    let mut traversal = Traversal::new(
        request,
        maximum_pages,
        TraversalMode::Incremental,
        Arc::clone(&drop_report),
    );
    traversal.resume(checkpoint)?;
    if maximum_pages == 0 {
        traversal.zero_bound();
    }
    let settled = ErasedTraversal::summary(&traversal);
    Ok(Pages {
        traversal: Some(Box::new(traversal)),
        settled,
        drop_report,
    })
}

impl<Item> Pages<Item> {
    /// Saves the next page position, including after a page bound or fetch failure.
    ///
    /// # Errors
    /// Returns a value-free error if finished, unsupported, sensitive or oversized.
    pub fn checkpoint(&self) -> Result<super::Checkpoint, super::CheckpointError> {
        self.traversal
            .as_ref()
            .ok_or_else(|| super::CheckpointError::new(super::CheckpointErrorKind::Finished))?
            .checkpoint()
    }

    /// Fetches, validates and delivers the next page, or returns `Ok(None)`
    /// once the traversal is exhausted, bounded, finished, failed or
    /// cancelled. See the type-level documentation for cancellation rules.
    pub fn next_page(&mut self) -> impl Future<Output = Result<Option<Page<Item>>, ApiError>> + '_ {
        NextPage {
            pages: self,
            in_flight: None,
        }
    }

    /// Returns the current traversal summary.
    #[must_use]
    pub fn summary(&self) -> TraversalSummary {
        match &self.traversal {
            Some(traversal) => traversal.summary(),
            None => self.reported_or_settled(),
        }
    }

    /// Ends the traversal and returns its final summary.
    ///
    /// An already terminal `Exhausted`, `PageBound`, `Failed` or `Cancelled`
    /// state is preserved; otherwise the traversal records
    /// [`StopReason::Application`]. An unstarted traversal keeps no operation
    /// identity and emits no observation.
    pub fn finish(mut self) -> TraversalSummary {
        match self.traversal.take() {
            Some(mut traversal) => traversal.finish(),
            None => {
                let mut summary = self.reported_or_settled();
                if !summary.is_terminal() {
                    summary.stop_reason = StopReason::Application;
                }
                summary
            }
        }
    }

    fn reported_or_settled(&self) -> TraversalSummary {
        self.drop_report
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| self.settled.clone())
    }
}

impl<Item> fmt::Debug for Pages<Item> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pages")
            .field("summary", &self.summary())
            .finish()
    }
}

/// The `next_page` future. The traversal is moved into the fetch on first poll
/// and handed back on completion, so dropping a started fetch drops (and
/// thereby cancels) the traversal synchronously.
struct NextPage<'a, Item> {
    pages: &'a mut Pages<Item>,
    in_flight: Option<BoxFuture<'static, Fetched<Item>>>,
}

impl<Item> Future for NextPage<'_, Item> {
    type Output = Result<Option<Page<Item>>, ApiError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.in_flight.is_none() {
            let Some(traversal) = this.pages.traversal.take() else {
                return Poll::Ready(Ok(None));
            };
            this.in_flight = Some(traversal.fetch());
        }
        let Some(in_flight) = this.in_flight.as_mut() else {
            return Poll::Ready(Ok(None));
        };
        let Fetched { traversal, result } = match in_flight.as_mut().poll(context) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(fetched) => fetched,
        };
        this.in_flight = None;
        let summary = traversal.summary();
        this.pages.traversal = Some(traversal);
        this.pages.settled = summary;
        Poll::Ready(result)
    }
}

/// Type-erased inert collection source carrying the originating request and
/// its controls until the collector is first polled.
trait PendingCollection<Item>: Send {
    fn options_mut(&mut self) -> &mut RequestOptions;
    fn control_summary(&self) -> ControlSummary;
    fn into_traversal(
        self: Box<Self>,
        requested: u32,
        mode: TraversalMode,
    ) -> Box<dyn ErasedTraversal<Item>>;
}

struct PendingRequest<E> {
    request: Request<E>,
}

impl<E> PendingCollection<PageItem<E>> for PendingRequest<E>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    fn options_mut(&mut self) -> &mut RequestOptions {
        &mut self.request.options
    }

    fn control_summary(&self) -> ControlSummary {
        let declared = self
            .request
            .endpoint
            .request_meta()
            .resolve_inherited_timeout(self.request.core.default_runtime_endpoint_timeout());
        self.request.options.control_summary(declared)
    }

    fn into_traversal(
        self: Box<Self>,
        requested: u32,
        mode: TraversalMode,
    ) -> Box<dyn ErasedTraversal<PageItem<E>>> {
        Box::new(Traversal::new(
            self.request,
            requested,
            mode,
            Arc::new(Mutex::new(None)),
        ))
    }
}

/// Shared controls and polling loop of both collectors.
struct Collection<Item> {
    requested: u32,
    source: Box<dyn PendingCollection<Item>>,
}

fn new_collection<E>(request: Request<E>, requested: u32) -> Collection<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    Collection {
        requested,
        source: Box::new(PendingRequest { request }),
    }
}

impl<Item> Collection<Item> {
    fn timeout(mut self, duration: Duration) -> Result<Self, ControlError> {
        self.source.options_mut().select_timeout(duration)?;
        Ok(self)
    }

    fn reset_timeout(mut self) -> Self {
        self.source.options_mut().reset_selected_timeout();
        self
    }

    fn cancel_with(mut self, token: CancellationToken) -> Result<Self, ControlError> {
        self.source.options_mut().add_cancellation(token)?;
        Ok(self)
    }

    fn with_options(mut self, options: &CallOptions) -> Self {
        self.source.options_mut().apply_call_options(options);
        self
    }

    fn control_summary(&self) -> ControlSummary {
        self.source.control_summary()
    }
}

impl<Item> Collection<Item>
where
    Item: Send + 'static,
{
    /// Drives the traversal to a terminal state, accumulating delivered items.
    /// Returns the terminal traversal so the caller can read its summary.
    async fn run(
        self,
        mode: TraversalMode,
    ) -> Result<(Vec<Item>, Box<dyn ErasedTraversal<Item>>), ApiError> {
        let mut traversal = self.source.into_traversal(self.requested, mode);
        let mut items = Vec::new();
        loop {
            let Fetched {
                traversal: returned,
                result,
            } = traversal.fetch().await;
            traversal = returned;
            match result? {
                Some(page) => items.extend(page.into_items()),
                None => return Ok((items, traversal)),
            }
        }
    }
}

/// An inert request to collect an explicit bounded prefix, returned by
/// `request.collect_prefix(maximum_pages)`.
///
/// Awaiting it yields a [`Prefix`] whose summary distinguishes exhaustion
/// from a reached page bound. `collect_prefix(0)` performs no shaping, body,
/// controller, credential, quota, transport or operation-identity work and
/// yields an empty prefix with a `PageBound`/`Unknown` summary.
///
/// Controls (timeout, caps, cancellation) may be applied before awaiting.
/// Dropping the started future cancels the traversal through its drop guard.
/// An error fails the whole collection: no partial items are returned through
/// success.
pub struct CollectPrefix<Item> {
    collection: Collection<Item>,
}

/// Constructs an inert prefix collector.
pub(super) fn new_collect_prefix<E>(
    request: Request<E>,
    maximum_pages: u32,
) -> CollectPrefix<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    CollectPrefix {
        collection: new_collection(request, maximum_pages),
    }
}

impl<Item> CollectPrefix<Item> {
    /// Replaces the selected finite timeout for the whole traversal. Caps are
    /// retained.
    pub fn timeout(self, duration: Duration) -> Result<Self, ControlError> {
        Ok(Self {
            collection: self.collection.timeout(duration)?,
        })
    }

    /// Restores the declared timeout. Caps are retained.
    #[must_use]
    pub fn reset_timeout(self) -> Self {
        Self {
            collection: self.collection.reset_timeout(),
        }
    }

    /// Attaches one more cancellation signal (at most eight distinct signals).
    pub fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Ok(Self {
            collection: self.collection.cancel_with(token)?,
        })
    }

    /// Intersects additional caps; caps only narrow execution.
    #[must_use]
    pub fn with_options(self, options: &CallOptions) -> Self {
        Self {
            collection: self.collection.with_options(options),
        }
    }

    /// Returns the value-free effective control summary.
    #[must_use]
    pub fn control_summary(&self) -> ControlSummary {
        self.collection.control_summary()
    }
}

impl<Item> RequestControls for CollectPrefix<Item> {
    fn with_options(self, options: &CallOptions) -> Self {
        Self::with_options(self, options)
    }

    fn control_summary(&self) -> ControlSummary {
        Self::control_summary(self)
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Self::cancel_with(self, token)
    }
}

impl<Item> IntoFuture for CollectPrefix<Item>
where
    Item: Send + 'static,
{
    type Output = Result<Prefix<Item>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let collection = self.collection;
        Box::pin(async move {
            if collection.requested == 0 {
                return Ok(Prefix {
                    items: Vec::new(),
                    summary: TraversalSummary::zero_bound(),
                });
            }
            let (items, traversal) = collection.run(TraversalMode::Incremental).await?;
            Ok(Prefix {
                items,
                summary: traversal.summary(),
            })
        })
    }
}

impl<Item> fmt::Debug for CollectPrefix<Item> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CollectPrefix")
            .field("maximum_pages", &self.collection.requested)
            .field("controls", &self.collection.source.control_summary())
            .finish()
    }
}

/// An inert request to collect a complete traversal, returned by
/// `request.collect_all(maximum_pages)`.
///
/// Awaiting it yields every item only after the controller proved exhaustion
/// (`PageStep::Done`) within the effective bound. Reaching the bound while a
/// valid continuation exists returns a `Pagination`/`CompletionLimitExceeded`
/// error carrying the summary (`PageBound`/`NextRequestKnown`); a success
/// never contains an undisclosed prefix. Any other failure fails the whole
/// collection.
///
/// Controls (timeout, caps, cancellation) may be applied before awaiting.
/// Dropping the started future cancels the traversal through its drop guard.
pub struct CollectAll<Item> {
    collection: Collection<Item>,
}

/// Constructs an inert complete collector.
pub(super) fn new_collect_all<E>(
    request: Request<E>,
    maximum_pages: NonZeroU32,
) -> CollectAll<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    CollectAll {
        collection: new_collection(request, maximum_pages.get()),
    }
}

impl<Item> CollectAll<Item> {
    /// Replaces the selected finite timeout for the whole traversal. Caps are
    /// retained.
    pub fn timeout(self, duration: Duration) -> Result<Self, ControlError> {
        Ok(Self {
            collection: self.collection.timeout(duration)?,
        })
    }

    /// Restores the declared timeout. Caps are retained.
    #[must_use]
    pub fn reset_timeout(self) -> Self {
        Self {
            collection: self.collection.reset_timeout(),
        }
    }

    /// Attaches one more cancellation signal (at most eight distinct signals).
    pub fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Ok(Self {
            collection: self.collection.cancel_with(token)?,
        })
    }

    /// Intersects additional caps; caps only narrow execution.
    #[must_use]
    pub fn with_options(self, options: &CallOptions) -> Self {
        Self {
            collection: self.collection.with_options(options),
        }
    }

    /// Returns the value-free effective control summary.
    #[must_use]
    pub fn control_summary(&self) -> ControlSummary {
        self.collection.control_summary()
    }
}

impl<Item> RequestControls for CollectAll<Item> {
    fn with_options(self, options: &CallOptions) -> Self {
        Self::with_options(self, options)
    }

    fn control_summary(&self) -> ControlSummary {
        Self::control_summary(self)
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Self::cancel_with(self, token)
    }
}

impl<Item> IntoFuture for CollectAll<Item>
where
    Item: Send + 'static,
{
    type Output = Result<Vec<Item>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let collection = self.collection;
        Box::pin(async move {
            let (items, traversal) = collection.run(TraversalMode::Complete).await?;
            if traversal.summary().stop_reason() == StopReason::Exhausted {
                Ok(items)
            } else {
                // A complete traversal ends in exhaustion or an error; any
                // other terminal state is an invariant failure, never success.
                Err(traversal.invariant_error())
            }
        })
    }
}

impl<Item> fmt::Debug for CollectAll<Item> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CollectAll")
            .field("maximum_pages", &self.collection.requested)
            .field("controls", &self.collection.source.control_summary())
            .finish()
    }
}
