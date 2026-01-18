use crate::client::ClientContext;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{Control, Controller, PageItems, ProgressKey, Stop};
use crate::policy::PolicyPatch;
use crate::transport::DecodedResponse;
use std::borrow::Cow;

/// Page/per_page pagination (page starts at 1 by default).
#[derive(Clone, Debug)]
pub struct PagedPagination {
    pub stop: Stop,

    /// Query key for the page number (ex: "page", "_page", "currentPage").
    pub page_key: Cow<'static, str>,
    /// Query key for page size (ex: "per_page", "_limit", "pageSize").
    pub per_page_key: Cow<'static, str>,

    /// Initial page number.
    pub page: u64,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// Optional stop condition: stop when the API returns fewer items than `per_page`.
    /// (Useful for APIs that do not return a total and do not return empty last pages.)
    pub stop_on_short_page: bool,
}

impl Default for PagedPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            page_key: Cow::from("page"),
            per_page_key: Cow::from("per_page"),
            page: 1,
            per_page: 20,
            stop_on_short_page: false,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PagedState {
    pub page: u64,
    pub per_page: u64,
}

impl<Cx, E> Controller<Cx, E> for PagedPagination
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
{
    type State = PagedState;

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        if self.per_page == 0 {
            let ctx = ErrorContext {
                endpoint: std::any::type_name::<E>(),
                method: E::METHOD.clone(),
            };
            return Err(ApiClientError::Pagination {
                ctx,
                msg: "paged: per_page=0".into(),
            });
        }
        if self.page == 0 {
            let ctx = ErrorContext {
                endpoint: std::any::type_name::<E>(),
                method: E::METHOD.clone(),
            };
            return Err(ApiClientError::Pagination {
                ctx,
                msg: "paged: page=0".into(),
            });
        }
        Ok(PagedState {
            page: self.page,
            per_page: self.per_page,
        })
    }

    fn apply_policy(
        &self,
        st: &Self::State,
        _ep: &E,
        policy: &mut PolicyPatch<'_>,
    ) -> Result<(), ApiClientError> {
        policy.set_query(self.page_key.as_ref(), st.page.to_string());
        policy.set_query(self.per_page_key.as_ref(), st.per_page.to_string());
        Ok(())
    }

    fn on_page(
        &self,
        st: &mut Self::State,
        _ep_next: &mut E,
        resp: &DecodedResponse<<E::Response as ResponseSpec>::Output>,
    ) -> Result<Control, ApiClientError> {
        if matches!(self.stop, Stop::OnEmpty) && resp.value.len() == 0 {
            return Ok(Control::Stop);
        }

        if self.stop_on_short_page && (resp.value.len() as u64) < st.per_page {
            return Ok(Control::Stop);
        }

        st.page = st.page.checked_add(1).ok_or_else(|| {
            let ctx = ErrorContext {
                endpoint: std::any::type_name::<E>(),
                method: E::METHOD.clone(),
            };
            ApiClientError::Pagination {
                ctx,
                msg: "paged: page overflow".into(),
            }
        })?;

        Ok(Control::Continue)
    }

    fn progress_key(&self, st: &Self::State, _: &E) -> Option<ProgressKey> {
        Some(ProgressKey::U64(st.page))
    }
}
