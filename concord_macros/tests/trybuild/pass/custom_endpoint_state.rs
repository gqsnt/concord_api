use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
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

#[derive(Default)]
pub struct HeaderPageBindings;

impl<Page> EndpointPagination<Page> for HeaderPagePagination
where
    Page: PageItems,
{
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "List",
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
        self.page = self.page.saturating_add(1);
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
