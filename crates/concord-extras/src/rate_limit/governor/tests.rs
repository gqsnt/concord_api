use std::num::NonZeroU32;

use http::{HeaderMap, Method, StatusCode};

use super::*;
use concord_core::contracts::SanitizedHeaders;
use concord_core::rate_limit::{
    MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE, RateLimitId, RateLimitKey,
    RateLimitKeyPart, RateLimitRule, RateLimitWindow,
};

fn rule(id: &str, key: &str, cost: u32, windows: &[(u32, Duration)]) -> RateLimitRule {
    RateLimitRule::new(
        RateLimitId::new(id).expect("ID"),
        if key.is_empty() {
            RateLimitKey::global()
        } else {
            RateLimitKey::from_parts([
                RateLimitKeyPart::new("tenant", key).expect("tenant key part")
            ])
            .expect("key")
        },
        NonZeroU32::new(cost).expect("cost"),
        windows.iter().map(|(capacity, duration)| {
            RateLimitWindow::new(NonZeroU32::new(*capacity).expect("capacity"), *duration)
                .expect("window")
        }),
    )
    .expect("rule")
}

fn plan(rules: Vec<RateLimitRule>) -> RateLimitPlan {
    RateLimitPlan::try_from_rules(rules).expect("valid test plan")
}

fn rate_error<T>(result: Result<T, RateLimitError>, message: &str) -> RateLimitError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

async fn acquire(limiter: &GovernorRateLimiter, plan: &RateLimitPlan, host: Option<&str>) {
    limiter
        .acquire(RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            safe_url: "https://example.test/get",
            host,
            page_index: 0,
            plan,
        })
        .await
        .expect("acquire");
}

fn spawn_acquire(
    limiter: Arc<GovernorRateLimiter>,
    plan: Arc<RateLimitPlan>,
    host: Option<&'static str>,
) -> tokio::task::JoinHandle<Result<RateLimitPermit, RateLimitError>> {
    tokio::spawn(async move {
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host,
                page_index: 0,
                plan: &plan,
            })
            .await
    })
}

#[derive(Clone, Copy)]
enum FixedScope {
    Buckets,
    Host,
    Client,
}

struct FixedObserver {
    duration: Duration,
    scope: FixedScope,
}

struct NoCooldownObserver;

impl RateLimitObserver for NoCooldownObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        None
    }
}

impl RateLimitObserver for FixedObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        let scope = match self.scope {
            FixedScope::Buckets => CooldownScope::CurrentBuckets,
            FixedScope::Host => CooldownScope::Host,
            FixedScope::Client => CooldownScope::Client,
        };
        Some(Cooldown {
            duration: self.duration,
            scope,
        })
    }
}

async fn observe(
    limiter: &Arc<GovernorRateLimiter>,
    plan: &RateLimitPlan,
    host: Option<&str>,
    status: StatusCode,
    cap: Duration,
) {
    let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
    let headers = HeaderMap::new();
    limiter_object
        .observe(RateLimitResponse {
            request: RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host,
                page_index: 0,
                plan,
            },
            status,
            headers: SanitizedHeaders::new(&headers),
            cooldown_cap: cap,
        })
        .await;
}

async fn observe_headers(
    limiter: &Arc<GovernorRateLimiter>,
    plan: &RateLimitPlan,
    host: Option<&str>,
    status: StatusCode,
    headers: &HeaderMap,
    cap: Duration,
) {
    let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
    limiter_object
        .observe(RateLimitResponse {
            request: RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host,
                page_index: 0,
                plan,
            },
            status,
            headers: SanitizedHeaders::new(headers),
            cooldown_cap: cap,
        })
        .await;
}

#[test]
fn defaults_and_configuration_validation_are_finite() {
    let limiter = GovernorRateLimiter::new();
    assert_eq!(limiter.config.quota_capacity, 4_096);
    assert_eq!(limiter.config.cooldown_capacity, 4_096);
    assert_eq!(limiter.config.idle_ttl, Duration::from_secs(15 * 60));
    assert_eq!(limiter.config.key_limit, 1_024);
    assert_eq!(
        rate_error(
            GovernorRateLimiter::new().quota_capacity(0),
            "zero quota capacity unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(
        rate_error(
            GovernorRateLimiter::new().cooldown_capacity(0),
            "zero cooldown capacity unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(
        rate_error(
            GovernorRateLimiter::new().idle_ttl(Duration::ZERO),
            "zero idle TTL unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    GovernorRateLimiter::new()
        .key_limit(0)
        .expect("zero key limit is valid");
}

#[tokio::test(start_paused = true)]
async fn governor_accepts_maximum_valid_plan() {
    let rules = (0..MAX_RATE_LIMIT_RULES_PER_PLAN)
        .map(|rule_index| {
            RateLimitRule::new(
                RateLimitId::new(format!("maximum-rule-{rule_index}")).expect("bounded ID"),
                RateLimitKey::global(),
                NonZeroU32::MIN,
                (0..MAX_RATE_LIMIT_WINDOWS_PER_RULE).map(|window_index| {
                    RateLimitWindow::new(
                        NonZeroU32::MIN,
                        Duration::from_secs(u64::try_from(window_index + 1).expect("test index")),
                    )
                    .expect("bounded window")
                }),
            )
            .expect("maximum-window rule")
        })
        .collect();
    let plan = plan(rules);
    let limiter = GovernorRateLimiter::new()
        .key_limit(0)
        .expect("global keys fit a zero-byte preparation limit");

    acquire(&limiter, &plan, Some("example.test")).await;

    assert_eq!(plan.rules().len(), MAX_RATE_LIMIT_RULES_PER_PLAN);
    assert!(
        plan.rules()
            .iter()
            .all(|rule| rule.windows().len() == MAX_RATE_LIMIT_WINDOWS_PER_RULE)
    );
    assert_eq!(
        limiter.test_quota_entry_count(),
        MAX_RATE_LIMIT_RULES_PER_PLAN * MAX_RATE_LIMIT_WINDOWS_PER_RULE
    );
}

#[tokio::test(start_paused = true)]
async fn first_burst_is_immediate_and_refill_wait_is_controlled() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let plan = Arc::new(plan(vec![rule(
        "tenant",
        "north",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    acquire(&limiter, &plan, Some("example.test")).await;

    let waiting_limiter = Arc::clone(&limiter);
    let waiting_plan = Arc::clone(&plan);
    let task = tokio::spawn(async move {
        acquire(&waiting_limiter, &waiting_plan, Some("example.test")).await;
    });
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    tokio::time::advance(Duration::from_secs(9)).await;
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    tokio::time::advance(Duration::from_secs(1)).await;
    task.await.expect("waiting task");
}

#[tokio::test(start_paused = true)]
async fn page_varying_actual_keys_are_distinct_and_equivalent_keys_aggregate() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let first = Arc::new(plan(vec![rule(
        "page",
        "offset-41082731",
        1,
        &[(1, Duration::from_secs(60))],
    )]));
    let second = Arc::new(plan(vec![rule(
        "page",
        "offset-41082733",
        1,
        &[(1, Duration::from_secs(60))],
    )]));

    acquire(&limiter, &first, Some("first.example.test")).await;
    acquire(&limiter, &second, Some("second.example.test")).await;
    assert_eq!(
        limiter.test_quota_entry_count(),
        2,
        "progressed page keys must occupy distinct governor buckets"
    );

    let repeated = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&first),
        Some("first.example.test"),
    );
    tokio::task::yield_now().await;
    assert!(
        !repeated.is_finished(),
        "an equivalent actual page key must aggregate with its existing bucket"
    );
    repeated.abort();
    let _ = repeated.await;
}

#[tokio::test(start_paused = true)]
async fn multiple_rules_and_windows_wait_for_the_complete_maximum() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let plan = Arc::new(plan(vec![
        rule(
            "tenant",
            "north",
            1,
            &[(2, Duration::from_secs(10)), (2, Duration::from_secs(20))],
        ),
        rule("host", "example", 1, &[(2, Duration::from_secs(5))]),
    ]));
    acquire(&limiter, &plan, Some("example.test")).await;
    acquire(&limiter, &plan, Some("example.test")).await;

    let waiting = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    tokio::time::advance(Duration::from_secs(9)).await;
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    tokio::time::advance(Duration::from_secs(1)).await;
    waiting
        .await
        .expect("waiting task")
        .expect("complete plan reservation");
}

#[tokio::test(start_paused = true)]
async fn unavailable_rule_causes_no_partial_reservation_or_lock_across_wait() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let unavailable = Arc::new(plan(vec![rule(
        "slow",
        "north",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    acquire(&limiter, &unavailable, Some("example.test")).await;

    let combined = Arc::new(plan(vec![
        rule("slow", "north", 1, &[(1, Duration::from_secs(10))]),
        rule("available", "west", 1, &[(2, Duration::from_secs(100))]),
    ]));
    let waiting = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&combined),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    assert!(
        limiter.state.try_lock().is_ok(),
        "governor lock must be released before sleeping"
    );
    waiting.abort();
    let _ = waiting.await;

    let available = Arc::new(plan(vec![rule(
        "available",
        "west",
        1,
        &[(2, Duration::from_secs(100))],
    )]));
    acquire(&limiter, &available, Some("example.test")).await;
    let second = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&available),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    assert!(
        second.is_finished(),
        "canceled combined wait must leave both available tokens unconsumed"
    );
    second
        .await
        .expect("available task")
        .expect("second available reservation");
}

#[tokio::test(start_paused = true)]
async fn cancellation_while_waiting_consumes_no_future_quota() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let plan = Arc::new(plan(vec![rule(
        "tenant",
        "north",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    acquire(&limiter, &plan, Some("example.test")).await;

    let waiting_limiter = Arc::clone(&limiter);
    let waiting_plan = Arc::clone(&plan);
    let waiting = tokio::spawn(async move {
        acquire(&waiting_limiter, &waiting_plan, Some("example.test")).await;
    });
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    waiting.abort();
    let _ = waiting.await;

    tokio::time::advance(Duration::from_secs(10)).await;
    acquire(&limiter, &plan, Some("example.test")).await;
}

#[tokio::test(start_paused = true)]
async fn successful_reservation_is_not_refunded_after_later_cancellation() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let plan = Arc::new(plan(vec![rule(
        "tenant",
        "north",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    acquire(&limiter, &plan, Some("example.test")).await;

    let later = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    assert!(
        !later.is_finished(),
        "a completed reservation remains consumed even if later request work is absent"
    );
    later.abort();
    let _ = later.await;
}

#[tokio::test(start_paused = true)]
async fn duplicate_equivalent_contributions_cannot_undercount() {
    let limiter = GovernorRateLimiter::new();
    let plan = plan(vec![rule(
        "same",
        "north",
        1,
        &[(2, Duration::from_secs(10)), (2, Duration::from_secs(10))],
    )]);
    acquire(&limiter, &plan, Some("example.test")).await;
    let request = limiter.acquire(RateLimitRequest {
        endpoint: "RateApi::get",
        method: &Method::GET,
        safe_url: "https://example.test/get",
        host: Some("example.test"),
        page_index: 0,
        plan: &plan,
    });
    tokio::pin!(request);
    assert!(
        std::future::poll_fn(|context| std::task::Poll::Ready(
            request.as_mut().poll(context).is_pending()
        ))
        .await
    );
}

#[tokio::test]
async fn equivalent_contributions_above_capacity_fail_before_state_insertion() {
    let limiter = GovernorRateLimiter::new();
    let plan = plan(vec![rule(
        "same",
        "north",
        1,
        &[
            (2, Duration::from_secs(10)),
            (2, Duration::from_secs(10)),
            (2, Duration::from_secs(10)),
        ],
    )]);
    let error = rate_error(
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &plan,
            })
            .await,
        "over-capacity aggregate unexpectedly granted",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    assert!(limiter.state.lock().expect("state").quota.is_empty());
}

#[tokio::test(start_paused = true)]
async fn quota_state_is_bounded_idle_pruned_and_eviction_allows_fresh_burst() {
    let limiter = GovernorRateLimiter::new()
        .quota_capacity(2)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(5))
        .expect("idle TTL");
    let first = plan(vec![rule(
        "first",
        "one",
        1,
        &[(1, Duration::from_secs(100))],
    )]);
    let second = plan(vec![rule(
        "second",
        "two",
        1,
        &[(1, Duration::from_secs(100))],
    )]);
    let third = plan(vec![rule(
        "third",
        "three",
        1,
        &[(1, Duration::from_secs(100))],
    )]);
    acquire(&limiter, &first, Some("example.test")).await;
    acquire(&limiter, &second, Some("example.test")).await;
    acquire(&limiter, &third, Some("example.test")).await;
    assert_eq!(limiter.state.lock().expect("state").quota.len(), 2);

    acquire(&limiter, &first, Some("example.test")).await;
    assert_eq!(
        limiter.state.lock().expect("state").quota.len(),
        2,
        "re-encountered evicted key receives a fresh local burst within the bound"
    );

    tokio::time::advance(Duration::from_secs(5)).await;
    acquire(&limiter, &third, Some("example.test")).await;
    let state = limiter.state.lock().expect("state");
    assert_eq!(state.quota.len(), 1, "idle entries are pruned at the TTL");
    assert_eq!(
        state
            .quota
            .keys()
            .next()
            .expect("one entry")
            .rule
            .id
            .as_ref(),
        "third"
    );
}

#[tokio::test(start_paused = true)]
async fn current_bucket_cooldown_is_exact_and_empty_plan_falls_back_to_host() {
    let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
        duration: Duration::from_secs(10),
        scope: FixedScope::Buckets,
    })));
    let first = Arc::new(plan(vec![rule(
        "first",
        "one",
        1,
        &[(10, Duration::from_secs(1))],
    )]));
    let second = Arc::new(plan(vec![rule(
        "second",
        "two",
        1,
        &[(10, Duration::from_secs(1))],
    )]));
    observe(
        &limiter,
        &first,
        Some("origin.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Duration::from_secs(60),
    )
    .await;

    let blocked = spawn_acquire(Arc::clone(&limiter), Arc::clone(&first), Some("other.test"));
    let unrelated = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&second),
        Some("origin.test"),
    );
    tokio::task::yield_now().await;
    assert!(!blocked.is_finished());
    assert!(unrelated.is_finished());
    unrelated
        .await
        .expect("unrelated task")
        .expect("unrelated bucket");
    blocked.abort();
    let _ = blocked.await;

    let empty = Arc::new(RateLimitPlan::empty());
    observe(
        &limiter,
        &empty,
        Some("empty.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Duration::from_secs(60),
    )
    .await;
    let host_blocked = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&second),
        Some("empty.test"),
    );
    let host_free = spawn_acquire(Arc::clone(&limiter), Arc::clone(&second), Some("free.test"));
    tokio::task::yield_now().await;
    assert!(!host_blocked.is_finished());
    assert!(host_free.is_finished());
    host_free.await.expect("free host task").expect("free host");
    host_blocked.abort();
    let _ = host_blocked.await;
}

#[tokio::test(start_paused = true)]
async fn host_and_client_cooldowns_affect_only_their_normative_scopes() {
    let plan = Arc::new(plan(vec![rule(
        "quota",
        "key",
        1,
        &[(100, Duration::from_secs(1))],
    )]));
    let host_limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
        duration: Duration::from_secs(10),
        scope: FixedScope::Host,
    })));
    observe(
        &host_limiter,
        &plan,
        Some("blocked.test"),
        StatusCode::OK,
        Duration::from_secs(60),
    )
    .await;
    let blocked = spawn_acquire(
        Arc::clone(&host_limiter),
        Arc::clone(&plan),
        Some("blocked.test"),
    );
    let free = spawn_acquire(
        Arc::clone(&host_limiter),
        Arc::clone(&plan),
        Some("free.test"),
    );
    tokio::task::yield_now().await;
    assert!(!blocked.is_finished());
    assert!(free.is_finished());
    free.await.expect("free task").expect("free host");
    blocked.abort();
    let _ = blocked.await;

    let client_limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
        duration: Duration::from_secs(10),
        scope: FixedScope::Client,
    })));
    observe(
        &client_limiter,
        &plan,
        Some("first.test"),
        StatusCode::OK,
        Duration::from_secs(60),
    )
    .await;
    let client_blocked = spawn_acquire(
        Arc::clone(&client_limiter),
        Arc::clone(&plan),
        Some("different.test"),
    );
    tokio::task::yield_now().await;
    assert!(!client_blocked.is_finished());
    client_blocked.abort();
    let _ = client_blocked.await;
}

#[tokio::test(start_paused = true)]
async fn cooldown_is_clamped_extended_never_shortened_and_zero_is_noop() {
    let plan = Arc::new(RateLimitPlan::empty());
    let long = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
        duration: Duration::from_secs(20),
        scope: FixedScope::Client,
    })));
    observe(
        &long,
        &plan,
        Some("example.test"),
        StatusCode::OK,
        Duration::from_secs(10),
    )
    .await;
    tokio::time::advance(Duration::from_secs(2)).await;
    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(3),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(10),
    );
    let waiting = spawn_acquire(Arc::clone(&long), Arc::clone(&plan), Some("example.test"));
    tokio::time::advance(Duration::from_secs(7)).await;
    tokio::task::yield_now().await;
    assert!(
        !waiting.is_finished(),
        "shorter update must not shorten the cap"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    waiting
        .await
        .expect("clamped task")
        .expect("clamped cooldown expired");

    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::ZERO,
            scope: CooldownScope::Client,
        },
        Duration::from_secs(60),
    );
    acquire(&long, &plan, Some("example.test")).await;

    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(4),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(60),
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(6),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(60),
    );
    let extended = spawn_acquire(Arc::clone(&long), Arc::clone(&plan), Some("example.test"));
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;
    assert!(
        !extended.is_finished(),
        "later expiration extends existing state"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    extended
        .await
        .expect("extended task")
        .expect("extended cooldown expired");
}

#[tokio::test(start_paused = true)]
async fn cooldown_state_capacity_uses_deterministic_eviction() {
    let limiter = Arc::new(
        GovernorRateLimiter::new()
            .cooldown_capacity(1)
            .expect("cooldown capacity")
            .observer(Arc::new(FixedObserver {
                duration: Duration::from_secs(10),
                scope: FixedScope::Host,
            })),
    );
    let plan = Arc::new(RateLimitPlan::empty());
    observe(
        &limiter,
        &plan,
        Some("first.test"),
        StatusCode::OK,
        Duration::from_secs(60),
    )
    .await;
    observe(
        &limiter,
        &plan,
        Some("second.test"),
        StatusCode::OK,
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(limiter.state.lock().expect("state").cooldowns.len(), 1);

    let evicted = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("first.test"));
    let retained = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("second.test"));
    tokio::task::yield_now().await;
    assert!(evicted.is_finished());
    assert!(!retained.is_finished());
    evicted
        .await
        .expect("evicted task")
        .expect("evicted cooldown is forgotten");
    retained.abort();
    let _ = retained.await;
}

#[tokio::test(start_paused = true)]
async fn default_retry_after_cooldown_is_future_only_host_fallback_and_runtime_capped() {
    let limiter = Arc::new(GovernorRateLimiter::new());
    let plan = Arc::new(RateLimitPlan::empty());
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static(" 20 "),
    );
    observe_headers(
        &limiter,
        &plan,
        Some("limited.test"),
        StatusCode::TOO_MANY_REQUESTS,
        &headers,
        Duration::from_secs(5),
    )
    .await;

    let blocked = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("limited.test"),
    );
    let free = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("free.test"));
    tokio::task::yield_now().await;
    assert!(!blocked.is_finished());
    assert!(free.is_finished());
    free.await.expect("free task").expect("other host");
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert!(!blocked.is_finished());
    tokio::time::advance(Duration::from_secs(1)).await;
    blocked
        .await
        .expect("blocked task")
        .expect("runtime-capped cooldown");

    headers.insert(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static("malformed"),
    );
    observe_headers(
        &limiter,
        &plan,
        Some("malformed.test"),
        StatusCode::TOO_MANY_REQUESTS,
        &headers,
        Duration::from_secs(5),
    )
    .await;
    acquire(&limiter, &plan, Some("malformed.test")).await;

    headers.insert(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static("18446744073709551615"),
    );
    observe_headers(
        &limiter,
        &plan,
        Some("excessive.test"),
        StatusCode::TOO_MANY_REQUESTS,
        &headers,
        Duration::from_secs(5),
    )
    .await;
    let excessive = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("excessive.test"),
    );
    tokio::task::yield_now().await;
    assert!(!excessive.is_finished());
    tokio::time::advance(Duration::from_secs(5)).await;
    excessive
        .await
        .expect("excessive task")
        .expect("excessive delta is safely runtime-capped");
}

#[tokio::test(start_paused = true)]
async fn configured_observer_replaces_rather_than_composes_with_default() {
    let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(NoCooldownObserver)));
    let plan = RateLimitPlan::empty();
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static("60"),
    );
    observe_headers(
        &limiter,
        &plan,
        Some("example.test"),
        StatusCode::TOO_MANY_REQUESTS,
        &headers,
        Duration::from_secs(60),
    )
    .await;
    acquire(&limiter, &plan, Some("example.test")).await;
    assert!(limiter.state.lock().expect("state").cooldowns.is_empty());
}

#[tokio::test]
async fn zero_key_limit_accepts_global_and_rejects_nonempty_without_text() {
    const KEY_SENTINEL: &str = "governor-key-private-ae7e93";
    let limiter = GovernorRateLimiter::new().key_limit(0).expect("limit");
    let global = plan(vec![rule("global", "", 1, &[(1, Duration::from_secs(1))])]);
    acquire(&limiter, &global, None).await;

    let keyed = plan(vec![rule(
        "keyed",
        KEY_SENTINEL,
        1,
        &[(1, Duration::from_secs(1))],
    )]);
    let error = rate_error(
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &keyed,
            })
            .await,
        "nonempty key unexpectedly fit zero limit",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert!(!error.to_string().contains(KEY_SENTINEL));
    assert!(!format!("{error:?}").contains(KEY_SENTINEL));
}

#[tokio::test]
async fn governor_rejects_first_excess_without_sensitive_output() {
    const ID_SENTINEL: &str = "configured-governor-id-private-b3f1";
    const KEY_SENTINEL: &str = "configured-governor-key-private-781d32";
    let configuration_error = rate_error(
        GovernorRateLimiter::new().key_limit(MAX_RATE_LIMIT_KEY_BYTES + 1),
        "key limit above the hard cap unexpectedly succeeded",
    );
    assert_eq!(
        configuration_error.kind(),
        RateLimitErrorKind::InvalidConfiguration
    );

    let limit = "tenant".len() + KEY_SENTINEL.len() - 1;
    let limiter = GovernorRateLimiter::new()
        .key_limit(limit)
        .expect("configured key limit");
    let keyed = plan(vec![rule(
        ID_SENTINEL,
        KEY_SENTINEL,
        1,
        &[(1, Duration::from_secs(1))],
    )]);
    let error = rate_error(
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &keyed,
            })
            .await,
        "oversized configured key unexpectedly granted",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert_eq!(limiter.test_acquisition_count(), 1);
    assert_eq!(limiter.test_quota_entry_count(), 0);
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(ID_SENTINEL));
        assert!(!current.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{current:?}").contains(ID_SENTINEL));
        assert!(!format!("{current:?}").contains(KEY_SENTINEL));
        source = current.source();
    }
}

#[test]
fn poisoned_state_fails_future_acquisition_closed() {
    let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
        duration: Duration::from_secs(1),
        scope: FixedScope::Client,
    })));
    let poison = Arc::clone(&limiter);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = poison.state.lock().expect("initial lock");
        panic!("intentional test poison");
    }));
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let empty = RateLimitPlan::empty();
    runtime.block_on(observe(
        &limiter,
        &empty,
        Some("example.test"),
        StatusCode::OK,
        Duration::from_secs(60),
    ));
    assert!(limiter.internal_failed.load(AtomicOrdering::Acquire));
    let error = rate_error(
        runtime.block_on(limiter.acquire(RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            safe_url: "https://example.test/get",
            host: Some("example.test"),
            page_index: 0,
            plan: &empty,
        })),
        "poisoned state unexpectedly granted",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::Internal);
}

#[test]
fn boundary_helpers_are_checked() {
    assert_eq!(ceil_div(0, 3), 0);
    assert_eq!(ceil_div(1, 3), 1);
    assert_eq!(ceil_div(4, 3), 2);
    assert_eq!(
        duration_from_nanos(NANOS_PER_SECOND + 7).expect("duration"),
        Duration::new(1, 7)
    );
    assert_eq!(
        duration_from_nanos(u128::MAX)
            .expect_err("unrepresentable duration")
            .kind(),
        RateLimitErrorKind::Internal
    );
    let now = tokio::time::Instant::now();
    if let Some(wake) = now.checked_add(Duration::MAX) {
        assert!(wake > now);
    }
    let maximum = plan(vec![rule(
        "maximum",
        "",
        u32::MAX,
        &[(u32::MAX, Duration::MAX)],
    )]);
    let contributions = Contributions::from_plan(&maximum, 0).expect("maximum contribution");
    let (key, cost) = contributions.0.first_key_value().expect("one bucket");
    assert_eq!(
        key.full_balance().expect("maximum full balance"),
        key.required_balance(*cost)
            .expect("maximum required balance")
    );
    let mut state = GovernorState::new();
    state.next_touch = u128::MAX;
    assert_eq!(
        state.next_touch().expect_err("touch overflow").kind(),
        RateLimitErrorKind::Internal
    );
}
