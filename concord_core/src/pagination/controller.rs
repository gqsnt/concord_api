use std::num::{NonZeroU32, NonZeroU64};

use super::{PaginationError, PaginationRequest};
use crate::error::PaginationErrorKind;

/// Deterministic application-defined continuation state for one sequential pagination operation.
///
/// Controller values can be cloned and used by independent concurrent
/// pagination operations, while calls within one operation remain sequential.
/// Decoded pages and continuation values may be sensitive; implementations own
/// their logging and storage and must bound CPU, memory, retries, maps, queues,
/// spawned tasks, and metric cardinality; they must be non-blocking and perform
/// no external I/O. Both callbacks are synchronous, execute outside the
/// per-page operation deadline, and cannot be preempted by Concord. Cancellation
/// prevents later pages only, and Concord does not automatically bound custom
/// controller state or callback time.
pub trait PaginationController<Page>: Clone + Send + Sync + 'static {
    /// Applies the current continuation to public query or header state.
    fn apply(&self, request: &mut PaginationRequest<'_>) -> Result<(), PaginationError>;

    /// Derives the next immutable controller state from one decoded page.
    fn next(&self, page: &Page) -> Result<Option<Self>, PaginationError>;
}

/// A decoded page that can expose an optional next cursor by reference.
///
/// The cursor and page may be sensitive. Implementations own logging and storage
/// and must keep synchronous CPU, memory, retries, maps, queues, tasks, metric
/// cardinality finite; the callback must be non-blocking and perform no external
/// I/O. Calls cannot be preempted, may occur in independent concurrent
/// operations, execute outside the per-page operation deadline, and stop only
/// before later work when cancellation is observed.
pub trait HasNextCursor<C> {
    /// Returns the next cursor, distinguishing an absent cursor from a present
    /// cursor whose value is empty.
    fn next_cursor(&self) -> Option<&C>;
}

/// Built-in checked offset pagination state.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OffsetPagination {
    offset: u64,
    page_size: NonZeroU32,
}

impl OffsetPagination {
    /// Creates offset pagination at `offset` with a positive page size.
    #[must_use]
    pub const fn new(offset: u64, page_size: NonZeroU32) -> Self {
        Self { offset, page_size }
    }

    /// Returns the current offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the requested page size.
    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }
}

impl<Page> PaginationController<Page> for OffsetPagination {
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            offset,
            page_size: self.page_size,
        }))
    }
}

/// Built-in checked one-based page-number pagination state.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PagePagination {
    page: NonZeroU64,
    page_size: NonZeroU32,
}

impl PagePagination {
    /// Creates page-number pagination at `page` with a positive page size.
    #[must_use]
    pub const fn new(page: NonZeroU64, page_size: NonZeroU32) -> Self {
        Self { page, page_size }
    }

    /// Returns the current page number.
    #[must_use]
    pub const fn page(&self) -> NonZeroU64 {
        self.page
    }

    /// Returns the requested page size.
    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }
}

impl<Page> PaginationController<Page> for PagePagination {
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let page = self
            .page
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            page,
            page_size: self.page_size,
        }))
    }
}

/// Built-in typed cursor pagination state.
#[derive(Clone, Eq, PartialEq)]
pub struct CursorPagination<C> {
    cursor: Option<C>,
}

impl<C> CursorPagination<C> {
    /// Creates cursor pagination with an optional initial cursor.
    #[must_use]
    pub const fn new(cursor: Option<C>) -> Self {
        Self { cursor }
    }

    /// Returns the current cursor by reference.
    #[must_use]
    pub const fn cursor(&self) -> Option<&C> {
        self.cursor.as_ref()
    }
}

impl<Page, C> PaginationController<Page> for CursorPagination<C>
where
    Page: HasNextCursor<C>,
    C: Clone + Send + Sync + 'static,
{
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, page: &Page) -> Result<Option<Self>, PaginationError> {
        Ok(page
            .next_cursor()
            .cloned()
            .map(|cursor| Self::new(Some(cursor))))
    }
}
