use concord_core::prelude::{ApiClient, RetryMode, StatusRetryConfig};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::StatusCode;
use perf::support::{PerfAuthVars, PerfCx};
use std::hint::black_box;

fn construct(c: &mut Criterion, name: &str, mode: RetryMode) {
    c.bench_function(name, |b| {
        b.iter_batched(
            || mode.clone(),
            |mode| {
                black_box(
                    ApiClient::<PerfCx>::with_retry_mode((), PerfAuthVars::default(), mode)
                        .expect("managed client construction"),
                );
            },
            BatchSize::SmallInput,
        );
    });
}

fn retry_modes(c: &mut Criterion) {
    construct(
        c,
        "managed_client_construction/protocol_recovery",
        RetryMode::ProtocolRecovery,
    );
    construct(
        c,
        "managed_client_construction/disabled",
        RetryMode::Disabled,
    );
    construct(
        c,
        "managed_client_construction/status",
        RetryMode::Status(
            StatusRetryConfig::new(1, [StatusCode::SERVICE_UNAVAILABLE])
                .expect("valid status mode"),
        ),
    );
}

criterion_group!(benches, retry_modes);
criterion_main!(benches);
