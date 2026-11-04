//! The extension's error: which operation failed, and why.

use std::fmt;

use concord::stream::OpenError;
use concord_example_sdk::context::ContextError;

/// Why an operation of this crate failed.
///
/// It names the operation ([`operation`](Self::operation), such as
/// [`EXPORT_OPERATION`](crate::EXPORT_OPERATION)) and holds the reason the request could not be
/// built (nothing was sent), Concord's error for the call, or the incremental reader's refusal of
/// the response head. That Concord error comes from
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
    Open(OpenError),
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

    pub(crate) fn open(operation: &'static str, error: OpenError) -> Self {
        Self {
            operation,
            cause: Cause::Open(error),
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
            _ => None,
        }
    }

    /// Concord's error for a request that was executed (or failed to be):
    /// [`kind`](concord::Error::kind) is `Transport`, `Status`, `Metadata` or `Decode`.
    pub fn concord(&self) -> Option<&concord::Error> {
        match &self.cause {
            Cause::Call(error) => Some(error),
            _ => None,
        }
    }

    /// The incremental route's refusal of a response before reading its body
    /// ([`export_rows`](crate::export_rows)): a non-2xx status or a refused head. The response is
    /// kept unread ([`OpenError::into_response`]).
    pub fn open_refusal(&self) -> Option<&OpenError> {
        match &self.cause {
            Cause::Open(error) => Some(error),
            _ => None,
        }
    }

    /// The opening refusal, owned (to take the unread response back); any other error is
    /// returned unchanged as `Err`.
    pub fn into_open_refusal(self) -> Result<OpenError, Self> {
        match self.cause {
            Cause::Open(error) => Ok(error),
            cause => Err(Self { cause, ..self }),
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
        match &self.cause {
            Cause::Invalid(error) => write!(f, "{}: {error}", self.operation),
            Cause::Call(error) => write!(f, "{}: {error}", self.operation),
            // The opening refusal already names the operation it was opened for.
            Cause::Open(error) => fmt::Display::fmt(error, f),
        }
    }
}

/// The source is the inner error, whose own text is redacted and which has no further source.
impl std::error::Error for ExtensionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Invalid(error) => Some(error),
            Cause::Call(error) => Some(error),
            Cause::Open(error) => Some(error),
        }
    }
}
