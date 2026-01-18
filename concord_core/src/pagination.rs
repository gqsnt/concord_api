pub mod cursor;
pub mod offset_limit;
pub mod paged;

use crate::client::ClientContext;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::ApiClientError;
use crate::policy::PolicyPatch;
use crate::transport::DecodedResponse;

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

pub trait Controller<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type State: Send + Sync + 'static;

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
    fn controller(vars: &Cx::Vars, ep: &E) -> Result<Self::Ctrl, ApiClientError>;
}

pub struct NoPagination;
pub struct NoController;

impl<Cx: ClientContext, E: Endpoint<Cx>> PaginationPart<Cx, E> for NoPagination {
    type Ctrl = NoController;
    fn controller(_: &Cx::Vars, _: &E) -> Result<Self::Ctrl, ApiClientError> {
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

pub trait PaginatedEndpoint<Cx: ClientContext>: Endpoint<Cx> {}
impl<Cx, E> PaginatedEndpoint<Cx> for E
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
    <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
{
}
