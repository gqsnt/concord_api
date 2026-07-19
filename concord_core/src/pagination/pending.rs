use std::future::IntoFuture;
use std::num::NonZeroU32;

use crate::error::ApiError;
use crate::hooks::BoxFuture;
use crate::request::Request;

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
/// Generated implementations monomorphize controller construction and the
/// page loop inside this method. The stable wrapper therefore exposes neither
/// a controller type nor generated endpoint capability in its public shape.
#[doc(hidden)]
pub trait PaginatedEndpoint: crate::request::EndpointSpec + Sized {
    /// One collected item from the decoded page's ordinary `IntoIterator`.
    type Item: Send + 'static;

    /// Starts the monomorphized pagination future on first poll.
    fn __concord_execute_pagination(
        request: Request<Self>,
        limit: PageLimit,
    ) -> BoxFuture<'static, Result<Vec<Self::Item>, ApiError>>;
}

#[doc(hidden)]
impl<E> IntoFuture for PaginatedRequest<E>
where
    E: PaginatedEndpoint,
{
    type Output = Result<Vec<E::Item>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        E::__concord_execute_pagination(self.request, self.limit)
    }
}
