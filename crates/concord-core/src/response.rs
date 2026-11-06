//! A received response before its body is consumed: [`Received`], the unread body handed to an
//! incremental reader ([`Incoming`]) and the safe identity both carry ([`Operation`]).
//!
//! The lifecycle of one call:
//!
//! ```text
//! Prepared<T> ──receive()──▶ Received<T> ──decode()────────▶ T            (finite, drain or stream)
//!   (built, no I/O)   one execute,     │  head(): status, version, headers (borrowed, unread)
//!                     any status,      ├─into_incoming()──▶ Incoming ──▶ a runtime reader
//!                     body unread      └─into_response()──▶ reqwest::Response (native escape)
//! ```
//!
//! Each consumer takes `self`: a response is decoded, opened or escaped exactly once, and the
//! compiler refuses a second consumer. Dropping a [`Received`] or an [`Incoming`] drops the
//! response (and its connection) without reading the body.

use std::fmt;

use reqwest::{StatusCode, Version};

use crate::__private::EndpointMeta;
use crate::codec::ResponseHead;
use crate::error::Error;
use crate::prepared::Decoder;

/// The safe identity of a call: a declared endpoint (`GitHub.repo.issue`, with its method), or a
/// static label an SDK chose for a response it obtained natively (a signed download URL).
///
/// It is what errors and incremental readers print to say which call failed. It holds no request
/// value, URL or header: only the static name the declaration or the SDK supplied. Generated code
/// attaches it to every [`Prepared`](crate::Prepared), [`Received`] and [`Incoming`]; an SDK never
/// repeats the endpoint's name as a string.
#[derive(Clone, Copy)]
pub struct Operation(Ident);

#[derive(Clone, Copy)]
enum Ident {
    Endpoint(&'static EndpointMeta),
    Label(&'static str),
}

impl Operation {
    pub(crate) fn endpoint(meta: &'static EndpointMeta) -> Self {
        Self(Ident::Endpoint(meta))
    }

    /// A static label for a response that no declared endpoint produced (a signed URL fetched
    /// natively). Choose a name that holds no request value: it is printed by errors.
    pub const fn label(name: &'static str) -> Self {
        Self(Ident::Label(name))
    }

    /// The declared endpoint's name (`Client.scope.endpoint`), or the label.
    pub fn name(&self) -> &'static str {
        match self.0 {
            Ident::Endpoint(meta) => meta.name,
            Ident::Label(name) => name,
        }
    }

    /// The declared HTTP method (`None` for a label).
    pub fn method(&self) -> Option<&'static str> {
        match self.0 {
            Ident::Endpoint(meta) => Some(meta.method),
            Ident::Label(_) => None,
        }
    }

    /// Whether this is a declared endpoint (not a label).
    pub fn is_endpoint(&self) -> bool {
        matches!(self.0, Ident::Endpoint(_))
    }

    /// Converts a transport failure of a request executed natively for this operation (a signed
    /// download labelled with [`Operation::label`]) into Concord's
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) error naming it, with the URL removed
    /// from the underlying error. For a declared endpoint, [`Decoder::transport_error`] is the same.
    pub fn transport_error(&self, error: reqwest::Error) -> Error {
        Error::from_transport(Some(*self), error)
    }

    pub(crate) fn meta(&self) -> Option<&'static EndpointMeta> {
        match self.0 {
            Ident::Endpoint(meta) => Some(meta),
            Ident::Label(_) => None,
        }
    }
}

impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        self.is_endpoint() == other.is_endpoint()
            && self.name() == other.name()
            && self.method() == other.method()
    }
}

impl Eq for Operation {}

impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.method() {
            Some(method) => write!(f, "Operation({method} {})", self.name()),
            None => write!(f, "Operation({:?})", self.name()),
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.method() {
            Some(method) => write!(f, "{method} {}", self.name()),
            None => f.write_str(self.name()),
        }
    }
}

/// A response received for a declared operation, **any status**, body not yet read, with the
/// operation's declared response handling and identity.
///
/// Obtained from [`Prepared::receive`](crate::Prepared::receive) (or
/// [`Decoder::execute`] / [`Decoder::receive`] for a request you executed or edited natively). It
/// is the boundary where an SDK reads provider metadata from the head, refuses a response on a
/// provider rule, or chooses how the body is consumed — without re-implementing execution:
///
/// ```no_run
/// # async fn run(prepared: concord_core::Prepared<String>) -> Result<(), concord_core::Error> {
/// let received = prepared.receive().await?; // one execute; transport failures are `Error`s
/// let request_id = received.head().headers().get("x-request-id").cloned(); // nothing read yet
/// let text = received.decode().await?; // the declared handling: status, format, body
/// # let _ = (request_id, text); Ok(()) }
/// ```
///
/// Exactly one consumer takes the response: [`decode`](Self::decode) (the declared handling),
/// [`into_incoming`](Self::into_incoming) (an incremental reader of your choosing),
/// [`into_response`](Self::into_response) or [`into_parts`](Self::into_parts) (native). Each takes
/// `self`, so a body cannot be decoded twice or decoded and opened:
///
/// ```compile_fail,E0382
/// # async fn run(prepared: concord_core::Prepared<String>) -> Result<(), concord_core::Error> {
/// let received = prepared.receive().await?;
/// let first = received.decode().await?;
/// let second = received.decode().await?; // the response was moved into the first decode
/// # let _ = (first, second); Ok(()) }
/// ```
///
/// Metadata that must survive a body failure is read from [`head`](Self::head) **before** the
/// consumer, into a value the SDK owns (a request id, a rate-limit counter); Concord copies no
/// header for you. `Debug` prints the operation, the status, the version and the header names —
/// never the URL or a header value.
#[must_use = "a received response is consumed by `decode`, `into_incoming` or `into_response`"]
pub struct Received<T> {
    response: reqwest::Response,
    decoder: Decoder<T>,
}

impl<T> Received<T> {
    pub(crate) fn new(response: reqwest::Response, decoder: Decoder<T>) -> Self {
        Self { response, decoder }
    }

    /// The response head (status, version, header fields), borrowed: reading it does not
    /// consume the response. The header values are the server's, unredacted.
    pub fn head(&self) -> ResponseHead<'_> {
        ResponseHead::of(&self.response)
    }

    /// The response status.
    pub fn status(&self) -> StatusCode {
        self.response.status()
    }

    /// The HTTP version.
    pub fn version(&self) -> Version {
        self.response.version()
    }

    /// The URL the response came from — after any redirect the client followed — **unredacted**:
    /// it can hold sensitive query values and path segments. For resolving a relative `Link` or
    /// `Location` against it, or checking where a redirect led; never log it. `Debug` of
    /// `Received` does not print it.
    pub fn url(&self) -> &reqwest::Url {
        self.response.url()
    }

    /// The call's identity: the declared endpoint (`None` for a [`Decoder::new`] decoder).
    pub fn operation(&self) -> Option<Operation> {
        self.decoder.operation()
    }

    /// Applies the operation's declared response handling, exactly as
    /// [`Prepared::send`](crate::Prepared::send) does after executing: a non-2xx status is
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) (response kept unread), then the format
    /// checks the head ([`Metadata`](crate::ErrorKind::Metadata), unread) and the body is
    /// decoded (a finite `-> T`), drained (no `->`) or opened for incremental reading
    /// (`-> stream T`; nothing is read).
    ///
    /// # Errors
    ///
    /// `Status`, `Metadata`, `Transport` (reading the body) or `Decode`, each naming the
    /// operation.
    pub async fn decode(self) -> Result<T, Error> {
        self.decoder.decode(self.response).await
    }

    /// The unread body for an incremental reader chosen at run time (a CSV dialect read from a
    /// job, say), after requiring a 2xx status. The declared format is not applied: the reader
    /// checks the head itself. The operation's identity travels with the body, so the reader's
    /// errors name it.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) for a non-2xx status, keeping the response
    /// unread ([`Error::into_response`]).
    pub fn into_incoming(self) -> Result<Incoming, Error> {
        Incoming::checked(self.response, self.decoder.operation())
    }

    /// The native response, unread, **leaving Concord's boundary**: its `Debug` and `url()` show
    /// the full URL (sensitive query values and path segments included), and its body is yours to
    /// read and bound.
    pub fn into_response(self) -> reqwest::Response {
        self.response
    }

    /// The native response (see [`into_response`](Self::into_response)) and the declared
    /// handling, to apply later with [`Decoder::decode`].
    pub fn into_parts(self) -> (reqwest::Response, Decoder<T>) {
        (self.response, self.decoder)
    }
}

impl<T> fmt::Debug for Received<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Received")
            .field("operation", &self.operation())
            .field("head", &self.head())
            .finish()
    }
}

/// A 2xx response whose body has not been polled, with the identity of the call that produced
/// it: what an incremental reader opens.
///
/// Produced by an endpoint declared `-> stream` (its `send()` returns one), by
/// [`Received::into_incoming`], or from a native response with [`Incoming::new`] (a signed
/// download URL, optionally labelled). A reader checks the head (`Content-Type`, a header
/// parameter) before polling the body; its later errors name the operation.
///
/// Dropping it drops the response without reading it. [`drain`](Self::drain) reads the body to
/// its end explicitly, discarding the bytes.
#[must_use = "an incoming body does nothing until a reader opens it (or it is drained)"]
pub struct Incoming {
    response: reqwest::Response,
    operation: Option<Operation>,
}

impl Incoming {
    pub(crate) fn checked(
        response: reqwest::Response,
        operation: Option<Operation>,
    ) -> Result<Self, Error> {
        if response.status().is_success() {
            Ok(Self {
                response,
                operation,
            })
        } else {
            Err(Error::from_status(operation, response))
        }
    }

    /// A native response obtained outside any declaration, after requiring a 2xx status;
    /// `operation` names it in errors (`Some(Operation::label("Shop.bulk_result"))`), or
    /// `None`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) for a non-2xx status, keeping the response
    /// unread.
    pub fn new(response: reqwest::Response, operation: Option<Operation>) -> Result<Self, Error> {
        Self::checked(response, operation)
    }

    /// The response head, borrowed (the status is a 2xx).
    pub fn head(&self) -> ResponseHead<'_> {
        ResponseHead::of(&self.response)
    }

    /// The identity of the call that produced the body.
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }

    /// The unread response and the identity, for a reader (`concord-extras` readers open an
    /// `Incoming` directly).
    pub fn into_parts(self) -> (reqwest::Response, Option<Operation>) {
        (self.response, self.operation)
    }

    /// The native response, unread, leaving Concord's boundary (see
    /// [`Received::into_response`]).
    pub fn into_response(self) -> reqwest::Response {
        self.response
    }

    /// Reads the body to its end and discards it (an explicit discard; dropping the value
    /// instead closes the connection without reading). Returns the bytes read. No size or time
    /// bound beyond the client's own timeouts.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) if reading fails.
    pub async fn drain(self) -> Result<u64, Error> {
        let Self {
            mut response,
            operation,
        } = self;
        let mut read = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::from_transport(operation, e))?
        {
            read += chunk.len() as u64;
        }
        Ok(read)
    }
}

impl fmt::Debug for Incoming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incoming")
            .field("operation", &self.operation)
            .field("head", &self.head())
            .finish()
    }
}
