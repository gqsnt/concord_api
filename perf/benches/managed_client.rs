use concord_core::prelude::{RetryMode, StatusRetryConfig};
use criterion::{Criterion, criterion_group, criterion_main};
use perf::BenchmarkClient;

fn bench(c: &mut Criterion) {
    c.bench_function("managed_client/default", |b| {
        b.iter(BenchmarkClient::new);
    });
    c.bench_function("managed_client/retry_disabled", |b| {
        b.iter(|| {
            BenchmarkClient::new_with_retry_mode(RetryMode::Disabled)
                .expect("managed no-retry client")
        });
    });
    c.bench_function("managed_client/protocol_recovery", |b| {
        b.iter(|| {
            BenchmarkClient::new_with_retry_mode(RetryMode::ProtocolRecovery)
                .expect("managed protocol-recovery client")
        });
    });
    let status =
        StatusRetryConfig::new(2, [http::StatusCode::BAD_GATEWAY]).expect("bounded status retry");
    c.bench_function("managed_client/status_retry", |b| {
        b.iter(|| {
            BenchmarkClient::new_with_retry_mode(RetryMode::Status(status.clone()))
                .expect("managed status-retry client")
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
