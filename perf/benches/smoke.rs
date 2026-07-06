use bytes::Bytes;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use concord_core::advanced::{
    RateLimitPlan, RequestMeta, Transport, TransportRequest, TransportRequestBody,
};
use concord_core::auth::RequestExtensions;
use http::{HeaderMap, Method, StatusCode};
use perf::support::{MockResponse, MockTransport, filled_bytes, runtime};
use std::hint::black_box;
use url::Url;

fn build_request(url: &Url, body: Bytes) -> TransportRequest {
    TransportRequest {
        meta: RequestMeta {
            endpoint: "perf_smoke",
            method: Method::POST,
            idempotent: false,
            attempt: 0,
            page_index: 0,
        },
        url: url.clone(),
        headers: HeaderMap::new(),
        body: TransportRequestBody::from_bytes(body),
        timeout: None,
        rate_limit: RateLimitPlan::new(),
        transport_auth: None,
        extensions: RequestExtensions::default(),
    }
}

fn smoke(c: &mut Criterion) {
    let runtime = runtime();
    let transport = MockTransport::repeating(
        MockResponse::bytes(StatusCode::OK, filled_bytes(32, 0xA5)).with_json_header(),
    );
    let request_url = Url::parse("https://bench.invalid/perf-smoke").expect("valid smoke URL");
    let request_body = filled_bytes(16, 0x11);

    c.bench_function("smoke_roundtrip", move |b| {
        let transport = transport.clone();
        let request_url = request_url.clone();
        let request_body = request_body.clone();
        b.to_async(&runtime).iter_batched(
            move || build_request(&request_url, request_body.clone()),
            move |request| {
                let transport = transport.clone();
                async move {
                    let mut response = transport
                        .send(request)
                        .await
                        .expect("mock transport response");
                    let chunk = response
                        .body
                        .next_chunk()
                        .await
                        .expect("response body read");
                    black_box(chunk.expect("response chunk"));
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, smoke);
criterion_main!(benches);
