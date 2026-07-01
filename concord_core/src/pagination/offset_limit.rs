use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use std::borrow::Cow;
use std::num::NonZeroUsize;

/// Offset/limit pagination (offset starts at 0 by default).
///
/// This is the single "engine" for all offset-based APIs:
/// - you bind `offset` and `limit` to endpoint params via `paginate { offset: start, limit: count }`
/// - codegen can hint the effective query keys so this controller remains opaque to codegen.
#[derive(Clone, Debug)]
pub struct OffsetLimitPagination {
    /// Query key used for the offset (ex: "offset", "start", "skip").
    pub offset_key: Cow<'static, str>,
    /// Query key used for the limit (ex: "limit", "count", "top").
    pub limit_key: Cow<'static, str>,
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
            offset_key: Cow::from("offset"),
            limit_key: Cow::from("limit"),
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
        _ctx: PageAdvance<'_>,
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
            offset_key: value.offset_key.into_owned(),
            limit_key: value.limit_key.into_owned(),
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
