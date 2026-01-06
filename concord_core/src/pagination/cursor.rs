use crate::client::ClientContext;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::ApiClientError;
use crate::pagination::{
    Control, Controller, ControllerBuild, ControllerValue, PageItems, ProgressKey, Stop,
};
use crate::policy::PolicyPatch;
use crate::transport::DecodedResponse;
use std::borrow::Cow;

/// Output helper trait for cursor pagination.
/// The output type must both:
/// - expose items (PageItems)
/// - expose a "next cursor" value (HasNextCursor)
pub trait HasNextCursor {
    type Cursor: ToString + Send + Sync + 'static;
    fn next_cursor(&self) -> Option<&Self::Cursor>;
}

/// Cursor pagination:
/// - request: cursor + per_page
/// - response: provides a "next cursor"
#[derive(Clone, Debug)]
pub struct CursorPagination {
    pub stop: Stop,

    /// Query key for cursor (ex: "cursor", "pageCursor", "starting_after").
    pub cursor_key: Cow<'static, str>,
    /// Query key for per-page (ex: "per_page", "pageSize", "limit").
    pub per_page_key: Cow<'static, str>,

    /// Initial cursor (usually None).
    pub cursor: Option<String>,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// If false, first request omits the cursor param when `cursor` is None.
    pub send_cursor_on_first: bool,

    /// If true, stop when response has no cursor (None/empty) after collecting that page.
    pub stop_when_cursor_missing: bool,
}

impl Default for CursorPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            cursor_key: Cow::from("cursor"),
            per_page_key: Cow::from("per_page"),
            cursor: None,
            per_page: 20,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CursorState {
    pub cursor: Option<String>,
    pub per_page: u64,
    pub started: bool,
}

impl<Cx, E> Controller<Cx, E> for CursorPagination
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems + HasNextCursor,
{
    type State = CursorState;

    fn hint_param_key(&mut self, param: &'static str, key: &'static str) {
        match param {
            "cursor" => self.cursor_key = Cow::from(key),
            "per_page" => self.per_page_key = Cow::from(key),
            _ => {}
        }
    }

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        if self.per_page == 0 {
            return Err(ApiClientError::Pagination("cursor: per_page=0".into()));
        }
        Ok(CursorState {
            cursor: self.cursor.clone(),
            per_page: self.per_page,
            started: false,
        })
    }

    fn apply_policy(
        &self,
        st: &Self::State,
        _ep: &E,
        policy: &mut PolicyPatch<'_>,
    ) -> Result<(), ApiClientError> {
        policy.set_query(self.per_page_key.as_ref(), st.per_page.to_string());

        let should_send_cursor = st.started || self.send_cursor_on_first;

        match (should_send_cursor, &st.cursor) {
            (true, Some(c)) if !c.is_empty() => {
                policy.set_query(self.cursor_key.as_ref(), c.clone());
            }
            _ => {
                // ensure it is absent (important when iterating)
                policy.remove_query(self.cursor_key.as_ref());
            }
        }

        Ok(())
    }

    fn on_page(
        &self,
        st: &mut Self::State,
        _ep_next: &mut E,
        resp: &DecodedResponse<<E::Response as ResponseSpec>::Output>,
    ) -> Result<Control, ApiClientError> {
        st.started = true;

        if matches!(self.stop, Stop::OnEmpty) && resp.value.len() == 0 {
            return Ok(Control::Stop);
        }

        let next = resp
            .value
            .next_cursor()
            .map(|c| c.to_string())
            .filter(|s| !s.is_empty());

        st.cursor = next;

        if st.cursor.is_none() && self.stop_when_cursor_missing {
            return Ok(Control::Stop);
        }

        Ok(Control::Continue)
    }

    fn progress_key(&self, st: &Self::State, _ep: &E) -> Option<ProgressKey> {
        st.cursor.clone().map(ProgressKey::Str)
    }
}

impl ControllerBuild for CursorPagination {
    fn set_kv(&mut self, key: &'static str, value: ControllerValue) -> Result<(), ApiClientError> {
        match key {
            "cursor" => {
                self.cursor = value.into_option_field::<String>().ok_or(
                    ApiClientError::ControllerConfig {
                        key,
                        expected: "Option<String>|String",
                    },
                )?;
                Ok(())
            }
            "per_page" => {
                self.per_page =
                    value
                        .into_typed::<u64>()
                        .ok_or(ApiClientError::ControllerConfig {
                            key,
                            expected: "u64",
                        })?;
                Ok(())
            }
            "send_cursor_on_first" => {
                self.send_cursor_on_first =
                    value
                        .into_typed::<bool>()
                        .ok_or(ApiClientError::ControllerConfig {
                            key,
                            expected: "bool",
                        })?;
                Ok(())
            }
            "stop_when_cursor_missing" => {
                self.stop_when_cursor_missing =
                    value
                        .into_typed::<bool>()
                        .ok_or(ApiClientError::ControllerConfig {
                            key,
                            expected: "bool",
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
