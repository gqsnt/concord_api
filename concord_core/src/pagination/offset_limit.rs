use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision, PageItems,
    ProgressKey,
};
use std::num::NonZeroUsize;

/// Offset/limit pagination (offset starts at 0 by default).
#[derive(Clone, Debug)]
pub struct OffsetLimitPagination {
    /// Initial offset value.
    pub offset: u64,
    /// Page size / limit (must be > 0).
    pub limit: u64,
}

impl Default for OffsetLimitPagination {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
        }
    }
}

impl<Page> EndpointPagination<Page> for OffsetLimitPagination
where
    Page: PageItems,
{
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_page_size(self.limit, "offset/limit", ctx.ctx)?),
        })
    }

    fn advance(
        &mut self,
        _page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = validate_page_size(
            self.limit,
            "offset/limit",
            &ErrorContext {
                endpoint: "pagination",
                method: http::Method::GET,
            },
        )?;
        self.offset =
            self.offset
                .checked_add(self.limit)
                .ok_or_else(|| ApiClientError::Pagination {
                    ctx: ErrorContext {
                        endpoint: "pagination",
                        method: http::Method::GET,
                    },
                    msg: "offset/limit: offset overflow".into(),
                })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.offset))
    }
}

impl From<OffsetLimitPagination> for crate::endpoint::PaginationPlan {
    fn from(value: OffsetLimitPagination) -> Self {
        Self::OffsetLimit {
            offset: value.offset,
            limit: value.limit,
        }
    }
}

fn validate_page_size(
    value: u64,
    controller: &'static str,
    ctx: &ErrorContext,
) -> Result<NonZeroUsize, ApiClientError> {
    let value = usize::try_from(value).map_err(|_| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: page size does not fit in usize").into(),
    })?;
    NonZeroUsize::new(value).ok_or_else(|| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: page size must be greater than zero").into(),
    })
}
