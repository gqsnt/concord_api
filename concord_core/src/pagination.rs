pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::client::{ApiClient, ClientContext};
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::ApiClientError;
use crate::policy::PolicyPatch;
use crate::transport::{DecodedResponse, RequestMeta};
use std::any::Any;
use std::collections::HashSet;
use std::future::IntoFuture;

pub use cursor::{CursorPagination, HasNextCursor};
pub use offset_limit::OffsetLimitPagination;
pub use paged::PagedPagination;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Control {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProgressKey {
    U64(u64),
    Str(String),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub enum ControllerValue {
    U64(u64),
    Str(String),
    Bytes(Vec<u8>),
    Any(Box<dyn Any + Send + Sync>),
}

impl ControllerValue {
    #[inline]
    fn into_any(self) -> Box<dyn Any + Send + Sync> {
        match self {
            ControllerValue::U64(v) => Box::new(v),
            ControllerValue::Str(v) => Box::new(v),
            ControllerValue::Bytes(v) => Box::new(v),
            ControllerValue::Any(v) => v,
        }
    }

    #[inline]
    pub fn into_typed<T: Any + Send + Sync>(self) -> Option<T> {
        self.into_any().downcast::<T>().ok().map(|b| *b)
    }

    #[inline]
    pub fn into_option_field<T: Any + Send + Sync>(self) -> Option<Option<T>> {
        let boxed = self.into_any();

        // 1) Si c'est déjà un Option<T>, on renvoie l'Option<T> telle quelle (même None).
        match boxed.downcast::<Option<T>>() {
            Ok(v) => Some(*v),

            // 2) Sinon, on réessaie en tant que T, et on l'emballe en Some(T).
            Err(boxed) => boxed.downcast::<T>().ok().map(|v| Some(*v)),
        }
    }
}

pub trait ControllerBuild: Default + Send + Sync + 'static {
    fn set_kv(&mut self, key: &'static str, value: ControllerValue) -> Result<(), ApiClientError>;
}

pub trait Controller<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type State: Send + Sync + 'static;

    fn hint_param_key(&mut self, _param: &'static str, _key: &'static str) {}

    fn init(&self, ep: &E) -> Result<Self::State, ApiClientError>;

    fn apply_policy(
        &self,
        _st: &Self::State,
        _ep: &E,
        _policy: &mut PolicyPatch<'_>,
    ) -> Result<(), ApiClientError> {
        Ok(())
    }

    fn on_page(
        &self,
        st: &mut Self::State,
        ep_next: &mut E,
        resp: &DecodedResponse<<E::Response as ResponseSpec>::Output>,
    ) -> Result<Control, ApiClientError>;

    fn progress_key(&self, _st: &Self::State, _ep: &E) -> Option<ProgressKey> {
        None
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Stop {
    #[default]
    OnEmpty,
}

#[derive(Copy, Clone, Debug)]
pub struct Caps {
    pub max_pages: u32,
    pub max_items: u64,
    pub detect_loops: bool,
}
impl Default for Caps {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_items: 100_000,
            detect_loops: true,
        }
    }
}
impl Caps {
    #[inline]
    pub fn max_pages(mut self, v: u32) -> Self {
        self.max_pages = v;
        self
    }
    #[inline]
    pub fn max_items(mut self, v: u64) -> Self {
        self.max_items = v;
        self
    }
    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.detect_loops = v;
        self
    }
}

/// Items container returned by a paginated endpoint.
///
/// Contract:
/// - `len()` must reflect the number of items returned in the page.
/// - `into_iter()` must yield exactly those items (no extra allocation required).
pub trait PageItems: Send + 'static {
    type Item: Send + 'static;
    type IntoIter: IntoIterator<Item = Self::Item>;

    fn len(&self) -> usize;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn inner_into_iter(self) -> Self::IntoIter;
}
impl<T: Send + 'static> PageItems for Vec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn inner_into_iter(self) -> Self::IntoIter {
        <Vec<T> as IntoIterator>::into_iter(self)
    }
}

pub trait PaginationPart<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type Ctrl: Controller<Cx, E>;
    fn controller(client: &ApiClient<Cx>, ep: &E) -> Result<Self::Ctrl, ApiClientError>;
}

pub struct NoPagination;
pub struct NoController;

impl<Cx: ClientContext, E: Endpoint<Cx>> PaginationPart<Cx, E> for NoPagination {
    type Ctrl = NoController;
    fn controller(_: &ApiClient<Cx>, _: &E) -> Result<Self::Ctrl, ApiClientError> {
        Ok(NoController)
    }
}

impl<Cx: ClientContext, E: Endpoint<Cx>> Controller<Cx, E> for NoController {
    type State = ();
    fn init(&self, _: &E) -> Result<Self::State, ApiClientError> {
        Ok(())
    }
    fn on_page(
        &self,
        _: &mut Self::State,
        _: &mut E,
        _: &DecodedResponse<<E::Response as ResponseSpec>::Output>,
    ) -> Result<Control, ApiClientError> {
        Ok(Control::Stop)
    }
}

pub trait CollectAllItemsEndpoint<Cx: ClientContext>: Endpoint<Cx> {}

impl<Cx, E> CollectAllItemsEndpoint<Cx> for E
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
    <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
{
}

// ---------------- Collection driver ----------------
pub struct CollectAllItems<'a, Cx: ClientContext, E: Endpoint<Cx>> {
    client: &'a ApiClient<Cx>,
    ep: E,
    caps: Caps,
}
impl<'a, Cx: ClientContext, E: Endpoint<Cx>> CollectAllItems<'a, Cx, E> {
    #[inline]
    pub(crate) fn new(client: &'a ApiClient<Cx>, ep: E, caps: Caps) -> Self {
        Self { client, ep, caps }
    }
    #[inline]
    pub fn max_pages(mut self, v: u32) -> Self {
        self.caps.max_pages = v;
        self
    }
    #[inline]
    pub fn max_items(mut self, v: u64) -> Self {
        self.caps.max_items = v;
        self
    }
    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.caps.detect_loops = v;
        self
    }
}

impl<'a, Cx, E> IntoFuture for CollectAllItems<'a, Cx, E>
where
    Cx: ClientContext,
    E: CollectAllItemsEndpoint<Cx>,
    <<E as Endpoint<Cx>>::Response as ResponseSpec>::Output: PageItems,
{
    type Output = Result<
        Vec<<<<E as Endpoint<Cx>>::Response as ResponseSpec>::Output as PageItems>::Item>,
        ApiClientError,
    >;

    type IntoFuture =
        std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(mut self) -> Self::IntoFuture {
        Box::pin(async move {
            let ctrl = <E::Pagination as PaginationPart<Cx, E>>::controller(self.client, &self.ep)?;
            let mut st = ctrl.init(&self.ep)?;
            let mut seen: HashSet<ProgressKey> = HashSet::new();
            let mut out: Vec<<<E::Response as ResponseSpec>::Output as PageItems>::Item> =
                Vec::new();
            let mut items_count: u64 = 0;

            for page_index in 0..self.caps.max_pages {
                if self.caps.detect_loops
                    && let Some(k) = ctrl.progress_key(&st, &self.ep)
                    && !seen.insert(k.clone())
                {
                    return Err(ApiClientError::Pagination(
                        format!(
                            "loop detected (endpoint={} page_index={} key={:?})",
                            self.ep.name(),
                            page_index,
                            k
                        )
                        .into(),
                    ));
                }
                let meta = RequestMeta {
                    endpoint: self.ep.name(),
                    method: E::METHOD.clone(),
                    idempotent: matches!(
                        E::METHOD,
                        http::Method::GET
                            | http::Method::HEAD
                            | http::Method::PUT
                            | http::Method::DELETE
                            | http::Method::OPTIONS
                    ),
                    attempt: 0,
                    page_index,
                };
                let resp: DecodedResponse<<E::Response as ResponseSpec>::Output> = self
                    .client
                    .execute_decoded_ref_with(&self.ep, meta, |policy| {
                        ctrl.apply_policy(&st, &self.ep, policy)
                    })
                    .await?;

                let control = ctrl.on_page(&mut st, &mut self.ep, &resp)?;

                let page_len = resp.value.len() as u64;
                if page_len > 0 {
                    let new_total = items_count.checked_add(page_len).ok_or_else(|| {
                        ApiClientError::Pagination(
                            format!("items overflow (endpoint={})", self.ep.name()).into(),
                        )
                    })?;
                    if new_total > self.caps.max_items {
                        return Err(ApiClientError::PaginationLimit(
                            format!(
                                "max_items reached (endpoint={} max={} seen={})",
                                self.ep.name(),
                                self.caps.max_items,
                                new_total
                            )
                            .into(),
                        ));
                    }
                    items_count = new_total;
                }
                out.extend(
                    <<E::Response as ResponseSpec>::Output as PageItems>::inner_into_iter(
                        resp.value,
                    ),
                );
                match control {
                    Control::Continue => continue,
                    Control::Stop => return Ok(out),
                }
            }
            Err(ApiClientError::PaginationLimit(
                format!(
                    "max_pages reached (endpoint={} max_pages={} seen_items={})",
                    self.ep.name(),
                    self.caps.max_pages,
                    items_count
                )
                .into(),
            ))
        })
    }
}
