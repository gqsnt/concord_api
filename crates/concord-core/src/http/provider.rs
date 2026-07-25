use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use url::Url;

#[cfg(test)]
use concord_test_support::executor::ProviderChannel;
#[cfg(test)]
use concord_test_support::script::BoundaryRequest;

use crate::auth::AuthError;
use crate::error::{AuthErrorKind, ErrorCategory, reqwest_category};
use crate::operation::OperationDeadline;

use super::build::TlsCapability;
use super::response_progress::{ResponseChunkError, ResponseChunkProgress, next_nonempty_chunk};

/// Private provider HTTP owner with an independent client and pool.
pub(super) struct ProviderHttpInner {
    client: reqwest::Client,
    tls: TlsCapability,
    request_limit: NonZeroUsize,
    response_limit: NonZeroUsize,
    #[cfg(test)]
    development: Option<ProviderChannel>,
}

/// An isolated, bounded provider-HTTP capability borrowed for one credential operation.
///
/// Requests use Concord's separate managed provider client and finite body limits,
/// and are constrained by the credential operation deadline plus any shorter
/// request timeout. Dropping the provider future cancels later polling but does
/// not roll back bytes already sent or other completed external effects. Provider
/// URLs, headers, bodies, and responses may be sensitive; applications and
/// providers remain responsible for their own logging and storage. Providers can
/// inspect the active semantic request-body ceiling through
/// [`ProviderHttp::request_body_limit`] without gaining access to native
/// transport state.
#[derive(Clone, Copy)]
pub struct ProviderHttp<'a> {
    inner: Option<&'a ProviderHttpInner>,
    deadline: Option<OperationDeadline>,
}

/// One bounded provider HTTP request.
pub struct ProviderRequest {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: ProviderBody,
    timeout: Option<Duration>,
    response_limit: Option<NonZeroUsize>,
}

impl ProviderRequest {
    /// Creates an empty provider request.
    #[must_use]
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: HeaderMap::new(),
            body: ProviderBody::Empty,
            timeout: None,
            response_limit: None,
        }
    }

    /// Inserts or replaces one provider request header.
    #[must_use]
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Replaces the provider request body.
    #[must_use]
    pub fn body(mut self, body: ProviderBody) -> Self {
        self.body = body;
        self
    }

    /// Sets the request-local provider timeout.
    #[must_use]
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Reduces the finite provider response limit for this request.
    #[must_use]
    pub fn response_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.response_limit = Some(bytes);
        self
    }
}

/// A replay-stable body for one provider HTTP operation.
pub enum ProviderBody {
    /// No request body.
    Empty,
    /// Complete retained bytes.
    Bytes(Bytes),
    /// A synchronous factory invoked once for this provider operation.
    Factory(Arc<dyn ProviderBodyFactory>),
}

/// Produces complete provider request bytes.
///
/// Calls for independent provider operations may overlap. Inputs, output, and
/// factory state may be sensitive; implementations own logging and storage and
/// must bound CPU, memory, retries, maps, queues, spawned tasks, metric
/// cardinality, and external I/O. The synchronous callback cannot be preempted;
/// its bytes are checked against the provider limit and operation deadline after
/// return. Cancellation prevents later work only, and Concord does not
/// automatically bound custom factory state.
pub trait ProviderBodyFactory: Send + Sync + 'static {
    /// Creates the complete bytes once for one provider operation.
    fn create(&self) -> Result<Bytes, AuthError>;
}

/// A complete bounded provider response, including non-success statuses.
pub struct ProviderResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl ProviderResponse {
    /// Returns the provider response status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the actual provider response headers.
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Borrows the complete bounded provider response body.
    #[must_use]
    pub const fn body(&self) -> &Bytes {
        &self.body
    }

    /// Extracts the complete bounded provider response body.
    #[must_use]
    pub fn into_body(self) -> Bytes {
        self.body
    }
}

impl<'a> ProviderHttp<'a> {
    const fn new(inner: &'a ProviderHttpInner, deadline: Option<OperationDeadline>) -> Self {
        Self {
            inner: Some(inner),
            deadline,
        }
    }

    #[cfg(test)]
    pub(crate) const fn unavailable_for_test() -> ProviderHttp<'static> {
        ProviderHttp {
            inner: None,
            deadline: None,
        }
    }

    /// Returns the active finite request-body byte ceiling.
    ///
    /// Built-in providers can use this semantic bound to reject encoded output
    /// before allocating it. The value exposes no native client, pool, TLS, or
    /// request-builder state.
    #[must_use]
    pub const fn request_body_limit(&self) -> NonZeroUsize {
        match self.inner {
            Some(inner) => inner.request_limit,
            None => NonZeroUsize::MIN,
        }
    }

    /// Sends one bounded request through the isolated provider client.
    pub async fn send(self, request: ProviderRequest) -> Result<ProviderResponse, AuthError> {
        let Some(inner) = self.inner else {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        };
        if request.url.scheme() != "https" || inner.tls == TlsCapability::Unavailable {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }

        let future = inner.send_bounded(request, self.deadline);
        match self.deadline {
            Some(deadline) => deadline
                .run(future)
                .await
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?,
            None => future.await,
        }
    }
}

impl ProviderHttpInner {
    pub(super) const fn new(
        client: reqwest::Client,
        tls: TlsCapability,
        request_limit: NonZeroUsize,
        response_limit: NonZeroUsize,
    ) -> Self {
        Self {
            client,
            tls,
            request_limit,
            response_limit,
            #[cfg(test)]
            development: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_development(mut self, development: ProviderChannel) -> Self {
        self.development = Some(development);
        self
    }

    #[cfg(test)]
    pub(super) const fn test_runtime_limits(&self) -> (NonZeroUsize, NonZeroUsize) {
        (self.request_limit, self.response_limit)
    }

    #[cfg(test)]
    pub(super) fn prepare(
        &self,
        method: Method,
        url: &str,
        known_body_length: Option<usize>,
    ) -> Result<reqwest::RequestBuilder, ErrorCategory> {
        let url = url::Url::parse(url).map_err(|_| ErrorCategory::Configuration)?;
        if url.scheme() != "https" || self.tls == TlsCapability::Unavailable {
            return Err(ErrorCategory::Configuration);
        }
        if known_body_length.is_some_and(|length| length > self.request_limit.get()) {
            return Err(ErrorCategory::RequestBody);
        }
        Ok(self.client.request(method, url))
    }

    pub(super) const fn capability(&self, deadline: Option<OperationDeadline>) -> ProviderHttp<'_> {
        ProviderHttp::new(self, deadline)
    }

    async fn send_bounded(
        &self,
        request: ProviderRequest,
        deadline: Option<OperationDeadline>,
    ) -> Result<ProviderResponse, AuthError> {
        let ProviderRequest {
            method,
            url,
            headers,
            body,
            timeout,
            response_limit,
        } = request;

        // Scheme/TLS validation occurs in ProviderHttp::send before this match,
        // so an unsafe URL cannot invoke application code in a body factory.
        let body = match body {
            ProviderBody::Empty => None,
            ProviderBody::Bytes(bytes) => Some(bytes),
            ProviderBody::Factory(factory) => Some(factory.create()?),
        };
        if body
            .as_ref()
            .is_some_and(|bytes| bytes.len() > self.request_limit.get())
        {
            return Err(AuthError::new(AuthErrorKind::RequestTooLarge));
        }

        let effective_response_limit = response_limit
            .map(NonZeroUsize::get)
            .unwrap_or(self.response_limit.get())
            .min(self.response_limit.get());
        let effective_timeout = match (timeout, deadline.and_then(OperationDeadline::remaining)) {
            (Some(request), Some(parent)) => Some(request.min(parent)),
            (Some(request), None) => Some(request),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        };
        #[cfg(test)]
        let native_url = url.as_str().to_owned();

        let mut builder = self
            .client
            .request(method.clone(), url)
            .headers(headers.clone());
        if let Some(body) = &body {
            builder = builder.body(body.clone());
        }

        let operation = async {
            #[cfg(test)]
            let response = {
                use concord_test_support::script::BodyCategory;
                let boundary = super::development::boundary_request(
                    "credential-provider",
                    method.clone(),
                    None,
                    &headers,
                    if body.is_some() {
                        BodyCategory::Bytes
                    } else {
                        BodyCategory::Empty
                    },
                    body.as_ref().map(Bytes::len),
                    body.clone(),
                )
                .__with_native(native_url, &headers);
                self.execute_at_boundary(builder, boundary)
                    .await
                    .map_err(auth_http_error)?
            };
            #[cfg(not(test))]
            let response = builder
                .send()
                .await
                .map_err(|error| auth_http_error(reqwest_category(error, false)))?;

            if response
                .content_length()
                .is_some_and(|length| length > effective_response_limit as u64)
            {
                return Err(AuthError::new(AuthErrorKind::ResponseTooLarge));
            }
            collect_provider_response(response, effective_response_limit).await
        };
        match effective_timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?,
            None => operation.await,
        }
    }

    #[cfg(test)]
    fn validate_response(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ErrorCategory> {
        if response
            .content_length()
            .is_some_and(|length| length > self.response_limit.get() as u64)
        {
            return Err(ErrorCategory::ResponseBody);
        }
        Ok(response)
    }

    #[cfg(test)]
    pub(super) async fn execute_at_boundary(
        &self,
        request: reqwest::RequestBuilder,
        boundary: BoundaryRequest,
    ) -> Result<reqwest::Response, ErrorCategory> {
        let response = if let Some(development) = &self.development {
            drop(request);
            development
                .__execute(boundary)
                .await
                .map(super::development::scripted_response)
                .map_err(|failure| super::development::failure_category(failure.kind()))?
        } else {
            request
                .send()
                .await
                .map_err(|error| reqwest_category(error, false))?
        };
        self.validate_response(response)
    }
}

async fn collect_provider_response(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<ProviderResponse, AuthError> {
    let status = response.status();
    let headers = response.headers().clone();
    let mut collected = BytesMut::new();
    let mut progress = ResponseChunkProgress::default();
    loop {
        let chunk = next_nonempty_chunk(&mut response, &mut progress)
            .await
            .map_err(|error| match error {
                ResponseChunkError::Acquisition => AuthError::new(AuthErrorKind::Acquisition),
                ResponseChunkError::LimitExceeded => {
                    AuthError::new(AuthErrorKind::ResponseTooLarge)
                }
            })?;
        let Some(chunk) = chunk else {
            return Ok(ProviderResponse {
                status,
                headers,
                body: collected.freeze(),
            });
        };
        let next = collected
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| AuthError::new(AuthErrorKind::ResponseTooLarge))?;
        if next > limit {
            return Err(AuthError::new(AuthErrorKind::ResponseTooLarge));
        }
        collected.extend_from_slice(&chunk);
    }
}

fn auth_http_error(category: ErrorCategory) -> AuthError {
    let kind = match category {
        ErrorCategory::RequestBody => AuthErrorKind::RequestTooLarge,
        ErrorCategory::ResponseBody => AuthErrorKind::ResponseTooLarge,
        ErrorCategory::Configuration => AuthErrorKind::InvalidConfiguration,
        _ => AuthErrorKind::Acquisition,
    };
    AuthError::new(kind)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};

    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;
    use concord_test_support::script::ScriptedResponse;
    use futures_core::Stream;

    use super::*;
    use crate::http::response_progress::test_support::{empty_chunks, response as native_response};
    use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};

    struct InfiniteEmptyBody {
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
        fairness_reached: Arc<tokio::sync::Notify>,
    }

    impl Stream for InfiniteEmptyBody {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let polls = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            if polls == MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
                self.fairness_reached.notify_one();
            }
            Poll::Ready(Some(Ok(Bytes::new())))
        }
    }

    impl Drop for InfiniteEmptyBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn infinite_empty_response(
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        fairness_reached: &Arc<tokio::sync::Notify>,
    ) -> reqwest::Response {
        let body = reqwest::Body::wrap_stream(InfiniteEmptyBody {
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
            fairness_reached: Arc::clone(fairness_reached),
        });
        http::Response::new(body).into()
    }

    struct CountingFactory {
        calls: Arc<AtomicUsize>,
        bytes: Bytes,
    }

    impl ProviderBodyFactory for CountingFactory {
        fn create(&self) -> Result<Bytes, AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.bytes.clone())
        }
    }

    fn inner(
        executor: &DeterministicExecutor,
        request_limit: usize,
        response_limit: usize,
        tls: TlsCapability,
    ) -> ProviderHttpInner {
        let _ = rustls::crypto::ring::default_provider().install_default();
        ProviderHttpInner::new(
            reqwest::Client::new(),
            tls,
            NonZeroUsize::new(request_limit).expect("request limit"),
            NonZeroUsize::new(response_limit).expect("response limit"),
        )
        .with_development(executor.provider())
    }

    fn error<T>(result: Result<T, AuthError>) -> AuthError {
        match result {
            Ok(_) => panic!("expected provider HTTP failure"),
            Err(error) => error,
        }
    }

    #[test]
    fn provider_http_exposes_the_active_request_body_limit() {
        let executor = DeterministicExecutor::new();
        let provider = inner(&executor, 37, 64, TlsCapability::Available);
        assert_eq!(provider.capability(None).request_body_limit().get(), 37);
    }

    #[tokio::test]
    async fn non_https_and_missing_tls_fail_before_factory_or_native_execution() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let factory = || {
            ProviderBody::Factory(Arc::new(CountingFactory {
                calls: Arc::clone(&calls),
                bytes: Bytes::from_static(b"protected-provider-body"),
            }))
        };
        let available = inner(&executor, 64, 64, TlsCapability::Available);
        let cleartext = ProviderRequest::new(
            Method::POST,
            Url::parse("http://provider.example.test/token").expect("URL"),
        )
        .body(factory());
        assert_eq!(
            error(available.capability(None).send(cleartext).await).kind(),
            AuthErrorKind::InvalidConfiguration
        );

        let unavailable = inner(&executor, 64, 64, TlsCapability::Unavailable);
        let protected = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/token").expect("URL"),
        )
        .body(factory());
        assert_eq!(
            error(unavailable.capability(None).send(protected).await).kind(),
            AuthErrorKind::InvalidConfiguration
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test]
    async fn request_limit_runs_factory_once_and_stops_before_provider_channel() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 3, 64, TlsCapability::Available);
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/token").expect("URL"),
        )
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"four"),
        })));
        assert_eq!(
            error(provider.capability(None).send(request).await).kind(),
            AuthErrorKind::RequestTooLarge
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
    }

    #[tokio::test]
    async fn successful_provider_operation_invokes_body_factory_exactly_once() {
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_expected_response(
                concord_test_support::script::ExpectedRequest::new()
                    .method(Method::POST)
                    .body(Bytes::from_static(b"factory-body")),
                ScriptedResponse::success(Bytes::from_static(b"ok")),
            )
            .expect("provider response");
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/factory").expect("URL"),
        )
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"factory-body"),
        })));
        let response = provider
            .capability(None)
            .send(request)
            .await
            .expect("provider operation");
        assert_eq!(response.body(), &Bytes::from_static(b"ok"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
    }

    #[tokio::test]
    async fn provider_owns_non_success_status_headers_and_bounded_body() {
        let executor = DeterministicExecutor::new();
        let mut headers = HeaderMap::new();
        headers.insert("x-provider", HeaderValue::from_static("present"));
        executor
            .provider()
            .queue_response(
                ScriptedResponse::status(
                    StatusCode::TOO_MANY_REQUESTS,
                    Bytes::from_static(b"safe"),
                )
                .headers(headers),
            )
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let response = provider
            .capability(None)
            .send(ProviderRequest::new(
                Method::GET,
                Url::parse("https://provider.example.test/status").expect("URL"),
            ))
            .await
            .expect("non-success belongs to provider");
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get("x-provider"),
            Some(&HeaderValue::from_static("present"))
        );
        assert_eq!(response.body(), &Bytes::from_static(b"safe"));
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test]
    async fn provider_response_suppresses_empty_chunks() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let response = collect_provider_response(
            native_response(
                [
                    Ok(Bytes::new()),
                    Ok(Bytes::from_static(b"provider")),
                    Ok(Bytes::new()),
                ],
                &polls,
                &dropped,
            ),
            64,
        )
        .await
        .expect("provider response");

        assert_eq!(response.body(), &Bytes::from_static(b"provider"));
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn provider_response_yields_executor_after_ready_empty_budget() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
        chunks.push(Ok(Bytes::from_static(b"deferred")));
        let mut collection = Box::pin(collect_provider_response(
            native_response(chunks, &polls, &dropped),
            64,
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(matches!(
            collection.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
    }

    #[tokio::test]
    async fn provider_response_first_excess_is_safe() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));

        let error = error(
            collect_provider_response(
                native_response(
                    empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
                    &polls,
                    &dropped,
                ),
                64,
            )
            .await,
        );
        assert_eq!(error.kind(), AuthErrorKind::ResponseTooLarge);
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 1);
        assert!(dropped.load(Ordering::SeqCst));
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
        assert!(!rendered.contains("empty chunk"));
    }

    #[tokio::test(start_paused = true)]
    async fn provider_response_deadline_can_fire_during_empty_chunk_run() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let fairness_reached = Arc::new(tokio::sync::Notify::new());
        let response = infinite_empty_response(&polls, &dropped, &fairness_reached);
        let collection = tokio::spawn(async move {
            tokio::time::timeout(
                Duration::from_secs(1),
                collect_provider_response(response, 64),
            )
            .await
        });
        fairness_reached.notified().await;
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(collection.await.expect("provider collection task").is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn provider_response_error_contains_no_body_or_url() {
        const BODY_SENTINEL: &str = "provider-body-secret-61f8";
        const URL_SENTINEL: &str = "https://provider-secret.invalid/token";
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = vec![Ok(Bytes::from_static(BODY_SENTINEL.as_bytes()))];
        chunks.extend(empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1));

        let error =
            error(collect_provider_response(native_response(chunks, &polls, &dropped), 64).await);
        let rendered = format!("{error} {error:?}");
        assert_eq!(error.kind(), AuthErrorKind::ResponseTooLarge);
        assert!(!rendered.contains(BODY_SENTINEL));
        assert!(!rendered.contains(URL_SENTINEL));
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
    }

    #[tokio::test]
    async fn known_streamed_and_request_local_response_limits_fail_closed() {
        let executor = DeterministicExecutor::new();
        let provider = inner(&executor, 64, 4, TlsCapability::Available);

        let mut known_headers = HeaderMap::new();
        known_headers.insert("content-length", HeaderValue::from_static("5"));
        executor
            .provider()
            .queue_response(
                ScriptedResponse::success(Bytes::from_static(b"12345")).headers(known_headers),
            )
            .expect("known response");
        let known = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/known").expect("URL"),
        );
        assert_eq!(
            error(provider.capability(None).send(known).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"12345")))
            .expect("streamed response");
        let streamed = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/streamed").expect("URL"),
        );
        assert_eq!(
            error(provider.capability(None).send(streamed).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"1234")))
            .expect("request-local response");
        let reduced = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/reduced").expect("URL"),
        )
        .response_limit(NonZeroUsize::new(3).expect("limit"));
        assert_eq!(
            error(provider.capability(None).send(reduced).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"12345")))
            .expect("non-increasing response");
        let cannot_increase = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/capped").expect("URL"),
        )
        .response_limit(NonZeroUsize::new(64).expect("limit"));
        assert_eq!(
            error(provider.capability(None).send(cannot_increase).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );
    }

    #[tokio::test]
    async fn expired_parent_budget_prevents_factory_and_provider_execution() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let deadline = OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::ZERO))
            .expect("deadline");
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/deadline").expect("URL"),
        )
        .timeout(Duration::from_secs(30))
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"body"),
        })));
        assert_eq!(
            error(provider.capability(Some(deadline)).send(request).await).kind(),
            AuthErrorKind::Acquisition
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn earlier_request_timeout_bounds_the_complete_provider_operation() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .provider()
            .queue_gate(gate.clone())
            .expect("provider gate");
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let parent =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(30)))
                .expect("parent deadline");
        let request = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/request-timeout").expect("URL"),
        )
        .timeout(Duration::from_secs(5));
        let send = provider.capability(Some(parent)).send(request);
        tokio::pin!(send);
        tokio::select! {
            () = gate.wait_until_reached() => {}
            result = &mut send => panic!("provider operation ended before its gate: {}", result.is_ok()),
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(error(send.await).kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn earlier_parent_budget_bounds_provider_request_timeout() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .provider()
            .queue_gate(gate.clone())
            .expect("provider gate");
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let parent =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(3)))
                .expect("parent deadline");
        let request = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/parent-timeout").expect("URL"),
        )
        .timeout(Duration::from_secs(30));
        let send = provider.capability(Some(parent)).send(request);
        tokio::pin!(send);
        tokio::select! {
            () = gate.wait_until_reached() => {}
            result = &mut send => panic!("provider operation ended before its gate: {}", result.is_ok()),
        }
        tokio::time::advance(Duration::from_secs(3)).await;
        assert_eq!(error(send.await).kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }
}
