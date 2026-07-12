use bytes::Bytes;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthHttpExecutor,
    AuthPlacement, AuthPreparationReuse, AuthRequirement, CredentialContext, CredentialId,
    CredentialProvider, CredentialRefreshReason, CredentialSlot, NoopDebugSink, NoopRateLimiter,
    PreparedAuthCredential, RequestMeta, RetryConfig, RetryIdempotency, Transport,
    apply_secret_credential,
};
use concord_core::internal::{
    ClientPlanContext, PreparedBody, RequestPlan, ResolvedPolicy, RetrySetting,
};
use concord_core::prelude::{AccessToken, ApiClient, ClientContext, Endpoint, IntoEndpointPlan};
use concord_core::{dangerous, error};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{Method, StatusCode};
use perf::support::{
    MockResponse, MockTransport, auth_requirement, client, execute_raw_plan, raw_plan_overrides,
    request_plan, runtime,
};
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn base_plan(name: &'static str, path: &'static str) -> RequestPlan {
    request_plan(
        name,
        Method::GET,
        path,
        ResolvedPolicy::default(),
        PreparedBody::empty(),
    )
}

fn with_auth(mut plan: RequestPlan, placement: AuthPlacement, label: &'static str) -> RequestPlan {
    plan.endpoint
        .policy
        .auth
        .requirements
        .push(auth_requirement(placement, label));
    plan
}

fn with_retry(mut plan: RequestPlan) -> RequestPlan {
    plan.endpoint.policy.retry = RetrySetting::Config(RetryConfig {
        max_attempts: 2,
        methods: vec![Method::GET],
        statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
        transport_errors: Vec::new(),
        respect_retry_after: false,
        idempotency: RetryIdempotency::SafeMethodsOnly,
    });
    plan
}

fn success_transport() -> MockTransport {
    MockTransport::repeating(MockResponse::text(
        StatusCode::OK,
        Bytes::from_static(b"ok"),
    ))
}

fn retry_transport() -> MockTransport {
    MockTransport::scripted(vec![
        MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            Bytes::from_static(b"retry"),
        ),
        MockResponse::text(StatusCode::OK, Bytes::from_static(b"ok")),
    ])
}

fn bench_success<F>(c: &mut Criterion, name: &str, setup_plan: F)
where
    F: Fn() -> RequestPlan + Clone + 'static,
{
    let rt = runtime();
    c.bench_function(name, |b| {
        let setup_plan = setup_plan.clone();
        b.to_async(&rt).iter_batched(
            move || (client(success_transport()), setup_plan()),
            move |(client, plan)| async move {
                let response = execute_raw_plan(&client, plan).await.expect("auth bench");
                black_box((response.status(), response.body().len()));
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_collision(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("collision/query/error_path", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let mut plan = with_auth(
                    base_plan("AuthCollision", "/perf/auth-collision"),
                    AuthPlacement::Query("api_key"),
                    "query",
                );
                plan.endpoint
                    .policy
                    .query
                    .push(("api_key".to_string(), "public-value".to_string()));
                (client(success_transport()), plan)
            },
            |(client, plan)| async move {
                let err = execute_raw_plan(&client, plan)
                    .await
                    .expect_err("query auth collision should fail");
                black_box(err.to_string().len());
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_repeated_credential(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("repeated_credential/retry_reuses_material", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let plan = with_retry(with_auth(
                    base_plan("AuthRetry", "/perf/auth-retry"),
                    AuthPlacement::Bearer,
                    "bearer",
                ));
                (client(retry_transport()), plan)
            },
            |(client, plan)| async move {
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("auth retry bench");
                black_box((response.status(), response.body().len()));
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_cached_preparation(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("cached_preparation/slot_retry_reuses_preparation", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let plan = with_retry(with_auth(
                    base_plan("CachedPreparation", "/perf/cached-preparation"),
                    AuthPlacement::Bearer,
                    "bearer",
                ));
                (slot_client(retry_transport()), plan)
            },
            |(client, plan)| async move {
                let response = execute_slot_raw_plan(&client, plan)
                    .await
                    .expect("slot auth retry bench");
                black_box((response.status(), response.body().len()));
            },
            BatchSize::SmallInput,
        )
    });
}

#[derive(Clone)]
struct SlotAuthCx;

#[derive(Clone)]
struct SlotAuthVars {
    slot: Arc<CredentialSlot<SlotAuthCx, BenchTokenProvider>>,
}

#[derive(Clone)]
struct SlotAuthState {
    slot: Arc<CredentialSlot<SlotAuthCx, BenchTokenProvider>>,
}

#[derive(Clone, Default)]
struct BenchTokenProvider {
    acquire_calls: Arc<AtomicUsize>,
}

impl CredentialProvider<SlotAuthCx> for BenchTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("bench", "slot-token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, SlotAuthCx>,
    ) -> concord_core::advanced::AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            self.acquire_calls.fetch_add(1, Ordering::Relaxed);
            Ok(AccessToken::new("BENCH_FAKE_TOKEN".to_string()))
        })
    }
}

impl ClientContext for SlotAuthCx {
    type Vars = ();
    type AuthVars = SlotAuthVars;
    type AuthState = SlotAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        SlotAuthState {
            slot: auth.slot.clone(),
        }
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let lease = auth_state
                .slot
                .get_or_refresh(
                    CredentialContext {
                        vars,
                        auth,
                        auth_state,
                        executor,
                        credential_id: requirement.credential.id.clone(),
                        reason: CredentialRefreshReason::Missing,
                    },
                    Default::default(),
                )
                .await?;
            let material = apply_secret_credential(request, requirement, &lease.value)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(lease.generation),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, material)
                .with_reuse(AuthPreparationReuse::RequestLocal))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: StatusCode,
        _headers: &'a http::HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move { Ok(AuthDecision::Continue) })
    }
}

struct SlotRawPlanEndpoint {
    plan: RequestPlan,
}

impl Endpoint<SlotAuthCx> for SlotRawPlanEndpoint {
    type Response = ();

    fn execute<'a, T>(
        _client: &'a ApiClient<SlotAuthCx, T>,
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
                msg: "SlotRawPlanEndpoint only supports execute_raw_response",
            })
        })
    }
}

impl IntoEndpointPlan<SlotAuthCx> for SlotRawPlanEndpoint {
    fn into_plan(
        self,
        _ctx: &ClientPlanContext<'_, SlotAuthCx>,
    ) -> Result<RequestPlan, error::ApiClientError> {
        raw_plan_overrides(&self.plan)?;
        Ok(self.plan)
    }
}

async fn execute_slot_raw_plan<T: Transport>(
    client: &ApiClient<SlotAuthCx, T>,
    plan: RequestPlan,
) -> Result<dangerous::BuiltResponse, error::ApiClientError> {
    let overrides = raw_plan_overrides(&plan)?;
    let mut pending = client.request(SlotRawPlanEndpoint { plan });
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

fn slot_client(transport: MockTransport) -> ApiClient<SlotAuthCx, MockTransport> {
    let slot = Arc::new(CredentialSlot::new(BenchTokenProvider::default()));
    let mut client = ApiClient::with_transport((), SlotAuthVars { slot }, transport);
    client.configure(|cfg| {
        cfg.debug_sink(Arc::new(NoopDebugSink));
        cfg.rate_limiter(Arc::new(NoopRateLimiter::new()));
    });
    client
}

fn bench_cached_slot(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("cached_credential/slot_two_requests", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let plan = with_auth(
                    base_plan("CachedSlotAuth", "/perf/cached-slot-auth"),
                    AuthPlacement::Bearer,
                    "bearer",
                );
                (slot_client(success_transport()), plan)
            },
            |(client, plan)| async move {
                let first = execute_slot_raw_plan(&client, plan)
                    .await
                    .expect("slot acquire request");
                let second = execute_slot_raw_plan(
                    &client,
                    with_auth(
                        base_plan("CachedSlotAuth", "/perf/cached-slot-auth"),
                        AuthPlacement::Bearer,
                        "bearer",
                    ),
                )
                .await
                .expect("slot cached request");
                black_box((first.status(), second.status()));
            },
            BatchSize::SmallInput,
        )
    });
}

fn auth_runtime(c: &mut Criterion) {
    bench_success(c, "baseline/no_auth", || {
        base_plan("NoAuth", "/perf/no-auth")
    });
    bench_success(c, "apply/bearer", || {
        with_auth(
            base_plan("BearerAuth", "/perf/bearer-auth"),
            AuthPlacement::Bearer,
            "bearer",
        )
    });
    bench_success(c, "apply/header", || {
        with_auth(
            base_plan("HeaderAuth", "/perf/header-auth"),
            AuthPlacement::Header("X-Api-Key"),
            "header",
        )
    });
    bench_success(c, "apply/query", || {
        with_auth(
            base_plan("QueryAuth", "/perf/query-auth"),
            AuthPlacement::Query("api_key"),
            "query",
        )
    });
    bench_success(c, "apply/multiple_requirements", || {
        with_auth(
            with_auth(
                base_plan("MultipleAuth", "/perf/multiple-auth"),
                AuthPlacement::Bearer,
                "bearer",
            ),
            AuthPlacement::Query("api_key"),
            "query",
        )
    });
    bench_collision(c);
    bench_repeated_credential(c);
    bench_cached_preparation(c);
    bench_cached_slot(c);
}

criterion_group!(benches, auth_runtime);
criterion_main!(benches);
