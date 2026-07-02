use concord_core::advanced::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;
use std::num::NonZeroUsize;

use self::custom_pagination_api::CustomPaginationApi;

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
pub struct HeaderCursorPagination;

pub struct HeaderCursorBindings<E> {
    pub page: EndpointField<E, u64>,
}

#[derive(Default)]
pub struct HeaderCursorState {
    pub page: u64,
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
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(2),
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
    client CustomPaginationApi { base "https://example.com" }

    GET List(page: u64 = 0)
        as list
        path ["items"]
        query {
            "page" = page,
        }
        paginate endpoint_state HeaderCursorPagination bindings HeaderCursorBindings {
            page = page
        }
        -> Json<Page>
}

fn usage(api: CustomPaginationApi) {
    let _ = api.list().paginate(PaginationTermination::hard_page_cap(2));
}

fn main() {}
