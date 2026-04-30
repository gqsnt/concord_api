use concord_core::advanced::{
    PageAdvance, PageDecision, PageInit, PageRequest, PaginationController, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;

use self::custom_pagination_api::CustomPaginationApi;

#[derive(Debug, Deserialize)]
pub struct Page {
    pub items: Vec<String>,
}

impl PageItems for Page {
    type Item = String;

    fn item_count(&self) -> usize {
        self.items.len()
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

impl PaginationController<Page> for HeaderCursorPagination {
    type State = HeaderCursorState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(HeaderCursorState::default())
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.is_empty() {
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
    client CustomPaginationApi { base https "example.com" }

    GET List
        as list
        path ["items"]
        paginate HeaderCursorPagination
        -> Json<Page>
}

fn usage(api: CustomPaginationApi) {
    let _ = api.list().paginate().max_pages(2);
}

fn main() {}
