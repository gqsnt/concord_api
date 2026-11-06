//! The single error type returned by generated code.

use std::error::Error as StdError;
use std::fmt;

use crate::__private::EndpointMeta;
use crate::codec::{CodecError, ResponseHead};
use crate::response::{Operation, ResponseMeta};

/// Why a Concord call failed.
///
/// `Display` and `Debug` contain the endpoint, the HTTP method, the failing placement or the
/// status code, and for a format's failure its [`CodecError`]'s static label and numeric
/// positions — never request values, URLs, header values or body content as text. The positions
/// are whatever numbers the format recorded (a line, a record, a version it read from a header):
/// a format decides what it exposes there. The `concord` facade's crate documentation states the
/// exact scope of this guarantee.
pub struct Error {
    inner: Box<Inner>,
}

struct Inner {
    operation: Option<Operation>,
    repr: Repr,
    /// The head of the response whose body was being read when the call failed (`Decode`,
    /// `Limit`, a mid-body `Transport`), moved out of the native response before its body was
    /// consumed. `Status` and `Metadata` errors keep the whole response in `repr` instead.
    head: Option<ResponseMeta>,
    /// The body that did not decode, kept by [`Received::document`](crate::Received::document).
    body: Option<bytes::Bytes>,
}

enum Repr {
    Build {
        what: &'static str,
        reason: &'static str,
    },
    /// A header set twice under two spellings the macro could not prove equal (`what` is the
    /// later declaration).
    HeaderConflict {
        what: &'static str,
        earlier: &'static str,
    },
    Encode(CodecError),
    Transport(reqwest::Error),
    Status(reqwest::Response),
    /// A 2xx response whose head the format refused; the response is unread.
    Metadata(CodecError, reqwest::Response),
    Decode(CodecError),
    /// A body read with a byte limit was longer.
    Limit(u64),
}

/// The stage at which a call failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The request could not be built from the selected values (invalid path segment, host
    /// label, header value, base URL or body). Nothing was sent.
    Build,
    /// reqwest failed to send the request or to read the response.
    Transport,
    /// The server answered with a non-2xx status.
    Status,
    /// The server answered 2xx, but the declared format refused the response head (its
    /// `Content-Type`, or another header the format checks) before the body was read. The
    /// response is kept unread ([`Error::response`], [`Error::into_response`]).
    Metadata,
    /// A 2xx response body could not be decoded with the declared format.
    Decode,
    /// A body read with a byte limit ([`Received::bytes_limited`](crate::Received::bytes_limited))
    /// was longer than the limit; the rest was not read.
    Limit,
}

impl Error {
    pub(crate) fn build(
        endpoint: Option<&'static EndpointMeta>,
        what: &'static str,
        reason: &'static str,
    ) -> Self {
        Self::new(
            endpoint.map(Operation::endpoint),
            Repr::Build { what, reason },
        )
    }

    pub(crate) fn header_conflict(
        endpoint: &'static EndpointMeta,
        what: &'static str,
        earlier: &'static str,
    ) -> Self {
        Self::new(
            Some(Operation::endpoint(endpoint)),
            Repr::HeaderConflict { what, earlier },
        )
    }

    pub(crate) fn from_encode(endpoint: &'static EndpointMeta, error: CodecError) -> Self {
        Self::new(Some(Operation::endpoint(endpoint)), Repr::Encode(error))
    }

    pub(crate) fn from_transport(operation: Option<Operation>, error: reqwest::Error) -> Self {
        Self::new(operation, Repr::Transport(error.without_url()))
    }

    pub(crate) fn from_status(operation: Option<Operation>, response: reqwest::Response) -> Self {
        Self::new(operation, Repr::Status(response))
    }

    pub(crate) fn from_metadata(
        operation: Option<Operation>,
        error: CodecError,
        response: reqwest::Response,
    ) -> Self {
        Self::new(operation, Repr::Metadata(error, response))
    }

    pub(crate) fn from_decode(operation: Option<Operation>, error: CodecError) -> Self {
        Self::new(operation, Repr::Decode(error))
    }

    pub(crate) fn limit(operation: Option<Operation>, limit: u64) -> Self {
        Self::new(operation, Repr::Limit(limit))
    }

    fn new(operation: Option<Operation>, repr: Repr) -> Self {
        Self {
            inner: Box::new(Inner {
                operation,
                repr,
                head: None,
                body: None,
            }),
        }
    }

    /// The same error, with the head of the response whose body it concerns.
    pub(crate) fn with_head(mut self, head: ResponseMeta) -> Self {
        self.inner.head = Some(head);
        self
    }

    /// The same error, with the body that did not decode.
    pub(crate) fn with_body(mut self, body: bytes::Bytes) -> Self {
        self.inner.body = Some(body);
        self
    }

    /// The stage at which the call failed.
    pub fn kind(&self) -> ErrorKind {
        match self.inner.repr {
            Repr::Build { .. } | Repr::HeaderConflict { .. } | Repr::Encode(_) => ErrorKind::Build,
            Repr::Transport(_) => ErrorKind::Transport,
            Repr::Status(_) => ErrorKind::Status,
            Repr::Metadata(..) => ErrorKind::Metadata,
            Repr::Decode(_) => ErrorKind::Decode,
            Repr::Limit(_) => ErrorKind::Limit,
        }
    }

    /// The endpoint's declared name, such as `GitHub.repo.issue` (`None` for a [`Decoder`]
    /// built with [`Decoder::new`](crate::Decoder::new), a base URL given to `with_base_url`, or
    /// a natively obtained [`Incoming`](crate::Incoming), labelled or not).
    ///
    /// [`Decoder`]: crate::Decoder
    pub fn endpoint(&self) -> Option<&'static str> {
        self.inner
            .operation
            .filter(Operation::is_endpoint)
            .map(|op| op.name())
    }

    /// The call's identity: a declared endpoint, or the label given to a natively obtained
    /// [`Incoming`](crate::Incoming). `None` when neither exists.
    pub fn operation(&self) -> Option<Operation> {
        self.inner.operation
    }

    /// The response status, for [`ErrorKind::Status`] only. A [`Metadata`](ErrorKind::Metadata)
    /// error's response was a 2xx; read it with [`response`](Self::response).
    pub fn status(&self) -> Option<reqwest::StatusCode> {
        match &self.inner.repr {
            Repr::Status(response) => Some(response.status()),
            _ => None,
        }
    }

    /// The unread response of an [`ErrorKind::Status`] error (a non-2xx) or an
    /// [`ErrorKind::Metadata`] error (a 2xx whose head the format refused), borrowed: its status
    /// and headers (`Retry-After`, a request id, the `Content-Type` the format refused) can be read
    /// while the error is kept, for example by a predicate written as `Fn(&Error) -> bool`. To
    /// read the body, take the response with [`into_response`](Self::into_response).
    ///
    /// The headers are the server's, not Concord's output: Concord does not redact them.
    ///
    /// ```
    /// fn retry_after(e: &concord_core::Error) -> Option<u64> {
    ///     e.response()?
    ///         .headers()
    ///         .get(reqwest::header::RETRY_AFTER)?
    ///         .to_str()
    ///         .ok()?
    ///         .parse()
    ///         .ok()
    /// }
    /// ```
    pub fn response(&self) -> Option<&reqwest::Response> {
        match &self.inner.repr {
            Repr::Status(response) | Repr::Metadata(_, response) => Some(response),
            _ => None,
        }
    }

    /// The head of the response this error concerns — status, version and header fields,
    /// borrowed — whenever a response arrived: a [`Status`](ErrorKind::Status) or
    /// [`Metadata`](ErrorKind::Metadata) error (the response is kept unread), and a
    /// [`Decode`](ErrorKind::Decode), [`Limit`](ErrorKind::Limit) or
    /// [`Transport`](ErrorKind::Transport) error raised while that response's body was read
    /// (Concord moved the head out of the native response before reading the body: no header is
    /// cloned). `None` when no response arrived (`Build`, a transport failure before a head) and
    /// for a body-only call ([`Decode::decode_body`](crate::Decode::decode_body) sees no head).
    /// The header values are the server's, unredacted.
    ///
    /// The HTTP status of any failed call is `error.head().map(|h| h.status())`, after a decode
    /// failure too; [`status`](Self::status) stays the status of a `Status` error only.
    pub fn head(&self) -> Option<ResponseHead<'_>> {
        match &self.inner.repr {
            Repr::Status(response) | Repr::Metadata(_, response) => {
                Some(ResponseHead::of(response))
            }
            _ => self.inner.head.as_ref().map(ResponseMeta::head),
        }
    }

    /// An owned copy of [`head`](Self::head) with this error's operation, for an SDK error that
    /// outlives this one: available for every error raised after a response arrived (see
    /// `head`). Clones the header map.
    pub fn meta(&self) -> Option<ResponseMeta> {
        self.head()
            .map(|head| ResponseMeta::new(head, self.operation()))
    }

    /// The unread response of an [`ErrorKind::Status`] or [`ErrorKind::Metadata`] error; any other
    /// error is returned unchanged as `Err`. Concord never reads or prints the body: use it to
    /// decode an API error document, for example with
    /// `<Json as Decode<Problem>>::decode_body((), response.bytes().await?)` (a body-only decode:
    /// no head check), or to read a body the format refused.
    ///
    /// ```no_run
    /// # async fn run(request: impl std::future::Future<Output = Result<(), concord_core::Error>>)
    /// # -> Result<(), concord_core::Error> {
    /// match request.await {
    ///     Ok(()) => {}
    ///     Err(e) => {
    ///         let response = e.into_response()?; // no response to take: propagate it
    ///         let status = response.status();
    ///         # let _ = status;
    ///     }
    /// }
    /// # Ok(()) }
    /// ```
    pub fn into_response(self) -> Result<reqwest::Response, Error> {
        let Inner {
            operation,
            repr,
            head,
            body,
        } = *self.inner;
        match repr {
            Repr::Status(response) | Repr::Metadata(_, response) => Ok(response),
            repr => Err(Error {
                inner: Box::new(Inner {
                    operation,
                    repr,
                    head,
                    body,
                }),
            }),
        }
    }

    /// The underlying reqwest error (with its URL removed), for [`ErrorKind::Transport`].
    ///
    /// **Not redacted below the top level**, which is why it is not this error's `source()`: its
    /// own chain comes from reqwest, hyper, TLS and DNS layers that Concord does not control and
    /// can contain URLs (a redirect error's source holds the redirect target, query included) and
    /// host names (a certificate mismatch), including sensitive values.
    pub fn transport(&self) -> Option<&reqwest::Error> {
        match &self.inner.repr {
            Repr::Transport(error) => Some(error),
            _ => None,
        }
    }

    /// The body that did not decode, for a [`Decode`](ErrorKind::Decode) error raised by
    /// [`Received::document`](crate::Received::document): an error document an SDK keeps raw when
    /// it is not the provider's (an HTML page from a proxy, an empty body). The declared
    /// `decode()` path does not keep it. **Unredacted** — body content can quote request values —
    /// and never printed by Concord.
    pub fn body_unredacted(&self) -> Option<&bytes::Bytes> {
        self.inner.body.as_ref()
    }

    /// The format's error, for [`ErrorKind::Metadata`], [`ErrorKind::Decode`] and a body that
    /// could not be encoded ([`ErrorKind::Build`]): a static label and numeric positions. Its
    /// underlying error, which may quote body content, is reachable only through
    /// [`CodecError::source_unredacted`], never through [`std::error::Error::source`].
    pub fn codec_error(&self) -> Option<&CodecError> {
        match &self.inner.repr {
            Repr::Encode(e) | Repr::Decode(e) | Repr::Metadata(e, _) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.inner.operation {
            write!(f, "{operation}: ")?;
        }
        match &self.inner.repr {
            // Without an endpoint no request was involved: a base URL given to `with_base_url`.
            Repr::Build { what, reason } if self.inner.operation.is_none() => {
                write!(f, "invalid {what}: it {reason}")
            }
            Repr::Build { what, reason } => write!(f, "request not built: {what} {reason}"),
            Repr::HeaderConflict { what, earlier } => write!(
                f,
                "request not built: {what} sets a header that {earlier} already sets (both name the same header); use `override` or `append`, or `remove` it first"
            ),
            Repr::Encode(e) => write!(f, "request not built: body: {e}"),
            Repr::Transport(e) => {
                f.write_str("transport error")?;
                let class = if e.is_timeout() {
                    Some("timeout")
                } else if e.is_connect() {
                    Some("connect")
                } else if e.is_body() || e.is_decode() {
                    Some("reading the response body")
                } else if e.is_redirect() {
                    Some("redirect")
                } else {
                    None
                };
                match class {
                    Some(class) => write!(f, " ({class})"),
                    None => Ok(()),
                }
            }
            Repr::Status(response) => {
                let status = response.status();
                write!(f, "HTTP status {}", status.as_u16())?;
                match status.canonical_reason() {
                    Some(reason) => write!(f, " {reason}"),
                    None => Ok(()),
                }
            }
            Repr::Metadata(e, _) => write!(f, "response metadata not accepted: {e}"),
            Repr::Decode(e) => write!(f, "response not decoded: {e}"),
            Repr::Limit(limit) => {
                write!(f, "response body longer than the limit of {limit} bytes")
            }
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Error");
        s.field("kind", &self.kind());
        s.field("endpoint", &self.endpoint());
        if let Some(status) = self.status() {
            s.field("status", &status.as_u16());
        }
        s.field("message", &format_args!("{self}"));
        s.finish()
    }
}

/// Concord errors expose no `source()` chain. Underlying errors are reachable only through the
/// explicit accessors [`Error::transport`] and [`Error::codec_error`]
/// ([`CodecError::source_unredacted`]), because their text is not
/// under Concord's control: reqwest nests errors that keep URLs (a refused redirect keeps its
/// target, which a server may build from any request value, headers included), TLS errors print
/// host names and decoders quote response bodies.
impl StdError for Error {}
