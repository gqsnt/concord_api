use concord_core::advanced::{
    PageAdvance, PageDecision, PageInit, PageRequest, PaginationController, ProgressKey,
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

#[derive(Default)]
pub struct HeaderCursorState {
    pub page: u64,
}

impl PaginationController<HeaderCursorPage> for HeaderCursorPagination {
    type State = HeaderCursorState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(HeaderCursorState::default())
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        // This example asks the remote for two items per page. The runtime uses
        // the exact item_count_hint plus this expected size to stop on a short
        // terminal page before calling advance().
        request.set_query("page", state.page);
        request.set_query("limit", 2);
        request.set_expected_items_per_page(
            NonZeroUsize::new(2).expect("example page size is non-zero"),
        );
        request.set_header(
            "x-page-cursor",
            http::HeaderValue::from_str(&state.page.to_string()).unwrap(),
        )?;
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        page: &HeaderCursorPage,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = page;
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

    GET ListItems
        as list_items
        path ["items"]
        paginate HeaderCursorPagination
        -> Json<HeaderCursorPage>
}

pub use self::custom_pagination_api::CustomPaginationApi;
