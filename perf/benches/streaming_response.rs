use concord_core::advanced::{OctetStream, RawStreamResponse, ResponseEntity};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute,
};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderValue, Method, StatusCode};
use perf::support::{MockResponse, MockTransport, client, filled_bytes, runtime};
use std::hint::black_box;

fn request_plan(response: concord_core::advanced::ResponseEntityPlan) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta { name: "RawStreamBench", method: Method::GET, idempotent: true, facade_path: &[] },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", "/perf/raw-stream"),
            policy: ResolvedPolicy::default(),
            body: BodyPlan::None,
            response: response.response_plan,
            pagination: None,
        },
        args: RequestArgs::empty(),
        overrides: RequestOverrides::default(),
        replayability: concord_core::internal::Replayability::Replayable,
    }
}

fn bench_raw(c: &mut Criterion, name: &str, chunks: usize, chunk_size: usize) {
    let rt = runtime();
    c.bench_function(name, |b| b.to_async(&rt).iter_batched(
        || {
            let response = MockResponse::chunked(StatusCode::OK, (0..chunks).map(|idx| filled_bytes(chunk_size, (idx % 251) as u8)))
                .with_header(http::header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
            let entity = RawStreamResponse::<OctetStream>::plan(concord_core::advanced::ErrorContext { endpoint: "RawStreamBench", method: Method::GET }).expect("stream plan");
            (client(MockTransport::repeating(response)), request_plan(entity))
        },
        |(client, plan)| async move {
            let mut response = RawStreamResponse::<OctetStream>::execute(&client, plan).await.expect("stream response");
            let mut bytes = 0usize;
            while let Some(chunk) = response.next_chunk().await.expect("stream chunk") {
                bytes += chunk.len();
                black_box(chunk);
            }
            assert_eq!(bytes, chunks * chunk_size);
        },
        BatchSize::SmallInput,
    ));
}

fn streaming_response(c: &mut Criterion) {
    bench_raw(c, "raw_drain/chunks_16", 16, 256);
    bench_raw(c, "raw_drain/chunks_1024", 1024, 256);
    if matches!(std::env::var("CONCORD_PERF_FULL"), Ok(value) if value == "1") {
        bench_raw(c, "raw_drain/chunks_8192", 8192, 256);
    }
}

criterion_group!(benches, streaming_response);
criterion_main!(benches);
