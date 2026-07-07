use bytes::Bytes;
use concord_core::advanced::{
    JsonSse, Mixed, MultipartResponse, NdJson, OctetStream, RawResponsePart, RawStreamResponse,
    RecordResponse, ResponseEntity, SseResponse,
};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute,
};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderValue, Method, StatusCode};
use perf::support::{MockResponse, MockTransport, chunked_bytes, client, filled_bytes, runtime};
use std::env;
use std::hint::black_box;

fn full_fixture_enabled() -> bool {
    matches!(env::var("CONCORD_PERF_FULL"), Ok(value) if value == "1")
}

fn request_plan(
    name: &'static str,
    path: &'static str,
    response: concord_core::advanced::ResponseEntityPlan,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: Method::GET,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
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

async fn drain_raw(
    mut response: concord_core::advanced::StreamResponse<OctetStream>,
) -> (usize, usize) {
    let mut bytes = 0usize;
    let mut chunks = 0usize;
    while let Some(chunk) = response.next_chunk().await.expect("raw chunk") {
        bytes += chunk.len();
        chunks += 1;
        black_box(chunk);
    }
    (bytes, chunks)
}

fn raw_response(chunks: usize, chunk_size: usize) -> MockResponse {
    MockResponse::chunked(
        StatusCode::OK,
        (0..chunks).map(|idx| filled_bytes(chunk_size, (idx % 251) as u8)),
    )
    .with_header(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    )
}

fn bench_raw(c: &mut Criterion, name: &str, chunks: usize, chunk_size: usize) {
    let rt = runtime();
    c.bench_function(name, |b| {
        b.to_async(&rt).iter_batched(
            || {
                let client = client(MockTransport::repeating(raw_response(chunks, chunk_size)));
                let entity =
                    RawStreamResponse::<OctetStream>::plan(concord_core::advanced::ErrorContext {
                        endpoint: "RawStreamBench",
                        method: Method::GET,
                    })
                    .expect("raw stream plan");
                (client, request_plan("RawStreamBench", "/perf/raw-stream", entity))
            },
            |(client, plan)| async move {
                let response = RawStreamResponse::<OctetStream>::execute(&client, plan)
                    .await
                    .expect("raw stream response");
                let (bytes, drained_chunks) = drain_raw(response).await;
                assert_eq!(drained_chunks, chunks, "raw stream chunk count mismatch");
                assert_eq!(
                    bytes,
                    chunks * chunk_size,
                    "raw stream byte total mismatch"
                );
                black_box((bytes, drained_chunks));
            },
            BatchSize::SmallInput,
        )
    });
}

fn ndjson_body(records: usize) -> Bytes {
    let mut body = String::new();
    for idx in 0..records {
        body.push_str(&idx.to_string());
        body.push('\n');
    }
    Bytes::from(body)
}

fn bench_ndjson(c: &mut Criterion, name: &str, records: usize) {
    let rt = runtime();
    c.bench_function(name, |b| {
        b.to_async(&rt).iter_batched(
            || {
                let response = MockResponse::chunked(
                    StatusCode::OK,
                    chunked_bytes(ndjson_body(records), 128),
                )
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/x-ndjson"),
                );
                let client = client(MockTransport::repeating(response));
                let entity =
                    RecordResponse::<u64, NdJson>::plan(concord_core::advanced::ErrorContext {
                        endpoint: "NdjsonBench",
                        method: Method::GET,
                    })
                    .expect("ndjson plan");
                (client, request_plan("NdjsonBench", "/perf/ndjson", entity))
            },
            |(client, plan)| async move {
                let mut stream = RecordResponse::<u64, NdJson>::execute(&client, plan)
                    .await
                    .expect("ndjson stream");
                let mut seen_records = 0usize;
                while let Some(record) = stream.next_record().await.expect("ndjson record") {
                    seen_records += 1;
                    black_box(record);
                }
                assert_eq!(seen_records, records, "ndjson record count mismatch");
                black_box(seen_records);
            },
            BatchSize::SmallInput,
        )
    });
}

fn sse_body(events: usize) -> Bytes {
    let mut body = String::new();
    for idx in 0..events {
        body.push_str("data: ");
        body.push_str(&idx.to_string());
        body.push_str("\n\n");
    }
    Bytes::from(body)
}

fn bench_sse(c: &mut Criterion, name: &str, events: usize, chunk_size: usize) {
    let rt = runtime();
    c.bench_function(name, |b| {
        b.to_async(&rt).iter_batched(
            || {
                let response = MockResponse::chunked(
                    StatusCode::OK,
                    chunked_bytes(sse_body(events), chunk_size),
                )
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("text/event-stream"),
                );
                let client = client(MockTransport::repeating(response));
                let entity =
                    SseResponse::<u64, JsonSse>::plan(concord_core::advanced::ErrorContext {
                        endpoint: "SseBench",
                        method: Method::GET,
                    })
                    .expect("sse plan");
                (client, request_plan("SseBench", "/perf/sse", entity))
            },
            |(client, plan)| async move {
                let mut stream = SseResponse::<u64, JsonSse>::execute(&client, plan)
                    .await
                    .expect("sse stream");
                let mut seen_events = 0usize;
                while let Some(event) = stream.next_event().await.expect("sse event") {
                    seen_events += 1;
                    black_box(event.data);
                }
                assert_eq!(seen_events, events, "sse event count mismatch");
                black_box(seen_events);
            },
            BatchSize::SmallInput,
        )
    });
}

fn multipart_body(parts: usize) -> Bytes {
    let mut body = String::new();
    for idx in 0..parts {
        body.push_str("--bench-boundary\r\ncontent-type: text/plain\r\n\r\n");
        body.push_str("part-");
        body.push_str(&idx.to_string());
        body.push_str("\r\n");
    }
    body.push_str("--bench-boundary--\r\n");
    Bytes::from(body)
}

fn bench_multipart(c: &mut Criterion, name: &str, parts: usize, chunk_size: usize) {
    let rt = runtime();
    c.bench_function(name, |b| {
        b.to_async(&rt).iter_batched(
            || {
                let response = MockResponse::chunked(
                    StatusCode::OK,
                    chunked_bytes(multipart_body(parts), chunk_size),
                )
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("multipart/mixed; boundary=bench-boundary"),
                );
                let client = client(MockTransport::repeating(response));
                let entity = MultipartResponse::<RawResponsePart, Mixed>::plan(
                    concord_core::advanced::ErrorContext {
                        endpoint: "MultipartBench",
                        method: Method::GET,
                    },
                )
                .expect("multipart plan");
                (client, request_plan("MultipartBench", "/perf/multipart", entity))
            },
            |(client, plan)| async move {
                let mut stream = MultipartResponse::<RawResponsePart, Mixed>::execute(&client, plan)
                    .await
                    .expect("multipart stream");
                let mut seen_parts = 0usize;
                while let Some(mut part) = stream.next_part().await.expect("multipart part") {
                    while let Some(chunk) = part.next_chunk().await.expect("multipart chunk") {
                        black_box(chunk.len());
                    }
                    seen_parts += 1;
                }
                assert_eq!(seen_parts, parts, "multipart part count mismatch");
                black_box(seen_parts);
            },
            BatchSize::SmallInput,
        )
    });
}

fn streaming_response(c: &mut Criterion) {
    bench_raw(c, "raw_drain/chunks_16", 16, 256);
    bench_raw(c, "raw_drain/chunks_1024", 1024, 256);
    bench_ndjson(c, "ndjson/records_128", 128);
    bench_sse(c, "sse/events_128", 128, 128);
    bench_sse(c, "sse/events_128_bytewise", 128, 1);
    bench_multipart(c, "multipart/parts_32", 32, 128);
    bench_multipart(c, "multipart/parts_32_bytewise", 32, 1);

    if full_fixture_enabled() {
        bench_raw(c, "raw_drain/chunks_8192", 8192, 256);
        bench_ndjson(c, "ndjson/records_4096", 4096);
        bench_sse(c, "sse/events_4096", 4096, 128);
        bench_multipart(c, "multipart/parts_512", 512, 128);
    }
}

criterion_group!(benches, streaming_response);
criterion_main!(benches);
