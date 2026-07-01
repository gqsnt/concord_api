use crate::endpoint::PaginationPlan;
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointField, EndpointPaginationController, PageAdvance, PageApply, PageApplyResult,
    PageDecision, PageItems, ProgressKey,
};
use std::borrow::Cow;
use std::num::NonZeroUsize;

/// Page/per_page pagination (page starts at 1 by default).
#[derive(Clone, Debug)]
pub struct PagedPagination {
    /// Legacy compatibility metadata for old snapshots/codepaths.
    pub page_key: Cow<'static, str>,
    /// Legacy compatibility metadata for old snapshots/codepaths.
    pub per_page_key: Cow<'static, str>,

    /// Initial page number.
    pub page: u64,
    /// Page size (must be > 0).
    pub per_page: u64,
}

impl Default for PagedPagination {
    fn default() -> Self {
        Self {
            page_key: Cow::from("page"),
            per_page_key: Cow::from("per_page"),
            page: 1,
            per_page: 20,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PagedBindings<E> {
    pub page: EndpointField<E, u64>,
    pub per_page: EndpointField<E, u64>,
}

#[derive(Clone, Debug)]
pub struct PagedState {
    pub page: u64,
    pub per_page: u64,
    ctx: ErrorContext,
}

impl<E: 'static, Page> EndpointPaginationController<E, Page> for PagedPagination
where
    Page: PageItems,
{
    type Bindings = PagedBindings<E>;
    type State = PagedState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        let page = bindings.page.get(endpoint);
        let per_page = bindings.per_page.get(endpoint);
        validate_paged_page(page, ctx.ctx)?;
        validate_paged_page_size(per_page, ctx.ctx)?;
        Ok(PagedState {
            page,
            per_page,
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
        bindings.page.set(endpoint, state.page);
        bindings.per_page.set(endpoint, state.per_page);
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_paged_page_size(state.per_page, ctx.ctx)?),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        _page: &Page,
        _page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.page = state
            .page
            .checked_add(1)
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: state.ctx.clone(),
                msg: "paged: page overflow".into(),
            })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;

    #[derive(Clone, Debug)]
    struct TestEndpoint {
        page: u64,
        per_page: u64,
    }

    #[test]
    fn paged_controller_reads_writes_and_advances_endpoint_state() {
        let controller = PagedPagination::default();
        let bindings = PagedBindings {
            page: EndpointField::new(|ep: &TestEndpoint| ep.page, |ep, value| ep.page = value),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut endpoint = TestEndpoint {
            page: 1,
            per_page: 20,
        };

        let mut state =
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::init(
                &controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid paged bindings");
        assert_eq!(state.page, 1);
        assert_eq!(state.per_page, 20);
        assert_eq!(
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::progress_key(
                &controller,
                &state
            ),
            Some(ProgressKey::U64(1))
        );

        state.page = 2;
        state.per_page = 7;
        let applied =
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::apply(
                &controller,
                &bindings,
                &mut state,
                &mut endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 1,
                    ctx: &ctx,
                },
            )
            .expect("paged apply");
        assert_eq!(endpoint.page, 2);
        assert_eq!(endpoint.per_page, 7);
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(7));

        let decision =
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::advance(
                &controller,
                &bindings,
                &mut state,
                &vec!["a".to_string(), "b".to_string()],
                PageAdvance {
                    endpoint: "List",
                    page_index: 1,
                    received_items: 2,
                },
            )
            .expect("paged advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(state.page, 3);
        assert_eq!(
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::progress_key(
                &controller,
                &state
            ),
            Some(ProgressKey::U64(3))
        );
    }

    #[test]
    fn paged_controller_rejects_zero_page_and_page_size() {
        let controller = PagedPagination::default();
        let bindings = PagedBindings {
            page: EndpointField::new(|ep: &TestEndpoint| ep.page, |ep, value| ep.page = value),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };

        let zero_page = TestEndpoint {
            page: 0,
            per_page: 20,
        };
        assert!(matches!(
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::init(
                &controller,
                &bindings,
                &zero_page,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));

        let zero_size = TestEndpoint {
            page: 1,
            per_page: 0,
        };
        assert!(matches!(
            <PagedPagination as EndpointPaginationController<TestEndpoint, Vec<String>>>::init(
                &controller,
                &bindings,
                &zero_size,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));
    }
}

impl From<PagedPagination> for PaginationPlan {
    fn from(value: PagedPagination) -> Self {
        Self::Paged {
            page: value.page,
            per_page: value.per_page,
        }
    }
}
