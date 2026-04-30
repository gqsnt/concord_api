pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::error::ApiClientError;
pub use cursor::{CursorPagination, HasNextCursor};
use http::{HeaderMap, HeaderName, HeaderValue};
pub use offset_limit::OffsetLimitPagination;
pub use paged::PagedPagination;

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
}

impl<'a> PageRequest<'a> {
    #[doc(hidden)]
    pub fn new(query: &'a mut Vec<(String, String)>, headers: &'a mut HeaderMap) -> Self {
        Self { query, headers }
    }

    pub fn set_query<T>(&mut self, key: &'static str, value: T)
    where
        T: std::fmt::Display,
    {
        self.remove_query(key);
        self.query.push((key.to_string(), value.to_string()));
    }

    pub fn remove_query(&mut self, key: &'static str) {
        self.query.retain(|(existing, _)| existing != key);
    }

    pub fn set_header(&mut self, name: &'static str, value: HeaderValue) {
        self.headers.insert(HeaderName::from_static(name), value);
    }

    pub fn remove_header(&mut self, name: &'static str) {
        let _ = self.headers.remove(HeaderName::from_static(name));
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

#[derive(Copy, Clone, Debug)]
pub struct Caps {
    pub max_pages: u32,
    pub max_items: u64,
    pub detect_loops: bool,
}
impl Default for Caps {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_items: 100_000,
            detect_loops: true,
        }
    }
}
impl Caps {
    #[inline]
    pub fn max_pages(mut self, v: u32) -> Self {
        self.max_pages = v;
        self
    }
    #[inline]
    pub fn max_items(mut self, v: u64) -> Self {
        self.max_items = v;
        self
    }
    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.detect_loops = v;
        self
    }
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
