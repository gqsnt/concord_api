pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::error::{ApiClientError, ErrorContext};
pub use cursor::{CursorPagination, HasNextCursor};
use http::{HeaderMap, HeaderName, HeaderValue};
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

#[derive(Clone, Copy)]
pub struct EndpointField<E, T> {
    get: fn(&E) -> T,
    set: fn(&mut E, T),
}

impl<E, T> EndpointField<E, T> {
    #[inline]
    pub const fn new(get: fn(&E) -> T, set: fn(&mut E, T)) -> Self {
        Self { get, set }
    }

    #[inline]
    pub fn get(&self, endpoint: &E) -> T {
        (self.get)(endpoint)
    }

    #[inline]
    pub fn set(&self, endpoint: &mut E, value: T) {
        (self.set)(endpoint, value)
    }
}

#[derive(Clone, Debug)]
pub struct PageApply<'a> {
    pub endpoint: &'a str,
    pub page_index: u64,
    pub ctx: &'a ErrorContext,
}

pub struct PageInit<'a> {
    pub endpoint: &'a str,
}

#[derive(Clone, Debug, Default)]
pub struct PageApplyResult {
    pub expected_items_per_page: Option<NonZeroUsize>,
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
    expected_items_per_page: Option<NonZeroUsize>,
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
            expected_items_per_page: None,
        }
    }

    pub fn set_query<T>(&mut self, key: impl Into<String>, value: T)
    where
        T: std::fmt::Display,
    {
        let key = key.into();
        // Deterministic override-by-key: remove all prior entries for the key,
        // then append the new value at the end of the logical query list.
        self.remove_query(&key);
        self.query.push((key, value.to_string()));
    }

    /// Removes every query entry matching `key`.
    ///
    /// Missing keys are a no-op. This keeps query mutation deterministic and
    /// preserves the relative order of the remaining keys.
    pub fn remove_query(&mut self, key: &str) {
        self.query.retain(|(existing, _)| existing != key);
    }

    /// Inserts or replaces a header on the page request.
    ///
    /// Header names are validated here and return a typed pagination error on
    /// failure. Header values are already represented as `HeaderValue`, so
    /// invalid values must be rejected before this call or remain
    /// unrepresentable by the caller's API.
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

    #[inline]
    /// Records the non-zero number of items requested for the current page.
    ///
    /// Custom pagination controllers should set this during `apply()` whenever
    /// they request a known page size. The value is scoped to the current page
    /// request. The value does not persist to the next page.
    pub fn set_expected_items_per_page(&mut self, n: NonZeroUsize) {
        self.expected_items_per_page = Some(n);
    }

    #[inline]
    /// Clears the expected item count for the current page request.
    pub fn clear_expected_items_per_page(&mut self) {
        self.expected_items_per_page = None;
    }

    #[inline]
    /// Returns the expected item count recorded for the current page request.
    pub fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        self.expected_items_per_page
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

pub trait EndpointPaginationController<E, Page>: Send + Sync + 'static
where
    Page: PageItems,
{
    type Bindings: Send + Sync + 'static;
    type State: Send + Sync + 'static;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError>;

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError>;

    fn advance(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        page: &Page,
        ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;
    use std::num::NonZeroUsize;

    #[derive(Clone, Debug)]
    struct TestEndpoint {
        start: u64,
        count: u64,
        cursor: Option<String>,
    }

    #[derive(Clone, Debug)]
    struct TestState {
        offset: u64,
        limit: u64,
    }

    struct TestBindings {
        start: EndpointField<TestEndpoint, u64>,
        count: EndpointField<TestEndpoint, u64>,
    }

    struct TestEndpointPaginationController;

    impl EndpointPaginationController<TestEndpoint, Vec<String>> for TestEndpointPaginationController {
        type Bindings = TestBindings;
        type State = TestState;

        fn init(
            &self,
            bindings: &Self::Bindings,
            endpoint: &TestEndpoint,
            ctx: PageApply<'_>,
        ) -> Result<Self::State, ApiClientError> {
            assert_eq!(ctx.endpoint, "TestEndpoint");
            assert_eq!(ctx.page_index, 0);
            assert_eq!(ctx.ctx.endpoint, "Items");
            Ok(TestState {
                offset: bindings.start.get(endpoint),
                limit: bindings.count.get(endpoint),
            })
        }

        fn apply(
            &self,
            bindings: &Self::Bindings,
            state: &mut Self::State,
            endpoint: &mut TestEndpoint,
            ctx: PageApply<'_>,
        ) -> Result<PageApplyResult, ApiClientError> {
            assert_eq!(ctx.endpoint, "TestEndpoint");
            assert_eq!(ctx.page_index, 1);
            assert_eq!(ctx.ctx.method, &Method::GET);
            bindings.start.set(endpoint, state.offset);
            bindings.count.set(endpoint, state.limit);
            Ok(PageApplyResult {
                expected_items_per_page: NonZeroUsize::new(state.limit as usize),
            })
        }

        fn advance(
            &self,
            _bindings: &Self::Bindings,
            state: &mut Self::State,
            page: &Vec<String>,
            ctx: PageAdvance<'_>,
        ) -> Result<PageDecision, ApiClientError> {
            assert_eq!(ctx.endpoint, "TestEndpoint");
            assert_eq!(ctx.page_index, 1);
            assert_eq!(ctx.received_items, page.len());
            state.offset = state.offset.checked_add(state.limit).unwrap();
            Ok(PageDecision::Continue)
        }
    }

    #[test]
    fn page_request_query_order_and_remove_semantics_are_deterministic() {
        let mut query = Vec::new();
        let mut headers = HeaderMap::new();
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut request = PageRequest::new(&mut query, &mut headers, ctx);

        request.set_query("tag", "base");
        request.set_query("q", "first");
        request.set_query("tag", "override");
        request.remove_query("missing");
        request.remove_query("q");
        request.set_query("q", "final");

        assert_eq!(
            &*request.query,
            &[
                ("tag".to_string(), "override".to_string()),
                ("q".to_string(), "final".to_string())
            ]
        );
    }

    #[test]
    fn page_request_invalid_header_name_returns_typed_error_with_context() {
        let mut query = Vec::new();
        let mut headers = HeaderMap::new();
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::POST,
        };
        let mut request = PageRequest::new(&mut query, &mut headers, ctx.clone());

        let err = request
            .set_header("bad header name", HeaderValue::from_static("value"))
            .expect_err("invalid header names should return a typed pagination error");

        assert!(matches!(err, ApiClientError::Pagination { .. }));
        assert_eq!(err.context().endpoint, ctx.endpoint);
        assert_eq!(err.context().method, &ctx.method);
        let msg = err.to_string();
        assert!(msg.contains("invalid pagination header name"));
        assert!(msg.contains("POST Items"));
    }

    #[test]
    fn endpoint_field_can_read_and_write_endpoint_state() {
        let start = EndpointField::new(
            |ep: &TestEndpoint| ep.start,
            |ep: &mut TestEndpoint, value| ep.start = value,
        );
        let mut ep = TestEndpoint {
            start: 0,
            count: 20,
            cursor: None,
        };

        assert_eq!(start.get(&ep), 0);
        start.set(&mut ep, 40);
        assert_eq!(ep.start, 40);
    }

    #[test]
    fn endpoint_field_generated_style_clone_getters_handle_non_copy_state() {
        let cursor = EndpointField::new(
            |ep: &TestEndpoint| ep.cursor.clone(),
            |ep: &mut TestEndpoint, value| ep.cursor = value,
        );
        let mut ep = TestEndpoint {
            start: 0,
            count: 20,
            cursor: Some("abc".to_string()),
        };

        assert_eq!(cursor.get(&ep), Some("abc".to_string()));
        cursor.set(&mut ep, Some("xyz".to_string()));
        assert_eq!(ep.cursor, Some("xyz".to_string()));
    }

    #[test]
    fn endpoint_pagination_controller_mutates_endpoint_fields_without_request_state() {
        let controller = TestEndpointPaginationController;
        let bindings = TestBindings {
            start: EndpointField::new(
                |ep: &TestEndpoint| ep.start,
                |ep: &mut TestEndpoint, value| ep.start = value,
            ),
            count: EndpointField::new(
                |ep: &TestEndpoint| ep.count,
                |ep: &mut TestEndpoint, value| ep.count = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let page_ctx = PageApply {
            endpoint: "TestEndpoint",
            page_index: 0,
            ctx: &ctx,
        };
        let mut endpoint = TestEndpoint {
            start: 0,
            count: 20,
            cursor: Some("start".to_string()),
        };

        let mut state = controller
            .init(&bindings, &endpoint, page_ctx.clone())
            .unwrap();
        assert_eq!(state.offset, 0);
        assert_eq!(state.limit, 20);

        state.offset = 40;
        state.limit = 10;
        let applied = controller
            .apply(
                &bindings,
                &mut state,
                &mut endpoint,
                PageApply {
                    endpoint: "TestEndpoint",
                    page_index: 1,
                    ctx: &ctx,
                },
            )
            .unwrap();
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(10));
        assert_eq!(endpoint.start, 40);
        assert_eq!(endpoint.count, 10);
        assert_eq!(endpoint.cursor, Some("start".to_string()));

        let decision = controller
            .advance(
                &bindings,
                &mut state,
                &vec!["a".to_string(), "b".to_string()],
                PageAdvance {
                    endpoint: "TestEndpoint",
                    page_index: 1,
                    received_items: 2,
                },
            )
            .unwrap();
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(state.offset, 50);
        assert_eq!(state.limit, 10);
    }

    #[test]
    fn page_request_remove_header_invalid_name_returns_typed_error() {
        let mut query = Vec::new();
        let mut headers = HeaderMap::new();
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::POST,
        };
        let mut request = PageRequest::new(&mut query, &mut headers, ctx);

        let err = request
            .remove_header("bad header name")
            .expect_err("invalid header names should return a typed pagination error");

        assert!(matches!(err, ApiClientError::Pagination { .. }));
        assert!(err.to_string().contains("invalid pagination header name"));
    }
}
