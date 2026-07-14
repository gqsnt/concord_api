use bytes::Bytes;
use concord_test_support::{ScriptedReply, deterministic_mock};
use criterion::{Criterion, criterion_group, criterion_main};
use perf::BenchmarkClient;

fn bench(c: &mut Criterion) {
    let (script, _handle) = deterministic_mock()
        .repeating(ScriptedReply::ok_text(Bytes::from_static(b"pong")))
        .build();
    let client =
        BenchmarkClient::new_with_safe_reqwest_builder(|builder| script.configure_both(builder))
            .expect("deterministic managed client");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime");
    c.bench_function("visible_execution/buffered_generated_call", |b| {
        b.to_async(&runtime)
            .iter(|| async { client.ping().await.expect("deterministic response") });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
