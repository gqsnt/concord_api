use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use std::num::NonZeroUsize;

/// Offset/limit pagination (offset starts at 0 by default).
///
/// This is the single controller model for offset-based APIs:
/// - you bind `offset` and `limit` to endpoint params via `paginate { offset: start, limit: count }`
/// - endpoint-state runtime reads the bound endpoint fields directly.
#[derive(Clone, Debug)]
pub struct OffsetLimitPagination {
    /// Initial offset value.
    pub offset: u64,
    /// Page size / limit (must be > 0).
    pub limit: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetLimitBindings<E> {
    pub offset: EndpointField<E, u64>,
    pub limit: EndpointField<E, u64>,
}

#[derive(Clone, Debug)]
pub struct OffsetLimitState {
    pub offset: u64,
    pub limit: u64,
    ctx: ErrorContext,
}

impl Default for OffsetLimitPagination {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
        }
    }
}

impl<E: 'static, Page> EndpointPaginationController<E, Page> for OffsetLimitPagination
where
    Page: PageItems,
{
    type Bindings = OffsetLimitBindings<E>;
    type State = OffsetLimitState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        let offset = bindings.offset.get(endpoint);
        let limit = bindings.limit.get(endpoint);
        validate_page_size(limit, "offset/limit", ctx.ctx)?;
        Ok(OffsetLimitState {
            offset,
            limit,
            ctx: ctx.ctx.clone(),
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        bindings.offset.set(endpoint, state.offset);
        bindings.limit.set(endpoint, state.limit);
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_page_size(
                state.limit,
                "offset/limit",
                ctx.ctx,
            )?),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        _page: &Page,
        _page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.offset =
            state
                .offset
                .checked_add(state.limit)
                .ok_or_else(|| ApiClientError::Pagination {
                    ctx: state.ctx.clone(),
                    msg: "offset/limit: offset overflow".into(),
                })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.offset))
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
