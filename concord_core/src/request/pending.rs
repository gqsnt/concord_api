use std::time::Duration;

use super::{EndpointSpec, MetadataResponse, RequestOptions, TimeoutOverride};
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
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.options = self
            .options
            .with_timeout(TimeoutOverride::Explicit(duration));
        self
    }

    #[must_use]
    pub fn disable_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Disabled);
        self
    }

    #[must_use]
    pub fn inherit_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Inherit);
        self
    }

    #[must_use]
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.options = self.options.with_debug(level);
        self
    }
}

// Rustdoc cannot display a method from an implementation constrained by the
// generated-private endpoint capability without also rendering that private
// bound. Keep the production implementation below capability-bound and give
// documentation builds the same public terminal signature plus its semantic
// availability rule.
#[cfg(doc)]
impl<E> Request<E> {
    /// Selects successful response metadata.
    ///
    /// This terminal is available on generated requests whose successful
    /// response mode is buffered, bytes, or no-content. Streaming responses
    /// carry their successful metadata directly and do not expose this method.
    #[must_use]
    pub fn response(self) -> ResponseRequest<E> {
        ResponseRequest { request: self }
    }
}

#[cfg(not(doc))]
#[doc(hidden)]
impl<E> Request<E>
where
    E: EndpointSpec,
    E::Response: MetadataResponse,
{
    /// Selects successful response metadata for response modes with a buffered
    /// metadata-bearing completion.
    #[must_use]
    pub fn response(self) -> ResponseRequest<E> {
        ResponseRequest { request: self }
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
