use bytes::Bytes;
use concord_test_support::{MockReply, mock};
use criterion::{Criterion, criterion_group, criterion_main};
use perf::BenchmarkClient;

fn bench(c: &mut Criterion) {
    let (server, _handle) = mock()
        .repeating(MockReply::ok_text(Bytes::from_static(b"pong")))
        .build();
    let client =
        BenchmarkClient::new_with_safe_reqwest_builder(|builder| server.configure_reqwest(builder))
            .expect("loopback managed client");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime");
    c.bench_function("visible_execution/buffered_generated_call", |b| {
        b.to_async(&runtime)
            .iter(|| async { client.ping().await.expect("loopback response") });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
