pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::error::{ApiClientError, ErrorContext};
pub use cursor::{CursorBindings, CursorPagination, CursorState, HasNextCursor};
pub use offset_limit::{OffsetLimitBindings, OffsetLimitPagination, OffsetLimitState};
pub use paged::{PagedBindings, PagedPagination, PagedState};
use std::marker::PhantomData;
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Debug, Default)]
pub struct PageApplyResult {
    pub expected_items_per_page: Option<NonZeroUsize>,
}

pub struct PageAdvance<'a> {
    pub endpoint: &'a str,
    pub page_index: u64,
    pub received_items: usize,
}

pub trait EndpointPaginationController<E, Page>: Send + Sync + 'static
where
    Page: PageItems,
{
    type Bindings: Send + 'static;
    type State: Send + 'static;

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
        page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

pub trait EndpointPaginationRuntime<E, Page>: Send
where
    Page: PageItems,
{
    fn init(&mut self, endpoint: &E, ctx: PageApply<'_>) -> Result<(), ApiClientError>;

    fn apply(
        &mut self,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError>;

    fn advance(
        &mut self,
        err_ctx: &ErrorContext,
        page: &Page,
        page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError>;

    fn progress_key(&self) -> Option<ProgressKey>;
}

pub struct EndpointPaginationRuntimeAdapter<C, E, Page>
where
    C: EndpointPaginationController<E, Page>,
    Page: PageItems,
{
    controller: C,
    bindings: C::Bindings,
    state: Option<C::State>,
    _marker: PhantomData<fn(E, Page)>,
}

impl<C, E, Page> EndpointPaginationRuntimeAdapter<C, E, Page>
where
    C: EndpointPaginationController<E, Page>,
    Page: PageItems,
{
    #[inline]
    pub fn new(controller: C, bindings: C::Bindings) -> Self {
        Self {
            controller,
            bindings,
            state: None,
            _marker: PhantomData,
        }
    }
}

impl<C, E, Page> EndpointPaginationRuntime<E, Page> for EndpointPaginationRuntimeAdapter<C, E, Page>
where
    C: EndpointPaginationController<E, Page>,
    Page: PageItems,
{
    fn init(&mut self, endpoint: &E, ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        if self.state.is_some() {
            return Err(ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "endpoint-state pagination runtime was initialized more than once".into(),
            });
        }
        let state = self.controller.init(&self.bindings, endpoint, ctx)?;
        self.state = Some(state);
        Ok(())
    }

    fn apply(
        &mut self,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        let state = self
            .state
            .as_mut()
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "endpoint-state pagination runtime was used before initialization".into(),
            })?;
        self.controller.apply(&self.bindings, state, endpoint, ctx)
    }

    fn advance(
        &mut self,
        err_ctx: &ErrorContext,
        page: &Page,
        page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let state = self
            .state
            .as_mut()
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: err_ctx.clone(),
                msg: "endpoint-state pagination runtime was used before initialization".into(),
            })?;
        self.controller
            .advance(&self.bindings, state, page, page_ctx)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        self.state
            .as_ref()
            .and_then(|state| self.controller.progress_key(state))
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[derive(Clone)]
    struct AdapterTestController {
        init_calls: Arc<AtomicUsize>,
        apply_calls: Arc<AtomicUsize>,
        advance_calls: Arc<AtomicUsize>,
    }

    impl AdapterTestController {
        fn new() -> Self {
            Self {
                init_calls: Arc::new(AtomicUsize::new(0)),
                apply_calls: Arc::new(AtomicUsize::new(0)),
                advance_calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl EndpointPaginationController<TestEndpoint, Vec<String>> for AdapterTestController {
        type Bindings = OffsetLimitBindings<TestEndpoint>;
        type State = TestState;

        fn init(
            &self,
            bindings: &Self::Bindings,
            endpoint: &TestEndpoint,
            _ctx: PageApply<'_>,
        ) -> Result<Self::State, ApiClientError> {
            self.init_calls.fetch_add(1, Ordering::SeqCst);
            Ok(TestState {
                offset: bindings.offset.get(endpoint),
                limit: bindings.limit.get(endpoint),
            })
        }

        fn apply(
            &self,
            bindings: &Self::Bindings,
            state: &mut Self::State,
            endpoint: &mut TestEndpoint,
            _ctx: PageApply<'_>,
        ) -> Result<PageApplyResult, ApiClientError> {
            self.apply_calls.fetch_add(1, Ordering::SeqCst);
            bindings.offset.set(endpoint, state.offset);
            bindings.limit.set(endpoint, state.limit);
            Ok(PageApplyResult {
                expected_items_per_page: NonZeroUsize::new(state.limit as usize),
            })
        }

        fn advance(
            &self,
            _bindings: &Self::Bindings,
            state: &mut Self::State,
            _page: &Vec<String>,
            _ctx: PageAdvance<'_>,
        ) -> Result<PageDecision, ApiClientError> {
            self.advance_calls.fetch_add(1, Ordering::SeqCst);
            state.offset = state.offset.checked_add(state.limit).unwrap();
            Ok(PageDecision::Continue)
        }

        fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
            Some(ProgressKey::U64(state.offset))
        }
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
    fn endpoint_pagination_runtime_adapter_calls_controller_and_tracks_progress() {
        let controller = AdapterTestController::new();
        let bindings = OffsetLimitBindings {
            offset: EndpointField::new(
                |ep: &TestEndpoint| ep.start,
                |ep: &mut TestEndpoint, value| ep.start = value,
            ),
            limit: EndpointField::new(
                |ep: &TestEndpoint| ep.count,
                |ep: &mut TestEndpoint, value| ep.count = value,
            ),
        };
        let mut adapter = EndpointPaginationRuntimeAdapter::new(controller.clone(), bindings);
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut endpoint = TestEndpoint {
            start: 7,
            count: 3,
            cursor: Some("start".to_string()),
        };

        assert_eq!(adapter.progress_key(), None);
        assert!(matches!(
            adapter.apply(
                &mut endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                }
            ),
            Err(ApiClientError::Pagination { .. })
        ));
        assert!(matches!(
            adapter.advance(
                &ctx,
                &vec!["a".to_string()],
                PageAdvance {
                    endpoint: "List",
                    page_index: 0,
                    received_items: 1,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));

        adapter
            .init(
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .unwrap();
        assert_eq!(controller.init_calls.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.progress_key(), Some(ProgressKey::U64(7)));

        let applied = adapter
            .apply(
                &mut endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 1,
                    ctx: &ctx,
                },
            )
            .unwrap();
        assert_eq!(controller.apply_calls.load(Ordering::SeqCst), 1);
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(3));
        assert_eq!(endpoint.start, 7);
        assert_eq!(endpoint.count, 3);

        let decision = adapter
            .advance(
                &ctx,
                &vec!["a".to_string(), "b".to_string()],
                PageAdvance {
                    endpoint: "List",
                    page_index: 1,
                    received_items: 2,
                },
            )
            .unwrap();
        assert_eq!(controller.advance_calls.load(Ordering::SeqCst), 1);
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(adapter.progress_key(), Some(ProgressKey::U64(10)));
    }
}
