use std::time::Duration;

use super::{EndpointSpec, RequestOptions, TimeoutOverride};
use crate::client::ClientCore;
use crate::hooks::DebugLevel;

/// An opaque typed pending request owned independently of its generated facade.
///
/// Construction and option changes have no execution side effects. Operation
/// identity, timing, URL preparation, hooks, debug output, and HTTP work begin
/// only when the request future is first polled.
///
/// Fields are intentionally private:
///
/// ```compile_fail
/// let _ = concord_core::Request::<()> {};
/// ```
pub struct Request<E> {
    pub(crate) core: ClientCore,
    pub(crate) endpoint: E,
    pub(crate) options: RequestOptions,
}

impl<E> Request<E> {
    pub(crate) fn new(core: ClientCore, endpoint: E) -> Self {
        Self {
            core,
            endpoint,
            options: RequestOptions::new(),
        }
    }

    #[must_use]
    /// Replaces the generated operation deadline with one explicit duration.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.options = self
            .options
            .with_timeout(TimeoutOverride::Explicit(duration));
        self
    }

    #[must_use]
    /// Resets the request to its retained effective endpoint deadline.
    pub fn reset_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Inherit);
        self
    }

    #[must_use]
    /// Replaces the built-in body-free debug level for this request.
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.options = self.options.with_debug(level);
        self
    }
}

#[doc(hidden)]
impl<E> Request<E>
where
    E: EndpointSpec,
{
    /// Selects universal accepted response metadata.
    #[must_use]
    pub fn response(self) -> ResponseRequest<E> {
        ResponseRequest { request: self }
    }
}

#[cfg(test)]
#[allow(private_bounds)]
impl<E> Request<E>
where
    E: EndpointSpec,
    <E::Response as crate::response::ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    /// Test-only construction for the capability wrapper used by generated
    /// endpoint request types.
    #[must_use]
    pub(crate) fn accepted_bytes(self) -> AcceptedBytesRequest<E> {
        AcceptedBytesRequest { request: self }
    }
}

/// An opaque pending request selecting accepted response metadata.
///
/// Fields are intentionally private:
///
/// ```compile_fail
/// let _ = concord_core::ResponseRequest::<()> {};
/// ```
pub struct ResponseRequest<E> {
    pub(crate) request: Request<E>,
}

impl<E> ResponseRequest<E> {
    pub(crate) fn from_generated(request: Request<E>) -> Self {
        Self { request }
    }
}

/// An opaque pending request selecting complete bounded response bytes.
///
/// Execution preserves the response specification's declared status
/// acceptance and `Accept` header while skipping its validation and decoding.
/// Limiting, authentication, hooks, deadlines, cancellation, and completion
/// remain managed. Actual response headers and complete bounded body bytes may
/// hold secrets, so callers own every later diagnostic and storage surface.
///
/// Fields are intentionally private:
///
/// ```compile_fail
/// let _ = concord_core::AcceptedBytesRequest::<()> {};
/// ```
pub struct AcceptedBytesRequest<E> {
    pub(crate) request: Request<E>,
}

impl<E> AcceptedBytesRequest<E> {
    pub(crate) fn from_generated(request: Request<E>) -> Self {
        Self { request }
    }
}
