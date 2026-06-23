pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::error::{ApiClientError, ErrorContext};
pub use cursor::{CursorPagination, HasNextCursor};
use http::{HeaderMap, HeaderName, HeaderValue};
pub use offset_limit::OffsetLimitPagination;
pub use paged::PagedPagination;

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

pub struct PageInit<'a> {
    pub endpoint: &'a str,
}

pub struct PageAdvance<'a> {
    pub endpoint: &'a str,
    pub page_index: u64,
    pub received_items: usize,
}

pub struct PageRequest<'a> {
    query: &'a mut Vec<(String, String)>,
    headers: &'a mut HeaderMap,
    ctx: ErrorContext,
}

impl<'a> PageRequest<'a> {
    pub(crate) fn new(
        query: &'a mut Vec<(String, String)>,
        headers: &'a mut HeaderMap,
        ctx: ErrorContext,
    ) -> Self {
        Self {
            query,
            headers,
            ctx,
        }
    }

    pub fn set_query<T>(&mut self, key: impl Into<String>, value: T)
    where
        T: std::fmt::Display,
    {
        let key = key.into();
        self.remove_query(&key);
        self.query.push((key, value.to_string()));
    }

    pub fn remove_query(&mut self, key: &str) {
        self.query.retain(|(existing, _)| existing != key);
    }

    pub fn set_header<N>(&mut self, name: N, value: HeaderValue) -> Result<(), ApiClientError>
    where
        N: TryInto<HeaderName>,
        N::Error: std::fmt::Display,
    {
        let name = name
            .try_into()
            .map_err(|source| ApiClientError::Pagination {
                ctx: self.ctx.clone(),
                msg: format!("invalid pagination header name: {source}").into(),
            })?;
        self.headers.insert(name, value);
        Ok(())
    }

    pub fn remove_header<N>(&mut self, name: N) -> Result<(), ApiClientError>
    where
        N: TryInto<HeaderName>,
        N::Error: std::fmt::Display,
    {
        let name = name
            .try_into()
            .map_err(|source| ApiClientError::Pagination {
                ctx: self.ctx.clone(),
                msg: format!("invalid pagination header name: {source}").into(),
            })?;
        let _ = self.headers.remove(name);
        Ok(())
    }
}

pub trait PaginationController<Page>: Send + Sync + 'static
where
    Page: PageItems,
{
    type State: Send + Sync + 'static;

    fn init(&self, ctx: PageInit<'_>) -> Result<Self::State, ApiClientError>;

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError>;

    fn advance(
        &self,
        state: &mut Self::State,
        page: &Page,
        ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey>;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Stop {
    #[default]
    OnEmpty,
}

/// Items container returned by a paginated endpoint.
pub trait PageItems: Send + 'static {
    type Item: Send + 'static;

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
