use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
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
pub struct HeaderCursorPagination {
    pub page: u64,
    pub limit: u64,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderCursorPaginationBindings;

impl<Page> EndpointPagination<Page> for HeaderCursorPagination
where
    Page: PageItems,
{
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        if self.limit == 0 {
            return Err(ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "ListItems",
                    method: ::http::Method::GET,
                },
                msg: "endpoint_state custom pagination requires a non-zero page size".into(),
            });
        }
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(self.limit as usize),
        })
    }

    fn advance(
        &mut self,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        self.page = self
            .page
            .checked_add(1)
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "ListItems",
                    method: ::http::Method::GET,
                },
                msg: "endpoint_state custom pagination page overflow".into(),
            })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.page))
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
        paginate endpoint_state HeaderCursorPagination bindings HeaderCursorPaginationBindings {
            page = page,
            limit = limit
        }
        -> Json<HeaderCursorPage>
}

pub use self::custom_pagination_api::CustomPaginationApi;
