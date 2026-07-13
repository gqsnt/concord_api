use criterion::{Criterion, criterion_group, criterion_main};
use perf::BenchmarkClient;
use std::hint::black_box;

fn bench(c: &mut Criterion) {
    let client = BenchmarkClient::new();
    c.bench_function("generated_client/request_facade", |b| {
        b.iter(|| black_box(client.ping()));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
