//! What sends a request.

use crate::error::Class;
use std::future::Future;
use std::sync::Arc;

/// What really sends a request and gives its answer: [`Http`], a `reqwest::Client`,
/// a fake, or a transport of your own.
///
/// A client has one transport: `with_transport(yours)`; the last one called wins.
/// Every try and every page goes through the client's middlewares first, in order,
/// and reaches the transport with a [`TryInfo`] naming the endpoint and the try.
/// For a signature, a token that is renewed, or a shared quota, write a
/// [`Middleware`](crate::Middleware).
///
/// Concord counts each trip through the chain as one try. Any resend that the HTTP
/// library makes internally is below that count and follows the library's settings.
/// The clock for silence starts at hand-over to the transport, after middleware.
/// `timeout` is checked there too: an expired call is never handed over.
/// All time spent inside a transport counts as transport time. A transport must
/// preserve what the description’s last middleware signed.
///
/// An answer should carry its own URL (for an `http::Response`, use
/// `reqwest::ResponseBuilderExt::url`). If it carries reqwest's placeholder URL,
/// Concord uses the request's URL, including when resolving a relative next URL.
///
/// A `tower` service is a transport too: `concord::tower::Tower`, with the feature `tower`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot send a request",
    note = "a reqwest::Client, concord::Http, a fake, or a type with concord::Transport"
)]
pub trait Transport: Send + Sync + 'static {
    /// Send the request.
    ///
    /// Not cancel safe, like `reqwest::Client::execute`: if the future is dropped, the
    /// request may have reached the server. After a drop Concord starts no further try; a
    /// request that was already handed to the transport may still go out.
    fn send(
        &self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send;
}

/// A `reqwest::Client` of yours really sends, keeping its own settings for redirects,
/// decompression and internal retries. Concord's middlewares run before it.
/// An unmarked, non-idempotent call is not repeated by Concord after a failed connection.
impl Transport for reqwest::Client {
    async fn send(
        &self,
        request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        self.execute(request).await.map_err(TransportError::from)
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

/// A failure with no answer.
pub struct TransportError {
    class: Class,
    request_failure: Option<RequestFailure>,
    clock: Option<crate::answer::BodyTimeout>,
    // A12 can discard the source; A13 still needs its value-free I/O classification.
    connection_kind: Option<std::io::ErrorKind>,
    // Some(None): the HTTP connect clock is known, but its configured length is not.
    connect_timeout: Option<Option<std::time::Duration>>,
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
    /// A failure of this class.
    pub fn new(class: Class) -> TransportError {
        TransportError {
            class,
            request_failure: None,
            clock: None,
            connection_kind: None,
            connect_timeout: None,
            source: None,
        }
    }

    /// A connection failed.
    pub fn connect() -> TransportError {
        TransportError::new(Class::Connect)
    }

    /// A timeout of the transport's own.
    pub fn timeout() -> TransportError {
        TransportError::new(Class::Timeout)
    }

    /// A middleware or transport refuses before sending. The tries end
    /// here, and this blocked error is the result.
    ///
    /// After an answer (for example a `401` before token renewal), return that answer
    /// or another failure instead.
    pub fn blocked() -> TransportError {
        TransportError::new(Class::Blocked)
    }

    /// The cause, for the `source()` of the error that the caller gets.
    ///
    /// It is shown as it is: give a source that holds no URL and no secret. For a
    /// `reqwest::Error`, use `TransportError::from(error)` instead: it takes the URL out.
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

    /// The class of the failure.
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
        self.source.as_deref().map(|e| e as _)
    }
}

impl From<reqwest::Error> for TransportError {
    /// Classify the failure; strip its URL, and discard unsafe status/decode sources.
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

/// What a middleware or transport is told about the request: which endpoint, and which try.
#[derive(Clone)]
pub struct TryInfo {
    endpoint: &'static str,
    number: u32,
    run: Option<crate::__private::Run>,
    conditional: Option<Arc<std::sync::atomic::AtomicBool>>,
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
    /// For generated code and for tests.
    pub fn new(endpoint: &'static str, number: u32) -> TryInfo {
        TryInfo {
            endpoint,
            number,
            run: None,
            conditional: None,
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

    pub(crate) fn for_run(run: crate::__private::Run, number: u32) -> Self {
        Self {
            endpoint: run.endpoint(),
            number,
            run: Some(run),
            conditional: None,
        }
    }

    pub(crate) fn run(&self) -> Option<&crate::__private::Run> {
        self.run.as_ref()
    }

    /// The name of the endpoint: `riot.matches.get`.
    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// The number of the try: 1 for the first.
    pub fn number(&self) -> u32 {
        self.number
    }
}

/// A transport from a function, for a test: it gets the request that would go out.
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
/// An answer should carry its URL; if it has reqwest's placeholder, the engine
/// takes it to come from the URL of this request.
pub fn fake<F>(answer: F) -> Fake<F>
where
    F: Fn(&reqwest::Request) -> Result<reqwest::Response, TransportError> + Send + Sync + 'static,
{
    Fake(answer)
}

/// What [`fake`] gives.
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
    async fn send(
        &self,
        request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        (self.0)(&request)
    }
}

/// Marks a fixture with no real response URL.
#[derive(Clone, Debug)]
pub(crate) struct SyntheticReply;

/// An answer made in memory.
///
/// Its URL is reqwest's placeholder, `http://no.url.provided.local/`.
///
/// # Panics
///
/// When `status` is not a status code (100 to 999). It is a helper for tests.
pub fn reply(status: u16, body: impl Into<reqwest::Body>) -> reqwest::Response {
    reply_with(status, &[], body)
}

/// An answer made in memory, with headers.
///
/// Its URL is reqwest's placeholder, `http://no.url.provided.local/`.
/// Bodies may stream, fail or stay pending; this helper does not read them.
///
/// # Panics
///
/// When `status` is not a status code, or a header is not a valid header. It is a helper for tests.
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
/// Follows no redirect. Decompression is off by default; [`Http::with`] may enable it.
/// Internal retries use `reqwest`'s settings and are below Concord's count of tries.
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
        let mut failure = TransportError::from(error);
        if failure.connect_timeout.is_some() {
            failure.connect_timeout = Some(self.connect_timeout);
        }
        failure
    }

    /// Create an HTTP client with Concord's defaults.
    pub fn new() -> Result<Http, reqwest::Error> {
        let mut http = Http::with(|builder| builder)?;
        http.connect_timeout = Some(std::time::Duration::from_secs(30));
        Ok(http)
    }

    /// Concord's HTTP client, with settings of yours: a certificate, a proxy.
    ///
    /// Before `f`, the defaults, which `f` may change: 30 seconds to connect, no `Referer`,
    /// no decompression (a body is given as it came). The closure may enable
    /// decompression; readers then receive, and their limits count, the bytes that
    /// the transport gives. After `f`, one thing that `f` cannot change:
    /// no redirect is followed. A `3xx` is an answer.
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
        // Then the one thing that a promise needs, which the closure cannot change.
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
    async fn send(
        &self,
        request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        self.client
            .execute(request)
            .await
            .map_err(|error| self.classify_error(error))
    }
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
            "transport error: name not found, in tasks.get"
        );

        // Reqwest represents an empty address set with io::ErrorKind::NotConnected;
        // A13 therefore gives it the closed-list connection-reset wording.
        let error = resolver_error(None);
        assert!(error.is_connect());
        assert!(!error.is_dns());
        let transport = TransportError::from(error);
        assert_eq!(transport.class(), Class::Connect);
        let error: Error = Error::transport("tasks.get", 1, transport);
        assert_eq!(
            error.to_string(),
            "transport error: connection reset, in tasks.get"
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
    async fn c6_2_d272_request_without_connect_or_io_is_connect_without_a_socket() {
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
    async fn c4_2_d272_standard_retry_retries_a_request_failure_without_a_socket() {
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
            "transport error: a timeout was reported by the transport, in tasks.get"
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
    fn c4_2_d272_real_get_retries_close_before_head_then_200() {
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
    fn c6_1_a12_real_status_reason_phrase_never_enters_a_source() {
        use std::io::Write;
        let (url, server) = local_server(|mut stream| {
            read_request_head(&mut stream);
            stream
                .write_all(b"HTTP/1.1 404 PRIVATE_SENTINEL\r\ncontent-length: 0\r\n\r\n")
                .unwrap();
        });
        // The sentinel occurs only in the reason phrase, never in this URL.
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
    fn c6_2_a13_real_refused_port_is_connect_with_closed_vocabulary() {
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
    fn c6_2_a13_real_reset_after_connecting_is_connect() {
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
            "transport error: connection reset, in tasks.get"
        );
        server.join().unwrap();
    }
    #[test]
    fn c6_2_a13_body_decode_with_an_io_connection_failure_is_connect_but_has_no_source() {
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
                "connection reset"
            };
            assert_eq!(
                error.to_string(),
                format!("transport error: {word}, in tasks.get")
            );
        }
    }
}
