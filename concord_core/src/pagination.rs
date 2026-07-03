pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::error::{ApiClientError, ErrorContext};
pub use cursor::{CursorPagination, HasNextCursor};
pub use offset_limit::OffsetLimitPagination;
pub use paged::PagedPagination;
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaginationTermination {
    /// Fetch pages until the pagination controller stops, but error if more than this many pages would be required.
    HardPageCap(usize),
    /// Fetch items until the pagination controller stops, but error if more than this many items would be collected.
    HardItemCap(usize),
    /// Fetch at most this many pages and stop cleanly even if more pages exist.
    TakePages(usize),
    /// Return at most this many items and stop cleanly, truncating the final page in collect().
    TakeItems(usize),
}

impl PaginationTermination {
    #[inline]
    pub const fn hard_page_cap(n: usize) -> Self {
        Self::HardPageCap(n)
    }

    #[inline]
    pub const fn hard_item_cap(n: usize) -> Self {
        Self::HardItemCap(n)
    }

    #[inline]
    pub const fn take_pages(n: usize) -> Self {
        Self::TakePages(n)
    }

    #[inline]
    pub const fn take_items(n: usize) -> Self {
        Self::TakeItems(n)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaginationCaps {
    pub termination: PaginationTermination,
    pub detect_loops: bool,
}

impl PaginationCaps {
    #[inline]
    pub const fn new(termination: PaginationTermination) -> Self {
        Self {
            termination,
            detect_loops: true,
        }
    }

    #[inline]
    pub const fn detect_loops(mut self, enabled: bool) -> Self {
        self.detect_loops = enabled;
        self
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Control {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProgressKey {
    U64(u64),
    Str(String),
    Bytes(Vec<u8>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PageDecision {
    Continue,
    Stop,
}

impl From<PageDecision> for Control {
    fn from(value: PageDecision) -> Self {
        match value {
            PageDecision::Continue => Self::Continue,
            PageDecision::Stop => Self::Stop,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PageApply<'a> {
    pub endpoint: &'a str,
    pub page_index: u64,
    pub ctx: &'a ErrorContext,
}

pub struct PageAdvance<'a> {
    pub endpoint: &'a str,
    pub page_index: u64,
    pub item_count_hint: Option<usize>,
}

/// Pagination runtime contract for stateful controllers.
///
/// Implementations own pagination state for a run and can update that state
/// before a page is sent and after a page is decoded. Pagination controllers
/// must not render HTTP query, header, path, or body material directly; the
/// endpoint plan remains responsible for that output.
pub trait EndpointPagination<Page>: Default + Send + Sync + 'static
where
    Page: PageItems,
{
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<(), ApiClientError>;

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        None
    }

    fn advance(
        &mut self,
        page: &Page,
        ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn progress_key(&self) -> Option<ProgressKey> {
        None
    }
}

/// Endpoint-to-pagination binding contract intended for generated endpoints.
///
/// The binding loads the pagination state from endpoint fields and stores the
/// current pagination state back to those fields before endpoint planning.
pub trait PaginateBinding<P> {
    fn load_pagination(&self) -> P;

    fn store_pagination(&mut self, pagination: &P);
}

/// Runtime adapter for pagination objects that own both pagination state and
/// pagination decisions.
pub trait PaginationRuntime<E, Page>: Send
where
    Page: PageItems,
{
    fn init(&mut self, endpoint: &E, ctx: PageApply<'_>) -> Result<(), ApiClientError>;

    fn apply(&mut self, endpoint: &mut E, ctx: PageApply<'_>) -> Result<(), ApiClientError>;

    fn advance(
        &mut self,
        endpoint: &mut E,
        err_ctx: &ErrorContext,
        page: &Page,
        page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn expected_items_per_page(&self) -> Option<NonZeroUsize>;

    fn progress_key(&self) -> Option<ProgressKey>;
}

pub struct PaginationRuntimeAdapter<P> {
    pagination: Option<P>,
}

impl<P> PaginationRuntimeAdapter<P> {
    #[inline]
    pub fn new() -> Self {
        Self { pagination: None }
    }
}

impl<E, Page, P> PaginationRuntime<E, Page> for PaginationRuntimeAdapter<P>
where
    E: PaginateBinding<P>,
    P: EndpointPagination<Page>,
    Page: PageItems,
{
    fn init(&mut self, endpoint: &E, ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        if self.pagination.is_some() {
            return Err(ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "pagination runtime was initialized more than once".into(),
            });
        }
        self.pagination = Some(endpoint.load_pagination());
        Ok(())
    }

    fn apply(&mut self, endpoint: &mut E, ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        let pagination = self
            .pagination
            .as_mut()
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "pagination runtime was used before initialization".into(),
            })?;
        pagination.apply(ctx)?;
        endpoint.store_pagination(pagination);
        Ok(())
    }

    fn advance(
        &mut self,
        endpoint: &mut E,
        err_ctx: &ErrorContext,
        page: &Page,
        page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let pagination = self
            .pagination
            .as_mut()
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: err_ctx.clone(),
                msg: "pagination runtime was used before initialization".into(),
            })?;
        let decision = pagination.advance(page, page_ctx)?;
        endpoint.store_pagination(pagination);
        Ok(decision)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        self.pagination
            .as_ref()
            .and_then(EndpointPagination::progress_key)
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        self.pagination
            .as_ref()
            .and_then(EndpointPagination::expected_items_per_page)
    }
}

/// Items container returned by a paginated endpoint.
pub trait PageItems: Send + 'static {
    type Item: Send + 'static;

    /// Returns the exact number of items in this page when it can be observed
    /// without consuming the page.
    ///
    /// If this returns `Some(n)`, `n` must be exact. The runtime uses this
    /// value for pre-advance empty/short-page termination and for
    /// `collect()` item-cap checks. Return `None` only when the page type
    /// cannot expose the count without consuming itself.
    fn item_count_hint(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.item_count_hint() == Some(0)
    }

    fn into_items(self) -> Vec<Self::Item>;
}
impl<T: Send + 'static> PageItems for Vec<T> {
    type Item = T;

    fn item_count_hint(&self) -> Option<usize> {
        Some(Vec::len(self))
    }

    fn into_items(self) -> Vec<Self::Item> {
        self
    }
}
