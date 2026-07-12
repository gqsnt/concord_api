use concord_core::advanced::{
    GovernorRateLimiter, RateLimitKeyPart, RateLimitResponsePolicy, RateLimiter,
};
use concord_core::prelude::RateLimitObservation;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures_util::{FutureExt, future::join_all};
use http::Method;
use http::StatusCode;
use perf::support::{
    context, multi_bucket_plan, response_context, response_headers, runtime, single_bucket_plan,
};
use std::env;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

const URL: &str = "https://example.com/perf/rate-limit";
const HOST: Option<&str> = Some("example.com");
const CONCURRENT_TASKS: usize = 32;
const FULL_CONCURRENT_TASKS: usize = 1000;
const WINDOW_PRUNE_PREFILL: usize = 256;
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

fn bench_concurrent_acquire(c: &mut Criterion, name: &str, unique_keys: bool, tasks: usize) {
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
    c.bench_function("cooldown/no_action_observation_fast_path", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new();
                let plan = concord_core::advanced::RateLimitPlan::new();
                let headers = response_headers();
                (limiter, plan, headers)
            },
            move |(limiter, plan, headers)| async move {
                let ctx = response_context(
                    acquire_context("rate_limit_no_cooldown", &plan),
                    StatusCode::OK,
                    &headers,
                );
                let action = limiter.on_response(ctx).await.expect("response action");
                black_box(action.cooldown_stored());
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("cooldown/on_response_store", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new().with_response_policy(Arc::new(
                    LimitedResponsePolicy {
                        delay: Duration::from_micros(1),
                    },
                ));
                let plan =
                    single_bucket_plan("bench", "cooldown", vec![RateLimitKeyPart::endpoint()]);
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

fn bench_cooldown_cardinality(c: &mut Criterion) {
    const BELOW_CAP_ENTRIES: usize = 32;
    const HIGH_CARDINALITY_CAP: usize = 128;
    const CAP_REACHED_ENTRIES: usize = 8;

    let runtime = runtime();
    c.bench_function("cooldown/cardinality_below_cap_32", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new()
                    .with_max_cooldown_entries(BELOW_CAP_ENTRIES + 1)
                    .with_response_policy(Arc::new(LimitedResponsePolicy {
                        delay: Duration::from_secs(1),
                    }));
                let plans = (0..BELOW_CAP_ENTRIES)
                    .map(|idx| {
                        single_bucket_plan(
                            "cooldown",
                            format!("below-cap-{idx}"),
                            vec![RateLimitKeyPart::static_value(
                                "tenant",
                                format!("tenant-{idx}"),
                            )],
                        )
                    })
                    .collect::<Vec<_>>();
                let headers = response_headers();
                let urls = (0..BELOW_CAP_ENTRIES)
                    .map(|idx| format!("https://example.com/perf/cooldown/{idx}"))
                    .collect::<Vec<_>>();
                (limiter, plans, headers, urls)
            },
            move |(limiter, plans, headers, urls)| async move {
                for (plan, url) in plans.iter().zip(urls.iter()) {
                    let ctx = response_context(
                        context("rate_limit_cooldown_cardinality", &METHOD, url, HOST, &plan),
                        StatusCode::TOO_MANY_REQUESTS,
                        &headers,
                    );
                    let action = limiter.on_response(ctx).await.expect("cooldown stored");
                    black_box(action.cooldown_stored());
                }
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("cooldown/high_cardinality_to_cap_128", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new()
                    .with_max_cooldown_entries(HIGH_CARDINALITY_CAP)
                    .with_response_policy(Arc::new(LimitedResponsePolicy {
                        delay: Duration::from_secs(1),
                    }));
                let plans = (0..HIGH_CARDINALITY_CAP)
                    .map(|idx| {
                        single_bucket_plan(
                            "cooldown",
                            format!("high-cardinality-{idx}"),
                            vec![RateLimitKeyPart::static_value(
                                "tenant",
                                format!("tenant-{idx}"),
                            )],
                        )
                    })
                    .collect::<Vec<_>>();
                let headers = response_headers();
                let urls = (0..HIGH_CARDINALITY_CAP)
                    .map(|idx| format!("https://example.com/perf/high-cardinality/{idx}"))
                    .collect::<Vec<_>>();
                (limiter, plans, headers, urls)
            },
            move |(limiter, plans, headers, urls)| async move {
                for (plan, url) in plans.iter().zip(urls.iter()) {
                    let ctx = response_context(
                        context(
                            "rate_limit_cooldown_high_cardinality",
                            &METHOD,
                            url,
                            HOST,
                            &plan,
                        ),
                        StatusCode::TOO_MANY_REQUESTS,
                        &headers,
                    );
                    let action = limiter.on_response(ctx).await.expect("cooldown stored");
                    black_box(action.cooldown_stored());
                }
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("cooldown/cap_reached_error_path", |b| {
        b.to_async(&runtime).iter_batched(
            move || {
                let limiter = GovernorRateLimiter::new()
                    .with_max_cooldown_entries(CAP_REACHED_ENTRIES)
                    .with_response_policy(Arc::new(LimitedResponsePolicy {
                        delay: Duration::from_secs(1),
                    }));
                let plans = (0..CAP_REACHED_ENTRIES)
                    .map(|idx| {
                        single_bucket_plan(
                            "cooldown",
                            format!("cap-fill-{idx}"),
                            vec![RateLimitKeyPart::static_value(
                                "tenant",
                                format!("tenant-{idx}"),
                            )],
                        )
                    })
                    .collect::<Vec<_>>();
                let overflow_plan = single_bucket_plan(
                    "cooldown",
                    "cap-overflow",
                    vec![RateLimitKeyPart::static_value("tenant", "tenant-overflow")],
                );
                let headers = response_headers();
                for (idx, plan) in plans.iter().enumerate() {
                    let url = format!("https://example.com/perf/cap-fill/{idx}");
                    let ctx = response_context(
                        context("rate_limit_cooldown_cap_fill", &METHOD, &url, HOST, plan),
                        StatusCode::TOO_MANY_REQUESTS,
                        &headers,
                    );
                    limiter
                        .on_response(ctx)
                        .now_or_never()
                        .expect("governor response future is ready")
                        .expect("prefill cooldown");
                }
                (limiter, overflow_plan, headers)
            },
            move |(limiter, plan, headers)| async move {
                let ctx = response_context(
                    context(
                        "rate_limit_cooldown_cap_error",
                        &METHOD,
                        "https://example.com/perf/cap-overflow",
                        HOST,
                        &plan,
                    ),
                    StatusCode::TOO_MANY_REQUESTS,
                    &headers,
                );
                let result = limiter.on_response(ctx).await;
                black_box(result.is_err());
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_window_pruning(c: &mut Criterion) {
    let setup_runtime = Arc::new(runtime());
    let bench_runtime = Arc::new(runtime());
    c.bench_function("window_pruning/hot_acquire_after_prefill_256", |b| {
        let setup_runtime = setup_runtime.clone();
        let bench_runtime = bench_runtime.clone();
        b.iter_batched(
            move || {
                let limiter =
                    GovernorRateLimiter::new().with_window_idle_ttl(Duration::from_secs(60));
                let hot_plan = single_bucket_plan(
                    "bench",
                    "hot-acquire",
                    vec![RateLimitKeyPart::static_value("tenant", "hot")],
                );
                setup_runtime.block_on(async {
                    for idx in 0..WINDOW_PRUNE_PREFILL {
                        let plan = single_bucket_plan(
                            "bench",
                            format!("prefill-{idx}"),
                            vec![RateLimitKeyPart::static_value(
                                "tenant",
                                format!("tenant-{idx}"),
                            )],
                        );
                        let ctx = acquire_context("rate_limit_prune_prefill", &plan);
                        black_box(limiter.acquire(ctx).await.expect("prefill permit"));
                    }
                });
                (limiter, hot_plan)
            },
            move |(limiter, plan)| {
                let ctx = acquire_context("rate_limit_prune_hot", &plan);
                bench_runtime.block_on(async {
                    black_box(limiter.acquire(ctx).await.expect("hot permit"));
                });
            },
            BatchSize::SmallInput,
        )
    });
}

fn rate_limit_governor(c: &mut Criterion) {
    bench_acquire(
        c,
        "empty_plan/acquire",
        concord_core::advanced::RateLimitPlan::new(),
    );
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
    bench_cooldown_cardinality(c);
    bench_window_pruning(c);

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
