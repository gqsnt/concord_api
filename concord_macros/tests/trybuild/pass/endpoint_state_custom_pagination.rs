use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
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
pub struct HeaderCursorPagination {
    pub page: u64,
}

#[derive(Default)]
pub struct HeaderCursorPaginationBindings;

impl<Page> EndpointPagination<Page> for HeaderCursorPagination
where
    Page: PageItems,
{
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(2),
        })
    }

    fn advance(
        &mut self,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        self.page = self.page.saturating_add(1);
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.page))
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
        paginate endpoint_state HeaderCursorPagination bindings HeaderCursorPaginationBindings {
            page = page
        }
        -> Json<Page>
}

fn usage(api: CustomPaginationApi) {
    let _ = api.list().paginate(PaginationTermination::hard_page_cap(2));
}

fn main() {}
