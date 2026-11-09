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

use reqwest::header::HeaderMap;
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
    /// A declared endpoint named by its generated `Endpoint::operation()` (no request built):
    /// name, method and whether it only reads.
    Declared(&'static str, &'static str, bool),
    Label(&'static str),
}

impl Operation {
    pub(crate) fn endpoint(meta: &'static EndpointMeta) -> Self {
        Self(Ident::Endpoint(meta))
    }

    pub(crate) fn declared(name: &'static str, method: &'static str, read: bool) -> Self {
        Self(Ident::Declared(name, method, read))
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
            Ident::Declared(name, ..) | Ident::Label(name) => name,
        }
    }

    /// The declared HTTP method (`None` for a label).
    pub fn method(&self) -> Option<&'static str> {
        match self.0 {
            Ident::Endpoint(meta) => Some(meta.method),
            Ident::Declared(_, method, _) => Some(method),
            Ident::Label(_) => None,
        }
    }

    /// Whether the declared endpoint only reads provider state: `GET`, `HEAD` and `OPTIONS` do,
    /// other methods do not, unless the endpoint or a scope above it is declared `#[read]`. A
    /// label is `false`. A fact of the declaration, not a permission to resend.
    pub fn is_read(&self) -> bool {
        match self.0 {
            Ident::Endpoint(meta) => meta.read,
            Ident::Declared(_, _, read) => read,
            Ident::Label(_) => false,
        }
    }

    /// Whether this is a declared endpoint (not a label).
    pub fn is_endpoint(&self) -> bool {
        matches!(self.0, Ident::Endpoint(_) | Ident::Declared(..))
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
            Ident::Declared(..) | Ident::Label(_) => None,
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

    /// An owned copy of the head and the operation, to keep next to the decoded value or a
    /// provider error after the body is consumed. Clones the header map.
    pub fn meta(&self) -> ResponseMeta {
        ResponseMeta::new(self.head(), self.operation())
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

    /// Reads the body, whatever the status, keeping at most `limit` bytes: for a provider's error
    /// document on a non-2xx response, read deliberately and bounded (a format's
    /// [`Decode::decode_body`](crate::Decode::decode_body) can then parse it, with no head
    /// check). The declared handling is not applied. Bytes reqwest decompressed count after
    /// decompression.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Limit`](crate::ErrorKind::Limit) when the body is longer than `limit` (the
    /// rest is not read); [`ErrorKind::Transport`](crate::ErrorKind::Transport) if reading fails.
    /// Both name the operation.
    pub async fn bytes_limited(self, limit: usize) -> Result<bytes::Bytes, Error> {
        let operation = self.decoder.operation();
        let mut response = self.response;
        let head = ResponseMeta::take(&mut response, operation);
        crate::prepared::read_limited(response, limit, operation)
            .await
            .map_err(|e| e.with_head(head))
    }

    /// Reads the body **whatever the status**, keeping at most `limit` bytes, and decodes it
    /// with the format `F` as a declared output's body is decoded: `F`'s
    /// [`check_content_type`](crate::Decode::check_content_type) and
    /// [`context`](crate::Decode::context) run on the head first, then the bounded read, then
    /// [`decode_body`](crate::Decode::decode_body) with that context. For a provider's error
    /// document on a non-2xx response, or a document on a status the SDK accepts itself (a
    /// `202` job, a `304`); the declared handling and its 2xx rule are not applied.
    ///
    /// Returns the response head (moved out of the response, not cloned) beside the outcome, so
    /// the status stays authoritative whatever happened to the document:
    ///
    /// ```no_run
    /// # use concord_core::{bytes::Bytes, CodecError, Decode, ResponseHead};
    /// # struct Json; struct Problem;
    /// # impl Decode<Problem> for Json {
    /// #     type Context = ();
    /// #     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
    /// #     fn decode_body((): (), _: Bytes) -> Result<Problem, CodecError> { Ok(Problem) }
    /// # }
    /// # async fn run(received: concord_core::Received<()>) {
    /// if !received.status().is_success() {
    ///     let (meta, problem) = received.document::<Json, Problem>(16 * 1024).await;
    ///     // `meta.status()` is the failure; `problem` is the typed document or why it is not one
    ///     # let _ = (meta, problem);
    /// }
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// The second element is [`Metadata`](crate::ErrorKind::Metadata) when the format refuses the
    /// head (the response is kept unread in the error), [`Limit`](crate::ErrorKind::Limit) when
    /// the body is longer than `limit`, [`Transport`](crate::ErrorKind::Transport) when reading
    /// fails, [`Decode`](crate::ErrorKind::Decode) when it does not decode (the body is kept:
    /// [`Error::body_unredacted`]) — each naming the operation, with the head ([`Error::head`]).
    pub async fn document<F, D>(self, limit: usize) -> (ResponseMeta, Result<D, Error>)
    where
        F: crate::Decode<D>,
    {
        let operation = self.decoder.operation();
        let mut response = self.response;
        let checked = F::check_content_type(&ResponseHead::of(&response))
            .and_then(|()| F::context(&ResponseHead::of(&response)));
        let context = match checked {
            Ok(context) => context,
            Err(e) => {
                let head = ResponseMeta::new(ResponseHead::of(&response), operation);
                return (head, Err(Error::from_metadata(operation, e, response)));
            }
        };
        let head = ResponseMeta::take(&mut response, operation);
        let outcome = match crate::prepared::read_limited(response, limit, operation).await {
            Ok(body) => F::decode_body(context, body.clone())
                .map_err(|e| Error::from_decode(operation, e).with_body(body)),
            Err(e) => Err(e),
        };
        let outcome = outcome.map_err(|e| e.with_head(head.clone()));
        (head, outcome)
    }

    /// [`decode`](Self::decode), also returning the response head owned, for an SDK that keeps
    /// metadata beside the value. For a finite output (`-> T`) or no output the head is moved out
    /// of the response once the format accepted it (nothing is cloned); a stream reader or an
    /// `Incoming` keeps its response, so its head is a copy. A failure after the head arrived
    /// carries the head too ([`Error::head`]).
    ///
    /// # Errors
    ///
    /// As [`decode`](Self::decode).
    pub async fn decode_with_meta(self) -> Result<(T, ResponseMeta), Error> {
        let limit = self.decoder.limit();
        self.decoder
            .decode_keeping(self.response, limit, true)
            .await
    }

    /// [`decode_limited`](Self::decode_limited), also returning the response head owned, as
    /// [`decode_with_meta`](Self::decode_with_meta).
    ///
    /// # Errors
    ///
    /// As [`decode_limited`](Self::decode_limited).
    pub async fn decode_limited_with_meta(self, limit: usize) -> Result<(T, ResponseMeta), Error> {
        self.decoder
            .decode_keeping(self.response, Some(limit), true)
            .await
    }

    /// [`decode`](Self::decode) with the finite body read bounded: a `-> T` output whose body is
    /// longer than `limit` bytes is [`ErrorKind::Limit`](crate::ErrorKind::Limit) and is not
    /// decoded (nothing more is read). Status and head checks are unchanged. A drain, a stream
    /// reader (which has its own limits) or an `Incoming` body is not affected.
    ///
    /// # Errors
    ///
    /// As [`decode`](Self::decode), and `Limit`.
    pub async fn decode_limited(self, limit: usize) -> Result<T, Error> {
        self.decoder.decode_within(self.response, Some(limit)).await
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

    /// An owned copy of the head and the operation (see [`Received::meta`]).
    pub fn meta(&self) -> ResponseMeta {
        ResponseMeta::new(self.head(), self.operation)
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

    /// The next chunk of the body, `Ok(None)` at its end: for a parser of your own (an event
    /// stream) that reads the body without leaving Concord's boundary. A failure is
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) naming the operation, its URL removed.
    ///
    /// Cancel-safe: a call dropped while it waits (a timeout, a `select!` branch) takes no byte
    /// with it; the next call returns the data in order. Every byte is held by the response,
    /// none by the call's future (`checks/phase-a/tests/streams.rs`
    /// `a_cancelled_chunk_call_loses_no_byte`).
    ///
    /// A failure is not remembered: after an `Err`, a further call may return `Ok(None)`. A
    /// parser keeps the error (and stops) instead of reading on, so a broken body never looks
    /// complete.
    ///
    /// # Errors
    ///
    /// `Transport` if reading fails.
    pub async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, Error> {
        match self.response.chunk().await {
            Ok(chunk) => Ok(chunk),
            // The response stays with this `Incoming`, so its error carries a copy of the head.
            Err(e) => Err(Error::from_transport(self.operation, e)
                .with_head(ResponseMeta::new(self.head(), self.operation))),
        }
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
        let head = ResponseMeta::take(&mut response, operation);
        let mut read = 0u64;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => read += chunk.len() as u64,
                Ok(None) => return Ok(read),
                Err(e) => return Err(Error::from_transport(operation, e).with_head(head)),
            }
        }
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

/// An owned copy of a response head — status, version, header fields — and the [`Operation`]
/// that produced it: what an SDK keeps beside a decoded value, a provider error or a page after
/// the body is gone ([`Received::meta`], [`Incoming::meta`]).
///
/// Header values are reachable, unredacted, through [`headers`](Self::headers); `Debug` prints
/// the status, the version, the header **names** and the operation name, never a value
/// (`Set-Cookie` and provider tokens are header values).
#[derive(Clone)]
pub struct ResponseMeta {
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    operation: Option<Operation>,
}

impl ResponseMeta {
    /// A copy of `head` (its header map is cloned) with the call's identity.
    pub fn new(head: ResponseHead<'_>, operation: Option<Operation>) -> Self {
        Self {
            status: head.status(),
            version: head.version(),
            headers: head.headers().clone(),
            operation,
        }
    }

    /// A copy of the head of a response that a reader keeps (`keep`), or, when nothing keeps it,
    /// only its status and version: an empty header map allocates nothing.
    pub(crate) fn kept(
        response: &reqwest::Response,
        operation: Option<Operation>,
        keep: bool,
    ) -> Self {
        if keep {
            return Self::new(ResponseHead::of(response), operation);
        }
        Self {
            status: response.status(),
            version: response.version(),
            headers: HeaderMap::new(),
            operation,
        }
    }

    /// The head of `response`, **moved** out of it (its header map is left empty): for a
    /// response whose body is about to be read and whose headers nothing reads any more. Reading
    /// the body does not consult the header map (reqwest set up any decompression when the
    /// response arrived).
    pub(crate) fn take(response: &mut reqwest::Response, operation: Option<Operation>) -> Self {
        Self {
            status: response.status(),
            version: response.version(),
            headers: std::mem::take(response.headers_mut()),
            operation,
        }
    }

    /// The response status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The HTTP version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The header fields, as received. Unredacted.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The identity of the call (`None` for an unlabelled native response).
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }

    /// The copy as a borrowed head, for code written against [`ResponseHead`].
    pub fn head(&self) -> ResponseHead<'_> {
        ResponseHead::new(self.status, self.version, &self.headers)
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status.as_u16())
            .field("version", &self.version)
            .field(
                "header_names",
                &self.headers.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
            )
            .field("operation", &self.operation.map(|o| o.name()))
            .finish()
    }
}
