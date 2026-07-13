use bytes::Bytes;
use concord_core::internal::{PreparedBody, ResolvedPolicy};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderValue, Method, StatusCode};
use perf::support::{
    MockResponse, MockTransport, client, execute_raw_plan, filled_bytes, request_plan, runtime,
};
use std::hint::black_box;
use std::sync::Arc;

fn build_plan(body: Bytes) -> concord_core::internal::RequestPlan {
    request_plan(
        "perf_smoke",
        Method::POST,
        "/perf-smoke",
        ResolvedPolicy::default(),
        PreparedBody::reusable_bytes(
            body,
            Some(HeaderValue::from_static("application/octet-stream")),
        ),
    )
}

fn smoke(c: &mut Criterion) {
    let runtime = runtime();
    let client = Arc::new(client(MockTransport::repeating(
        MockResponse::bytes(StatusCode::OK, filled_bytes(32, 0xA5)).with_json_header(),
    )));
    let request_body = filled_bytes(16, 0x11);

    c.bench_function("smoke_roundtrip", move |b| {
        b.to_async(&runtime).iter_batched(
            || build_plan(request_body.clone()),
            {
                let client = client.clone();
                move |plan| {
                    let client = client.clone();
                    async move {
                        let response = execute_raw_plan(&client, plan)
                            .await
                            .expect("native loopback response");
                        black_box(response.body().clone());
                    }
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, smoke);
criterion_main!(benches);
