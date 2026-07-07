use bytes::Bytes;
use concord_core::advanced::{
    NoopDebugSink, NoopRateLimiter, StreamBody, Transport, TransportError, TransportRequest,
    TransportResponse,
};
use concord_core::internal::{BodyPlan, RequestArgs, RequestPlan, ResolvedPolicy, Replayability};
use futures_util::StreamExt;
use http::{Method, StatusCode};
use perf::support::{
    AllocationSnapshot, CountingAllocator, EmptyBody, InMemoryAsyncRead, MockResponse,
    MockTransport, auth_requirement, client, filled_bytes, request_plan, reset_allocation_counts,
    runtime, snapshot_allocation_counts,
};
use std::alloc::System;
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;
use tokio::runtime::Runtime;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

fn base_plan(name: &'static str, path: &'static str) -> RequestPlan {
    request_plan(
        name,
        Method::GET,
        path,
        ResolvedPolicy::default(),
        BodyPlan::None,
        RequestArgs::empty(),
        Replayability::Replayable,
    )
}

fn success_transport() -> MockTransport {
    MockTransport::repeating(MockResponse::text(
        StatusCode::OK,
        Bytes::from_static(b"ok"),
    ))
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

fn run_attempt_pipeline(rt: &Runtime) {
    report_case(
        rt,
        "attempt_pipeline/mock_transport_success/minimal_get",
        || client(success_transport()),
        |client| {
            Box::pin(async move {
                let plan = base_plan("MinimalGet", "/perf/minimal-get");
                let response = client
                    .execute_plan_raw(plan)
                    .await
                    .expect("allocation report request");
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
                let mut plan = base_plan("BearerAuth", "/perf/bearer-auth");
                plan.endpoint.policy.auth.requirements.push(auth_requirement(
                    concord_core::advanced::AuthPlacement::Bearer,
                    "bearer",
                ));
                let response = client
                    .execute_plan_raw(plan)
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
                cfg.debug_sink(std::sync::Arc::new(NoopDebugSink));
                cfg.debug_level(concord_core::prelude::DebugLevel::VV);
                cfg.rate_limiter(std::sync::Arc::new(NoopRateLimiter::new()));
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
                    BodyPlan::None,
                    RequestArgs::empty(),
                    Replayability::Replayable,
                );
                let response = client
                    .execute_plan_raw(plan)
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
                transport_auth: _,
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
                    BodyPlan::RawStream {
                        content_type: http::HeaderValue::from_static("application/octet-stream"),
                    },
                    RequestArgs::with_stream_body(stream_body),
                    Replayability::NonReplayable,
                );
                let response = client
                    .execute_plan_raw(plan)
                    .await
                    .expect("allocation report streaming upload");
                black_box((response.status, response.body.len()));
            })
        },
    );
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
}
