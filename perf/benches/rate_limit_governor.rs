use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures_util::future::join_all;
use http::Method;
use perf::support::{
    context, multi_bucket_plan, response_context, response_headers, runtime, single_bucket_plan,
};
use concord_core::advanced::{
    GovernorRateLimiter, RateLimitKeyPart, RateLimitResponsePolicy, RateLimiter,
};
use concord_core::prelude::RateLimitObservation;
use http::StatusCode;
use std::env;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

const URL: &str = "https://example.com/perf/rate-limit";
const HOST: Option<&str> = Some("example.com");
const CONCURRENT_TASKS: usize = 32;
const FULL_CONCURRENT_TASKS: usize = 1000;
static METHOD: Method = Method::GET;

fn full_fixture_enabled() -> bool {
    matches!(env::var("CONCORD_PERF_FULL"), Ok(value) if value == "1")
}

#[derive(Clone)]
struct LimitedResponsePolicy {
    delay: Duration,
}

impl RateLimitResponsePolicy for LimitedResponsePolicy {
    fn observe(
        &self,
        _ctx: &concord_core::advanced::RateLimitResponseContext<'_>,
    ) -> RateLimitObservation {
        RateLimitObservation::limited().with_delay(self.delay)
    }
}

fn acquire_context<'a>(
    endpoint: &'static str,
    plan: &'a concord_core::advanced::RateLimitPlan,
) -> concord_core::advanced::RateLimitContext<'a> {
    context(endpoint, &METHOD, URL, HOST, plan)
}

fn bench_acquire(c: &mut Criterion, name: &str, plan: concord_core::advanced::RateLimitPlan) {
    let runtime = runtime();
    c.bench_function(name, |b| {
        let plan = plan.clone();
        b.to_async(&runtime).iter_batched(
            move || GovernorRateLimiter::new(),
            move |limiter| {
                let plan = plan.clone();
                async move {
                    let ctx = acquire_context("rate_limit_acquire", &plan);
                    black_box(limiter.acquire(ctx).await.expect("permit"));
                }
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_concurrent_acquire(
    c: &mut Criterion,
    name: &str,
    unique_keys: bool,
    tasks: usize,
) {
    let runtime = runtime();
    c.bench_function(name, |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = Arc::new(GovernorRateLimiter::new());
                let plans = (0..tasks)
                    .map(|idx| {
                        if unique_keys {
                            single_bucket_plan(
                                "bench",
                                format!("bucket-{idx}"),
                                vec![RateLimitKeyPart::static_value(
                                    "tenant",
                                    format!("tenant-{idx}"),
                                )],
                            )
                        } else {
                            single_bucket_plan(
                                "bench",
                                "same-bucket",
                                vec![RateLimitKeyPart::static_value("tenant", "same")],
                            )
                        }
                    })
                    .collect::<Vec<_>>();
                (limiter, plans)
            },
            move |(limiter, plans)| async move {
                let futures = plans.into_iter().map({
                    let limiter = limiter.clone();
                    move |plan| {
                        let limiter = limiter.clone();
                        async move {
                            let ctx = acquire_context("rate_limit_concurrent", &plan);
                            black_box(limiter.acquire(ctx).await.expect("permit"));
                        }
                    }
                });
                join_all(futures).await;
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_cooldown_store(c: &mut Criterion) {
    let runtime = runtime();
    c.bench_function("cooldown/on_response_store", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new().with_response_policy(Arc::new(
                    LimitedResponsePolicy {
                        delay: Duration::from_micros(1),
                    },
                ));
                let plan = single_bucket_plan(
                    "bench",
                    "cooldown",
                    vec![RateLimitKeyPart::endpoint()],
                );
                (limiter, plan)
            },
            move |(limiter, plan)| async move {
                let headers = response_headers();
                let ctx = response_context(
                    acquire_context("rate_limit_cooldown", &plan),
                    StatusCode::TOO_MANY_REQUESTS,
                    &headers,
                );
                let action = limiter.on_response(ctx).await.expect("response action");
                black_box(action.cooldown_stored());
            },
            BatchSize::SmallInput,
        )
    });
}

fn rate_limit_governor(c: &mut Criterion) {
    bench_acquire(c, "empty_plan/acquire", concord_core::advanced::RateLimitPlan::new());
    bench_acquire(
        c,
        "single_bucket_x1_window/acquire",
        single_bucket_plan("bench", "single", vec![RateLimitKeyPart::endpoint()]),
    );
    bench_acquire(
        c,
        "multi_bucket_windows/acquire",
        multi_bucket_plan(4, 3, false),
    );
    bench_concurrent_acquire(c, "same_key/joined_futures_32", false, CONCURRENT_TASKS);
    bench_concurrent_acquire(
        c,
        "high_cardinality_keys/joined_futures_32",
        true,
        CONCURRENT_TASKS,
    );
    bench_cooldown_store(c);

    if full_fixture_enabled() {
        bench_concurrent_acquire(
            c,
            "high_cardinality_keys/joined_futures_1000",
            true,
            FULL_CONCURRENT_TASKS,
        );
    }
}

criterion_group!(benches, rate_limit_governor);
criterion_main!(benches);
