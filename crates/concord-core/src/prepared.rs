//! A built native request plus the declared response handling.

use std::fmt;

use bytes::Bytes;

use crate::__private::{EndpointMeta, redacted_url};
use crate::codec::{CodecError, Decode, Incremental, ResponseHead};
use crate::error::Error;
use crate::response::{Incoming, Operation, Received, ResponseMeta};

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
    /// The URL came from the server ([`Prepared::follow`]): the endpoint's placement-based
    /// redaction does not describe it, so `Debug` withholds it whole.
    followed: bool,
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
            followed: false,
        }
    }

    /// The same request with a URL the server returned (a `Link: rel="next"` target, a `next`
    /// field): the endpoint's method, headers (credentials included), body, declared response
    /// handling and identity are kept; only the URL is replaced, verbatim.
    ///
    /// The URL must have the **same origin** (scheme, host, port) as the prepared one, so the
    /// endpoint's credentials never reach another host through a link; any further rule (a path
    /// prefix, a collection) is the SDK's to check before calling this. A cross-origin URL (a
    /// signed download) is a native request of its own, without these headers. `Debug` withholds
    /// a followed URL whole: the endpoint's redaction describes its own placements, not the
    /// server's.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Build`](crate::ErrorKind::Build) if `url` does not parse or has another origin;
    /// nothing is sent.
    pub fn follow(mut self, url: &str) -> Result<Self, Error> {
        let meta = self.decoder.operation.and_then(|op| op.meta());
        let parsed = reqwest::Url::parse(url)
            .map_err(|_| Error::build(meta, "followed URL", "does not parse"))?;
        if parsed.origin() != self.request.url().origin() {
            return Err(Error::build(
                meta,
                "followed URL",
                "has another origin than the endpoint's",
            ));
        }
        *self.request.url_mut() = parsed;
        self.followed = true;
        Ok(self)
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

/// Reads a body, keeping at most `limit` bytes: `ErrorKind::Limit` beyond, `Transport` on failure.
pub(crate) async fn read_limited(
    mut response: reqwest::Response,
    limit: usize,
    operation: Option<Operation>,
) -> Result<Bytes, Error> {
    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::from_transport(operation, e))?
    {
        if body.len() + chunk.len() > limit {
            return Err(Error::limit(operation, limit as u64));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
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

/// A client's **send policy**: what every generated `send()` returns. An SDK whose callers should
/// receive its own error type and response metadata, or whose requests must be signed after they
/// are built, implements it once and names the policy on the client; each endpoint's `send()` then
/// calls [`send`](Self::send) with the endpoint. No wrapper method per operation, no trait to
/// import, one terminal method.
///
/// The policy is a **value**:
///
/// - `send value;` names a required client value (`pub client S3(signer: Signer) { send signer; }`)
///   holding the state the policy needs — credentials, a region, a clock. It is not placed on the
///   wire; each `send()` clones it (keep it cheap: an `Arc` inside), so it must be `Clone`, and
///   `Send + Sync` for the future to be `Send`.
/// - `send Policy;` names a stateless type; each `send()` uses `Policy::default()`.
///
/// ```ignore
/// #[derive(Default)]
/// pub struct Policy;
/// impl<T: Send> concord::SendWith<T> for Policy {
///     type Output = Result<WithMeta<T>, SdkError>;
///     async fn send<E: concord::Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
///         let received = endpoint.prepare()?.receive().await?;
///         let meta = Meta::capture(&received.head());
///         Ok(WithMeta { value: received.decode().await?, meta })
///     }
/// }
/// // declaration: `pub client Api { base "…"; send Policy; … }`
/// // caller: `api.items().get(7).send().await?` returns `Result<WithMeta<Item>, SdkError>`
/// ```
///
/// Without `send`, the generated `send()` is `prepare()?.send()` (Concord's `Error`). `prepare()`
/// is unchanged either way, for callers that need the native (unsigned) request.
#[diagnostic::on_unimplemented(
    message = "the send policy `{Self}` does not send a `{T}` output",
    label = "named by the client's `send` statement; it needs `{Self}: SendWith<{T}>`",
    note = "a send policy implements `concord::SendWith<T>` for every output `T` its endpoints declare (usually generically: `impl<T: Send> SendWith<T> for Policy`)"
)]
pub trait SendWith<T> {
    /// What `send()` returns.
    type Output;

    /// Sends the endpoint's request according to the policy. `self` is the policy value: a client
    /// value named by `send value;` (credentials, a signer, a region, a clock), cloned for each
    /// call, or `P::default()` for `send P;`.
    fn send<E>(&self, endpoint: E) -> impl std::future::Future<Output = Self::Output> + Send
    where
        E: Endpoint<T> + Send;
}

/// A send policy that is also told **which operation** it sends: the sibling of [`SendWith`] for a
/// client that declares `operations Name;`.
///
/// That statement makes `api!` generate `Name`, an enum with one variant per endpoint of the
/// client, and makes each generated `send()` call [`send`](Self::send) with its endpoint's variant
/// next to the endpoint. The policy classifies the operation with a `match` over `Name` **without
/// a wildcard arm**, so an endpoint added to the declaration and not classified is a compile error
/// of the SDK (E0004, naming the variant), not a silent default at run time. The enum only
/// identifies the operation; what each one means (a scope, a quota bucket, whether it is signed
/// or retried) is the policy's.
///
/// ```ignore
/// // declaration: `pub client Api(policy: Policy) { ...; send policy; operations ApiOperation; ... }`
/// impl<T: Send> concord::SendOperation<T, ApiOperation> for Policy {
///     type Output = Result<T, SdkError>;
///     async fn send<E: concord::Endpoint<T> + Send>(
///         &self,
///         operation: ApiOperation,
///         endpoint: E,
///     ) -> Self::Output {
///         let write = match operation {
///             ApiOperation::ItemsGet | ApiOperation::ItemsList => false,
///             ApiOperation::ItemsUpdate => true,
///         };
///         // ...
///     }
/// }
/// ```
///
/// Everything else is as for [`SendWith`]: the policy value (`send value;` or `send P;`), the
/// `send P -> Template;` result, [`Sends`], and `prepare()` bypassing the policy. A client without
/// `operations` keeps [`SendWith`].
#[diagnostic::on_unimplemented(
    message = "the send policy `{Self}` does not send a `{T}` output identified by `{O}`",
    label = "the client declares `operations` and `send`: it needs `{Self}: SendOperation<{T}, {O}>`",
    note = "a client with `operations Name;` passes each endpoint's `Name` variant to its policy: implement `concord::SendOperation<T, Name>` (usually generically over `T`) instead of `SendWith<T>`, and classify the operation with a `match` without a wildcard arm"
)]
pub trait SendOperation<T, O> {
    /// What `send()` returns.
    type Output;

    /// Sends the endpoint's request according to the policy. `operation` is the endpoint's
    /// variant of the client's generated operation enum; `self` is the policy value, as for
    /// [`SendWith::send`].
    fn send<E>(
        &self,
        operation: O,
        endpoint: E,
    ) -> impl std::future::Future<Output = Self::Output> + Send
    where
        E: Endpoint<T> + Send;
}

/// The generated `send()` of an endpoint whose client names a send policy (`send P;` or
/// `send value;`), seen generically: each such endpoint implements `Sends<O>` for `O`, the
/// policy's [`SendWith::Output`] (or [`SendOperation::Output`]) for its output. An SDK helper
/// written once over several endpoints — every page of a listing, a batch — then sends through
/// the client's policy (its errors, metadata, signing, the policy value cloned from the client)
/// exactly as callers do, without being handed the policy:
///
/// ```ignore
/// async fn all_pages<E>(first: E) -> Result<Vec<Item>, SdkError>
/// where
///     E: concord::Sends<Result<Response<Page>, SdkError>> + ListOptions + Clone,
/// { /* first.clone().set_cursor(..).send().await? ... */ }
/// ```
///
/// The output is a type parameter, as in [`Endpoint`], so a public client may keep private
/// output types. A client without a policy generates no `Sends`: its `send()` is
/// `prepare()?.send()`, which generic code reaches through [`Endpoint`].
pub trait Sends<O>: Sized {
    /// The endpoint's generated `send()`.
    fn send(self) -> impl std::future::Future<Output = O> + Send;
}

impl<T> fmt::Debug for Prepared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.decoder.operation.and_then(|op| op.meta());
        let headers = DebugHeaders(self.request.headers());
        f.debug_struct("Prepared")
            .field("endpoint", &meta.map(|m| m.name))
            .field("method", &self.request.method().as_str())
            .field(
                "url",
                &DebugUrl(self.request.url(), meta.filter(|_| !self.followed)),
            )
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
    /// The declared bound of a finite body read (`limit N;`), applied by `decode` and `send`.
    limit: Option<usize>,
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
            limit: None,
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
            limit: None,
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
            limit: None,
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
            limit: None,
        }
    }
}

impl<T> Decoder<T> {
    pub(crate) fn typed(meta: &'static EndpointMeta, head: HeadStep<T>) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Typed(head),
            limit: None,
        }
    }

    pub(crate) fn drained(meta: &'static EndpointMeta, unit: fn() -> T) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Drain(unit),
            limit: None,
        }
    }

    pub(crate) fn opened(meta: &'static EndpointMeta, head: OpenHeadStep<T>) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Open(head),
            limit: None,
        }
    }

    pub(crate) fn incoming_for(meta: &'static EndpointMeta, wrap: fn(Incoming) -> T) -> Self {
        Self {
            operation: Some(Operation::endpoint(meta)),
            kind: Kind::Incoming(wrap),
            limit: None,
        }
    }

    /// This handling with its finite body read bounded by `limit` bytes: a `-> T` output whose
    /// body is longer is [`ErrorKind::Limit`](crate::ErrorKind::Limit) and is not decoded (nothing
    /// more is read), on every path that applies the handling (`decode`, `finish`,
    /// [`Received::decode`], [`Prepared::send`]). [`Received::decode_limited`] still replaces it
    /// for one call. A drain, a stream reader or an `Incoming` is unaffected. Generated endpoints
    /// carry the bound declared with `limit N;`.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// The finite body bound this handling applies (`None`: unbounded).
    pub fn limit(&self) -> Option<usize> {
        self.limit
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
    /// A request whose body is buffered and empty is sent with `Content-Length: 0` (unless it
    /// already has one): reqwest omits the field, and an empty PUT/POST would otherwise look like
    /// a request without content. The rule is applied here, to the body actually sent, so a body
    /// replaced natively after `into_parts` is framed by its own length; a request executed with
    /// `reqwest::Client::execute` directly gets reqwest's framing.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) only, naming the endpoint.
    pub async fn execute(
        self,
        client: &reqwest::Client,
        mut request: reqwest::Request,
    ) -> Result<Received<T>, Error> {
        frame_empty_body(&mut request);
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
        let limit = self.limit;
        self.decode_within(response, limit).await
    }

    /// [`decode`](Self::decode) with a finite body read bounded by `limit` (`-> T` outputs; a drain,
    /// a stream or an `Incoming` is unaffected).
    pub(crate) async fn decode_within(
        self,
        response: reqwest::Response,
        limit: Option<usize>,
    ) -> Result<T, Error> {
        self.decode_keeping(response, limit, false)
            .await
            .map(|(value, _)| value)
    }

    /// The declared handling, also returning the response head. A finite output or a drain moves
    /// the head out of the response once the format accepted it (an error while reading or
    /// decoding the body carries it; nothing is cloned); a reader or an `Incoming` keeps its
    /// response, so its head is a copy when `keep` is set and holds no header otherwise.
    pub(crate) async fn decode_keeping(
        self,
        mut response: reqwest::Response,
        limit: Option<usize>,
        keep: bool,
    ) -> Result<(T, ResponseMeta), Error> {
        let operation = self.operation;
        if !response.status().is_success() {
            return Err(Error::from_status(operation, response));
        }
        match self.kind {
            Kind::Drain(unit) => {
                let head = ResponseMeta::take(&mut response, operation);
                loop {
                    match response.chunk().await {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(e) => return Err(Error::from_transport(operation, e).with_head(head)),
                    }
                }
                Ok((unit(), head))
            }
            Kind::Typed(step) => {
                // The head is borrowed only for the format's checks; the response is then moved,
                // either into the error (unread) or into the body read, its head moved out first.
                let step = match step(&ResponseHead::of(&response)) {
                    Ok(step) => step,
                    Err(e) => return Err(Error::from_metadata(operation, e, response)),
                };
                let head = ResponseMeta::take(&mut response, operation);
                let body = match limit {
                    None => response
                        .bytes()
                        .await
                        .map_err(|e| Error::from_transport(operation, e)),
                    Some(limit) => read_limited(response, limit, operation).await,
                };
                let body = match body {
                    Ok(body) => body,
                    Err(e) => return Err(e.with_head(head)),
                };
                match step(body) {
                    Ok(value) => Ok((value, head)),
                    Err(e) => Err(Error::from_decode(operation, e).with_head(head)),
                }
            }
            Kind::Open(step) => {
                // Same head discipline as `Typed`; the body is not polled, only handed over.
                let step = match step(&ResponseHead::of(&response)) {
                    Ok(step) => step,
                    Err(e) => return Err(Error::from_metadata(operation, e, response)),
                };
                let head = ResponseMeta::kept(&response, operation, keep);
                Ok((step(Incoming::checked(response, operation)?), head))
            }
            Kind::Incoming(wrap) => {
                let head = ResponseMeta::kept(&response, operation, keep);
                Ok((wrap(Incoming::checked(response, operation)?), head))
            }
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

/// `Content-Length: 0` for a buffered empty body (RFC 9110 §8.6: content of length zero is still
/// content), which reqwest omits; streamed bodies and requests without a body are left alone.
fn frame_empty_body(request: &mut reqwest::Request) {
    use reqwest::header::{CONTENT_LENGTH, HeaderValue};
    let empty = request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .is_some_and(<[u8]>::is_empty);
    if empty && !request.headers().contains_key(CONTENT_LENGTH) {
        request
            .headers_mut()
            .insert(CONTENT_LENGTH, HeaderValue::from_static("0"));
    }
}
