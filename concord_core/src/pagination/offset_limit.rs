use crate::client::ClientContext;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::ApiClientError;
use crate::pagination::{
    Control, Controller, ControllerBuild, ControllerValue, PageItems, ProgressKey, Stop,
};
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
}

impl Default for OffsetLimitPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            offset_key: Cow::from("offset"),
            limit_key: Cow::from("limit"),
            offset: 0,
            limit: 20,
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

    fn hint_param_key(&mut self, param: &'static str, key: &'static str) {
        match param {
            "offset" => self.offset_key = Cow::from(key),
            "limit" => self.limit_key = Cow::from(key),
            _ => {}
        }
    }

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        if self.limit == 0 {
            return Err(ApiClientError::Pagination("offset/limit: limit=0".into()));
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
        st.offset = st
            .offset
            .checked_add(st.limit)
            .ok_or_else(|| ApiClientError::Pagination("offset/limit: offset overflow".into()))?;
        Ok(Control::Continue)
    }

    fn progress_key(&self, st: &Self::State, _: &E) -> Option<ProgressKey> {
        Some(ProgressKey::U64(st.offset))
    }
}

impl ControllerBuild for OffsetLimitPagination {
    fn set_kv(&mut self, key: &'static str, value: ControllerValue) -> Result<(), ApiClientError> {
        match key {
            "offset" => {
                self.offset =
                    value
                        .into_typed::<u64>()
                        .ok_or(ApiClientError::ControllerConfig {
                            key,
                            expected: "u64",
                        })?;
                Ok(())
            }
            "limit" => {
                self.limit = value
                    .into_typed::<u64>()
                    .ok_or(ApiClientError::ControllerConfig {
                        key,
                        expected: "u64",
                    })?;
                Ok(())
            }
            _ => Err(ApiClientError::ControllerConfig {
                key,
                expected: "known key",
            }),
        }
    }
}
