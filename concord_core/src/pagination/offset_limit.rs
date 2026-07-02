use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointField, EndpointPagination, EndpointPaginationController, PageAdvance, PageApply,
    PageApplyResult, PageDecision, PageItems, ProgressKey,
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;

    #[test]
    fn offset_limit_single_object_pagination_advances_offset() {
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut pagination = OffsetLimitPagination {
            offset: 0,
            limit: 2,
        };

        let applied = <OffsetLimitPagination as EndpointPagination<Vec<String>>>::apply(
            &mut pagination,
            PageApply {
                endpoint: "List",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .expect("offset apply");
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
        assert_eq!(
            <OffsetLimitPagination as EndpointPagination<Vec<String>>>::progress_key(&pagination),
            Some(ProgressKey::U64(0))
        );

        let decision = <OffsetLimitPagination as EndpointPagination<Vec<String>>>::advance(
            &mut pagination,
            &vec!["a".to_string()],
            PageAdvance {
                endpoint: "List",
                page_index: 0,
                received_items: 1,
            },
        )
        .expect("offset advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(pagination.offset, 2);
        assert_eq!(
            <OffsetLimitPagination as EndpointPagination<Vec<String>>>::progress_key(&pagination),
            Some(ProgressKey::U64(2))
        );

        let mut zero_limit = OffsetLimitPagination {
            offset: 0,
            limit: 0,
        };
        assert!(matches!(
            <OffsetLimitPagination as EndpointPagination<Vec<String>>>::apply(
                &mut zero_limit,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));

        let mut overflow = OffsetLimitPagination {
            offset: u64::MAX,
            limit: 1,
        };
        assert!(matches!(
            <OffsetLimitPagination as EndpointPagination<Vec<String>>>::advance(
                &mut overflow,
                &vec!["a".to_string()],
                PageAdvance {
                    endpoint: "List",
                    page_index: 0,
                    received_items: 1,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));
    }
}
