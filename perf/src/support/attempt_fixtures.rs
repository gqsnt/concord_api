use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind,
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, NoopDebugSink, NoopRateLimiter,
    PreparedAuthCredential, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, Transport,
};
use concord_core::auth::{ApiKey, CredentialId, CredentialRef, apply_secret_credential};
use concord_core::error;
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, Replayability, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ClientContext, DebugLevel, Endpoint, IntoEndpointPlan};
use http::{HeaderValue, Method, StatusCode};
use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
pub struct PerfAuthVars {
    pub token: Option<String>,
}

#[derive(Clone)]
pub struct PerfCx;

impl ClientContext for PerfCx {
    type Vars = ();
    type AuthVars = PerfAuthVars;
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
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<PreparedAuthCredential, AuthError>,
    > {
        Box::pin(async move {
            let token = auth.token.as_deref().ok_or_else(|| {
                AuthError::new(AuthErrorKind::MissingCredential, "missing perf auth token")
            })?;
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let material = ApiKey::new(token.to_string());
                    apply_secret_credential(request, requirement, &material)?
                }
                AuthPlacement::Basic => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "perf context only uses bearer/header/query auth",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
        _status: StatusCode,
        _headers: &'a http::HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move { Ok(AuthDecision::Continue) })
    }
}

#[derive(Clone, Default)]
pub struct CountingRateLimiter {
    acquire_calls: Arc<AtomicUsize>,
    response_calls: Arc<AtomicUsize>,
}

impl CountingRateLimiter {
    pub fn acquire_calls(&self) -> usize {
        self.acquire_calls.load(Ordering::Relaxed)
    }

    pub fn response_calls(&self) -> usize {
        self.response_calls.load(Ordering::Relaxed)
    }
}

impl fmt::Debug for CountingRateLimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CountingRateLimiter")
            .field("acquire_calls", &self.acquire_calls())
            .field("response_calls", &self.response_calls())
            .finish()
    }
}

impl RateLimiter for CountingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, concord_core::prelude::ApiClientError>> {
        let acquire_calls = self.acquire_calls.clone();
        Box::pin(async move {
            acquire_calls.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, concord_core::prelude::ApiClientError>>
    {
        let response_calls = self.response_calls.clone();
        Box::pin(async move {
            response_calls.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

pub fn client<T: Transport>(transport: T) -> ApiClient<PerfCx, T> {
    configured_client(transport, DebugLevel::None, Arc::new(NoopRateLimiter::new()))
}

pub fn configured_client<T: Transport>(
    transport: T,
    debug_level: DebugLevel,
    rate_limiter: Arc<dyn RateLimiter>,
) -> ApiClient<PerfCx, T> {
    let mut client = ApiClient::with_transport(
        (),
        PerfAuthVars {
            token: Some("BENCH_FAKE_TOKEN".to_string()),
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.debug_sink(Arc::new(NoopDebugSink));
        cfg.debug_level(debug_level);
        cfg.rate_limiter(rate_limiter);
    });
    client
}

pub struct RawPlanEndpoint {
    plan: RequestPlan,
    _not_reusable: PhantomData<Cell<()>>,
}

pub fn raw_plan_overrides(
    plan: &RequestPlan,
) -> Result<RequestOverrides, error::ApiClientError> {
    if plan.overrides.page_index != 0 {
        return Err(error::ApiClientError::PolicyViolation {
            ctx: error::ErrorContext {
                endpoint: plan.endpoint.meta.name,
                method: plan.endpoint.meta.method.clone(),
            },
            msg: "performance raw-plan adapter requires page_index=0",
        });
    }
    Ok(plan.overrides.clone())
}

impl RawPlanEndpoint {
    pub fn new(plan: RequestPlan) -> Self {
        Self {
            plan,
            _not_reusable: PhantomData,
        }
    }
}

impl<Cx: ClientContext> Endpoint<Cx> for RawPlanEndpoint {
    type Response = ();

    fn execute<'a, T>(
        _client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, error::ApiClientError>> + Send + 'a>>
    where
        T: Transport + 'a,
    {
        let ctx = error::ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method,
        };
        Box::pin(async move {
            Err(error::ApiClientError::PolicyViolation {
                ctx,
                msg: "RawPlanEndpoint only supports execute_raw_response",
            })
        })
    }
}

impl IntoEndpointPlan<PerfCx> for RawPlanEndpoint {
    fn into_plan(
        self,
        _ctx: &ClientPlanContext<'_, PerfCx>,
    ) -> Result<RequestPlan, error::ApiClientError> {
        raw_plan_overrides(&self.plan)?;
        Ok(self.plan)
    }
}

pub async fn execute_raw_plan<T: Transport>(
    client: &ApiClient<PerfCx, T>,
    plan: RequestPlan,
) -> Result<concord_core::dangerous::BuiltResponse, error::ApiClientError> {
    let overrides = raw_plan_overrides(&plan)?;
    let mut pending = client.request(RawPlanEndpoint::new(plan));
    if let Some(level) = overrides.debug_level {
        pending = pending.debug_level(level);
    }
    if let Some(timeout) = overrides.timeout {
        pending = pending.timeout(timeout);
    }
    pending
        .attempt(overrides.attempt)
        .execute_raw_response()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use concord_core::advanced::{
        DebugSink, SanitizedHeaders, StreamBody, StreamBodyError, TransportError,
        TransportErrorKind, TransportRequest, TransportRequestBody, TransportResponse,
    };
    use concord_core::internal::BodyPlan;
    use futures_util::{StreamExt, stream};
    use std::sync::atomic::AtomicU8;
    use crate::support::{EmptyBody, MockResponse, MockTransport, runtime};

    #[derive(Default)]
    struct RecordingDebugSink {
        request_level: AtomicU8,
    }

    impl DebugSink for RecordingDebugSink {
        fn request_start(
            &self,
            dbg: DebugLevel,
            _method: &Method,
            _url: &str,
            _endpoint: &'static str,
            _page_index: u32,
        ) {
            self.request_level.store(dbg as u8, Ordering::Relaxed);
        }

        fn request_headers(&self, _dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {}

        fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, _url: &str, _ok: bool) {}

        fn response_headers(&self, _dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {}
    }

    #[derive(Clone)]
    struct BodyConsumingTransport {
        chunks: Arc<AtomicUsize>,
    }

    impl Transport for BodyConsumingTransport {
        fn send(
            &self,
            req: TransportRequest,
        ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
            let chunks = self.chunks.clone();
            Box::pin(async move {
                if let TransportRequestBody::Stream(mut body) = req.body {
                    while let Some(chunk) = body.next().await {
                        chunk?;
                        chunks.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status: StatusCode::OK,
                    headers: http::HeaderMap::new(),
                    content_length: Some(0),
                    rate_limit: req.rate_limit,
                    body: Box::new(EmptyBody),
                })
            })
        }
    }

    #[derive(Clone)]
    struct FailingTransport;

    impl Transport for FailingTransport {
        fn send(
            &self,
            _req: TransportRequest,
        ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
            Box::pin(async {
                Err(TransportError::with_kind(
                    TransportErrorKind::Connect,
                    std::io::Error::other("perf adapter transport failure"),
                ))
            })
        }
    }

    #[test]
    fn raw_plan_adapter_uses_public_escape_hatch_once() {
        let transport = MockTransport::repeating(MockResponse::text(
            StatusCode::OK,
            Bytes::from_static(b"ok"),
        ));
        let client = client(transport.clone());
        let plan = request_plan(
            "RawPlanAdapter",
            Method::GET,
            "/perf/raw-plan-adapter",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );

        let response = runtime()
            .block_on(execute_raw_plan(&client, plan))
            .expect("raw response should succeed");

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body.as_ref(), b"ok");
        let requests = transport.recorded_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].meta.endpoint, "RawPlanAdapter");
        assert_eq!(requests[0].meta.method, Method::GET);
    }

    #[test]
    fn raw_plan_adapter_typed_execution_returns_error_without_sending() {
        let transport = MockTransport::repeating(MockResponse::empty(StatusCode::OK));
        let client = client(transport.clone());
        let plan = request_plan(
            "RawPlanTypedAttempt",
            Method::GET,
            "/perf/raw-plan-typed-attempt",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );

        let error = runtime().block_on(
            client
                .request(RawPlanEndpoint::new(plan))
                .execute(),
        );

        assert!(matches!(
            error,
            Err(error::ApiClientError::PolicyViolation { .. })
        ));
        assert!(transport.recorded_requests().is_empty());
    }

    #[test]
    fn raw_plan_adapter_preserves_debug_timeout_and_attempt_overrides() {
        let transport = MockTransport::repeating(MockResponse::empty(StatusCode::OK));
        let sink = Arc::new(RecordingDebugSink::default());
        let mut client = client(transport.clone());
        client.set_debug_sink(sink.clone());
        client.set_debug_level(DebugLevel::None);
        let mut plan = request_plan(
            "RawPlanOverrides",
            Method::GET,
            "/perf/raw-plan-overrides",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );
        plan.overrides.debug_level = Some(DebugLevel::V);
        plan.overrides.timeout = Some(std::time::Duration::from_secs(17));
        plan.overrides.attempt = 4;

        runtime()
            .block_on(execute_raw_plan(&client, plan))
            .expect("overridden raw response should succeed");

        let request = &transport.recorded_requests()[0];
        assert_eq!(sink.request_level.load(Ordering::Relaxed), DebugLevel::V as u8);
        assert_eq!(request.meta.attempt, 4);
        assert_eq!(request.timeout, Some(std::time::Duration::from_secs(17)));
    }

    #[test]
    fn raw_plan_adapter_rejects_nonzero_page_index() {
        let transport = MockTransport::repeating(MockResponse::empty(StatusCode::OK));
        let client = client(transport.clone());
        let mut plan = request_plan(
            "RawPlanPageIndex",
            Method::GET,
            "/perf/raw-plan-page-index",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );
        plan.overrides.page_index = 3;

        let error = runtime().block_on(execute_raw_plan(&client, plan));
        assert!(matches!(
            error,
            Err(error::ApiClientError::PolicyViolation { msg, .. })
                if msg.contains("page_index=0")
        ));
        assert!(transport.recorded_requests().is_empty());
    }

    #[test]
    fn raw_plan_endpoint_rejects_nonzero_page_index_without_helper() {
        let transport = MockTransport::repeating(MockResponse::empty(StatusCode::OK));
        let client = client(transport.clone());
        let mut plan = request_plan(
            "RawPlanDirectPageIndex",
            Method::GET,
            "/perf/raw-plan-direct-page-index",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );
        plan.overrides.page_index = 9;

        let error = runtime().block_on(
            client
                .request(RawPlanEndpoint::new(plan))
                .execute_raw_response(),
        );
        assert!(matches!(
            error,
            Err(error::ApiClientError::PolicyViolation { msg, .. })
                if msg.contains("page_index=0")
        ));
        assert!(transport.recorded_requests().is_empty());
    }

    #[test]
    fn raw_plan_adapter_consumes_a_non_replayable_body_once() {
        let chunks = Arc::new(AtomicUsize::new(0));
        let transport = BodyConsumingTransport {
            chunks: chunks.clone(),
        };
        let client = client(transport);
        let stream = stream::iter(vec![Ok::<Bytes, StreamBodyError>(Bytes::from_static(
            b"one-shot",
        ))]);
        let mut plan = request_plan(
            "RawPlanOneShot",
            Method::POST,
            "/perf/raw-plan-one-shot",
            ResolvedPolicy::default(),
            BodyPlan::RawStream {
                content_type: HeaderValue::from_static("application/octet-stream"),
            },
            RequestArgs::with_stream_body(StreamBody::from_byte_stream(stream)),
            Replayability::NonReplayable,
        );
        plan.overrides.attempt = 2;

        runtime()
            .block_on(execute_raw_plan(&client, plan))
            .expect("one-shot raw response should succeed");
        assert_eq!(chunks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn raw_plan_adapter_propagates_transport_failure() {
        let client = client(FailingTransport);
        let plan = request_plan(
            "RawPlanTransportFailure",
            Method::GET,
            "/perf/raw-plan-transport-failure",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::empty(),
            Replayability::Replayable,
        );

        let error = runtime().block_on(execute_raw_plan(&client, plan));
        assert!(matches!(error, Err(error::ApiClientError::Transport { .. })));
    }
}

pub fn auth_requirement(placement: AuthPlacement, label: &'static str) -> AuthRequirement {
    AuthRequirement {
        credential: CredentialRef {
            id: CredentialId::new("bench", label),
        },
        placement,
        usage_id: AuthUsageId::new(label),
        step_id: Some("bench"),
        provenance: AuthProvenance::new("perf"),
        challenge: Default::default(),
    }
}

pub fn request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
    replayability: Replayability,
) -> RequestPlan {
    let idempotent = method == Method::GET || method == Method::HEAD;
    let meta_method = method.clone();
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: meta_method,
                idempotent,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
        replayability,
    }
}
