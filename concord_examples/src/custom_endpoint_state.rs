use concord_core::advanced::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EndpointStateItem {
    pub id: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderPage {
    pub items: Vec<EndpointStateItem>,
}

impl PageItems for HeaderPage {
    type Item = EndpointStateItem;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Default)]
pub struct HeaderPagePagination;

#[derive(Clone, Debug)]
pub struct HeaderPageBindings<E> {
    pub page: EndpointField<E, u64>,
    pub count: EndpointField<E, u64>,
}

#[derive(Clone, Debug)]
pub struct HeaderPageState {
    page: u64,
    count: u64,
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
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        let count = bindings.count.get(endpoint);
        if count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "endpoint_state custom pagination requires a non-zero page size".into(),
            });
        }

        Ok(HeaderPageState {
            page: bindings.page.get(endpoint),
            count,
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
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(state.count as usize),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.page += 1;
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
        as list_items
        path ["items"]
        headers {
            "X-Page" = page,
            "X-Count" = count,
        }
        paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {
            page = page,
            count = count
        }
        -> Json<HeaderPage>
}

pub use self::custom_endpoint_state_api::CustomEndpointStateApi;
