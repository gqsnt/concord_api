use concord_core::advanced::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderCursorPage {
    pub items: Vec<Item>,
}

impl PageItems for HeaderCursorPage {
    type Item = Item;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Default)]
pub struct HeaderCursorPagination;

#[derive(Clone, Debug)]
pub struct HeaderCursorBindings<E> {
    pub page: EndpointField<E, u64>,
    pub limit: EndpointField<E, u64>,
}

#[derive(Default)]
pub struct HeaderCursorState {
    pub page: u64,
    pub limit: u64,
}

impl<E, Page> EndpointPaginationController<E, Page> for HeaderCursorPagination
where
    E: 'static,
    Page: PageItems,
{
    type Bindings = HeaderCursorBindings<E>;
    type State = HeaderCursorState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        _ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        Ok(HeaderCursorState {
            page: bindings.page.get(endpoint),
            limit: bindings.limit.get(endpoint),
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        _ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        // This example asks the remote for two items per page. The runtime uses
        // the exact item_count_hint plus this expected size to stop on a short
        // terminal page before calling advance().
        bindings.page.set(endpoint, state.page);
        bindings.limit.set(endpoint, state.limit);
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(state.limit as usize),
        })
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
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

api! {
    client CustomPaginationApi {
        base "https://example.com"
    }

    GET ListItems(page: u64 = 0, limit: u64 = 2)
        as list_items
        path ["items"]
        query {
            "page" = page,
            "limit" = limit,
        }
        headers {
            "x-page-cursor" = page,
        }
        paginate endpoint_state HeaderCursorPagination bindings HeaderCursorBindings {
            page = page,
            limit = limit
        }
        -> Json<HeaderCursorPage>
}

pub use self::custom_pagination_api::CustomPaginationApi;
