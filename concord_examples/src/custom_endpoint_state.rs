use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
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
pub struct HeaderPagePagination {
    pub page: u64,
    pub count: u64,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderPageBindings;

impl<Page> EndpointPagination<Page> for HeaderPagePagination
where
    Page: PageItems,
{
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "ListItems",
                    method: ::http::Method::GET,
                },
                msg: "endpoint_state custom pagination requires a non-zero page size".into(),
            });
        }
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(self.count as usize),
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
