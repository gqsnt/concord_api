use std::time::Duration;

use super::{EndpointSpec, RequestOptions};
use crate::client::ClientCore;
use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
};
use crate::hooks::DebugLevel;

/// An opaque typed pending request owned independently of its generated facade.
///
/// Construction and option changes have no execution side effects. Operation
/// identity, timing, URL preparation, hooks, debug output, and HTTP work begin
/// only when the request future is first polled.
///
/// **Controls (CONTROL-01..03).** The request carries its endpoint's declared
/// timeout, an optional caller-selected finite timeout and independent
/// non-widenable caps. [`timeout`](Self::timeout) replaces only the
/// selection and [`reset_timeout`](Self::reset_timeout) restores the
/// declaration; neither removes a cap. [`with_options`](Self::with_options)
/// intersects caps componentwise and [`cancel_with`](Self::cancel_with)
/// attaches up to eight distinct cancellation signals. Every control is
/// carried unchanged into [`response`](Self::response), `accepted_bytes`,
/// `store_as_*`, `pages`, `collect_prefix` and `collect_all`. These methods
/// are also the generated-code bridge: a macro-generated named request that
/// owns a `Request<E>` forwards its inherent control methods and its
/// [`RequestControls`] implementation to them.
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

/// A body-free generated request is cloneable (VALUE-05): each executed
/// clone is a new operation and cloning grants no retry authority. The
/// endpoint value decides: generated specifications with a body slot are not
/// `Clone`.
impl<E: Clone> Clone for Request<E> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            endpoint: self.endpoint.clone(),
            options: self.options.clone(),
        }
    }
}

impl<E> Request<E> {
    pub(crate) fn new(core: ClientCore, endpoint: E) -> Self {
        Self {
            core,
            endpoint,
            options: RequestOptions::new(),
        }
    }

    /// Replaces the selected finite timeout preference.
    ///
    /// Caps are retained: a 5-second timeout cap followed by `timeout(60s)`
    /// still limits the operation to 5 seconds.
    ///
    /// # Errors
    ///
    /// [`ControlError::ZeroDuration`] or
    /// [`ControlError::UnrepresentableDuration`]; the request is not changed
    /// and nothing executes.
    pub fn timeout(mut self, duration: Duration) -> Result<Self, ControlError> {
        self.options.select_timeout(duration)?;
        Ok(self)
    }

    /// Restores the effective declared (or default) timeout. Caps are
    /// retained.
    #[must_use]
    pub fn reset_timeout(mut self) -> Self {
        self.options.reset_selected_timeout();
        self
    }

    /// Attaches one more cancellation signal, deduplicated by identity.
    ///
    /// A signal already cancelled at first poll prevents the operation; one
    /// cancelled later stops local work with truthful dispatch knowledge.
    ///
    /// # Errors
    ///
    /// [`ControlError::TooManyCancellationSources`] when a ninth distinct
    /// signal would be attached; existing signals are kept.
    pub fn cancel_with(mut self, token: CancellationToken) -> Result<Self, ControlError> {
        self.options.add_cancellation(token)?;
        Ok(self)
    }

    /// Intersects additional caps (componentwise minimum); caps only narrow
    /// execution and never grant a retry the endpoint's effect, body or
    /// credential rules disallow.
    #[must_use]
    pub fn with_options(mut self, options: &CallOptions) -> Self {
        self.options.apply_call_options(options);
        self
    }

    #[must_use]
    /// Replaces the built-in body-free debug level for this request.
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.options = self.options.with_debug(level);
        self
    }
}

impl<E> Request<E>
where
    E: EndpointSpec,
{
    /// Returns the value-free effective control summary. The declared timeout
    /// is the endpoint's effective declaration (or the client default).
    #[must_use]
    pub fn control_summary(&self) -> ControlSummary {
        let declared = self
            .endpoint
            .request_meta()
            .resolve_inherited_timeout(self.core.default_runtime_endpoint_timeout());
        self.options.control_summary(declared)
    }

    /// Selects universal accepted response metadata. Every control is
    /// carried into the returned terminal.
    #[must_use]
    pub fn response(self) -> ResponseRequest<E::Output> {
        crate::execute::response_request(self)
    }
}

impl<E> RequestControls for Request<E>
where
    E: EndpointSpec,
{
    fn with_options(self, options: &CallOptions) -> Self {
        Self::with_options(self, options)
    }

    fn control_summary(&self) -> ControlSummary {
        Self::control_summary(self)
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Self::cancel_with(self, token)
    }
}

/// Type-erased pending operation behind a terminal wrapper.
///
/// The wrapper owns the controls; the typed request (endpoint, core and
/// response selection) is hidden so the public terminal type names only its
/// output.
pub(crate) trait ErasedOperation<T>: Send {
    fn options(&self) -> &RequestOptions;
    fn options_mut(&mut self) -> &mut RequestOptions;
    fn declared_timeout(&self) -> Option<Duration>;
    fn start(
        self: Box<Self>,
    ) -> crate::hooks::BoxFuture<'static, Result<T, crate::error::ApiError>>;
}

/// Implements the inherent control methods and [`RequestControls`] of an
/// erased terminal wrapper by forwarding to its owned options.
macro_rules! erased_terminal_controls {
    ($terminal:ty $(, $generic:ident)?) => {
        impl$(<$generic>)? $terminal {
            /// Replaces the selected finite timeout preference. Caps are
            /// retained.
            ///
            /// # Errors
            ///
            /// [`ControlError::ZeroDuration`] or
            /// [`ControlError::UnrepresentableDuration`].
            pub fn timeout(mut self, duration: Duration) -> Result<Self, ControlError> {
                self.inner.options_mut().select_timeout(duration)?;
                Ok(self)
            }

            /// Restores the declared timeout. Caps are retained.
            #[must_use]
            pub fn reset_timeout(mut self) -> Self {
                self.inner.options_mut().reset_selected_timeout();
                self
            }

            /// Attaches one more cancellation signal (at most eight distinct
            /// signals).
            ///
            /// # Errors
            ///
            /// [`ControlError::TooManyCancellationSources`] for a ninth
            /// distinct signal.
            pub fn cancel_with(mut self, token: CancellationToken) -> Result<Self, ControlError> {
                self.inner.options_mut().add_cancellation(token)?;
                Ok(self)
            }

            /// Intersects additional caps; caps only narrow execution.
            #[must_use]
            pub fn with_options(mut self, options: &CallOptions) -> Self {
                self.inner.options_mut().apply_call_options(options);
                self
            }

            /// Returns the value-free effective control summary.
            #[must_use]
            pub fn control_summary(&self) -> ControlSummary {
                self.inner
                    .options()
                    .control_summary(self.inner.declared_timeout())
            }
        }

        impl$(<$generic>)? RequestControls for $terminal {
            fn with_options(self, options: &CallOptions) -> Self {
                Self::with_options(self, options)
            }

            fn control_summary(&self) -> ControlSummary {
                Self::control_summary(self)
            }

            fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
                Self::cancel_with(self, token)
            }
        }

        impl$(<$generic>)? std::fmt::Debug for $terminal {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct(stringify!($terminal))
                    .field("controls", &self.control_summary())
                    .finish_non_exhaustive()
            }
        }
    };
}

/// An opaque pending request selecting accepted response metadata.
///
/// It resolves to `Result<ApiResponse<O>, ApiError>` where `O` is the
/// declared output, and carries every control of the request it was created
/// from; its own control methods can only narrow caps further or replace the
/// finite timeout selection. Construction is inert.
pub struct ResponseRequest<O> {
    pub(crate) inner: Box<dyn ErasedOperation<crate::response::ApiResponse<O>>>,
}

erased_terminal_controls!(ResponseRequest<O>, O);

/// An opaque pending request selecting complete bounded response bytes.
///
/// Execution preserves the response specification's declared status
/// acceptance and `Accept` header while skipping its validation and decoding.
/// Limiting, authentication, hooks, deadlines, cancellation, caps and
/// completion remain managed. Actual response headers and complete bounded
/// body bytes may hold secrets, so callers own every later diagnostic and
/// storage surface. It resolves to `Result<ApiResponse<Bytes>, ApiError>`.
pub struct AcceptedBytesRequest {
    pub(crate) inner: Box<dyn ErasedOperation<crate::response::ApiResponse<bytes::Bytes>>>,
}

erased_terminal_controls!(AcceptedBytesRequest);
