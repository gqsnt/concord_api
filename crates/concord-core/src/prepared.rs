//! A built native request plus the declared response handling.

use std::fmt;

use bytes::Bytes;

use crate::__private::{EndpointMeta, redacted_url};
use crate::codec::{CodecError, Decode, Incremental, ResponseHead};
use crate::error::Error;
use crate::response::{Incoming, Operation, Received};

/// A request built by an endpoint, not yet sent. `T` is the endpoint's output type (`()` when it
/// declares none).
///
/// Building performs no I/O. `Debug` renders the method, URL and headers with every sensitive
/// placement replaced by `<redacted>`.
///
/// To change the request natively (extra headers, a per-request timeout, a streaming body), take
/// it apart with [`into_parts`](Self::into_parts). That is a one-way door: Concord's redaction
/// metadata does not follow the native request, whose own `Debug` prints the full URL (sensitive
/// query values and path segments included) and masks only header values flagged sensitive.
#[must_use = "a prepared request does nothing until it is sent"]
pub struct Prepared<T> {
    client: reqwest::Client,
    request: reqwest::Request,
    decoder: Decoder<T>,
}

impl<T> Prepared<T> {
    pub(crate) fn new(
        client: reqwest::Client,
        request: reqwest::Request,
        decoder: Decoder<T>,
    ) -> Self {
        Self {
            client,
            request,
            decoder,
        }
    }

    /// The identity of the endpoint that built this request.
    pub fn operation(&self) -> Option<Operation> {
        self.decoder.operation
    }

    /// The native request, for inspection. Its `Debug` output is **not** Concord's: it prints the full
    /// URL, sensitive query values and path segments included; only header values flagged sensitive
    /// (by Concord or already on a native value) are masked (reqwest prints them as `Sensitive`).
    pub fn request(&self) -> &reqwest::Request {
        &self.request
    }

    /// Executes the request once with the application's `reqwest::Client`, requires a 2xx status,
    /// lets the declared format check the response head, then reads and decodes the body (an
    /// endpoint without `->` drains it and returns `()`). Never retries.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) if reqwest fails to send the request
    /// or read the response, [`Status`](crate::ErrorKind::Status) for a non-2xx response (kept
    /// unread), [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the 2xx response's
    /// head (kept unread), [`Decode`](crate::ErrorKind::Decode) if the body does not decode.
    pub async fn send(self) -> Result<T, Error> {
        self.receive().await?.decode().await
    }

    /// Executes the request once and returns the response for **any** status with its body
    /// unread, keeping the endpoint's declared handling and identity: the boundary where an SDK
    /// reads response metadata, applies a provider rule to the head, or chooses an incremental
    /// reader, then consumes the body once ([`Received`]). Never retries.
    ///
    /// `send()` is `receive()` followed by [`Received::decode`].
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) only (no response arrived), naming
    /// the endpoint, with the URL removed from the underlying error.
    pub async fn receive(self) -> Result<Received<T>, Error> {
        let (client, request, decoder) = self.into_parts();
        decoder.execute(&client, request).await
    }

    /// Executes the request once and returns the native response without checking its status or
    /// reading its body.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) only: any status is returned as a
    /// response.
    pub async fn send_raw(self) -> Result<reqwest::Response, Error> {
        Ok(self.receive().await?.into_response())
    }

    /// The application's client, the native request and the declared response handling.
    ///
    /// `reqwest::RequestBuilder::from_parts(client, request)` turns the pair back into a builder.
    /// After executing the request yourself, [`Decoder::finish`] applies the endpoint's response
    /// handling and Concord's error conversion.
    ///
    /// The decoder is fixed: changing the request (its `Accept`, say) does not change which
    /// format checks and decodes the response.
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<T>) {
        (self.client, self.request, self.decoder)
    }
}

/// A generated endpoint request, seen generically: every endpoint type `api!` generates implements
/// `Endpoint<T>` for its declared output `T` (the decoded `T` of `-> T`, `()` without `->`, the
/// reader of `-> stream T`, `Incoming` for `-> stream`), so an SDK can write **one** adapter for all
/// of its operations instead of a wrapper method per operation — a send that returns the SDK's own
/// error and response metadata, say:
///
/// ```ignore
/// pub trait Call<T>: concord::Endpoint<T> {
///     fn call(self) -> impl Future<Output = Result<WithMeta<T>, SdkError>>;
/// }
/// impl<T, E: concord::Endpoint<T>> Call<T> for E {
///     async fn call(self) -> Result<WithMeta<T>, SdkError> {
///         let received = self.prepare()?.receive().await?;
///         let meta = Meta::capture(&received.head());
///         Ok(WithMeta { value: received.decode().await?, meta })
///     }
/// }
/// // caller: `use my_sdk::Call;` then `api.items().get(7).call().await?`
/// ```
///
/// The output is a type parameter, not an associated type, so a public client may keep private
/// output types (an associated type in this public impl would be E0446). The generated type's own
/// `prepare()` and `send()` stay what callers use directly; the trait adds no method to the generated
/// namespace.
pub trait Endpoint<T>: Sized {
    /// The generated `prepare()`: builds the request, no I/O.
    ///
    /// # Errors
    ///
    /// As the generated `prepare()` (`ErrorKind::Build`).
    fn prepare(self) -> Result<Prepared<T>, Error>;

    /// The endpoint's declared identity, without building a request.
    fn operation() -> Operation;
}

/// A client's **send policy**: what every generated `send()` returns, for a client declared with
/// `send P;`. An SDK whose callers should receive its own error type and response metadata (or
/// whose requests must be signed after they are built) implements it once, on a type of its own,
/// and names that type in the declaration; each endpoint's `send()` then calls
/// [`send`](Self::send) with the endpoint. No wrapper method per operation, no trait to import,
/// one terminal method.
///
/// ```ignore
/// pub struct Policy;
/// impl<T: Send> concord::SendWith<T> for Policy {
///     type Output = Result<WithMeta<T>, SdkError>;
///     async fn send<E: concord::Endpoint<T> + Send>(endpoint: E) -> Self::Output {
///         let received = endpoint.prepare()?.receive().await?;
///         let meta = Meta::capture(&received.head());
///         Ok(WithMeta { value: received.decode().await?, meta })
///     }
/// }
/// // declaration: `pub client Api { base "…"; send Policy; … }`
/// // caller: `api.items().get(7).send().await?` returns `Result<WithMeta<Item>, SdkError>`
/// ```
///
/// Without `send P;` the generated `send()` is `prepare()?.send()` (Concord's `Error`). `prepare()`
/// is unchanged either way, for callers that need the native request.
#[diagnostic::on_unimplemented(
    message = "the send policy `{Self}` does not send a `{T}` output",
    label = "named by the client's `send` statement; it needs `{Self}: SendWith<{T}>`",
    note = "a send policy implements `concord::SendWith<T>` for every output `T` its endpoints declare (usually generically: `impl<T: Send> SendWith<T> for Policy`)"
)]
pub trait SendWith<T> {
    /// What `send()` returns.
    type Output;

    /// Sends the endpoint's request according to the policy.
    fn send<E>(endpoint: E) -> impl std::future::Future<Output = Self::Output> + Send
    where
        E: Endpoint<T> + Send;
}

impl<T> fmt::Debug for Prepared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.decoder.operation.and_then(|op| op.meta());
        let headers = DebugHeaders(self.request.headers());
        f.debug_struct("Prepared")
            .field("endpoint", &meta.map(|m| m.name))
            .field("method", &self.request.method().as_str())
            .field("url", &DebugUrl(self.request.url(), meta))
            .field("headers", &headers)
            .field("body", &self.request.body().map(|_| "..."))
            .finish()
    }
}

/// The URL with the endpoint's sensitive placements redacted. A [`Prepared`] is only built by
/// generated code, which always has the endpoint's metadata; without it the URL is withheld.
struct DebugUrl<'a>(&'a reqwest::Url, Option<&'static EndpointMeta>);

impl fmt::Debug for DebugUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(meta) => fmt::Debug::fmt(&redacted_url(self.0, meta), f),
            None => f.write_str("<redacted>"),
        }
    }
}

struct DebugHeaders<'a>(&'a reqwest::header::HeaderMap);

impl fmt::Debug for DebugHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (name, value) in self.0 {
            if value.is_sensitive() {
                list.entry(&format_args!("{name}: <redacted>"));
            } else if let Ok(text) = value.to_str() {
                list.entry(&format_args!("{name}: {text}"));
            } else {
                // A native value that is not text: its escaped bytes (`"caf\xe9"`).
                list.entry(&format_args!("{name}: {value:?}"));
            }
        }
        list.finish()
    }
}

/// The format's continuation once the head was accepted: decodes the body with the context taken
/// from the head. Boxing a zero-sized context allocates nothing.
type BodyStep<T> = Box<dyn FnOnce(Bytes) -> Result<T, CodecError> + Send>;

/// Checks a response head with a format and returns its body step.
pub(crate) type HeadStep<T> = fn(&ResponseHead<'_>) -> Result<BodyStep<T>, CodecError>;

/// An incremental format's continuation once the head was accepted: opens the unread body.
type OpenStep<T> = Box<dyn FnOnce(Incoming) -> T + Send>;

/// Checks a response head with an incremental format and returns its opening step.
pub(crate) type OpenHeadStep<T> = fn(&ResponseHead<'_>) -> Result<OpenStep<T>, CodecError>;

/// An incremental format's head checks and opening, erased like [`head_step`].
pub(crate) fn open_step<F: Incremental<T> + 'static, T: 'static>(
    head: &ResponseHead<'_>,
) -> Result<OpenStep<F::Records>, CodecError>
where
    F::Records: 'static,
{
    F::check_content_type(head)?;
    let context = F::context(head)?;
    Ok(Box::new(move |body| F::open(context, body)))
}

/// A format's head checks and body decoding, monomorphized for `F` and erased behind a plain
/// function pointer, so a [`Decoder`] stays `Copy` and names only its output type.
pub(crate) fn head_step<F: Decode<T> + 'static, T: 'static>(
    head: &ResponseHead<'_>,
) -> Result<BodyStep<T>, CodecError> {
    F::check_content_type(head)?;
    let context = F::context(head)?;
    Ok(Box::new(move |body| F::decode_body(context, body)))
}

enum Kind<T> {
    /// No typed output: any 2xx, the body drained unbuffered.
    Drain(fn() -> T),
    Typed(HeadStep<T>),
    /// `-> stream T`: the head checked, the unread body opened into the reader `T`.
    Open(OpenHeadStep<T>),
    /// `-> stream`: any 2xx, the unread body returned as is (`T` is `Incoming`).
    Incoming(fn(Incoming) -> T),
}

impl<T> Clone for Kind<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Kind<T> {}

/// A response handling, applicable to any native response: requires a 2xx status, lets the
/// format check the head (its `Content-Type`, then anything else it checks) before the body is
/// read, then reads and decodes the body. For an endpoint without `->` it drains the body
/// without buffering it instead.
///
/// Every endpoint has one ([`Prepared::into_parts`]); [`Decoder::new`] builds one for a format
/// and output type of your choosing, without a declaration.
pub struct Decoder<T> {
    pub(crate) operation: Option<Operation>,
    kind: Kind<T>,
}

impl<T: 'static> Decoder<T> {
    /// The response handling of the format `F` for the output `T`, outside any declaration
    /// (errors then name no endpoint): for a response you obtained yourself, with the same
    /// status, head and body rules as a generated endpoint.
    ///
    /// ```no_run
    /// # use concord_core::{bytes::Bytes, CodecError, Decode, Decoder, ResponseHead};
    /// # struct Lines;
    /// # impl Decode<usize> for Lines {
    /// #     type Context = ();
    /// #     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
    /// #     fn decode_body((): (), b: Bytes) -> Result<usize, CodecError> { Ok(b.len()) }
    /// # }
    /// # async fn run(client: concord_core::reqwest::Client) -> Result<(), concord_core::Error> {
    /// let response = client.get("https://example.test/report").send().await;
    /// let lines: usize = Decoder::<usize>::new::<Lines>().finish(response).await?;
    /// # let _ = lines; Ok(()) }
    /// ```
    pub fn new<F: Decode<T> + 'static>() -> Self {
        Self {
            operation: None,
            kind: Kind::Typed(head_step::<F, T>),
        }
    }

    /// The opening of the incremental format `F` for items `I` (its reader is `T`), outside any
    /// declaration: the status and head rules of an endpoint declared `-> stream I`.
    pub fn open<F, I>() -> Self
    where
        F: Incremental<I, Records = T> + 'static,
        I: 'static,
    {
        Self {
            operation: None,
            kind: Kind::Open(open_step::<F, I>),
        }
    }
}

impl Decoder<Incoming> {
    /// The handling of an endpoint declared `-> stream` without an item: any 2xx, the body
    /// returned unread as an [`Incoming`] for a reader chosen at run time.
    pub fn incoming() -> Self {
        Self {
            operation: None,
            kind: Kind::Incoming(|body| body),
        }
    }
}

impl Decoder<()> {
    /// The response handling of an endpoint without typed output: any 2xx, the body drained
    /// without being buffered (no time or size bound beyond the client's own timeouts).
    pub fn drain() -> Self {
        Self {
            operation: None,
            kind: Kind::Drain(|| ()),
        }
    }
}

impl<T> Decoder<T> {
    pub(crate) fn typed(meta: &'static EndpointMeta, head: HeadStep<T>) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Typed(head),
        }
    }

    pub(crate) fn drained(meta: &'static EndpointMeta, unit: fn() -> T) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Drain(unit),
        }
    }

    pub(crate) fn opened(meta: &'static EndpointMeta, head: OpenHeadStep<T>) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Open(head),
        }
    }

    pub(crate) fn incoming_for(meta: &'static EndpointMeta, wrap: fn(Incoming) -> T) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Incoming(wrap),
        }
    }

    /// The identity of the endpoint this handling belongs to (`None` for [`Decoder::new`],
    /// [`Decoder::open`], [`Decoder::incoming`] and [`Decoder::drain`]).
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }

    /// Executes a native request once (one you edited after
    /// [`Prepared::into_parts`]: a `Link` URL, a per-request timeout) and returns the response,
    /// any status, unread, with this handling attached: the same boundary as
    /// [`Prepared::receive`].
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) only, naming the endpoint.
    pub async fn execute(
        self,
        client: &reqwest::Client,
        request: reqwest::Request,
    ) -> Result<Received<T>, Error> {
        match client.execute(request).await {
            Ok(response) => Ok(Received::new(response, self)),
            Err(e) => Err(self.transport_error(e)),
        }
    }

    /// Attaches this handling to a response you obtained yourself, unread.
    pub fn receive(self, response: reqwest::Response) -> Received<T> {
        Received::new(response, self)
    }

    /// Converts a transport failure of a request you executed yourself into Concord's
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) error naming this endpoint, with the
    /// URL removed (synchronous; nothing to await, no impossible success branch).
    pub fn transport_error(&self, error: reqwest::Error) -> Error {
        Error::from_transport(self.operation, error)
    }

    /// Handles `response` as the endpoint would have: a non-2xx status is
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) and keeps the response unread; the format
    /// then checks the head and, if it accepts it, the body is read and decoded.
    ///
    /// # Errors
    ///
    /// [`Status`](crate::ErrorKind::Status) for a non-2xx response,
    /// [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the head (response kept
    /// unread), [`Transport`](crate::ErrorKind::Transport) if reading the body fails,
    /// [`Decode`](crate::ErrorKind::Decode) if it does not decode.
    pub async fn decode(self, response: reqwest::Response) -> Result<T, Error> {
        let operation = self.operation;
        if !response.status().is_success() {
            return Err(Error::from_status(operation, response));
        }
        match self.kind {
            Kind::Drain(unit) => {
                let mut response = response;
                while response
                    .chunk()
                    .await
                    .map_err(|e| Error::from_transport(operation, e))?
                    .is_some()
                {}
                Ok(unit())
            }
            Kind::Typed(head) => {
                // The head is borrowed only for the format's checks; the response is then moved,
                // either into the error (unread) or into `bytes()`.
                let step = match head(&ResponseHead::of(&response)) {
                    Ok(step) => step,
                    Err(e) => return Err(Error::from_metadata(operation, e, response)),
                };
                let body = response
                    .bytes()
                    .await
                    .map_err(|e| Error::from_transport(operation, e))?;
                step(body).map_err(|e| Error::from_decode(operation, e))
            }
            Kind::Open(head) => {
                // Same head discipline as `Typed`; the body is not polled, only handed over.
                let step = match head(&ResponseHead::of(&response)) {
                    Ok(step) => step,
                    Err(e) => return Err(Error::from_metadata(operation, e, response)),
                };
                Ok(step(Incoming::checked(response, operation)?))
            }
            Kind::Incoming(wrap) => Ok(wrap(Incoming::checked(response, operation)?)),
        }
    }

    /// Completes a request you executed yourself: a transport failure becomes a Concord
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) error (with the same redaction as
    /// [`Prepared::send`]), and a response is handled as by [`decode`](Self::decode).
    ///
    /// # Errors
    ///
    /// [`Transport`](crate::ErrorKind::Transport) for `Err` or a failed body read,
    /// [`Status`](crate::ErrorKind::Status) for a non-2xx response,
    /// [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the head,
    /// [`Decode`](crate::ErrorKind::Decode) if the body does not decode.
    ///
    /// ```no_run
    /// # async fn run(prepared: concord_core::Prepared<String>) -> Result<(), concord_core::Error> {
    /// let (client, mut request, decoder) = prepared.into_parts();
    /// *request.timeout_mut() = Some(std::time::Duration::from_secs(2));
    /// let text = decoder.finish(client.execute(request).await).await?;
    /// # let _ = text; Ok(()) }
    /// ```
    pub async fn finish(
        self,
        result: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<T, Error> {
        match result {
            Ok(response) => self.decode(response).await,
            Err(e) => Err(self.transport_error(e)),
        }
    }
}

impl<T> Clone for Decoder<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Decoder<T> {}

impl<T> fmt::Debug for Decoder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("endpoint", &self.operation.map(|op| op.name()))
            .finish()
    }
}
