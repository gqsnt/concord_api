//! The transport: what actually sends a request.

use crate::error::Class;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// What actually sends a request and returns its response: [`Http`], a `reqwest::Client`,
/// a fake, or your own transport.
///
/// A client has one transport, set with `with_transport(yours)`; the last call wins.
/// Every attempt and every page goes through the client's middleware first, in order,
/// and reaches the transport with a [`TryInfo`] that names the endpoint and the attempt.
/// A signature, a token that is renewed and a limit shared by the calls of a client are each a
/// [`Middleware`](crate::Middleware), not a transport: Concord ships `concord::auth::Renewed`,
/// `concord::auth::Rotating`, `concord::sign::SigV4` and `concord::limit::Limit`.
///
/// Concord counts each trip through the chain as one attempt. Any resend that the HTTP
/// library of a transport makes internally is below that count and follows that library's
/// settings; [`Http`] lets `reqwest` resend nothing on its own.
/// The idle timeout starts when the request is handed to the transport, after middleware.
/// `timeout` is checked there too: an expired call is never handed over.
/// All time spent inside a transport counts as transport time. A transport must
/// preserve what the description’s last middleware signed.
///
/// A response should carry its own URL (for an `http::Response`, use
/// `reqwest::ResponseBuilderExt::url`). If it carries reqwest's placeholder URL,
/// Concord uses the request's URL, including when resolving a relative next URL.
///
/// A `tower` service is a transport too: `concord::tower::Tower`, with the `tower` feature.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot send a request",
    note = "a `reqwest::Client`, `concord::Http`, a test transport, or a type that implements `concord::Transport`"
)]
pub trait Transport: Send + Sync + 'static {
    /// Sends the request.
    ///
    /// Not cancel safe, like `reqwest::Client::execute`: if the future is dropped, the
    /// request may have reached the server. After a drop, Concord starts no further attempt,
    /// but a request that was already handed to the transport may still be sent.
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send;
}

/// Your own `reqwest::Client` sends the request, keeping its own settings for redirects,
/// decompression and internal retries. Concord's middleware runs before it.
/// Concord does not retry an unmarked, non-idempotent call after a failed connection, but
/// `reqwest`'s default resends a request whose body is in memory or empty, up to twice and
/// whatever its method, after a remote HTTP/2 `REFUSED_STREAM` reset or a `GOAWAY` with
/// `NO_ERROR`, below Concord's count of attempts; [`Http`] does not. Build the client with
/// `.retry(reqwest::retry::never())` to leave every repetition to Concord's engine. Its
/// failures never state that nothing was sent ([`TransportError::not_sent`]).
impl Transport for reqwest::Client {
    // Not `async`: the send is recorded when it is created, before its first poll.
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send {
        counted(info.sends(), move || async move {
            self.execute(request).await.map_err(TransportError::from)
        })
    }
}

impl<T: Transport> Transport for Arc<T> {
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send {
        (**self).send(request, info)
    }
}

/// Give a response without its own URL the URL of the request that produced it.
#[doc(hidden)]
pub fn __response_at(response: reqwest::Response, url: reqwest::Url) -> reqwest::Response {
    if response.url().as_str() != "http://no.url.provided.local/" {
        return response;
    }
    use reqwest::ResponseBuilderExt;
    let response: http::Response<reqwest::Body> = response.into();
    let (mut parts, body) = response.into_parts();
    let mut builder = http::Response::builder().url(url);
    if let Some(extensions) = builder.extensions_mut() {
        parts.extensions.extend(std::mem::take(extensions));
    }
    http::Response::from_parts(parts, body).into()
}

/// A request failure whose fixed wording is independent of the HTTP library.
#[derive(Clone, Copy)]
pub(crate) enum RequestFailure {
    Head,
    Body,
}

/// A failure with no response.
pub struct TransportError {
    class: Class,
    request_failure: Option<RequestFailure>,
    clock: Option<crate::answer::BodyTimeout>,
    // Discard the source after retaining its value-free I/O classification.
    connection_kind: Option<std::io::ErrorKind>,
    // Some(None): the HTTP connect clock is known, but its configured length is not.
    connect_timeout: Option<Option<std::time::Duration>>,
    // The transport stated that no byte of the request was written: see `not_sent`.
    not_sent: bool,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl std::fmt::Debug for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never call Debug on the source, even when the caller requests pretty output.
        write!(
            f,
            "TransportError {{ class: {:?}, source: .. }}",
            self.class
        )
    }
}

impl TransportError {
    /// Creates a failure of this class.
    pub fn new(class: Class) -> TransportError {
        TransportError {
            class,
            request_failure: None,
            clock: None,
            connection_kind: None,
            connect_timeout: None,
            not_sent: false,
            source: None,
        }
    }

    /// Creates the error for a failed connection.
    ///
    /// It says nothing about what was written: a connection can fail after the request, or
    /// part of it, was sent. When your transport knows that nothing was, add
    /// [`not_sent`](TransportError::not_sent).
    pub fn connect() -> TransportError {
        TransportError::new(Class::Connect)
    }

    /// Creates the error for a timeout of the transport's own.
    pub fn timeout() -> TransportError {
        TransportError::new(Class::Timeout)
    }

    /// Creates the error for a middleware layer or transport that rejects the request before
    /// sending. The attempts end here, and this blocked error is the result.
    ///
    /// After a response has arrived (for example a `401`), return that
    /// response or another failure instead.
    pub fn blocked() -> TransportError {
        TransportError::new(Class::Blocked)
    }

    pub(crate) fn upload(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            request_failure: Some(RequestFailure::Body),
            source: Some(source.into()),
            ..Self::new(Class::Other)
        }
    }

    /// States that no byte of the request was written to the network during this send, and
    /// that none can still be written after it returns: the connection could not be made,
    /// or the transport stopped before handing the request over.
    ///
    /// The call's error then reports it with
    /// [`Error::request_not_sent`](crate::Error::request_not_sent), unless something else
    /// was sent during the call. A caller may rely on it to reuse what the request carried,
    /// such as a single-use refresh token: state it only when it is certain. A connection
    /// that broke after it was made, a timeout that may have expired after a write, a send
    /// that another task may still be making, and an error of your HTTP library that you
    /// did not classify yourself are not certain. [`Http`] states it for a connection that
    /// it could not make; [`TransportError::from`] for a `reqwest::Error` never does. The
    /// retry policy does not read it: the class decides, as before.
    ///
    /// ```
    /// # use concord_core as concord;
    /// use concord::transport::TransportError;
    /// // Checked before any socket was opened.
    /// let offline = TransportError::connect().not_sent();
    /// ```
    #[must_use]
    pub fn not_sent(mut self) -> TransportError {
        self.not_sent = true;
        self
    }

    /// Whether this failure states that nothing was written: `not_sent`, or `blocked`.
    pub(crate) fn states_not_sent(&self) -> bool {
        self.not_sent || self.class == Class::Blocked
    }

    /// Sets the cause, for the `source()` of the error that the caller receives.
    ///
    /// It is shown as it is: pass a source that holds no URL and no secret. For a
    /// `reqwest::Error`, use `TransportError::from(error)` instead: it removes the URL.
    pub fn with_source(
        mut self,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> TransportError {
        self.source = Some(source.into());
        self
    }

    pub(crate) fn clock(timeout: crate::answer::BodyTimeout) -> Self {
        Self {
            clock: Some(timeout),
            ..Self::timeout()
        }
    }

    pub(crate) fn clock_timeout(&self) -> Option<crate::answer::BodyTimeout> {
        self.clock
    }

    /// Move the safe source into the error of the call, without adding a wrapper.
    pub(crate) fn into_parts(self) -> (Class, Option<Box<dyn std::error::Error + Send + Sync>>) {
        (self.class, self.source)
    }

    /// The closed-list I/O kind, retained even when a decode source is discarded.
    pub(crate) fn connection_kind(&self) -> Option<std::io::ErrorKind> {
        self.connection_kind
    }

    /// A malformed answer head or a failure of the caller's upload stream.
    pub(crate) fn request_failure(&self) -> Option<RequestFailure> {
        self.request_failure
    }

    pub(crate) fn connect_timeout(&self) -> Option<Option<std::time::Duration>> {
        self.connect_timeout
    }

    /// Returns the class of the failure.
    pub fn class(&self) -> Class {
        self.class
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the transport failed ({:?})", self.class)
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(crate::body::repeatable::original_source)
    }
}

impl From<reqwest::Error> for TransportError {
    /// Classifies the failure, strips its URL, and discards unsafe status or decode sources.
    fn from(error: reqwest::Error) -> TransportError {
        let mut source = std::error::Error::source(&error);
        let mut connection_kind = None;
        let mut request_failure = None;
        while let Some(cause) = source {
            if error.is_request() && request_failure.is_none() {
                if let Some(error) = cause.downcast_ref::<hyper::Error>() {
                    request_failure = if error.is_parse() {
                        Some(RequestFailure::Head)
                    } else if error.is_user() {
                        Some(RequestFailure::Body)
                    } else {
                        None
                    };
                }
            }
            if let Some(error) = cause.downcast_ref::<std::io::Error>() {
                use std::io::ErrorKind::*;
                if connection_kind.is_none()
                    && matches!(
                        error.kind(),
                        ConnectionRefused
                            | ConnectionReset
                            | ConnectionAborted
                            | BrokenPipe
                            | NotConnected
                            | UnexpectedEof
                    )
                {
                    connection_kind = Some(error.kind());
                }
            }
            source = cause.source();
        }
        let class = if request_failure.is_some() {
            Class::Other
        } else if error.is_timeout() {
            Class::Timeout
        } else if error.is_connect() || connection_kind.is_some() || error.is_request() {
            Class::Connect
        } else if error.is_body() || error.is_decode() {
            Class::Body
        } else {
            Class::Other
        };
        let mut failure = TransportError::new(class);
        failure.request_failure = request_failure;
        failure.connection_kind = connection_kind;
        if request_failure.is_none() && error.is_timeout() && error.is_connect() {
            failure.connect_timeout = Some(None);
        }
        if error.is_status() || error.is_decode() {
            failure
        } else {
            failure.with_source(error.without_url())
        }
    }
}

/// What a middleware layer or transport is told about the request: which endpoint, and which
/// attempt.
///
/// A copy kept after the attempt has ended, by a record of your own or by a task, stops the
/// call's error from stating that nothing was sent
/// ([`Error::request_not_sent`](crate::Error::request_not_sent)), since a send made with it
/// could still write. To keep what it says, keep [`endpoint`](TryInfo::endpoint) and
/// [`number`](TryInfo::number), or `TryInfo::new(info.endpoint(), info.number())`.
///
/// Before a middleware layer or a transport states [`TransportError::not_sent`] or
/// [`TransportError::blocked`], or forwards such a failure, it accounts for every send that
/// it made or still holds, through the chain or beside it: a send that it makes with a
/// transport of its own, without this `TryInfo`, is not seen by Concord. A false statement
/// can make a credential send a refresh token twice, and a server that sees a token used
/// twice may revoke every session of the account.
#[derive(Clone)]
pub struct TryInfo {
    endpoint: &'static str,
    number: u32,
    run: Option<crate::__private::Run>,
    conditional: Option<Arc<std::sync::atomic::AtomicBool>>,
    sends: Option<Arc<Sends>>,
    pub(crate) repeatable_body: bool,
}

/// The sends that reached a transport during one attempt of a call, and how many of them
/// ended with a failure stating that nothing was written.
///
/// The engine creates one for each attempt and owns it. The `Next` that it hands to the
/// middleware borrows it, and every `Next::send` is counted there when it is called,
/// whatever `TryInfo` a middleware passes with it; the layers below receive that `TryInfo`
/// bound to the record again. The attempt's `TryInfo` holds a clone, through which a
/// `Next::to` chain, [`Http`], `reqwest::Client` and [`fake`] count their sends too, so that
/// a send beside the chain or a transport of the author's that wraps them is checked. A send
/// that was dropped before it returned, or before its first poll, is counted as started and
/// never as unsent.
///
/// The engine reads it only once it holds the record alone ([`Sends::none_written`]): after
/// the attempt, when no borrow through `Next` remains and every clone held by a `TryInfo`
/// (kept by a middleware, given to a task) has been dropped. Nothing can then change it.
#[derive(Default)]
pub(crate) struct Sends {
    started: AtomicU32,
    unsent: AtomicU32,
}

impl Sends {
    fn add(count: &AtomicU32) {
        // Saturating: a count that reached the maximum never compares equal below.
        let mut now = count.load(Ordering::SeqCst);
        while let Some(next) = now.checked_add(1) {
            match count.compare_exchange_weak(now, next, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(current) => now = current,
            }
        }
    }

    /// No send of this attempt can have written a byte of the request, nor can one still:
    /// `false` while anything else holds the record.
    pub(crate) fn none_written(record: Arc<Sends>) -> bool {
        let Some(sends) = Arc::into_inner(record) else {
            return false;
        };
        let started = sends.started.into_inner();
        started != u32::MAX && started == sends.unsent.into_inner()
    }
}

/// Counts one send in this record, if there is one: as started now, before `send` runs any
/// code of the author's, and as unsent when it ends with a failure stating that nothing was
/// written. A panic in `send` or in its future leaves it started.
pub(crate) fn counted<F>(
    sends: Option<Arc<Sends>>,
    send: impl FnOnce() -> F,
) -> impl Future<Output = Result<reqwest::Response, TransportError>>
where
    F: Future<Output = Result<reqwest::Response, TransportError>>,
{
    if let Some(sends) = &sends {
        Sends::add(&sends.started);
    }
    let send = send();
    async move {
        let result = send.await;
        if let Some(sends) = &sends {
            if result.as_ref().is_err_and(TransportError::states_not_sent) {
                Sends::add(&sends.unsent);
            }
        }
        result
    }
}

impl std::fmt::Debug for TryInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TryInfo")
            .field("endpoint", &self.endpoint)
            .field("number", &self.number)
            .finish_non_exhaustive()
    }
}

impl TryInfo {
    /// Creates the information; for generated code and for tests.
    pub fn new(endpoint: &'static str, number: u32) -> TryInfo {
        TryInfo {
            endpoint,
            number,
            run: None,
            conditional: None,
            sends: None,
            repeatable_body: false,
        }
    }

    pub(crate) fn with_conditional(mut self, fallback: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.conditional = Some(fallback);
        self
    }

    pub(crate) fn unclaimed_conditional(&self, status: http::StatusCode) -> bool {
        if status == http::StatusCode::NOT_MODIFIED {
            if let Some(fallback) = &self.conditional {
                fallback.store(true, std::sync::atomic::Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    pub(crate) fn for_run(run: crate::__private::Run, number: u32, sends: Arc<Sends>) -> Self {
        Self {
            endpoint: run.endpoint(),
            number,
            run: Some(run),
            conditional: None,
            sends: Some(sends),
            repeatable_body: false,
        }
    }

    pub(crate) fn run(&self) -> Option<&crate::__private::Run> {
        self.run.as_ref()
    }

    /// The record of this attempt's sends; `None` outside an attempt of the engine.
    pub(crate) fn sends(&self) -> Option<Arc<Sends>> {
        self.sends.clone()
    }

    /// The same information, carrying this record of the engine's attempt.
    pub(crate) fn bound_to(mut self, record: &Arc<Sends>) -> Self {
        self.sends = Some(record.clone());
        self
    }

    /// Returns the name of the endpoint: `riot.matches.get`.
    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// Returns the number of the attempt: 1 for the first.
    pub fn number(&self) -> u32 {
        self.number
    }
}

/// A transport built from a function, for tests: the function receives the request that
/// would be sent.
///
/// ```
/// # use concord_core as concord;
/// let transport = concord::fake(|request| {
///     assert_eq!(request.url().path(), "/api/tasks/7");
///     Ok(concord::reply(200, r#"{ "id": 7 }"#))
/// });
/// # use futures_util::FutureExt;
/// # async {
/// let request = concord::reqwest::Request::new(concord::http::Method::GET,
///     "https://example.test/api/tasks/7".parse()?);
/// let response = concord::Transport::send(&transport, request,
///     concord::transport::TryInfo::new("tasks.get", 1)).await?;
/// assert_eq!(response.status(), 200);
/// assert_eq!(response.bytes().await?, r#"{ "id": 7 }"#);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # }.now_or_never().unwrap().unwrap();
/// ```
///
/// A response should carry its URL; if it has reqwest's placeholder, the engine
/// assumes that it came from the URL of this request.
pub fn fake<F>(answer: F) -> Fake<F>
where
    F: Fn(&reqwest::Request) -> Result<reqwest::Response, TransportError> + Send + Sync + 'static,
{
    Fake(answer)
}

/// The transport that [`fake`] returns.
pub struct Fake<F>(F);

impl<F> std::fmt::Debug for Fake<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fake")
    }
}

impl<F> Transport for Fake<F>
where
    F: Fn(&reqwest::Request) -> Result<reqwest::Response, TransportError> + Send + Sync + 'static,
{
    // Not `async`: the send is recorded when it is created, before the callback runs.
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send {
        counted(info.sends(), move || async move { (self.0)(&request) })
    }
}

/// Marks a fixture with no real response URL.
#[derive(Clone, Debug)]
pub(crate) struct SyntheticReply;

/// A response built in memory.
///
/// Its URL is reqwest's placeholder, `http://no.url.provided.local/`.
///
/// # Panics
///
/// Panics when `status` is not a status code (100 to 999). This is a helper for tests.
pub fn reply(status: u16, body: impl Into<reqwest::Body>) -> reqwest::Response {
    reply_with(status, &[], body)
}

/// A response built in memory, with headers.
///
/// Its URL is reqwest's placeholder, `http://no.url.provided.local/`.
/// Bodies may stream, fail or stay pending; this helper does not read them.
///
/// # Panics
///
/// Panics when `status` is not a status code, or a header is not valid. This is a helper for tests.
// This test-fixture helper explicitly documents invalid fixtures as programmer errors.
// PANIC-SAFETY: Known fixture-only exception: invalid fixtures deliberately panic; c9_2 should_panic tests require this behavior.
#[allow(clippy::expect_used)]
pub fn reply_with(
    status: u16,
    headers: &[(&str, &str)],
    body: impl Into<reqwest::Body>,
) -> reqwest::Response {
    let mut response = http::Response::builder()
        .status(status)
        .extension(SyntheticReply);
    for (name, value) in headers {
        response = response.header(*name, *value);
    }
    response
        .body(body.into())
        // PANIC-SAFETY: Known fixture-only exception: reply_with rejects invalid status/headers by panic; existing c9_2 tests require it.
        .expect("reply requires a valid status and valid headers")
        .into()
}

/// Concord's HTTP client, and the default transport.
///
/// It follows no redirect. Decompression is off by default; [`Http::with`] may enable it.
/// The `gzip` and `deflate` Cargo features make those decoders available; neither
/// enables decoding or sends `Accept-Encoding` by itself. The `socks` feature
/// permits SOCKS proxies configured through [`Http::with`]. These features are
/// not defaults. See [`Http::with`] for a compression example.
///
/// It puts a request on the wire once per attempt: `reqwest`'s own resends are off,
/// including its default resend of an HTTP/2 request that the server refused with
/// `REFUSED_STREAM` or `GOAWAY`. A request is repeated only by Concord's engine, when the
/// retry policy and the request allow it, and each repeat is an attempt. One resend
/// remains below the count of attempts: when a reused connection turns out to be
/// closed and hands the request back before writing any of it, the connection pool
/// sends it on another connection.
///
/// When it cannot make a connection (refused, a host name not resolved, a TLS handshake
/// or a proxy that failed, its connect timeout), its error states that nothing was sent:
/// see [`TransportError::not_sent`] and [`Error::request_not_sent`](crate::Error::request_not_sent).
/// Any other failure states nothing, including one whose chain of causes it does not
/// expect, and every failure over HTTP/3: that is the safe side, not a proof that the
/// request was sent.
///
/// Both promises rest on how `reqwest`, `hyper-util`, `hyper` and `h2` behave, read in the
/// versions of Concord's own `Cargo.lock` (reqwest 0.13.5, hyper-util 0.1.21, hyper 1.12.0,
/// h2 0.4.20), which its tests run with. The `Cargo.lock` of your application decides the
/// versions that it builds with.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    connect_timeout: Option<std::time::Duration>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http").finish_non_exhaustive()
    }
}

impl Http {
    fn classify_error(&self, error: reqwest::Error) -> TransportError {
        let not_sent = connector_failed(&error);
        let mut failure = TransportError::from(error);
        if failure.connect_timeout.is_some() {
            failure.connect_timeout = Some(self.connect_timeout);
        }
        failure.not_sent = not_sent && failure.request_failure.is_none();
        failure
    }

    /// Creates an HTTP client with Concord's defaults.
    pub fn new() -> Result<Http, reqwest::Error> {
        let mut http = Http::with(|builder| builder)?;
        http.connect_timeout = Some(std::time::Duration::from_secs(30));
        Ok(http)
    }

    /// Creates Concord's HTTP client with your own settings: a certificate, a proxy.
    ///
    /// Before `f` runs, the defaults are set, and `f` may change them: 30 seconds to connect,
    /// no `Referer`, no decompression (a body is passed on as it arrived). The closure may
    /// enable decompression; readers then receive, and their limits count, the bytes that
    /// the transport returns. After `f`, two settings are applied that `f` cannot change:
    /// no redirect is followed (a `3xx` is a response), and `reqwest` resends nothing on its
    /// own, so a retry policy set with `builder.retry(…)` is replaced. To keep `reqwest`'s
    /// own redirects or retries, pass your own `reqwest::Client` as the transport instead.
    ///
    /// To make gzip available, add it to your dependency's features:
    ///
    /// ```toml
    /// concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["gzip"] }
    /// ```
    ///
    /// Then enable decoding for this client and pass it to your generated
    /// client's `with_transport(http)`:
    ///
    /// ```no_run
    /// # #[cfg(feature = "gzip")]
    /// # fn main() -> Result<(), concord_core::reqwest::Error> {
    /// # use concord_core as concord;
    /// let http = concord::Http::with(|builder| builder.gzip(true))?;
    /// # let _ = http;
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "gzip"))]
    /// # fn main() {}
    /// ```
    ///
    /// A reader's limit counts the decoded bytes, not the compressed size on
    /// the wire. A gzip response that expands past the default 10 MiB reader
    /// limit is a Mismatch, even when its compressed body is small.
    pub fn with(
        f: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Http, reqwest::Error> {
        // Defaults first: the closure may change any of them.
        let builder = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .referer(false)
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate();
        // Then what the promises need, which the closure cannot change: no resend of
        // reqwest's own (`retry::never()` replaces the whole policy, for every protocol),
        // and no redirect.
        //
        // reqwest 0.13.5 offers no finer rule. Its default classifier, `ProtocolNacks`, is
        // installed only by the private `retry::Builder::default()` (`retry.rs:195`); every
        // public builder starts from `Never` (`77`, `88`, `105`), and `classify_fn` replaces
        // the classifier. A classifier of ours sees `ReqRep`: the method, the URI, a status
        // and the error (`407` to `423`). A refusal has no status, `is_request()` covers every
        // failure without a response, and `is_connect()` names the connector, not a refusal;
        // neither the error's text (h2 prints the same text for other errors, and a GOAWAY may
        // append debug data: h2 `error.rs:162`) nor the depth of its `source()` chain
        // identifies one. Telling one apart needs a downcast to `h2::Error`
        // (`retry.rs:273` to `315`), a dependency this crate does not have. The one public
        // workaround, `Body::wrap` on every body that must not be resent so that it cannot be
        // cloned (`body.rs:138`), relies on reqwest's stock policy, which `f` may have
        // replaced; and `ClientBuilder` is not `Clone`, so `f` cannot build a second client.
        let f = move |builder: reqwest::ClientBuilder| f(builder).retry(reqwest::retry::never());
        let client = f(builder)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        // Reqwest has no getter for the builder's configured timeout. The closure
        // may have replaced it, so its length is deliberately unknown here.
        Ok(Http {
            client,
            connect_timeout: None,
        })
    }
}

impl Transport for Http {
    // Not `async`: the send is recorded when it is created, before its first poll.
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send {
        counted(info.sends(), move || async move {
            self.client
                .execute(request)
                .await
                .map_err(|error| self.classify_error(error))
        })
    }
}

/// Whether this error of `Http`'s own client is the failure of its connector, which proves
/// that no byte of the request was written during this send (`TransportError::not_sent`).
///
/// A false "not sent" lets a caller reuse a single-use credential, which a server may answer
/// by revoking the account's sessions, so this holds only on these premises, read in the
/// versions that Concord locks: reqwest 0.13.5, hyper-util 0.1.21, hyper 1.12.0, h2 0.4.20.
/// The tests named beside a premise check it on the bytes that a local server received.
///
/// 1. One dispatch per send. `Http::with` sets `reqwest::retry::never()` after the author's
///    closure: reqwest never sends a copy of the request (`retry.rs:88`, `469`).
///    Test: `c4_1_reqwest_resends_a_refused_post_and_http_does_not`.
/// 2. The direct source of a request error is hyper-util's error: reqwest wraps it in
///    `HyperService::call` (`async_impl/client.rs:156`); its total and read timeouts give
///    `TimedOut` instead (`client.rs:3100`, `3108`), which has no source.
/// 3. hyper-util's `Connect` kind comes only from the connector's future or a pool checkout
///    (`client/legacy/client.rs:378`, `464`, `473`, `517`): before the request is handed to a
///    connection in that round of its loop. The connector receives the URI, never the
///    request (`client.rs:516`). Its connect timeout is inside it (reqwest `connect.rs:904`,
///    `ConnectorService::call` at `928`), so a connect timeout is the connector's failure too.
///    Tests: `c9_5_http_states_not_sent_for_a_refused_connection` and
///    `c9_5_http_states_not_sent_after_the_connect_timeout`.
/// 4. An earlier round of that loop on a reused connection resends only a request that
///    hyper handed back unstarted (`client.rs:248`, `324`): hyper returns the request only
///    when it failed before serializing it (`client/conn/http1.rs:258`,
///    `client/conn/http2.rs:190`).
/// 5. The other errors of hyper-util have no cause, a message, or a `hyper::Error`
///    (`client.rs:330`, `336`, `532`, `1705`, `1709`), and a failure of the request body
///    after dispatch reaches it through a `hyper::Error`. `reqwest::Error::is_connect`
///    finds a `Connect` error anywhere in the chain; with no `hyper::Error` and no other
///    `reqwest::Error` in it, by 2 and this premise the one it found is the first cause.
///    A `Connect` error deeper than that belongs to something else, such as the body
///    stream of an upload that failed with another request's error. Tests:
///    `c9_5_http_does_not_state_not_sent_after_the_request_was_written` and
///    `c9_5_http_does_not_state_not_sent_for_a_nested_connect_error`.
///
/// 6. No redirect: `Http::with` forces `redirect::Policy::none()` after the closure, so no
///    second request follows a response within one send.
///
/// The check is narrower than "the first cause is hyper-util's `Connect` error": a genuine
/// connector failure whose chain holds another `reqwest::Error`, or a chain longer than the
/// bound below, is left unknown, which is the safe side.
///
/// No test covers: the race of 4 (a pooled connection that closes as a request is handed
/// to it; a test needs a deterministic hook on that hand-off, which the test kit does not
/// have), a transport of the author's built over another client, the connector layers that
/// `Http::with` lets an author add (a layer receives the URI, not the request), HTTP/3
/// (reqwest's `http3` needs `--cfg reqwest_unstable` and adds a wrapper of its own around
/// the error; its errors carry no hyper-util error, so it is never stated), and a
/// dependency resolution other than the one locked here: a consumer's own `Cargo.lock`
/// decides the versions it builds with.
fn connector_failed(error: &reqwest::Error) -> bool {
    if !error.is_request() || !error.is_connect() {
        return false;
    }
    let mut cause = std::error::Error::source(error);
    // A bound on the walk: a chain this long is not one of the shapes above.
    for _ in 0..64 {
        let Some(current) = cause else {
            return true;
        };
        if current.is::<hyper::Error>() || current.is::<reqwest::Error>() {
            return false;
        }
        cause = current.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c10_7_transport_error_debug_redacts_its_source() {
        struct PrivateSource(&'static str);
        impl std::fmt::Debug for PrivateSource {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("source Debug must not be called");
            }
        }
        impl std::fmt::Display for PrivateSource {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        impl std::error::Error for PrivateSource {}

        let error = TransportError::connect().with_source(PrivateSource("private-source"));
        let expected = "TransportError { class: Connect, source: .. }";
        assert_eq!(format!("{error:?}"), expected);
        assert_eq!(format!("{error:#?}"), expected);
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            "private-source"
        );
        assert_eq!(
            format!("{:?}", TransportError::timeout()),
            "TransportError { class: Timeout, source: .. }"
        );
    }

    #[test]
    fn c9_5_try_info_debug_shows_only_the_endpoint_and_try_number() {
        let info = TryInfo::new("tasks.list", 2);
        assert_eq!(
            format!("{info:?}"),
            "TryInfo { endpoint: \"tasks.list\", number: 2, .. }"
        );
    }

    #[test]
    fn c10_7_http_debug_hides_client_settings() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-private", "private-header-value".parse().unwrap());
        let http =
            Http::with(|builder| builder.default_headers(headers).user_agent("private-agent"))
                .unwrap();
        assert_eq!(format!("{http:?}"), "Http { .. }");
        assert_eq!(format!("{http:#?}"), "Http { .. }");
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::error::Error;
    use futures_util::FutureExt;
    use std::error::Error as _;
    use std::io;

    fn ready<F: Future>(future: F) -> F::Output {
        future
            .now_or_never()
            .expect("an in-memory fixture must be immediately ready")
    }

    #[test]
    fn c9_5_transport_error_classes_constructors_and_source() {
        for class in [
            Class::Connect,
            Class::Timeout,
            Class::Body,
            Class::Blocked,
            Class::Other,
        ] {
            let error = TransportError::new(class);
            assert_eq!(error.class(), class);
            assert!(error.source().is_none());
            let error = error.with_source(io::Error::other("safe cause"));
            assert_eq!(error.source().unwrap().to_string(), "safe cause");
            assert!(!format!("{error} {error:?} {error:#?}").contains("safe cause"));
            let error = error.with_source(io::Error::other("replacement"));
            assert_eq!(error.source().unwrap().to_string(), "replacement");
        }
        assert_eq!(TransportError::blocked().class(), Class::Blocked);
        assert_eq!(TransportError::connect().class(), Class::Connect);
        assert_eq!(TransportError::timeout().class(), Class::Timeout);
    }

    #[test]
    fn c6_1_reqwest_builder_and_status_errors_are_other_and_lose_their_urls() {
        let client = reqwest::Client::new();
        let error = client.get("not a URL").build().unwrap_err();
        assert!(error.is_builder());
        let error = error.with_url(
            "https://user:PRIVATE_SENTINEL@example.invalid/PRIVATE_SENTINEL?token=PRIVATE_SENTINEL"
                .parse()
                .unwrap(),
        );
        let error = TransportError::from(error);
        assert_eq!(error.class(), Class::Other);
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .unwrap();
        assert_eq!(source.url(), None);
        assert!(!format!("{source} {source:?}").contains("PRIVATE_SENTINEL"));

        let error = reply(503, "PRIVATE_SENTINEL")
            .error_for_status()
            .unwrap_err();
        assert!(error.is_status());
        let error = TransportError::from(error);
        assert_eq!(error.class(), Class::Other);
        assert!(error.source().is_none());
    }

    fn broken_body(kind: io::ErrorKind) -> reqwest::Response {
        reply(
            200,
            reqwest::Body::wrap_stream(futures_util::stream::iter([
                Ok(bytes::Bytes::from_static(b"PRIVATE_SENTINEL")),
                Err(io::Error::from(kind)),
            ])),
        )
    }

    #[test]
    fn c6_1_reqwest_body_decode_errors_are_body_without_network() {
        let error = ready(broken_body(io::ErrorKind::Other).bytes()).unwrap_err();
        assert!(error.is_decode());
        assert!(!error.is_timeout());
        let error = TransportError::from(error);
        assert_eq!(error.class(), Class::Body);
        assert!(error.source().is_none());
        let mut chain: Option<&dyn std::error::Error> = Some(&error);
        while let Some(cause) = chain {
            assert!(!format!("{cause} {cause:?}").contains("PRIVATE_SENTINEL"));
            chain = cause.source();
        }
    }

    #[test]
    fn c6_1_reqwest_timeout_overrides_body_class_without_network() {
        let error = ready(broken_body(io::ErrorKind::TimedOut).bytes()).unwrap_err();
        assert!(error.is_decode());
        assert!(error.is_timeout());
        let error = TransportError::from(error);
        assert_eq!(error.class(), Class::Timeout);
        assert!(error.source().is_none());
    }

    // No socket or real DNS: fail in the injected resolver, or return no addresses.
    struct OfflineResolver(Option<io::ErrorKind>);
    impl reqwest::dns::Resolve for OfflineResolver {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            let result: Result<reqwest::dns::Addrs, Box<dyn std::error::Error + Send + Sync>> =
                match self.0 {
                    Some(kind) => Err(Box::new(io::Error::from(kind))),
                    None => Ok(Box::new(std::iter::empty())),
                };
            Box::pin(std::future::ready(result))
        }
    }

    fn resolver_error(kind: Option<io::ErrorKind>) -> reqwest::Error {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                reqwest::Client::builder()
                    .no_proxy()
                    .retry(reqwest::retry::never())
                    .dns_resolver(Arc::new(OfflineResolver(kind)))
                    .build()
                    .unwrap()
                    .get("http://fixture.invalid/PRIVATE_SENTINEL?token=PRIVATE_SENTINEL")
                    .send()
                    .await
                    .unwrap_err()
            })
    }

    #[test]
    fn c6_1_reqwest_connect_and_dns_are_classified_without_network() {
        let error = resolver_error(Some(io::ErrorKind::NotFound));
        assert!(error.is_connect());
        assert!(error.is_dns());
        assert!(!error.is_timeout());
        let transport = TransportError::from(error);
        assert_eq!(transport.class(), Class::Connect);
        let source = transport
            .source()
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .unwrap();
        assert_eq!(source.url(), None);
        assert!(!format!("{source} {source:?}").contains("PRIVATE_SENTINEL"));
        let error: Error = Error::transport("tasks.get", 1, transport);
        assert_eq!(
            error.to_string(),
            "transport error: the host name could not be resolved, in tasks.get"
        );

        // Reqwest represents an empty address set with io::ErrorKind::NotConnected;
        // Use the closed-list connection-reset wording.
        let error = resolver_error(None);
        assert!(error.is_connect());
        assert!(!error.is_dns());
        let transport = TransportError::from(error);
        assert_eq!(transport.class(), Class::Connect);
        let error: Error = Error::transport("tasks.get", 1, transport);
        assert_eq!(
            error.to_string(),
            "transport error: connection failed, in tasks.get"
        );
    }

    #[test]
    fn c6_1_reqwest_timeout_overrides_connect_class_without_network() {
        let error = resolver_error(Some(io::ErrorKind::TimedOut));
        assert!(error.is_connect());
        assert!(error.is_dns());
        assert!(error.is_timeout());
        let error = TransportError::from(error);
        assert_eq!(error.class(), Class::Timeout);
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .unwrap();
        assert_eq!(source.url(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn c6_2_request_without_connect_or_io_is_connect_without_a_socket() {
        // Hyper refuses HTTP/0.9 before DNS or a socket. This produces a real
        // reqwest Request error whose source has neither connect nor an I/O kind.
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let error = client
            .get("http://fixture.invalid/PRIVATE_SENTINEL")
            .version(http::Version::HTTP_09)
            .send()
            .await
            .unwrap_err();
        assert!(error.is_request());
        assert!(!error.is_connect());
        assert!(!error.is_timeout());
        let failure = TransportError::from(error);
        assert_eq!(failure.connection_kind(), None);
        assert_eq!(failure.class(), Class::Connect);
        let error: Error = Error::transport("tasks.get", 1, failure);
        assert_eq!(
            error.to_string(),
            "transport error: no connection, in tasks.get"
        );
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .unwrap();
        assert_eq!(source.url(), None);
        assert!(!format!("{error} {error:?} {source} {source:?}").contains("PRIVATE_SENTINEL"));
    }

    #[tokio::test(start_paused = true)]
    async fn c4_2_standard_retry_retries_a_request_failure_without_a_socket() {
        use crate::__private::{Core, Draft, Idempotent, Options, Piece, Run};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct FirstRequestFails(AtomicUsize);
        impl Transport for FirstRequestFails {
            async fn send(
                &self,
                mut request: reqwest::Request,
                _: TryInfo,
            ) -> Result<reqwest::Response, TransportError> {
                if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
                    return Ok(reply(200, ""));
                }
                *request.version_mut() = http::Version::HTTP_09;
                reqwest::Client::builder()
                    .no_proxy()
                    .build()
                    .unwrap()
                    .execute(request)
                    .await
                    .map_err(Into::into)
            }
        }
        let transport = Arc::new(FirstRequestFails(AtomicUsize::new(0)));
        let core = Core::<()>::default()
            .with_transport(transport.clone())
            .with_retry(crate::Retry::standard());
        let options = Options::default();
        let run = Run::start(&core, &options, "tasks.get");
        let mut draft = Draft::new(http::Method::GET);
        draft.base(&[Piece::Lit("http://fixture.invalid/")]);
        let response = draft
            .prepare(
                run,
                core,
                options,
                Idempotent::ByMethod,
                |response, _| async move {
                    Ok(crate::Answer::new(
                        response.status(),
                        response.headers().clone().into(),
                        (),
                    ))
                },
            )
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(transport.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    /// Also proves C6.2.
    fn c7_1_connect_timeout_names_the_http_clock_and_only_a_known_length() {
        for (http, length) in [
            (Some(Http::new().unwrap()), " after 30 s"),
            (
                Some(Http::with(|b| b.connect_timeout(std::time::Duration::from_secs(7))).unwrap()),
                "",
            ),
            (None, ""),
        ] {
            let source = resolver_error(Some(io::ErrorKind::TimedOut));
            assert!(source.is_connect() && source.is_timeout());
            let transport = match http {
                Some(http) => http.classify_error(source),
                None => TransportError::from(source),
            };
            let error: Error = Error::transport("tasks.get", 1, transport);
            assert_eq!(error.class(), Some(Class::Timeout));
            assert_eq!(error.to_string(), format!("transport error: no connection{length} (the connect timeout of the HTTP client), in tasks.get"));
            assert_eq!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<reqwest::Error>()
                    .unwrap()
                    .url(),
                None
            );
        }
        let error: Error = Error::transport("tasks.get", 1, TransportError::timeout());
        assert_eq!(
            error.to_string(),
            "transport error: the transport reported a timeout, in tasks.get"
        );
    }

    #[cfg(feature = "rustls")]
    #[tokio::test(start_paused = true)]
    async fn c7_1_real_stalled_tls_handshake_reports_the_30_s_http_connect_timeout() {
        use std::io::Read;
        use std::sync::atomic::{AtomicBool, Ordering};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://{}/", listener.local_addr().unwrap());
        let hello = Arc::new(AtomicBool::new(false));
        let received = hello.clone();
        let (release, hold) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            stream.read_exact(&mut [0]).unwrap();
            received.store(true, Ordering::SeqCst);
            // Hold the TCP connection open without replying to the ClientHello.
            let _ = hold.recv();
        });
        let http = Http::new().unwrap();
        let request = reqwest::Request::new(http::Method::GET, url.parse().unwrap());
        let mut sending = Box::pin(http.send(request, TryInfo::new("tasks.get", 1)));
        // Keep polling I/O without allowing the paused clock to auto-advance
        // before the server has seen a TLS handshake.
        while !hello.load(Ordering::SeqCst) {
            assert!(sending.as_mut().now_or_never().is_none());
            tokio::task::yield_now().await;
        }
        tokio::time::advance(std::time::Duration::from_secs(30)).await;
        let failure = sending.await.unwrap_err();
        release.send(()).unwrap();
        server.join().unwrap();
        let error: Error = Error::transport("tasks.get", 1, failure);
        assert_eq!(error.class(), Some(Class::Timeout));
        assert_eq!(error.to_string(), "transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get");
    }

    #[tokio::test(start_paused = true)]
    async fn c7_1_http_connect_timeout_bounds_pending_dns_without_a_socket() {
        struct PendingResolver;
        impl reqwest::dns::Resolve for PendingResolver {
            fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
                Box::pin(std::future::pending())
            }
        }
        let client =
            Http::with(|builder| builder.no_proxy().dns_resolver(Arc::new(PendingResolver)))
                .unwrap();
        let request = reqwest::Request::new(
            http::Method::GET,
            "http://never-resolved.test/".parse().unwrap(),
        );
        let start = tokio::time::Instant::now();
        let failure = tokio::time::timeout(
            std::time::Duration::from_secs(31),
            client.send(request, TryInfo::new("dns.get", 1)),
        )
        .await
        .expect("the HTTP connect timeout must expire first")
        .unwrap_err();
        assert_eq!(
            tokio::time::Instant::now() - start,
            std::time::Duration::from_secs(30)
        );
        assert_eq!(failure.class(), Class::Timeout);
    }

    #[test]
    fn c4_2_real_get_retries_close_before_head_then_200() {
        use crate::__private::{Core, Draft, Idempotent, Options, Piece, Run};
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        struct Observed {
            client: reqwest::Client,
            failed: AtomicBool,
            tries: AtomicUsize,
        }
        impl Transport for Observed {
            async fn send(
                &self,
                request: reqwest::Request,
                info: TryInfo,
            ) -> Result<reqwest::Response, TransportError> {
                self.tries.fetch_add(1, Ordering::SeqCst);
                let result = self.client.send(request, info).await;
                if let Err(error) = &result {
                    assert_eq!(error.class(), Class::Connect);
                    self.failed.store(true, Ordering::SeqCst);
                }
                result
            }
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let seen = accepted.clone();
        let server = std::thread::spawn(move || {
            for n in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                read_request_head(&mut stream);
                seen.fetch_add(1, Ordering::SeqCst);
                if n == 1 {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        )
                        .unwrap();
                }
                // First request: close cleanly, with no response head.
            }
        });
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::pause();
                let transport = Arc::new(Observed {
                    client: local_client(),
                    failed: AtomicBool::new(false),
                    tries: AtomicUsize::new(0),
                });
                let core = Core::<()>::default()
                    .with_base(url.clone())
                    .with_transport(transport.clone())
                    .with_retry(crate::Retry::standard());
                let options = Options::default();
                let run = Run::start(&core, &options, "tasks.get");
                let mut draft = Draft::new(http::Method::GET);
                draft.base(&[Piece::Lit("http://fixture.invalid/")]);
                let sending = draft
                    .prepare(
                        run,
                        core,
                        options,
                        Idempotent::ByMethod,
                        |response, _| async move {
                            Ok(crate::Answer::new(
                                response.status(),
                                response.headers().clone().into(),
                                (),
                            ))
                        },
                    )
                    .unwrap()
                    .send();
                let mut sending = Box::pin(sending);
                // Freeze time during real I/O; advance only the retry backoff.
                // Auto-advance could otherwise outrun the server's OS thread.
                let response = loop {
                    if let Some(result) = sending.as_mut().now_or_never() {
                        break result.unwrap();
                    }
                    if transport.failed.swap(false, Ordering::SeqCst) {
                        tokio::time::advance(std::time::Duration::from_secs(1)).await;
                    } else {
                        tokio::task::yield_now().await;
                    }
                };
                assert_eq!(response.status(), http::StatusCode::OK);
                assert_eq!(transport.tries.load(Ordering::SeqCst), 2);
            });
        server.join().unwrap();
        assert_eq!(accepted.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn c9_2_reply_preserves_status_headers_duplicates_and_body() {
        for status in [100, 200, 404, 999] {
            let response = reply(status, "body");
            assert_eq!(response.status().as_u16(), status);
            assert!(response.headers().is_empty());
            assert!(response.extensions().get::<SyntheticReply>().is_some());
            assert_eq!(response.url().as_str(), "http://no.url.provided.local/");
            assert_eq!(ready(response.bytes()).unwrap(), "body");
        }
        let response = reply_with(
            429,
            &[
                ("retry-after", "30"),
                ("x-multi", "first"),
                ("X-Multi", "second"),
            ],
            vec![0, 255, 1],
        );
        assert_eq!(response.status().as_u16(), 429);
        assert_eq!(response.headers().get("retry-after").unwrap(), "30");
        assert_eq!(
            response
                .headers()
                .get_all("x-multi")
                .iter()
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(ready(response.bytes()).unwrap().as_ref(), &[0, 255, 1]);
    }

    #[test]
    fn c9_2_fake_sees_the_request_and_returns_reply_or_class() {
        let fake = fake(|request| {
            assert_eq!(request.url().path(), "/api/tasks/7");
            assert_eq!(request.headers().get("x-signed").unwrap(), "yes");
            Ok(reply_with(201, &[("x-request-id", "test")], "answer"))
        });
        let mut request = reqwest::Request::new(
            http::Method::GET,
            "http://example.invalid/api/tasks/7".parse().unwrap(),
        );
        request
            .headers_mut()
            .insert("x-signed", http::HeaderValue::from_static("yes"));
        let response = ready(fake.send(request, TryInfo::new("tasks.get", 1))).unwrap();
        assert_eq!(response.status().as_u16(), 201);
        assert_eq!(ready(response.text()).unwrap(), "answer");
        let fake = super::fake(|_| Err(TransportError::blocked()));
        let request = reqwest::Request::new(
            http::Method::GET,
            "http://example.invalid/".parse().unwrap(),
        );
        assert_eq!(
            ready(fake.send(request, TryInfo::new("tasks.get", 1)))
                .unwrap_err()
                .class(),
            Class::Blocked
        );
    }

    #[test]
    fn c9_2_reply_keeps_stream_chunks_and_failure() {
        let mut response = broken_body(io::ErrorKind::Other);
        assert_eq!(
            ready(response.chunk()).unwrap().unwrap(),
            "PRIVATE_SENTINEL"
        );
        let error = ready(response.chunk()).unwrap_err();
        assert_eq!(TransportError::from(error).class(), Class::Body);
    }

    #[test]
    fn c9_2_reply_does_not_poll_a_pending_body() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = polls.clone();
        let stream = futures_util::stream::poll_fn(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::<Option<Result<bytes::Bytes, io::Error>>>::Pending
        });
        let mut response = reply(200, reqwest::Body::wrap_stream(stream));
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert!(response.chunk().now_or_never().is_none());
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[should_panic(expected = "reply requires a valid status and valid headers")]
    fn c9_2_invalid_reply_status_is_a_fixture_error() {
        reply(99, "");
    }

    #[test]
    #[should_panic(expected = "reply requires a valid status and valid headers")]
    fn c9_2_invalid_reply_header_is_a_fixture_error() {
        reply_with(200, &[("bad name", "value")], "");
    }

    #[test]
    #[should_panic(expected = "reply requires a valid status and valid headers")]
    fn c9_2_invalid_reply_header_value_is_a_fixture_error() {
        reply_with(200, &[("x-name", "bad\r\nvalue")], "");
    }
    fn local_client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap()
    }
    fn local_server(
        serve: impl FnOnce(std::net::TcpStream) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            serve(stream);
        });
        (format!("http://{addr}/PRIVATE_SENTINEL"), task)
    }
    fn read_request_head(stream: &mut std::net::TcpStream) {
        use std::io::Read;
        let mut seen = Vec::new();
        let mut byte = [0];
        while !seen.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            seen.push(byte[0]);
        }
    }
    #[test]
    fn c6_1_real_status_reason_phrase_never_enters_a_source() {
        use std::io::Write;
        let (url, server) = local_server(|mut stream| {
            read_request_head(&mut stream);
            stream
                .write_all(b"HTTP/1.1 404 PRIVATE_SENTINEL\r\ncontent-length: 0\r\n\r\n")
                .unwrap();
        });
        // The private marker occurs only in the reason phrase, never in this URL.
        let url = url.replace("/PRIVATE_SENTINEL", "/status");
        let http = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                local_client()
                    .get(url)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap_err()
            });
        assert!(http.is_status());
        assert!(format!("{http:?}").contains("PRIVATE_SENTINEL"));
        let error: Error = Error::transport("tasks.get", 1, TransportError::from(http));
        assert_eq!(error.class(), Some(Class::Other));
        assert!(error.source().is_none());
        assert!(!format!("{error} {error:?} {}", error.details()).contains("PRIVATE_SENTINEL"));
        server.join().unwrap();
    }
    #[test]
    fn c6_2_real_refused_port_is_connect_with_closed_vocabulary() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let http = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                local_client()
                    .get(format!("http://{addr}/PRIVATE_SENTINEL"))
                    .send()
                    .await
                    .unwrap_err()
            });
        let error: Error = Error::transport("tasks.get", 1, TransportError::from(http));
        assert_eq!(error.class(), Some(Class::Connect));
        assert_eq!(
            error.to_string(),
            "transport error: connection refused, in tasks.get"
        );
        let mut cause = error.source();
        let mut count = 0;
        while let Some(source) = cause {
            assert!(!format!("{source} {source:?}").contains("PRIVATE_SENTINEL"));
            count += 1;
            cause = source.source();
        }
        assert!(count >= 4);
    }
    #[test]
    fn c6_2_real_reset_after_connecting_is_connect() {
        use std::io::Read;
        let (url, server) = local_server(|mut stream| {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
        });
        let http = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                local_client()
                    .post(url)
                    .body(vec![b'x'; 256 * 1024])
                    .send()
                    .await
                    .unwrap_err()
            });
        assert!(!http.is_connect());
        let error: Error = Error::transport("tasks.get", 1, TransportError::from(http));
        assert_eq!(error.class(), Some(Class::Connect));
        assert_eq!(
            error.to_string(),
            "transport error: connection failed, in tasks.get"
        );
        server.join().unwrap();
    }
    #[test]
    fn c6_2_body_decode_with_an_io_connection_failure_is_connect_but_has_no_source() {
        for kind in [
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::NotConnected,
            io::ErrorKind::UnexpectedEof,
        ] {
            let http = ready(broken_body(kind).bytes()).unwrap_err();
            assert!(http.is_decode());
            assert!(!http.is_connect());
            let error = TransportError::from(http);
            assert_eq!(error.class(), Class::Connect);
            assert!(error.source().is_none());
            let error: Error = Error::transport("tasks.get", 1, error);
            assert!(error.source().is_none());
            let word = if kind == io::ErrorKind::ConnectionRefused {
                "connection refused"
            } else {
                "connection failed"
            };
            assert_eq!(
                error.to_string(),
                format!("transport error: {word}, in tasks.get")
            );
        }
    }
}
