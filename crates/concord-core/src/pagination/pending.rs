use std::future::IntoFuture;
use std::num::NonZeroU32;

use crate::error::ApiError;
use crate::hooks::BoxFuture;
use crate::request::Request;
use crate::response::{PageResponseSpec, ResponseBodySpec};

use super::{PaginationController, PaginationError};

/// A finite requested pagination limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageLimit {
    /// Collect at most this many pages and succeed after the final allowed
    /// page without asking the controller for another state.
    Take(u32),
    /// Permit natural termination through this page and fail when another
    /// controller state still exists at the cap.
    HardCap(NonZeroU32),
}

impl From<u32> for PageLimit {
    fn from(value: u32) -> Self {
        Self::Take(value)
    }
}

/// An opaque directly awaitable sequential pagination operation.
///
/// Construction has no execution side effects. Controller construction,
/// endpoint planning, operation identity allocation, hooks, credentials,
/// limiter work, native request construction, and HTTP all begin only after
/// the returned future is polled.
pub struct PaginatedRequest<E> {
    request: Request<E>,
    limit: PageLimit,
}

impl<E> PaginatedRequest<E> {
    pub(crate) fn new(request: Request<E>, limit: PageLimit) -> Self {
        Self { request, limit }
    }
}

/// Generated-private typed pagination integration.
///
/// Generated implementations provide only typed construction and binding;
/// core owns the single page loop. The stable wrapper exposes neither this
/// capability nor its associated controller in public Rustdoc.
#[doc(hidden)]
pub trait PaginatedEndpoint: crate::request::EndpointSpec + Clone + Sized
where
    Self::Response: PageResponseSpec,
    <Self::Response as ResponseBodySpec>::Output: IntoIterator<Item = Self::Item> + Send + 'static,
{
    /// One collected item from the decoded page's ordinary `IntoIterator`.
    type Item: Send + 'static;

    /// Request-local typed pagination controller.
    type Controller: PaginationController<<Self::Response as ResponseBodySpec>::Output>;

    /// Constructs the complete initial controller exactly once.
    fn initial_controller(&self) -> Result<Self::Controller, PaginationError>;

    /// Writes current bound controller fields into one fresh endpoint clone.
    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError>;
}

#[doc(hidden)]
impl<E> IntoFuture for PaginatedRequest<E>
where
    E: PaginatedEndpoint,
    E::Response: PageResponseSpec,
    <E::Response as ResponseBodySpec>::Output: IntoIterator<Item = E::Item> + Send + 'static,
{
    type Output = Result<Vec<E::Item>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        super::execute::paginate_with(self.request, self.limit)
    }
}
