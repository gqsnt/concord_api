use bytes::Bytes;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind,
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, BuiltRequest, BuiltResponse,
    CacheAfter, CacheBefore, CacheConfig, CacheFuture, CacheKey, CacheRevalidation, CacheStore,
    DecodedResponse, PostResponseHookContext, PreSendHookContext, RateLimitContext,
    RateLimitFuture, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
    RateLimiter, RequestMeta, RuntimeHooks, Transport, TransportBody, TransportError,
    TransportErrorHookContext, TransportErrorKind, TransportRequest, TransportResponse,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, PaginationPlan, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, ApiKey, ClientContext, Endpoint};
use concord_core::prelude::{HasNextCursor, PageItems};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct TestAuthVars {
    pub token: Option<String>,
    pub identity: &'static str,
}

impl Default for TestAuthVars {
    fn default() -> Self {
        Self {
            token: None,
            identity: "anon",
        }
    }
}

#[derive(Clone)]
pub struct TestCx;

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, AuthError>,
    > {
        Box::pin(async move {
            let token = auth.token.as_deref().ok_or_else(|| {
                AuthError::new(
                    AuthErrorKind::MissingCredential,
                    format!(
                        "missing credential `{}`; acquire or configure it before sending request",
                        requirement.credential.id
                    ),
                )
            })?;
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let material = ApiKey::new(token.to_string());
                    concord_core::advanced::apply_secret_credential(
                        request,
                        requirement,
                        &material,
                    )?
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "test context supports bearer/header/query auth only",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(concord_core::advanced::PreparedAuthCredential::new(
                applied,
                application,
            ))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move {
            if status == StatusCode::UNAUTHORIZED {
                if auth.identity == "refresh" {
                    return Ok(AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: concord_core::advanced::AuthRetryReason::Unauthorized,
                    });
                }
                if auth.identity == "refresh-error" {
                    return Err(AuthError::new(
                        AuthErrorKind::ProviderRejected,
                        "auth refresh failed",
                    ));
                }
                Ok(AuthDecision::Fail)
            } else {
                Ok(AuthDecision::Continue)
            }
        })
    }
}

#[derive(Clone)]
pub struct TextEndpoint {
    pub name: &'static str,
    pub method: Method,
    pub path: &'static str,
    pub policy: ResolvedPolicy,
    pub pagination: Option<PaginationPlan>,
}

impl Default for TextEndpoint {
    fn default() -> Self {
        Self {
            name: "Text",
            method: Method::GET,
            path: "/text",
            policy: ResolvedPolicy::default(),
            pagination: None,
        }
    }
}

impl Endpoint<TestCx> for TextEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        ))
    }
}

#[derive(Clone)]
pub struct ItemsEndpoint {
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Endpoint<TestCx> for ItemsEndpoint {
    type Response = Vec<String>;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            "Items",
            Method::GET,
            "/items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_items,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorItems {
    pub items: Vec<String>,
    pub next: Option<String>,
}

impl PageItems for CursorItems {
    type Item = String;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

impl HasNextCursor for CursorItems {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        self.next.clone()
    }
}

#[derive(Clone)]
pub struct CursorItemsEndpoint {
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Endpoint<TestCx> for CursorItemsEndpoint {
    type Response = CursorItems;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            "CursorItems",
            Method::GET,
            "/cursor-items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_cursor_items,
        ))
    }
}

pub fn request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    pagination: Option<PaginationPlan>,
    decode: fn(
        BuiltResponse,
        concord_core::advanced::ErrorContext,
    ) -> Result<Box<dyn std::any::Any + Send>, ApiClientError>,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body: BodyPlan::None,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
                decode,
            },
            pagination,
        },
        args: RequestArgs::default(),
        overrides: RequestOverrides::default(),
    }
}

pub fn decode_string(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let value = std::str::from_utf8(&resp.body)
        .map(str::to_string)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value,
    }))
}

pub fn decode_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let value = if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').map(ToOwned::to_owned).collect()
    };
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value,
    }))
}

pub fn decode_cursor_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let (items_text, next_text) = text.split_once('|').unwrap_or((text, ""));
    let items = if items_text.is_empty() {
        Vec::new()
    } else {
        items_text.split(',').map(ToOwned::to_owned).collect()
    };
    let next = next_text
        .strip_prefix("next=")
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: CursorItems { items, next },
    }))
}

pub fn auth_policy(placement: AuthPlacement) -> ResolvedPolicy {
    ResolvedPolicy {
        auth: concord_core::advanced::AuthPlan {
            requirements: vec![AuthRequirement {
                credential: concord_core::advanced::CredentialRef {
                    id: concord_core::advanced::CredentialId::new("test", "token"),
                },
                placement,
                usage_id: AuthUsageId::new("test-token"),
                step_id: Some("test"),
                provenance: AuthProvenance::new("test"),
                challenge: Default::default(),
            }],
        },
        ..Default::default()
    }
}

pub fn cache_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        cache: concord_core::internal::CacheSetting::Config(CacheConfig::new()),
        ..Default::default()
    }
}

pub fn retry_policy(max_attempts: u32) -> ResolvedPolicy {
    retry_policy_for_statuses(max_attempts, vec![StatusCode::INTERNAL_SERVER_ERROR])
}

pub fn retry_policy_for_statuses(max_attempts: u32, statuses: Vec<StatusCode>) -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses,
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

pub fn retry_policy_for_transport_errors(
    max_attempts: u32,
    transport_errors: Vec<TransportErrorKind>,
) -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses: Vec::new(),
            transport_errors,
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct MockTransport {
    outcomes: Arc<Mutex<VecDeque<MockOutcome>>>,
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<TransportRequest>>>,
    delay: Option<Duration>,
}

#[derive(Clone)]
pub enum MockOutcome {
    Response(MockResponse),
    TransportError(TransportErrorKind),
}

impl From<MockResponse> for MockOutcome {
    fn from(value: MockResponse) -> Self {
        Self::Response(value)
    }
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub content_length: Option<u64>,
    pub chunks: Option<Vec<Bytes>>,
}

impl MockResponse {
    pub fn text(status: StatusCode, body: impl Into<Bytes>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        Self {
            status,
            headers,
            body: body.into(),
            content_length: None,
            chunks: None,
        }
    }

    pub fn with_content_length(mut self, content_length: Option<u64>) -> Self {
        self.content_length = content_length;
        self
    }

    pub fn with_chunks(mut self, chunks: Vec<Bytes>) -> Self {
        self.chunks = Some(chunks);
        self
    }
}

impl MockTransport {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        Self::with_outcomes(events, responses.into_iter().map(Into::into).collect())
    }

    pub fn with_outcomes(events: Arc<Mutex<Vec<String>>>, outcomes: Vec<MockOutcome>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(outcomes.into())),
            events,
            requests: Arc::new(Mutex::new(Vec::new())),
            delay: None,
        }
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    pub async fn requests(&self) -> Vec<TransportRequest> {
        self.requests.lock().await.clone()
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let outcomes = self.outcomes.clone();
        let events = self.events.clone();
        let requests = self.requests.clone();
        let delay = self.delay;
        Box::pin(async move {
            events.lock().await.push("transport".to_string());
            requests.lock().await.push(req.clone());
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let outcome = outcomes
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok").into());
            let response = match outcome {
                MockOutcome::Response(response) => response,
                MockOutcome::TransportError(kind) => {
                    return Err(TransportError::with_kind(
                        kind,
                        std::io::Error::other("mock transport error"),
                    ));
                }
            };
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length.or_else(|| {
                    response
                        .chunks
                        .is_none()
                        .then_some(response.body.len() as u64)
                }),
                rate_limit: req.rate_limit,
                body: if let Some(chunks) = response.chunks {
                    Box::new(ChunkBody {
                        chunks: chunks.into(),
                    })
                } else {
                    Box::new(StaticBody(Some(response.body)))
                },
            })
        })
    }
}

struct StaticBody(Option<Bytes>);

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}

struct ChunkBody {
    chunks: VecDeque<Bytes>,
}

impl TransportBody for ChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.chunks.pop_front()) })
    }
}

#[derive(Default)]
pub struct RecordingRateLimiter {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Default)]
pub struct RecordingRuntimeHooks {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl RecordingRuntimeHooks {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RuntimeHooks for RecordingRuntimeHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("pre_send".to_string());
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        _ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("classify_response".to_string());
        })
    }

    fn transport_error<'a>(
        &'a self,
        _ctx: TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("transport_error".to_string());
        })
    }
}

pub struct RecordingCache {
    pub before: CacheBefore,
    pub events: Arc<Mutex<Vec<String>>>,
    pub after_response_count: Arc<Mutex<u32>>,
    pub after_error_count: Arc<Mutex<u32>>,
    pub serve_stale_on_error: bool,
}

impl RecordingCache {
    pub fn miss(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            before: CacheBefore::Miss,
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
            serve_stale_on_error: false,
        }
    }

    pub fn hit(events: Arc<Mutex<Vec<String>>>, response: BuiltResponse) -> Self {
        Self {
            before: CacheBefore::Hit(response),
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
            serve_stale_on_error: false,
        }
    }

    pub fn revalidate(events: Arc<Mutex<Vec<String>>>, cached_response: BuiltResponse) -> Self {
        Self {
            before: CacheBefore::Revalidate {
                request_headers: HeaderMap::new(),
                cached: CacheRevalidation {
                    key: CacheKey::new("stale".to_string()),
                    cached_response,
                },
            },
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
            serve_stale_on_error: false,
        }
    }

    pub fn revalidate_stale_on_error(
        events: Arc<Mutex<Vec<String>>>,
        cached_response: BuiltResponse,
    ) -> Self {
        Self {
            before: CacheBefore::Revalidate {
                request_headers: HeaderMap::new(),
                cached: CacheRevalidation {
                    key: CacheKey::new("stale".to_string()),
                    cached_response,
                },
            },
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
            serve_stale_on_error: true,
        }
    }
}

impl CacheStore for RecordingCache {
    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            self.events.lock().await.push(format!(
                "cache_before:{}",
                request
                    .extensions
                    .auth_identities
                    .first()
                    .map(String::as_str)
                    .unwrap_or("<none>")
            ));
            if matches!(self.before, CacheBefore::Hit(_)) {
                self.events.lock().await.push("cache_hit".to_string());
            }
            self.before.clone()
        })
    }

    fn after_response<'a>(
        &'a self,
        request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.after_response_count.lock().await += 1;
            self.events
                .lock()
                .await
                .push("cache_after_response".to_string());
            if let concord_core::internal::CacheSetting::Config(config) = &request.cache
                && let Some(max_body_bytes) = config.max_body_bytes
                && response.body.len() > max_body_bytes
            {
                self.events
                    .lock()
                    .await
                    .push("cache_max_body_skip".to_string());
                return CacheAfter::NotStored(concord_core::advanced::CacheSkipReason::TooLarge);
            }
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            *self.after_error_count.lock().await += 1;
            self.events
                .lock()
                .await
                .push("cache_after_error".to_string());
            if self.serve_stale_on_error {
                revalidation.map(|cached| cached.cached_response)
            } else {
                None
            }
        })
    }
}

pub fn built_response(
    endpoint: &'static str,
    status: StatusCode,
    body: impl Into<Bytes>,
) -> BuiltResponse {
    BuiltResponse {
        meta: RequestMeta {
            endpoint,
            method: Method::GET,
            idempotent: true,
            attempt: 0,
            page_index: 0,
        },
        url: "https://example.com/text".parse().expect("test url"),
        status,
        headers: {
            let mut h = HeaderMap::new();
            h.insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            );
            h
        },
        body: body.into(),
        rate_limit: Default::default(),
    }
}

pub fn client(auth: TestAuthVars, transport: MockTransport) -> ApiClient<TestCx, MockTransport> {
    ApiClient::with_transport((), auth, transport)
}

pub fn configure_runtime(
    client: &mut ApiClient<TestCx, MockTransport>,
    cache: Option<Arc<dyn CacheStore>>,
    limiter: Option<Arc<dyn RateLimiter>>,
) {
    client.configure(|cfg| {
        cfg.debug(concord_core::prelude::DebugLevel::V);
        cfg.pagination(concord_core::advanced::Caps {
            max_pages: 8,
            max_items: 1_000,
            detect_loops: true,
        });
        if let Some(cache) = cache {
            cfg.cache_store(cache);
        }
        if let Some(limiter) = limiter {
            cfg.rate_limiter(limiter);
        }
    });
}
