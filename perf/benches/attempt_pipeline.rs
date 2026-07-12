use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, NoopDebugSink, RateLimiter, RetryBackoff, RetryConfig, RetryIdempotency,
};
use concord_core::internal::{PreparedBody, RequestOverrides, RequestPlan, ResolvedPolicy};
use concord_core::prelude::DebugLevel;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderValue, Method, StatusCode, header::HeaderName};
use perf::support::{
    CountingRateLimiter, MockResponse, MockTransport, auth_requirement, client, configured_client,
    execute_raw_plan, request_plan, runtime,
};
use std::hint::black_box;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn base_plan(name: &'static str, method: Method, path: &'static str) -> RequestPlan {
    request_plan(
        name,
        method,
        path,
        ResolvedPolicy::default(),
        PreparedBody::empty(),
    )
}

fn with_headers(mut plan: RequestPlan, count: usize) -> RequestPlan {
    for idx in 0..count {
        let name = format!("x-bench-header-{idx}");
        plan.endpoint.policy.headers.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("valid benchmark header name"),
            HeaderValue::from_str(&format!("value-{idx}")).expect("valid benchmark header value"),
        );
    }
    plan
}

fn with_query_params(mut plan: RequestPlan, count: usize) -> RequestPlan {
    for idx in 0..count {
        plan.endpoint
            .policy
            .query
            .push((format!("query-{idx}"), format!("value-{idx}")));
    }
    plan
}

fn with_retry(mut plan: RequestPlan, max_attempts: u32) -> RequestPlan {
    plan.endpoint.policy.retry = concord_core::internal::RetrySetting::Config(RetryConfig {
        max_attempts,
        methods: vec![Method::GET],
        statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
        transport_errors: Vec::new(),
        backoff: RetryBackoff::None,
        respect_retry_after: false,
        idempotency: RetryIdempotency::SafeMethodsOnly,
    });
    plan
}

fn with_bearer_auth(mut plan: RequestPlan) -> RequestPlan {
    plan.endpoint
        .policy
        .auth
        .requirements
        .push(auth_requirement(AuthPlacement::Bearer, "bearer"));
    plan
}

fn with_query_auth(mut plan: RequestPlan) -> RequestPlan {
    plan.endpoint
        .policy
        .auth
        .requirements
        .push(auth_requirement(
            AuthPlacement::Query("auth_token"),
            "query",
        ));
    plan
}

fn bench_case<F>(c: &mut Criterion, runtime: &Runtime, name: &str, mut setup: F)
where
    F: FnMut() -> (
        concord_core::prelude::ApiClient<perf::support::PerfCx, MockTransport>,
        RequestPlan,
    ),
{
    c.bench_function(name, |b| {
        b.to_async(runtime).iter_batched(
            &mut setup,
            |(client, plan)| async move {
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("benchmark request");
                black_box((response.status(), response.body().len()));
            },
            BatchSize::SmallInput,
        )
    });
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

fn minimal_get(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "mock_transport_success/minimal_get", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = base_plan("MinimalGet", Method::GET, "/perf/minimal-get");
        (client, plan)
    });
}

fn many_headers(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "many_headers/32", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = with_headers(
            base_plan("ManyHeaders", Method::GET, "/perf/many-headers"),
            32,
        );
        (client, plan)
    });
}

fn many_query_params(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "many_query_params/32", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = with_query_params(
            base_plan("ManyQueryParams", Method::GET, "/perf/many-query"),
            32,
        );
        (client, plan)
    });
}

fn bearer_auth(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "bearer_auth", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = with_bearer_auth(base_plan("BearerAuth", Method::GET, "/perf/bearer-auth"));
        (client, plan)
    });
}

fn query_auth(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "query_auth", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = with_query_auth(base_plan("QueryAuth", Method::GET, "/perf/query-auth"));
        (client, plan)
    });
}

fn retry_configured_success(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "retry_configured_but_success", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = with_retry(
            base_plan("RetryConfigured", Method::GET, "/perf/retry-configured"),
            2,
        );
        (client, plan)
    });
}

fn retry_once_then_success(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "retry_once_then_success", || {
        let transport = retry_transport();
        let client = client(transport);
        let plan = with_retry(base_plan("RetryOnce", Method::GET, "/perf/retry-once"), 2);
        (client, plan)
    });
}

fn debug_levels(c: &mut Criterion, rt: &Runtime) {
    for level in [DebugLevel::None, DebugLevel::V, DebugLevel::VV] {
        let name = format!("debug_level/{level}");
        bench_case(c, rt, &name, move || {
            let transport = success_transport();
            let mut client = client(transport);
            client.set_debug_sink(Arc::new(NoopDebugSink));
            client.set_debug_level(level);
            let mut plan = base_plan("DebugLevel", Method::GET, "/perf/debug-level");
            plan.overrides = RequestOverrides {
                debug_level: Some(level),
                ..Default::default()
            };
            (client, plan)
        });
    }
}

fn noop_hooks_and_rate_limiter(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "noop_hooks/noop_rate_limiter", || {
        let transport = success_transport();
        let client = client(transport);
        let plan = base_plan("NoopRuntime", Method::GET, "/perf/noop-runtime");
        (client, plan)
    });
}

fn custom_rate_limiter(c: &mut Criterion, rt: &Runtime) {
    bench_case(c, rt, "custom_rate_limiter/counting", || {
        let transport = success_transport();
        let limiter = Arc::new(CountingRateLimiter::default()) as Arc<dyn RateLimiter>;
        let client = configured_client(transport, DebugLevel::None, limiter);
        let plan = base_plan("CountingLimiter", Method::GET, "/perf/counting-limiter");
        (client, plan)
    });
}

fn attempt_pipeline(c: &mut Criterion) {
    let rt = runtime();
    minimal_get(c, &rt);
    many_headers(c, &rt);
    many_query_params(c, &rt);
    bearer_auth(c, &rt);
    query_auth(c, &rt);
    retry_configured_success(c, &rt);
    retry_once_then_success(c, &rt);
    debug_levels(c, &rt);
    noop_hooks_and_rate_limiter(c, &rt);
    custom_rate_limiter(c, &rt);
}

criterion_group!(benches, attempt_pipeline);
criterion_main!(benches);
