use bytes::Bytes;
use concord_core::advanced::{
    GovernorRateLimiter, NoopDebugSink, NoopRateLimiter, OffsetLimitPagination, PaginateBinding,
    PaginationRuntimeAdapter, RateLimiter, StreamBody, Transport, TransportError, TransportRequest,
    TransportResponse,
};
use concord_core::internal::{
    PreparedBody,
    ClientPlanContext, EndpointMeta, EndpointPlan, PaginationMarker, RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan, };
use concord_core::prelude::{
    ApiClientError, Endpoint, PageItems, PaginatedEndpoint, PaginationTermination, ReusableEndpoint,
    Text,
};
use futures_util::StreamExt;
use http::{Method, StatusCode, header::HeaderValue};
use perf::support::{
    AllocationSnapshot, CountingAllocator, EmptyBody, InMemoryAsyncRead, MockResponse,
    MockTransport, auth_requirement, client, context, execute_raw_plan, filled_bytes, request_plan,
    reset_allocation_counts, runtime, snapshot_allocation_counts,
};
use std::alloc::System;
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

fn base_plan(name: &'static str, path: &'static str) -> RequestPlan {
    request_plan(
        name,
        Method::GET,
        path,
        ResolvedPolicy::default(),
        PreparedBody::empty(),
    )
}

fn success_transport() -> MockTransport {
    MockTransport::repeating(MockResponse::text(
        StatusCode::OK,
        Bytes::from_static(b"ok"),
    ))
}

fn retry_transport() -> MockTransport {
    MockTransport::scripted(vec![
        MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, Bytes::from_static(b"retry")),
        MockResponse::text(StatusCode::OK, Bytes::from_static(b"ok")),
    ])
}

fn report_case<T, Setup, Run>(rt: &Runtime, label: &'static str, setup: Setup, mut run: Run)
where
    Setup: FnOnce() -> T,
    for<'a> Run: FnMut(&'a mut T) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
{
    let mut state = setup();
    reset_allocation_counts();
    rt.block_on(run(&mut state));
    let counts = snapshot_allocation_counts();
    print_report(label, counts);
    drop(state);
}

fn print_report(label: &str, counts: AllocationSnapshot) {
    println!(
        "allocation_counts/{label} allocs={} deallocs={} bytes_allocated={} bytes_deallocated={} report_only=true caveat=setup_teardown_excluded async_runtime_may_be_included",
        counts.alloc_calls,
        counts.dealloc_calls,
        counts.bytes_allocated,
        counts.bytes_deallocated,
    );
}

fn report_case_with_caveat<T, Setup, Run>(
    rt: &Runtime,
    label: &'static str,
    setup: Setup,
    mut run: Run,
    caveat: &'static str,
)
where
    Setup: FnOnce() -> T,
    for<'a> Run: FnMut(&'a mut T) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
{
    let mut state = setup();
    reset_allocation_counts();
    rt.block_on(run(&mut state));
    let counts = snapshot_allocation_counts();
    println!(
        "allocation_counts/{label} allocs={} deallocs={} bytes_allocated={} bytes_deallocated={} report_only=true caveat={caveat}",
        counts.alloc_calls,
        counts.dealloc_calls,
        counts.bytes_allocated,
        counts.bytes_deallocated,
    );
    drop(state);
}

fn sanity_check_counter() {
    reset_allocation_counts();
    {
        let mut probe = Vec::with_capacity(8);
        probe.extend_from_slice(b"sanity");
        black_box(&probe);
    }
    let counts = snapshot_allocation_counts();
    assert!(
        counts.alloc_calls > 0,
        "allocation counter sanity check should observe at least one allocation"
    );
    reset_allocation_counts();
}

fn with_bearer_auth(mut plan: RequestPlan) -> RequestPlan {
    plan.endpoint.policy.auth.requirements.push(auth_requirement(
        concord_core::advanced::AuthPlacement::Bearer,
        "bearer",
    ));
    plan
}

fn with_retry(mut plan: RequestPlan, max_attempts: u32) -> RequestPlan {
    plan.endpoint.policy.retry = concord_core::internal::RetrySetting::Config(
        concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        },
    );
    plan
}

fn run_attempt_pipeline(rt: &Runtime) {
    report_case(
        rt,
        "attempt_pipeline/mock_transport_success/minimal_get",
        || client(success_transport()),
        |client| {
            Box::pin(async move {
                let plan = base_plan("MinimalGet", "/perf/minimal-get");
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("allocation report request");
                black_box((response.status, response.body.len()));
            })
        },
    );

    report_case(
        rt,
        "attempt_pipeline/retry_once_then_success",
        || client(retry_transport()),
        |client| {
            Box::pin(async move {
                let plan = with_retry(base_plan("RetryOnce", "/perf/retry-once"), 2);
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("allocation report retry");
                black_box((response.status, response.body.len()));
            })
        },
    );
}

fn run_auth_runtime(rt: &Runtime) {
    report_case(
        rt,
        "auth_runtime/apply/bearer",
        || client(success_transport()),
        |client| {
            Box::pin(async move {
                let plan = with_bearer_auth(base_plan("BearerAuth", "/perf/bearer-auth"));
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("allocation report bearer auth");
                black_box((response.status, response.body.len()));
            })
        },
    );
}

fn run_redaction_hooks(rt: &Runtime) {
    report_case(
        rt,
        "redaction_hooks/headers/mixed_case",
        || {
            let transport = MockTransport::repeating(MockResponse::text(
                StatusCode::OK,
                Bytes::from_static(b"ok"),
            ));
            let mut client = client(transport);
            client.configure(|cfg| {
                cfg.debug_sink(Arc::new(NoopDebugSink));
                cfg.debug_level(concord_core::prelude::DebugLevel::VV);
                cfg.rate_limiter(Arc::new(NoopRateLimiter::new()));
            });
            client
        },
        |client| {
            Box::pin(async move {
                let mut policy = ResolvedPolicy::default();
                policy.headers.insert(
                    http::header::HeaderName::from_static("x-api-token-0"),
                    http::HeaderValue::from_static("BENCH_FAKE_HEADER_SECRET_0"),
                );
                policy.headers.insert(
                    http::header::HeaderName::from_static("x-visible-1"),
                    http::HeaderValue::from_static("visible"),
                );
                let plan = request_plan(
                    "RedactionHooks",
                    Method::GET,
                    "/perf/redaction",
                    policy,
                    PreparedBody::empty(),
                );
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("allocation report redaction hooks");
                black_box((response.status, response.body.len()));
            })
        },
    );
}

#[derive(Clone, Default)]
struct DrainingTransport;

impl Transport for DrainingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout: _,
                rate_limit,
                extensions: _,
            } = req;

            let drained_bytes = match body {
                concord_core::transport::TransportRequestBody::Empty => 0usize,
                concord_core::transport::TransportRequestBody::Bytes(bytes) => bytes.len(),
                concord_core::transport::TransportRequestBody::Stream(mut stream) => {
                    let mut total = 0usize;
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.expect("allocation report stream should not fail");
                        total += chunk.len();
                    }
                    total
                }
            };

            Ok(TransportResponse {
                meta,
                url,
                status: StatusCode::OK,
                headers,
                content_length: Some(drained_bytes as u64),
                rate_limit,
                body: Box::new(EmptyBody),
            })
        })
    }
}

fn run_streaming_upload(rt: &Runtime) {
    report_case(
        rt,
        "streaming_upload/async_read/1MiB/chunk_8KiB",
        || {
            let transport = DrainingTransport;
            let client = client(transport);
            // Keep the fixture payload alive until after the snapshot. The measured block
            // clones the payload into the async-read stream body so the case reports request
            // execution and stream consumption work, not setup teardown.
            let payload = filled_bytes(1024 * 1024, 0xA5);
            (client, payload)
        },
        |(client, payload)| {
            Box::pin(async move {
                let stream_body = StreamBody::from_async_read_with_chunk_size(
                    InMemoryAsyncRead::new(payload.clone()),
                    8 * 1024,
                )
                .expect("valid chunk size");
                let plan = request_plan(
                    "StreamingUpload",
                    Method::POST,
                    "/perf/streaming-upload",
                    ResolvedPolicy::default(),
                    PreparedBody::from_stream_body(
                        stream_body,
                        Some(HeaderValue::from_static("application/octet-stream")),
                    ),
                );
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("allocation report streaming upload");
                black_box((response.status, response.body.len()));
            })
        },
    );
}

fn run_pagination(rt: &Runtime) {
    report_case(
        rt,
        "pagination/collect_pages",
        || {
            let transport = MockTransport::scripted(vec![
                MockResponse::text(StatusCode::OK, "item-0"),
                MockResponse::text(StatusCode::OK, "item-1"),
            ]);
            (client(transport), PaginationEndpoint::new(2))
        },
        |state| {
            let client = &state.0;
            let endpoint = state.1.clone();
            Box::pin(async move {
                let items = client
                    .request(endpoint)
                    .paginate(PaginationTermination::take_pages(2))
                    .collect()
                    .await
                    .expect("allocation report pagination");
                black_box(items.len());
            })
        },
    );
}

fn run_rate_limit_governor(rt: &Runtime) {
    report_case_with_caveat(
        rt,
        "rate_limit_governor/empty_plan_context_and_acquire",
        || (GovernorRateLimiter::new(), concord_core::advanced::RateLimitPlan::new()),
        |state| {
            let limiter = &state.0;
            let plan = &state.1;
            Box::pin(async move {
                let ctx = context(
                    "allocation_counts_empty_plan",
                    &Method::GET,
                    "https://example.com/perf/empty-plan",
                    Some("example.com"),
                    plan,
                );
                let permit = limiter.acquire(ctx).await.expect("empty plan permit");
                black_box(permit);
            })
        },
        "context_construction_included empty_plan_fast_path",
    );
}

#[derive(Clone)]
struct PaginationEndpoint {
    count: u64,
}

impl PaginationEndpoint {
    fn new(count: u64) -> Self {
        Self { count }
    }
}

impl Endpoint<perf::support::PerfCx> for PaginationEndpoint {
    type Response = PaginationPage;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<perf::support::PerfCx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        Box::pin(async move {
            let decoded = client.execute_plan::<Text<String>>(plan).await?;
            Ok(PaginationPage::parse(decoded.into_value()))
        })
    }
}

impl ReusableEndpoint<perf::support::PerfCx> for PaginationEndpoint {
    fn plan(
        &self,
        _ctx: &ClientPlanContext<'_, perf::support::PerfCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "AllocationPagination",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.com",
                    "/perf/pagination",
                ),
                policy: ResolvedPolicy::default(),
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                },
                pagination: Some(PaginationMarker),
            },
            body: PreparedBody::empty(),
            overrides: RequestOverrides::default(),
        })
    }
}

impl PaginatedEndpoint<perf::support::PerfCx> for PaginationEndpoint {
    type Pagination = OffsetLimitPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>> {
        Some(Box::new(PaginationRuntimeAdapter::<OffsetLimitPagination>::new()))
    }
}

impl PaginateBinding<OffsetLimitPagination> for PaginationEndpoint {
    fn load_pagination(&self) -> OffsetLimitPagination {
        OffsetLimitPagination {
            offset: 0,
            limit: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &OffsetLimitPagination) {
        self.count = pagination.limit;
        let _ = pagination.offset;
    }
}

#[derive(Clone)]
struct PaginationPage {
    items: Vec<String>,
}

impl PaginationPage {
    fn parse(raw: String) -> Self {
        let items = raw
            .split(',')
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
        Self { items }
    }
}

impl PageItems for PaginationPage {
    type Item = String;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

fn main() {
    let rt = runtime();
    sanity_check_counter();
    println!("allocation_counts_report report_only=true note=counts_are_local_to_this_process");
    reset_allocation_counts();
    run_attempt_pipeline(&rt);
    run_auth_runtime(&rt);
    run_redaction_hooks(&rt);
    run_streaming_upload(&rt);
    run_pagination(&rt);
    run_rate_limit_governor(&rt);
}
