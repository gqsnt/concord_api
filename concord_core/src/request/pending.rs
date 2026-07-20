use std::time::Duration;

#[cfg(feature = "dangerous-raw-response")]
use super::RawResponseEndpoint;
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

// As with `response`, Rustdoc cannot render the generated-private capability
// bound without exposing an integration name. Documentation builds retain the
// public signature and state the exact generated availability rule.
#[cfg(all(doc, feature = "dangerous-raw-response"))]
impl<E> Request<E> {
    /// Selects the complete bounded successful response without decoding it.
    ///
    /// Requires the Cargo feature `dangerous-raw-response`.
    ///
    /// Generated buffered, text, JSON, custom buffered, bytes, and no-content
    /// requests expose this terminal. Generated streaming requests and public
    /// prepared endpoints intentionally do not. Response headers and body bytes
    /// may contain secrets; callers assume responsibility for their storage and
    /// diagnostics.
    #[must_use]
    pub fn raw_response(self) -> RawResponseRequest<E> {
        RawResponseRequest { request: self }
    }
}

#[cfg(all(not(doc), feature = "dangerous-raw-response"))]
#[doc(hidden)]
impl<E> Request<E>
where
    E: RawResponseEndpoint,
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
