use concord_core::advanced::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Debug, Deserialize)]
pub struct Page {
    pub items: Vec<String>,
}

impl PageItems for Page {
    type Item = String;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Default)]
pub struct HeaderPagePagination {
    pub page: u64,
    pub count: u64,
}

pub struct HeaderPageBindings<E> {
    pub page: EndpointField<E, u64>,
    pub count: EndpointField<E, u64>,
}

#[derive(Clone)]
pub struct HeaderPageState {
    pub page: u64,
    pub count: u64,
}

impl HeaderPageState {
    fn expected_items_per_page(&self) -> PageApplyResult {
        PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(self.count as usize),
        }
    }
}

impl<E, Page> EndpointPaginationController<E, Page> for HeaderPagePagination
where
    E: 'static,
    Page: PageItems,
{
    type Bindings = HeaderPageBindings<E>;
    type State = HeaderPageState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        _ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        Ok(HeaderPageState {
            page: bindings.page.get(endpoint),
            count: bindings.count.get(endpoint),
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        _ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        bindings.page.set(endpoint, state.page);
        bindings.count.set(endpoint, state.count);
        Ok(state.expected_items_per_page())
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.item_count_hint() == Some(0) {
            return Ok(PageDecision::Stop);
        }
        state.page = state.page.saturating_add(1);
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

api! {
    client CustomEndpointStateApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        headers {
            "X-Page" = page,
            "X-Count" = count,
        }
        paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {
            page = page,
            count = count
        }
        -> Json<Page>
}

fn usage(api: crate::custom_endpoint_state_api::CustomEndpointStateApi) {
    let _ = api.list().paginate(PaginationTermination::hard_page_cap(2));
}

fn main() {}
