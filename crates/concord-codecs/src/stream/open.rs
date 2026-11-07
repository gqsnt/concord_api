//! What an incremental reader opens: a native response, an [`Incoming`] body or a [`Received`]
//! response, with the operation identity each carries.

use concord_core::{CodecError, Incoming, Received, ResponseHead, reqwest};

use super::error::OpenError;

mod sealed {
    pub trait Sealed {}
    impl Sealed for concord_core::reqwest::Response {}
    impl Sealed for concord_core::Incoming {}
    impl<T> Sealed for concord_core::Received<T> {}
    impl Sealed for (concord_core::reqwest::Response, concord_core::Operation) {}
}

/// A response an incremental reader can open (`reader.open(..)`): sealed, implemented for
///
/// - [`Incoming`]: the unread 2xx body of an endpoint declared `-> stream` (its `send()` returns
///   one) or of [`Received::into_incoming`]; the reader's errors name the declared endpoint (or
///   the label of a native [`Incoming::new`]);
/// - [`Received<T>`]: a received response of any declared endpoint, any status; a non-2xx is an
///   [`OpenError`] of kind `Status` that keeps it unread and names the endpoint;
/// - `reqwest::Response`: a native response, no identity;
/// - `(reqwest::Response, Operation)`: a native response named by an operation, typically
///   [`Operation::label`](concord_core::Operation::label) for a signed download: a non-2xx is an
///   [`OpenError`] of kind `Status` naming it (where [`Incoming::new`] would return a
///   `concord::Error` instead).
///
/// Opening requires a 2xx status, then applies the reader's head checks, before the body is
/// polled.
pub trait Openable: sealed::Sealed {
    #[doc(hidden)]
    fn into_open(self) -> (reqwest::Response, Option<&'static str>);
}

impl Openable for reqwest::Response {
    fn into_open(self) -> (reqwest::Response, Option<&'static str>) {
        (self, None)
    }
}

impl Openable for Incoming {
    fn into_open(self) -> (reqwest::Response, Option<&'static str>) {
        let (response, operation) = self.into_parts();
        (response, operation.map(|op| op.name()))
    }
}

impl Openable for (reqwest::Response, concord_core::Operation) {
    fn into_open(self) -> (reqwest::Response, Option<&'static str>) {
        (self.0, Some(self.1.name()))
    }
}

impl<T> Openable for Received<T> {
    fn into_open(self) -> (reqwest::Response, Option<&'static str>) {
        let operation = self.operation().map(|op| op.name());
        (self.into_response(), operation)
    }
}

/// Requires a 2xx status, then runs `check` on the borrowed head; both before the body is polled.
pub(crate) fn open_input(
    input: impl Openable,
    check: impl FnOnce(&ResponseHead<'_>) -> Result<(), CodecError>,
) -> Result<(reqwest::Response, Option<&'static str>), OpenError> {
    let (response, operation) = input.into_open();
    let response = super::error::open_response(response, operation, check)?;
    Ok((response, operation))
}
