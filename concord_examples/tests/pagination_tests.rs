mod common;

use concord_core::internal::{Control, Controller, PaginationPart, ResponseSpec};
use concord_core::prelude::*;
use concord_macros::api;

// ----------------------------
// Custom pagination controller
// ----------------------------

#[derive(Clone, Debug)]
pub struct AfterTakePagination {
    pub stop: Stop,
    pub after_key: std::borrow::Cow<'static, str>,
    pub take_key: std::borrow::Cow<'static, str>,
    pub after: u64,
    pub take: u64,
}

impl Default for AfterTakePagination {
    fn default() -> Self {
        Self {
            stop: Stop::OnEmpty,
            after_key: std::borrow::Cow::from("after"),
            take_key: std::borrow::Cow::from("take"),
            after: 0,
            take: 20,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AfterTakeState {
    pub after: u64,
    pub take: u64,
}

impl<Cx, E> Controller<Cx, E> for AfterTakePagination
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
{
    type State = AfterTakeState;

    fn hint_param_key(&mut self, param: &'static str, key: &'static str) {
        match param {
            "after" => self.after_key = std::borrow::Cow::from(key),
            "take" => self.take_key = std::borrow::Cow::from(key),
            _ => {}
        }
    }

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        if self.take == 0 {
            return Err(ApiClientError::Pagination("after/take: take=0".into()));
        }
        Ok(AfterTakeState {
            after: self.after,
            take: self.take,
        })
    }

    fn apply_policy(
        &self,
        st: &Self::State,
        _ep: &E,
        policy: &mut PolicyPatch,
    ) -> Result<(), ApiClientError> {
        policy.set_query(self.after_key.as_ref(), st.after.to_string());
        policy.set_query(self.take_key.as_ref(), st.take.to_string());
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
        st.after = st
            .after
            .checked_add(st.take)
            .ok_or(ApiClientError::Pagination(
                "after/take: after overflow".into(),
            ))?;
        Ok(Control::Continue)
    }

    fn progress_key(&self, st: &Self::State, _ep: &E) -> Option<ProgressKey> {
        Some(ProgressKey::U64(st.after))
    }
}

impl ControllerBuild for AfterTakePagination {
    fn set_kv(&mut self, key: &'static str, value: ControllerValue) -> Result<(), ApiClientError> {
        match key {
            "after" => {
                self.after = value
                    .into_typed::<u64>()
                    .ok_or(ApiClientError::ControllerConfig {
                        key,
                        expected: "u64",
                    })?;
                Ok(())
            }
            "take" => {
                self.take = value
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

// ----------------------------
// Offset/limit
// ----------------------------

api! {
    client OlClient {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }
    path "items" {
        GET List ""
        query {
        start: u64 = 0,
        limit: u64 = 10,
    }
    paginate OffsetLimitPagination {
        offset: start,
        limit,
    }
    -> JsonEncoding<Vec<u8>>;
}
}

#[test]
fn offset_limit_infers_query_keys_from_bindings() {
    type Cx = ol_client::OlClientCx;
    type E = ol_client::endpoints::List;

    let vars = ol_client::OlClientVars::new();
    let client = ApiClient::<Cx>::new(vars);
    let ep = E::new();

    let ctrl = <<E as Endpoint<Cx>>::Pagination as PaginationPart<Cx, E>>::controller(&client, &ep)
        .unwrap();

    // values from endpoint defaults
    assert_eq!(ctrl.offset, 0);
    assert_eq!(ctrl.limit, 10);

    // keys inferred from query blocks
    assert_eq!(ctrl.offset_key.as_ref(), "start");
    assert_eq!(ctrl.limit_key.as_ref(), "limit");
}

// ----------------------------
// Page/per_page
// ----------------------------

api! {
    client PgClient {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }
    path "items" {
    GET List ""
        query {
            "_page" => page: u64 = 1,
            "_limit" => per_page: u64 = 25,
        }
        paginate PagedPagination {
            page,
            per_page,
        }
        -> JsonEncoding<Vec<u8>>;
    }
}

#[test]
fn paged_infers_query_keys_from_bindings() {
    type Cx = pg_client::PgClientCx;
    type E = pg_client::endpoints::List;

    let vars = pg_client::PgClientVars::new();
    let client = ApiClient::<Cx>::new(vars);
    let ep = E::new();

    let ctrl = <<E as Endpoint<Cx>>::Pagination as PaginationPart<Cx, E>>::controller(&client, &ep)
        .unwrap();

    // values from endpoint defaults
    assert_eq!(ctrl.page, 1);
    assert_eq!(ctrl.per_page, 25);

    // keys inferred from query blocks
    assert_eq!(ctrl.page_key.as_ref(), "_page");
    assert_eq!(ctrl.per_page_key.as_ref(), "_limit");
}

// ----------------------------
// Cursor pagination (per_page key inference)
// ----------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CursorPage {
    pub items: Vec<u8>,
    pub next: Option<String>,
}

impl PageItems for CursorPage {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn len(&self) -> usize {
        self.items.len()
    }

    fn inner_into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor for CursorPage {
    type Cursor = String;
    fn next_cursor(&self) -> Option<&Self::Cursor> {
        self.next.as_ref()
    }
}

api! {
    client CurClient {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }
    path "items" {
    GET List ""
        query {
            "pageSize" => per_page: u64 = 50,
             "pageCursor" => cursor?: String,
        }
        paginate CursorPagination {
            per_page,
        }
        -> JsonEncoding<CursorPage>;
    }
}

#[test]
fn cursor_infers_per_page_key_from_bindings() {
    type Cx = cur_client::CurClientCx;
    type E = cur_client::endpoints::List;

    let vars = cur_client::CurClientVars::new();
    let client = ApiClient::<Cx>::new(vars);
    let ep = E::new();

    let ctrl = <<E as Endpoint<Cx>>::Pagination as PaginationPart<Cx, E>>::controller(&client, &ep)
        .unwrap();
    // value from endpoint default
    assert_eq!(ctrl.per_page, 50);

    // per-page key inferred from query blocks
    assert_eq!(ctrl.per_page_key.as_ref(), "pageSize");

    // cursor key explicitly set in paginate clause
    assert_eq!(ctrl.cursor_key.as_ref(), "cursor"); // "pageCursor");
}

// ----------------------------
// Custom pagination (after/take) + key inference
// ----------------------------

api! {
    client CustomClient {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }
    path "items" {
    GET List ""
        query {
            "after_id" => after: u64 = 0,
            "take_n" => take: u64 = 10,
        }
        paginate AfterTakePagination {
            after,
            take,
        }
        -> JsonEncoding<Vec<u8>>;
    }
}

#[test]
fn custom_pagination_infers_query_keys_from_bindings() {
    type Cx = custom_client::CustomClientCx;
    type E = custom_client::endpoints::List;

    let vars = custom_client::CustomClientVars::new();
    let client = ApiClient::<Cx>::new(vars);
    let ep = E::new();

    let ctrl = <<E as Endpoint<Cx>>::Pagination as PaginationPart<Cx, E>>::controller(&client, &ep)
        .unwrap();

    // values from endpoint defaults
    assert_eq!(ctrl.after, 0);
    assert_eq!(ctrl.take, 10);

    // keys inferred from query blocks
    assert_eq!(ctrl.after_key.as_ref(), "after_id");
    assert_eq!(ctrl.take_key.as_ref(), "take_n");
}
