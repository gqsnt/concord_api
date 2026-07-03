use crate::error::{ApiClientError, ErrorContext, PaginationErrorKind};
use crate::pagination::{
    EndpointPagination, PageAdvance, PageApply, PageDecision, PageItems, ProgressKey,
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
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        validate_page_size(self.limit, "offset/limit", ctx.ctx)?;
        Ok(())
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        usize::try_from(self.limit).ok().and_then(NonZeroUsize::new)
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
        self.offset = self.offset.checked_add(self.limit).ok_or_else(|| {
            ApiClientError::pagination(
                ErrorContext {
                    endpoint: "pagination",
                    method: http::Method::GET,
                },
                PaginationErrorKind::Overflow,
                "offset/limit: offset overflow",
            )
        })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.offset))
    }
}

fn validate_page_size(
    value: u64,
    controller: &'static str,
    ctx: &ErrorContext,
) -> Result<NonZeroUsize, ApiClientError> {
    let value = usize::try_from(value).map_err(|_| {
        ApiClientError::pagination(
            ctx.clone(),
            PaginationErrorKind::Overflow,
            format!("{controller}: page size does not fit in usize"),
        )
    })?;
    NonZeroUsize::new(value).ok_or_else(|| {
        ApiClientError::pagination(
            ctx.clone(),
            PaginationErrorKind::InvalidSize,
            format!("{controller}: page size must be greater than zero"),
        )
    })
}
