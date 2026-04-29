use bytes::Bytes;
use concord_core::advanced::{
    AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind, AuthIdentity, AuthPlacement,
    AuthProvenance, AuthRequirement, AuthUsageId, BuiltRequest, BuiltResponse, CacheAfter,
    CacheBefore, CacheConfig, CacheFuture, CacheKey, CacheRevalidation, CacheStore,
    DecodedResponse, InflightRegistry, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, RequestMeta,
    SafeMethodInflightPolicy, Transport, TransportBody, TransportError, TransportResponse,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, PaginationPlan, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, ClientContext, Endpoint};
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
        request: &'a mut BuiltRequest,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthAppliedCredential, AuthError>> {
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
            match requirement.placement {
                AuthPlacement::Bearer => {
                    request.headers.insert(
                        http::header::AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
                            AuthError::new(
                                AuthErrorKind::UnsupportedScheme,
                                "invalid bearer token for authorization header",
                            )
                        })?,
                    );
                }
                AuthPlacement::Header(name) => {
                    request.headers.insert(
                        http::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                            AuthError::new(AuthErrorKind::UnsupportedScheme, "invalid header name")
                        })?,
                        HeaderValue::from_str(token).map_err(|_| {
                            AuthError::new(AuthErrorKind::UnsupportedScheme, "invalid header value")
                        })?,
                    );
                }
                AuthPlacement::Query(name) => {
                    request.url.query_pairs_mut().append_pair(name, token);
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "test context supports bearer/header/query auth only",
                    ));
                }
            }
            Ok(AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                identity: AuthIdentity::User(auth.identity.to_string()),
                provenance: requirement.provenance.clone(),
            })
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move {
            if status == StatusCode::UNAUTHORIZED {
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
                accept: "text/plain",
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
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct MockTransport {
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<BuiltRequest>>>,
    delay: Option<Duration>,
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
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
        }
    }
}

impl MockTransport {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
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

    pub async fn requests(&self) -> Vec<BuiltRequest> {
        self.requests.lock().await.clone()
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let responses = self.responses.clone();
        let events = self.events.clone();
        let requests = self.requests.clone();
        let delay = self.delay;
        Box::pin(async move {
            events.lock().await.push("transport".to_string());
            requests.lock().await.push(req.clone());
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let response = responses
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok"));
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: Some(response.body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody(Some(response.body))),
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

pub struct RecordingCache {
    pub before: CacheBefore,
    pub events: Arc<Mutex<Vec<String>>>,
    pub after_response_count: Arc<Mutex<u32>>,
    pub after_error_count: Arc<Mutex<u32>>,
}

impl RecordingCache {
    pub fn miss(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            before: CacheBefore::Miss,
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn hit(events: Arc<Mutex<Vec<String>>>, response: BuiltResponse) -> Self {
        Self {
            before: CacheBefore::Hit(response),
            events,
            after_response_count: Arc::new(Mutex::new(0)),
            after_error_count: Arc::new(Mutex::new(0)),
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
        }
    }
}

impl CacheStore for RecordingCache {
    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            self.events.lock().await.push(format!(
                "cache_before:{}",
                request
                    .headers
                    .get(http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("<none>")
            ));
            self.before.clone()
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.after_response_count.lock().await += 1;
            self.events
                .lock()
                .await
                .push("cache_after_response".to_string());
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            *self.after_error_count.lock().await += 1;
            self.events
                .lock()
                .await
                .push("cache_after_error".to_string());
            None
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
    inflight: bool,
    registry: Option<Arc<InflightRegistry>>,
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
        if inflight {
            cfg.inflight_policy(Arc::new(SafeMethodInflightPolicy));
        }
        if let Some(registry) = registry {
            cfg.inflight_registry = registry;
        }
    });
}
