use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
};
use std::num::NonZeroUsize;

/// Page/per_page pagination (page starts at 1 by default).
#[derive(Clone, Debug)]
pub struct PagedPagination {
    /// Initial page number.
    pub page: u64,
    /// Page size (must be > 0).
    pub per_page: u64,
}

impl Default for PagedPagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl<Page> EndpointPagination<Page> for PagedPagination
where
    Page: PageItems,
{
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        validate_paged_page(self.page, ctx.ctx)?;
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_paged_page_size(self.per_page, ctx.ctx)?),
        })
    }

    fn advance(
        &mut self,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = validate_paged_page_size(
            self.per_page,
            &ErrorContext {
                endpoint: "pagination",
                method: http::Method::GET,
            },
        )?;
        self.page = self
            .page
            .checked_add(1)
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: ErrorContext {
                    endpoint: "pagination",
                    method: http::Method::GET,
                },
                msg: "paged: page overflow".into(),
            })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.page))
    }
}

fn validate_paged_page_size(
    value: u64,
    ctx: &ErrorContext,
) -> Result<NonZeroUsize, ApiClientError> {
    let value = usize::try_from(value).map_err(|_| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: "paged: page size does not fit in usize".into(),
    })?;
    NonZeroUsize::new(value).ok_or_else(|| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: "paged: page size must be greater than zero".into(),
    })
}

fn validate_paged_page(value: u64, ctx: &ErrorContext) -> Result<(), ApiClientError> {
    if value == 0 {
        return Err(ApiClientError::Pagination {
            ctx: ctx.clone(),
            msg: "paged: page=0".into(),
        });
    }
    Ok(())
}
