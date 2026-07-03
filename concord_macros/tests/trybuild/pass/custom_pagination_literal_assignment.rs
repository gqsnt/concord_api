use concord_core::advanced::{
    EndpointPagination, PageAdvance, PageApply, PageDecision, PageItems,
    ProgressKey,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Debug, Deserialize)]
pub struct PaginationPage {
    pub items: Vec<String>,
}

impl PageItems for PaginationPage {
    type Item = String;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Default)]
pub struct HeaderPagePagination {
    pub page: u64,
    pub count: u64,
    pub max_pages: u64,
}

impl<Page> EndpointPagination<Page> for HeaderPagePagination
where
    Page: PageItems,
{
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::pagination(
                concord_core::advanced::ErrorContext {
                    endpoint: "List",
                    method: ::http::Method::GET,
                },
                concord_core::error::PaginationErrorKind::InvalidSize,
                "custom pagination requires a non-zero page size",
            ));
        }
        Ok(())
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        usize::try_from(self.count).ok().and_then(NonZeroUsize::new)
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
    client CustomPaginationLiteralAssignmentApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        headers {
            "X-Page" = page,
            "X-Count" = count,
        }
        paginate HeaderPagePagination {
            page = page,
            count = count,
            max_pages = 3
        }
        -> Json<PaginationPage>
}

fn usage(api: crate::custom_pagination_literal_assignment_api::CustomPaginationLiteralAssignmentApi) {
    let _ = api.list().paginate(PaginationTermination::hard_page_cap(2));
}

fn main() {}
