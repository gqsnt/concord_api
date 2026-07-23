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
    /// Disables the generated operation deadline for this request.
    pub fn disable_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Disabled);
        self
    }

    #[must_use]
    /// Restores the endpoint's inherited deadline selection.
    pub fn inherit_timeout(mut self) -> Self {
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
    /// Selects universal successful response metadata.
    #[must_use]
    pub fn response(self) -> ResponseRequest<E> {
        ResponseRequest { request: self }
    }
}

#[cfg(feature = "dangerous-raw-response")]
#[doc(hidden)]
#[allow(private_bounds)]
impl<E> Request<E>
where
    E: EndpointSpec + super::RawResponseEndpoint,
    <E::Response as crate::response::ResponseBodySpec>::Mode: crate::response::RawResponseMode,
{
    /// Selects the complete bounded successful response without invoking the
    /// endpoint decoder.
    ///
    /// Requires the Cargo feature `dangerous-raw-response`.
    #[must_use]
    pub fn raw_response(self) -> RawResponseRequest<E> {
        RawResponseRequest { request: self }
    }
}

/// An opaque pending request selecting successful response metadata.
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

/// An opaque pending request selecting a dangerous buffered raw success.
///
/// Execution retains ordinary status classification, limits, authentication,
/// limiting, hooks, retry containment, deadlines, cancellation, and completion.
/// The successful response's actual headers and complete bounded body may hold
/// secrets, so callers assume responsibility for every later diagnostic and
/// storage surface.
///
/// Fields are intentionally private:
///
/// ```compile_fail
/// let _ = concord_core::dangerous::RawResponseRequest::<()> {};
/// ```
#[cfg(feature = "dangerous-raw-response")]
pub struct RawResponseRequest<E> {
    pub(crate) request: Request<E>,
}

#[cfg(feature = "dangerous-raw-response")]
impl<E> RawResponseRequest<E> {
    pub(crate) fn from_generated(request: Request<E>) -> Self {
        Self { request }
    }
}
