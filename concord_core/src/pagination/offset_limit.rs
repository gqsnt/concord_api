use crate::client::ClientContext;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{Control, Controller, PageItems, ProgressKey, Stop};
use crate::policy::PolicyPatch;
use crate::transport::DecodedResponse;
use std::borrow::Cow;

/// Offset/limit pagination (offset starts at 0 by default).
///
/// This is the single "engine" for all offset-based APIs:
/// - you bind `offset` and `limit` to any endpoint placeholders via `paginate { offset: start, limit: count }`
/// - codegen can hint the effective query keys so this controller remains opaque to codegen.
#[derive(Clone, Debug)]
pub struct OffsetLimitPagination {
    pub stop: Stop,
    /// Query key used for the offset (ex: "offset", "start", "skip").
    pub offset_key: Cow<'static, str>,
    /// Query key used for the limit (ex: "limit", "count", "top").
    pub limit_key: Cow<'static, str>,
    /// Initial offset value.
    pub offset: u64,
    /// Page size / limit (must be > 0).
    pub limit: u64,
    pub stop_on_short_page: bool,
}

impl Default for OffsetLimitPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            offset_key: Cow::from("offset"),
            limit_key: Cow::from("limit"),
            offset: 0,
            limit: 20,
            stop_on_short_page: true,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct OffsetLimitState {
    pub offset: u64,
    pub limit: u64,
}

impl<Cx, E> Controller<Cx, E> for OffsetLimitPagination
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
{
    type State = OffsetLimitState;

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        if self.limit == 0 {
            let ctx = ErrorContext {
                endpoint: std::any::type_name::<E>(),
                method: E::METHOD.clone(),
            };
            return Err(ApiClientError::Pagination {
                ctx,
                msg: "offset/limit: limit=0".into(),
            });
        }
        Ok(OffsetLimitState {
            offset: self.offset,
            limit: self.limit,
        })
    }

    fn apply_policy(
        &self,
        st: &Self::State,
        _ep: &E,
        policy: &mut PolicyPatch<'_>,
    ) -> Result<(), ApiClientError> {
        policy.set_query(self.offset_key.as_ref(), st.offset.to_string());
        policy.set_query(self.limit_key.as_ref(), st.limit.to_string());
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

        if self.stop_on_short_page && (resp.value.len() as u64) < st.limit {
            return Ok(Control::Stop);
        }

        st.offset = st.offset.checked_add(st.limit).ok_or_else(|| {
            let ctx = ErrorContext {
                endpoint: std::any::type_name::<E>(),
                method: E::METHOD.clone(),
            };
            ApiClientError::Pagination {
                ctx,
                msg: "offset/limit: offset overflow".into(),
            }
        })?;

        Ok(Control::Continue)
    }

    fn progress_key(&self, st: &Self::State, _: &E) -> Option<ProgressKey> {
        Some(ProgressKey::U64(st.offset))
    }
}
