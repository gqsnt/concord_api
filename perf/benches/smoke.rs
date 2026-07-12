use bytes::Bytes;
use concord_core::advanced::{DynBody, RequestExecutionContext, RequestMeta, Transport};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{Method, StatusCode};
use perf::support::{MockResponse, MockTransport, filled_bytes, runtime};
use std::hint::black_box;
use url::Url;

fn build_request(url: &Url, body: Bytes) -> http::Request<DynBody> {
    let mut request = http::Request::new(DynBody::from_bytes(body));
    *request.method_mut() = Method::POST;
    *request.uri_mut() = url.as_str().parse().expect("URI");
    request.extensions_mut().insert(RequestExecutionContext {
        meta: RequestMeta {
            endpoint: "perf_smoke",
            method: Method::POST,
            idempotent: false,
            attempt: 0,
            page_index: 0,
        },
        timeout: None,
    });
    request
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
                    let frame = http_body_util::BodyExt::frame(response.body_mut())
                        .await
                        .expect("response frame")
                        .expect("response body read");
                    black_box(frame.into_data().expect("response data frame"));
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, smoke);
criterion_main!(benches);
