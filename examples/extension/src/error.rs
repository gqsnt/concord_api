//! The extension's error: which operation failed, and why.

use std::fmt;

use concord_example_sdk::context::ContextError;

/// Why an operation of this crate failed.
///
/// It names the operation ([`operation`](Self::operation), such as
/// [`EXPORT_OPERATION`](crate::EXPORT_OPERATION)) and holds either the reason the request could
/// not be built (nothing was sent) or Concord's error for the call. That Concord error comes from
/// an unlabeled public decoder, so its `endpoint()` is `None`: this crate never borrows a
/// declared endpoint's name.
///
/// `Display`, `Debug` and the `source()` chain contain the operation and the inner errors'
/// redacted text only: no tenant, key, URL or body content. Concord's own disclosure boundaries
/// apply to its explicit accessors (`Error::transport`, `Error::response`,
/// `CodecError::source_unredacted`).
#[derive(Debug)]
pub struct ExtensionError {
    operation: &'static str,
    cause: Cause,
}

#[derive(Debug)]
enum Cause {
    Invalid(ContextError),
    Call(concord::Error),
}

impl ExtensionError {
    pub(crate) fn invalid(operation: &'static str, error: ContextError) -> Self {
        Self {
            operation,
            cause: Cause::Invalid(error),
        }
    }

    pub(crate) fn call(operation: &'static str, error: concord::Error) -> Self {
        Self {
            operation,
            cause: Cause::Call(error),
        }
    }

    /// The operation that failed.
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The reason the request could not be built (an empty, `.` or `..` tenant or segment, an
    /// invalid configured header value). Nothing was sent.
    pub fn invalid_request(&self) -> Option<&ContextError> {
        match &self.cause {
            Cause::Invalid(error) => Some(error),
            Cause::Call(_) => None,
        }
    }

    /// Concord's error for a request that was executed (or failed to be):
    /// [`kind`](concord::Error::kind) is `Transport`, `Status`, `Metadata` or `Decode`.
    pub fn concord(&self) -> Option<&concord::Error> {
        match &self.cause {
            Cause::Call(error) => Some(error),
            Cause::Invalid(_) => None,
        }
    }

    /// Concord's error, owned (to take an unread response with
    /// [`concord::Error::into_response`]); any other error is returned unchanged as `Err`.
    pub fn into_concord(self) -> Result<concord::Error, Self> {
        match self.cause {
            Cause::Call(error) => Ok(error),
            cause => Err(Self { cause, ..self }),
        }
    }
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.operation)?;
        match &self.cause {
            Cause::Invalid(error) => fmt::Display::fmt(error, f),
            Cause::Call(error) => fmt::Display::fmt(error, f),
        }
    }
}

/// The source is the inner error, whose own text is redacted and which has no further source.
impl std::error::Error for ExtensionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Invalid(error) => Some(error),
            Cause::Call(error) => Some(error),
        }
    }
}
