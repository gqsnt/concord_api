use concord_core::advanced::{
    PageAdvance, PageDecision, PageInit, PageRequest, PaginationController, ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Page {
    pub items: Vec<String>,
}

impl PageItems for Page {
    type Item = String;

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

pub struct HeaderCursorPagination {
    _private: (),
}

pub struct HeaderCursorState;

impl PaginationController<Page> for HeaderCursorPagination {
    type State = HeaderCursorState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(HeaderCursorState)
    }

    fn apply(
        &self,
        _state: &Self::State,
        _request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        Ok(())
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

api! {
    client MissingDefaultPaginationApi { base "https://example.com" }

    GET List
        as list
        path ["items"]
        paginate HeaderCursorPagination
        -> Json<Page>
}

fn main() {}
