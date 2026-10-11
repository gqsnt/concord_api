use std::num::NonZeroU32;

use http::{Method, StatusCode};

use super::*;
use concord_core::contracts::AttemptInfo;
use concord_core::rate_limit::{
    MAX_RATE_LIMIT_POLICIES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_POLICY, RateLimitId, RateLimitKey,
    RateLimitPolicy, RateLimitWindow,
};

fn policy(id: &str, key: &str, cost: u32, windows: &[(u32, Duration)]) -> RateLimitPolicy {
    RateLimitPolicy::new(
        RateLimitId::new(id).expect("ID"),
        if key.is_empty() {
            RateLimitKey::from_canonical_values([("scope", "global")]).expect("constant key")
        } else {
            RateLimitKey::from_canonical_values([("tenant", key)]).expect("key")
        },
        NonZeroU32::new(cost).expect("cost"),
        windows.iter().map(|(capacity, duration)| {
            RateLimitWindow::new(NonZeroU32::new(*capacity).expect("capacity"), *duration)
                .expect("window")
        }),
    )
    .expect("policy")
}

fn plan(rules: Vec<RateLimitPolicy>) -> RateLimitPlan {
    RateLimitPlan::try_from_policies(rules).expect("valid test plan")
}

const fn initial_attempt() -> AttemptInfo {
    AttemptInfo {
        index: 0,
        policy_retry_index: 0,
        authentication_recovery: false,
    }
}

fn rate_error<T>(result: Result<T, RateLimitError>, message: &str) -> RateLimitError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

async fn acquire(limiter: &LocalRateLimiter, plan: &RateLimitPlan, logical_origin: Option<&str>) {
    try_acquire(limiter, plan, logical_origin)
        .await
        .expect("acquire");
}

async fn try_acquire(
    limiter: &LocalRateLimiter,
    plan: &RateLimitPlan,
    logical_origin: Option<&str>,
) -> Result<RateLimitPermit, RateLimitError> {
    limiter
        .acquire(RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            logical_origin,
            page_index: 0,
            attempt: initial_attempt(),
            plan,
            capability: LocalRateLimiter::CAPABILITY,
        })
        .await
}

fn spawn_acquire(
    limiter: Arc<LocalRateLimiter>,
    plan: Arc<RateLimitPlan>,
    logical_origin: Option<&'static str>,
) -> tokio::task::JoinHandle<Result<RateLimitPermit, RateLimitError>> {
    tokio::spawn(async move {
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                logical_origin,
                page_index: 0,
                attempt: initial_attempt(),
                plan: &plan,
                capability: LocalRateLimiter::CAPABILITY,
            })
            .await
    })
}

async fn observe(
    limiter: &Arc<LocalRateLimiter>,
    plan: &RateLimitPlan,
    logical_origin: Option<&str>,
    status: StatusCode,
    cap: Duration,
) {
    let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
    limiter_object.observe(RateLimitResponse {
        request: RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            logical_origin,
            page_index: 0,
            attempt: initial_attempt(),
            plan,
            capability: LocalRateLimiter::CAPABILITY,
        },
        status,
        server_delay: Some(Duration::from_secs(10)),
        cooldown_cap: cap,
    });
}

async fn observe_feedback(
    limiter: &Arc<LocalRateLimiter>,
    plan: &RateLimitPlan,
    logical_origin: Option<&str>,
    status: StatusCode,
    server_delay: Option<Duration>,
    cap: Duration,
) {
    let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
    limiter_object.observe(RateLimitResponse {
        request: RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            logical_origin,
            page_index: 0,
            attempt: initial_attempt(),
            plan,
            capability: LocalRateLimiter::CAPABILITY,
        },
        status,
        server_delay,
        cooldown_cap: cap,
    });
}

#[test]
fn defaults_and_configuration_validation_are_finite() {
    let limiter = LocalRateLimiter::new();
    assert_eq!(limiter.config.quota_capacity, 4_096);
    assert_eq!(limiter.config.cooldown_capacity, 4_096);
    assert_eq!(limiter.config.idle_ttl, Duration::from_secs(15 * 60));
    assert_eq!(limiter.config.key_limit, 1_024);
    assert_eq!(
        rate_error(
            LocalRateLimiter::new().quota_capacity(0),
            "zero quota capacity unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(
        rate_error(
            LocalRateLimiter::new().cooldown_capacity(0),
            "zero cooldown capacity unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(
        rate_error(
            LocalRateLimiter::new().idle_ttl(Duration::ZERO),
            "zero idle TTL unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
    LocalRateLimiter::new()
        .key_limit(0)
        .expect("zero key limit is valid");
}

#[tokio::test(start_paused = true)]
async fn local_limiter_accepts_maximum_valid_plan() {
    let rules = (0..MAX_RATE_LIMIT_POLICIES_PER_PLAN)
        .map(|rule_index| {
            RateLimitPolicy::new(
                RateLimitId::new(format!("maximum-policy-{rule_index}")).expect("bounded ID"),
                RateLimitKey::from_canonical_values([("scope", "maximum")]).expect("key"),
                NonZeroU32::MIN,
                (0..MAX_RATE_LIMIT_WINDOWS_PER_POLICY).map(|window_index| {
                    RateLimitWindow::new(
                        NonZeroU32::MIN,
                        Duration::from_secs(u64::try_from(window_index + 1).expect("test index")),
                    )
                    .expect("bounded window")
                }),
            )
            .expect("maximum-window policy")
        })
        .collect();
    let plan = plan(rules);
    let limiter = LocalRateLimiter::new();

    acquire(&limiter, &plan, Some("example.test")).await;

    assert_eq!(plan.policies().len(), MAX_RATE_LIMIT_POLICIES_PER_PLAN);
    assert!(
        plan.policies()
            .iter()
            .all(|policy| policy.windows().len() == MAX_RATE_LIMIT_WINDOWS_PER_POLICY)
    );
    assert_eq!(
        limiter.test_quota_entry_count(),
        MAX_RATE_LIMIT_POLICIES_PER_PLAN * MAX_RATE_LIMIT_WINDOWS_PER_POLICY
    );
}

#[tokio::test(start_paused = true)]
async fn first_burst_is_immediate_and_refill_wait_is_controlled() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
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
async fn separately_built_limiters_have_separate_state() {
    let first = Arc::new(LocalRateLimiter::new());
    let second = LocalRateLimiter::new();
    let plan = Arc::new(plan(vec![policy(
        "shared-key",
        "tenant",
        1,
        &[(1, Duration::from_secs(10))],
    )]));

    acquire(&first, &plan, Some("example.test")).await;
    acquire(&second, &plan, Some("example.test")).await;

    let repeated = spawn_acquire(Arc::clone(&first), Arc::clone(&plan), Some("example.test"));
    tokio::task::yield_now().await;
    assert!(!repeated.is_finished());
    repeated.abort();
    let _ = repeated.await;
}

#[tokio::test(start_paused = true)]
async fn shared_arc_shares_state() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let shared = Arc::clone(&limiter);
    let plan = Arc::new(plan(vec![policy(
        "shared-key",
        "tenant",
        1,
        &[(1, Duration::from_secs(10))],
    )]));

    acquire(&limiter, &plan, Some("example.test")).await;
    let repeated = spawn_acquire(shared, Arc::clone(&plan), Some("example.test"));
    tokio::task::yield_now().await;
    assert!(!repeated.is_finished());
    repeated.abort();
    let _ = repeated.await;
}

#[tokio::test(start_paused = true)]
async fn same_resolved_key_shares_state() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let first = Arc::new(plan(vec![policy(
        "same-policy",
        "same-value",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    let equivalent = Arc::new(plan(vec![policy(
        "same-policy",
        "same-value",
        1,
        &[(1, Duration::from_secs(10))],
    )]));

    acquire(&limiter, &first, Some("first-host.test")).await;
    let repeated = spawn_acquire(
        Arc::clone(&limiter),
        equivalent,
        Some("different-host.test"),
    );
    tokio::task::yield_now().await;
    assert!(
        !repeated.is_finished(),
        "equal resolved policy IDs, key parts, and windows must share state"
    );
    repeated.abort();
    let _ = repeated.await;
}

#[tokio::test(start_paused = true)]
async fn page_varying_actual_keys_are_distinct_and_equivalent_keys_aggregate() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let first = Arc::new(plan(vec![policy(
        "page",
        "offset-41082731",
        1,
        &[(1, Duration::from_secs(60))],
    )]));
    let second = Arc::new(plan(vec![policy(
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
        "progressed page keys must occupy distinct local limiter buckets"
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
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![
        policy(
            "tenant",
            "north",
            1,
            &[(2, Duration::from_secs(10)), (2, Duration::from_secs(20))],
        ),
        policy("host", "example", 1, &[(2, Duration::from_secs(5))]),
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
async fn whole_plan_wait_causes_no_partial_reservation_or_lock_across_wait() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let unavailable = Arc::new(plan(vec![policy(
        "slow",
        "north",
        1,
        &[(1, Duration::from_secs(10))],
    )]));
    acquire(&limiter, &unavailable, Some("example.test")).await;

    let combined = Arc::new(plan(vec![
        policy("slow", "north", 1, &[(1, Duration::from_secs(10))]),
        policy("available", "west", 1, &[(2, Duration::from_secs(100))]),
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
        "local limiter lock must be released before sleeping"
    );
    waiting.abort();
    let _ = waiting.await;

    let available = Arc::new(plan(vec![policy(
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
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
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
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
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

#[test]
fn repeated_window_durations_never_reach_the_limiter() {
    for windows in [
        &[(2, Duration::from_secs(10)), (2, Duration::from_secs(10))][..],
        &[(2, Duration::from_secs(10)), (3, Duration::from_secs(10))][..],
    ] {
        let error = rate_error(
            RateLimitPolicy::new(
                RateLimitId::new("same").expect("ID"),
                RateLimitKey::from_canonical_values([("tenant", "north")]).expect("key"),
                NonZeroU32::MIN,
                windows.iter().map(|(capacity, duration)| {
                    RateLimitWindow::new(NonZeroU32::new(*capacity).expect("capacity"), *duration)
                        .expect("window")
                }),
            ),
            "repeated window duration unexpectedly accepted",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    }
}

#[tokio::test(start_paused = true)]
async fn state_capacity_is_recoverable() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(5))
        .expect("idle TTL");
    let first = plan(vec![policy(
        "first",
        "one",
        1,
        &[(1, Duration::from_secs(5))],
    )]);
    let second = plan(vec![policy(
        "second",
        "two",
        1,
        &[(1, Duration::from_secs(5))],
    )]);

    acquire(&limiter, &first, Some("example.test")).await;
    let error = rate_error(
        try_acquire(&limiter, &second, Some("example.test")).await,
        "new state above capacity unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);

    tokio::time::advance(Duration::from_secs(5)).await;
    acquire(&limiter, &second, Some("example.test")).await;
    assert!(
        !limiter.internal_failed.load(AtomicOrdering::Acquire),
        "recoverable capacity pressure must not poison the limiter"
    );
}

#[tokio::test(start_paused = true)]
async fn state_capacity_does_not_poison_limiter() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity");
    let first = plan(vec![policy(
        "first",
        "one",
        1,
        &[(2, Duration::from_secs(100))],
    )]);
    let second = plan(vec![policy(
        "second",
        "two",
        1,
        &[(1, Duration::from_secs(100))],
    )]);

    acquire(&limiter, &first, Some("example.test")).await;
    let error = rate_error(
        try_acquire(&limiter, &second, Some("example.test")).await,
        "new state above capacity unexpectedly succeeded",
    );

    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    assert!(!limiter.internal_failed.load(AtomicOrdering::Acquire));
}

#[tokio::test(start_paused = true)]
async fn existing_bucket_works_after_new_bucket_capacity_failure() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity");
    let existing = plan(vec![policy(
        "existing",
        "one",
        1,
        &[(2, Duration::from_secs(100))],
    )]);
    let new = plan(vec![policy(
        "new",
        "two",
        1,
        &[(1, Duration::from_secs(100))],
    )]);

    acquire(&limiter, &existing, Some("example.test")).await;
    assert_eq!(
        rate_error(
            try_acquire(&limiter, &new, Some("example.test")).await,
            "new bucket unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::StateCapacity
    );
    acquire(&limiter, &existing, Some("example.test")).await;
}

#[tokio::test(start_paused = true)]
async fn active_quota_is_not_evicted() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity");
    let active = plan(vec![policy(
        "active",
        "one",
        1,
        &[(1, Duration::from_secs(100))],
    )]);
    let incoming = plan(vec![policy(
        "incoming",
        "two",
        1,
        &[(1, Duration::from_secs(100))],
    )]);

    acquire(&limiter, &active, Some("example.test")).await;
    let error = rate_error(
        try_acquire(&limiter, &incoming, Some("example.test")).await,
        "active state was unexpectedly evicted",
    );

    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    let state = limiter.state.lock().expect("state");
    assert_eq!(state.quota.len(), 1);
    assert_eq!(
        state
            .quota
            .keys()
            .next()
            .expect("active key")
            .policy
            .id
            .as_ref(),
        "active"
    );
}

#[tokio::test(start_paused = true)]
async fn consumed_long_window_is_not_pruned_by_short_idle_ttl() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(5))
        .expect("idle TTL");
    let long = plan(vec![policy(
        "long",
        "one",
        1,
        &[(1, Duration::from_secs(100))],
    )]);
    let incoming = plan(vec![policy(
        "incoming",
        "two",
        1,
        &[(1, Duration::from_secs(1))],
    )]);

    acquire(&limiter, &long, Some("example.test")).await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = rate_error(
        try_acquire(&limiter, &incoming, Some("example.test")).await,
        "partially refilled long window was unexpectedly pruned",
    );

    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    let state = limiter.state.lock().expect("state");
    assert_eq!(state.quota.len(), 1);
    let (key, entry) = state.quota.first_key_value().expect("long entry");
    assert_eq!(key.policy.id.as_ref(), "long");
    assert!(entry.balance < key.full_balance().expect("full balance"));
}

#[tokio::test(start_paused = true)]
async fn fully_replenished_idle_bucket_is_pruned() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(5))
        .expect("idle TTL");
    let first = plan(vec![policy(
        "first",
        "one",
        1,
        &[(1, Duration::from_secs(5))],
    )]);

    acquire(&limiter, &first, Some("example.test")).await;
    tokio::time::advance(Duration::from_secs(5)).await;
    acquire(&limiter, &RateLimitPlan::empty(), Some("example.test")).await;

    assert!(limiter.state.lock().expect("state").quota.is_empty());
}

#[tokio::test(start_paused = true)]
async fn pruning_frees_capacity_for_new_bucket() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(5))
        .expect("idle TTL");
    let first = plan(vec![policy(
        "first",
        "one",
        1,
        &[(1, Duration::from_secs(5))],
    )]);
    let second = plan(vec![policy(
        "second",
        "two",
        1,
        &[(1, Duration::from_secs(5))],
    )]);

    acquire(&limiter, &first, Some("example.test")).await;
    tokio::time::advance(Duration::from_secs(5)).await;
    acquire(&limiter, &second, Some("example.test")).await;

    let state = limiter.state.lock().expect("state");
    assert_eq!(state.quota.len(), 1);
    assert_eq!(
        state
            .quota
            .keys()
            .next()
            .expect("second key")
            .policy
            .id
            .as_ref(),
        "second"
    );
}

#[tokio::test(start_paused = true)]
async fn quota_insert_is_transactional() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity");
    let existing = plan(vec![policy(
        "first",
        "one",
        1,
        &[(2, Duration::from_secs(100))],
    )]);
    let combined = plan(vec![
        policy("first", "one", 1, &[(2, Duration::from_secs(100))]),
        policy("second", "two", 1, &[(1, Duration::from_secs(100))]),
    ]);

    acquire(&limiter, &existing, Some("example.test")).await;
    let balance_before = limiter
        .state
        .lock()
        .expect("state")
        .quota
        .values()
        .next()
        .expect("existing state")
        .balance;
    assert_eq!(
        rate_error(
            try_acquire(&limiter, &combined, Some("example.test")).await,
            "one additional entry unexpectedly fit a full state map",
        )
        .kind(),
        RateLimitErrorKind::StateCapacity
    );
    let state = limiter.state.lock().expect("state");
    assert_eq!(state.quota.len(), 1);
    assert_eq!(
        state.quota.values().next().expect("retained state").balance,
        balance_before,
        "capacity failure must not reserve from an existing contribution"
    );
}

#[tokio::test(start_paused = true)]
async fn current_bucket_cooldown_uses_resolved_by_key() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let first = Arc::new(plan(vec![policy(
        "first",
        "one",
        1,
        &[(10, Duration::from_secs(1))],
    )]));
    let second = Arc::new(plan(vec![policy(
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
async fn host_cooldown_remains_host_scoped() {
    let plan = Arc::new(plan(vec![policy(
        "quota",
        "key",
        1,
        &[(100, Duration::from_secs(1))],
    )]));
    let host_limiter = Arc::new(LocalRateLimiter::new());
    host_limiter
        .apply_cooldown(
            Some("blocked.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(10),
                scope: CooldownScope::LogicalOrigin,
            },
            Duration::from_secs(60),
        )
        .expect("host cooldown");
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
}

#[tokio::test(start_paused = true)]
async fn client_cooldown_remains_limiter_scoped() {
    let plan = Arc::new(plan(vec![policy(
        "quota",
        "key",
        1,
        &[(100, Duration::from_secs(1))],
    )]));
    let client_limiter = Arc::new(LocalRateLimiter::new());
    client_limiter
        .apply_cooldown(
            Some("first.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(10),
                scope: CooldownScope::Client,
            },
            Duration::from_secs(60),
        )
        .expect("client cooldown");
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
    let long = Arc::new(LocalRateLimiter::new());
    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(20),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(10),
    )
    .expect("initial client cooldown");
    tokio::time::advance(Duration::from_secs(2)).await;
    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(3),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(10),
    )
    .expect("shorter cooldown update");
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
    )
    .expect("zero cooldown");
    acquire(&long, &plan, Some("example.test")).await;

    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(4),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(60),
    )
    .expect("initial extended cooldown");
    tokio::time::advance(Duration::from_secs(1)).await;
    long.apply_cooldown(
        Some("example.test"),
        &plan,
        Cooldown {
            duration: Duration::from_secs(6),
            scope: CooldownScope::Client,
        },
        Duration::from_secs(60),
    )
    .expect("later extended cooldown");
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
async fn active_cooldown_is_not_evicted() {
    let limiter = Arc::new(
        LocalRateLimiter::new()
            .cooldown_capacity(1)
            .expect("cooldown capacity"),
    );
    let plan = Arc::new(RateLimitPlan::empty());
    observe(
        &limiter,
        &plan,
        Some("first.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Duration::from_secs(60),
    )
    .await;
    let error = limiter
        .apply_cooldown(
            Some("second.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(10),
                scope: CooldownScope::LogicalOrigin,
            },
            Duration::from_secs(60),
        )
        .expect_err("active cooldown cannot be evicted");
    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);

    let retained = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("first.test"));
    let rejected = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("second.test"));
    tokio::task::yield_now().await;
    assert!(!retained.is_finished());
    assert!(rejected.is_finished());
    rejected
        .await
        .expect("rejected host task")
        .expect("capacity failure installed no cooldown");
    retained.abort();
    let _ = retained.await;
}

#[tokio::test(start_paused = true)]
async fn expired_cooldown_is_pruned() {
    let limiter = Arc::new(
        LocalRateLimiter::new()
            .cooldown_capacity(1)
            .expect("cooldown capacity"),
    );
    let plan = Arc::new(RateLimitPlan::empty());
    observe(
        &limiter,
        &plan,
        Some("first.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Duration::from_secs(5),
    )
    .await;
    tokio::time::advance(Duration::from_secs(5)).await;
    observe(
        &limiter,
        &plan,
        Some("second.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(limiter.state.lock().expect("state").cooldowns.len(), 1);
    let first = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("first.test"));
    let second = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("second.test"));
    tokio::task::yield_now().await;
    assert!(first.is_finished());
    assert!(!second.is_finished());
    first
        .await
        .expect("expired host task")
        .expect("expired host cooldown was pruned");
    second.abort();
    let _ = second.await;
}

#[tokio::test(start_paused = true)]
async fn cooldown_capacity_failure_is_recoverable() {
    let limiter = LocalRateLimiter::new()
        .cooldown_capacity(1)
        .expect("cooldown capacity");
    let plan = RateLimitPlan::empty();
    let cooldown = || Cooldown {
        duration: Duration::from_secs(5),
        scope: CooldownScope::LogicalOrigin,
    };

    limiter
        .apply_cooldown(
            Some("first.test"),
            &plan,
            cooldown(),
            Duration::from_secs(60),
        )
        .expect("first cooldown");
    let error = limiter
        .apply_cooldown(
            Some("second.test"),
            &plan,
            cooldown(),
            Duration::from_secs(60),
        )
        .expect_err("second active cooldown exceeds capacity");
    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    assert!(!limiter.internal_failed.load(AtomicOrdering::Acquire));

    tokio::time::advance(Duration::from_secs(5)).await;
    limiter
        .apply_cooldown(
            Some("second.test"),
            &plan,
            cooldown(),
            Duration::from_secs(60),
        )
        .expect("capacity recovers after expiration");
}

#[tokio::test(start_paused = true)]
async fn existing_cooldown_remains_after_capacity_failure() {
    let limiter = Arc::new(
        LocalRateLimiter::new()
            .cooldown_capacity(1)
            .expect("cooldown capacity"),
    );
    let plan = Arc::new(RateLimitPlan::empty());
    limiter
        .apply_cooldown(
            Some("first.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(10),
                scope: CooldownScope::LogicalOrigin,
            },
            Duration::from_secs(60),
        )
        .expect("first cooldown");
    let expiration = limiter
        .state
        .lock()
        .expect("state")
        .cooldowns
        .values()
        .next()
        .expect("first cooldown state")
        .expiration;

    let error = limiter
        .apply_cooldown(
            Some("second.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(20),
                scope: CooldownScope::LogicalOrigin,
            },
            Duration::from_secs(60),
        )
        .expect_err("second cooldown exceeds capacity");
    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    {
        let state = limiter.state.lock().expect("state");
        assert_eq!(state.cooldowns.len(), 1);
        assert_eq!(
            state
                .cooldowns
                .values()
                .next()
                .expect("retained cooldown")
                .expiration,
            expiration
        );
    }

    let blocked = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("first.test"));
    tokio::task::yield_now().await;
    assert!(!blocked.is_finished());
    blocked.abort();
    let _ = blocked.await;
}

#[tokio::test(start_paused = true)]
async fn default_retry_after_cooldown_is_future_only_host_fallback_and_runtime_capped() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(RateLimitPlan::empty());
    observe_feedback(
        &limiter,
        &plan,
        Some("limited.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Some(Duration::from_secs(20)),
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

    observe_feedback(
        &limiter,
        &plan,
        Some("malformed.test"),
        StatusCode::TOO_MANY_REQUESTS,
        None,
        Duration::from_secs(5),
    )
    .await;
    acquire(&limiter, &plan, Some("malformed.test")).await;

    observe_feedback(
        &limiter,
        &plan,
        Some("excessive.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Some(Duration::MAX),
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

#[tokio::test]
async fn zero_key_limit_admits_no_policy_and_rejects_without_text() {
    const KEY_SENTINEL: &str = "local-limiter-key-private-ae7e93";
    let limiter = LocalRateLimiter::new().key_limit(0).expect("limit");
    acquire(&limiter, &RateLimitPlan::empty(), None).await;

    let keyed = plan(vec![policy(
        "keyed",
        KEY_SENTINEL,
        1,
        &[(1, Duration::from_secs(1))],
    )]);
    let error = rate_error(
        try_acquire(&limiter, &keyed, Some("https://example.test")).await,
        "nonempty key unexpectedly fit zero limit",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert!(!error.to_string().contains(KEY_SENTINEL));
    assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    assert_eq!(limiter.test_quota_entry_count(), 0);
}

#[tokio::test]
async fn local_limiter_rejects_first_excess_without_sensitive_output() {
    const ID_SENTINEL: &str = "configured-local-limiter-id-private-b3f1";
    const KEY_SENTINEL: &str = "configured-local-limiter-key-private-781d32";
    let configuration_error = rate_error(
        LocalRateLimiter::new().key_limit(MAX_RATE_LIMIT_KEY_BYTES + 1),
        "key limit above the hard cap unexpectedly succeeded",
    );
    assert_eq!(
        configuration_error.kind(),
        RateLimitErrorKind::InvalidConfiguration
    );

    let limit = "tenant".len() + KEY_SENTINEL.len() - 1;
    let limiter = LocalRateLimiter::new()
        .key_limit(limit)
        .expect("configured key limit");
    let keyed = plan(vec![policy(
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
                logical_origin: Some("https://example.test"),
                page_index: 0,
                attempt: initial_attempt(),
                plan: &keyed,
                capability: LocalRateLimiter::CAPABILITY,
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
    let limiter = Arc::new(LocalRateLimiter::new());
    let poison = Arc::clone(&limiter);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = poison.state.lock().expect("initial lock");
        panic!("intentional test poison");
    }));
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let empty = RateLimitPlan::empty();
    limiter.observe(RateLimitResponse {
        request: RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            logical_origin: None,
            page_index: 0,
            attempt: initial_attempt(),
            plan: &empty,
            capability: LocalRateLimiter::CAPABILITY,
        },
        status: StatusCode::TOO_MANY_REQUESTS,
        server_delay: Some(Duration::from_secs(1)),
        cooldown_cap: Duration::from_secs(60),
    });
    assert!(limiter.internal_failed.load(AtomicOrdering::Acquire));
    let error = rate_error(
        runtime.block_on(limiter.acquire(RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            logical_origin: None,
            page_index: 0,
            attempt: initial_attempt(),
            plan: &empty,
            capability: LocalRateLimiter::CAPABILITY,
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
    let maximum = plan(vec![policy(
        "maximum",
        "",
        u32::MAX,
        &[(u32::MAX, Duration::MAX)],
    )]);
    let contributions = Contributions::from_plan(&maximum, DEFAULT_RATE_LIMIT_KEY_BYTES)
        .expect("maximum contribution");
    let (key, cost) = contributions.0.first_key_value().expect("one bucket");
    assert_eq!(
        key.full_balance().expect("maximum full balance"),
        key.required_balance(*cost)
            .expect("maximum required balance")
    );
}

fn labelled_policy(
    id: &str,
    label: &str,
    value: &str,
    cost: u32,
    windows: &[(u32, Duration)],
) -> RateLimitPolicy {
    RateLimitPolicy::new(
        RateLimitId::new(id).expect("ID"),
        RateLimitKey::from_canonical_values([(label, value)]).expect("key"),
        NonZeroU32::new(cost).expect("cost"),
        windows.iter().map(|(capacity, duration)| {
            RateLimitWindow::new(NonZeroU32::new(*capacity).expect("capacity"), *duration)
                .expect("window")
        }),
    )
    .expect("policy")
}

/// Returns whether one acquisition of `plan` is admitted immediately, without
/// leaving a pending waiter behind.
async fn admits_now(limiter: &Arc<LocalRateLimiter>, plan: &Arc<RateLimitPlan>) -> bool {
    let task = spawn_acquire(Arc::clone(limiter), Arc::clone(plan), Some("example.test"));
    tokio::task::yield_now().await;
    if task.is_finished() {
        task.await
            .expect("admission task")
            .expect("immediate admission");
        true
    } else {
        task.abort();
        let _ = task.await;
        false
    }
}

async fn drain(limiter: &Arc<LocalRateLimiter>, plan: &Arc<RateLimitPlan>) -> usize {
    let mut admitted = 0;
    while admits_now(limiter, plan).await {
        admitted += 1;
        assert!(admitted <= 1_000, "bounded test drain");
    }
    admitted
}

#[test]
fn local_capability_is_explicitly_local_and_valid() {
    let limiter = LocalRateLimiter::new();
    let capability = limiter.capability();
    assert_eq!(capability, LocalRateLimiter::CAPABILITY);
    assert_eq!(capability.enforcement(), Enforcement::Local);
    assert_eq!(capability.namespace(), "concord.local");
    assert_eq!(capability.algorithm_version(), 1);
    capability.validate().expect("valid local capability");
}

// ---- V3-R07: atomic conjunctive admission -------------------------------

#[tokio::test(start_paused = true)]
async fn multi_policy_state_capacity_failure_debits_no_prefix() {
    let limiter = Arc::new(
        LocalRateLimiter::new()
            .quota_capacity(2)
            .expect("quota capacity"),
    );
    let first = Arc::new(plan(vec![policy(
        "first",
        "one",
        1,
        &[(2, Duration::from_secs(100))],
    )]));
    acquire(&limiter, &first, Some("example.test")).await;

    let combined = plan(vec![
        policy("first", "one", 1, &[(2, Duration::from_secs(100))]),
        policy("second", "two", 1, &[(1, Duration::from_secs(100))]),
        policy("third", "three", 1, &[(1, Duration::from_secs(100))]),
    ]);
    let error = rate_error(
        try_acquire(&limiter, &combined, Some("example.test")).await,
        "a plan whose missing state exceeds capacity was admitted",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::StateCapacity);
    assert_eq!(
        limiter.test_quota_entry_count(),
        1,
        "no new state is inserted for a rejected plan"
    );
    assert_eq!(
        drain(&limiter, &first).await,
        1,
        "the already known first policy kept its remaining token"
    );
}

#[tokio::test(start_paused = true)]
async fn multi_policy_invalid_key_failure_debits_and_inserts_nothing() {
    const KEY_SENTINEL: &str = "local-atomic-key-sentinel-5d20";
    let valid = policy("valid", "one", 1, &[(1, Duration::from_secs(100))]);
    let limit = valid.key().total_bytes();
    let limiter = Arc::new(LocalRateLimiter::new().key_limit(limit).expect("key limit"));
    let combined = plan(vec![
        valid.clone(),
        policy(
            "oversized",
            KEY_SENTINEL,
            1,
            &[(1, Duration::from_secs(100))],
        ),
    ]);
    let error = rate_error(
        try_acquire(&limiter, &combined, Some("example.test")).await,
        "an oversized key was admitted",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    assert_eq!(limiter.test_quota_entry_count(), 0);
    let valid_only = Arc::new(plan(vec![valid]));
    assert_eq!(drain(&limiter, &valid_only).await, 1);
}

#[tokio::test(start_paused = true)]
async fn multi_policy_wait_and_cancellation_debit_no_available_policy() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let exhausted = Arc::new(plan(vec![policy(
        "exhausted",
        "north",
        1,
        &[(1, Duration::from_secs(60))],
    )]));
    acquire(&limiter, &exhausted, Some("example.test")).await;
    let combined = Arc::new(plan(vec![
        policy("available", "west", 1, &[(3, Duration::from_secs(60))]),
        policy("exhausted", "north", 1, &[(1, Duration::from_secs(60))]),
        policy("other", "east", 1, &[(3, Duration::from_secs(60))]),
    ]));
    // Several blocked attempts, each cancelled while waiting.
    for _ in 0..5 {
        assert!(!admits_now(&limiter, &combined).await);
    }
    let available = Arc::new(plan(vec![policy(
        "available",
        "west",
        1,
        &[(3, Duration::from_secs(60))],
    )]));
    let other = Arc::new(plan(vec![policy(
        "other",
        "east",
        1,
        &[(3, Duration::from_secs(60))],
    )]));
    assert_eq!(drain(&limiter, &available).await, 3);
    assert_eq!(drain(&limiter, &other).await, 3);
}

#[tokio::test(start_paused = true)]
async fn committed_multi_policy_admission_is_non_refundable_after_cancellation() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let combined = Arc::new(plan(vec![
        policy("first", "one", 1, &[(1, Duration::from_secs(60))]),
        policy("second", "two", 2, &[(2, Duration::from_secs(60))]),
    ]));
    // A committed admission followed by cancelled request work (for example
    // a cancelled operation or a credential-freshness restart) stays debited.
    acquire(&limiter, &combined, Some("example.test")).await;
    let restart = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&combined),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    assert!(!restart.is_finished(), "the restart needs a new admission");
    restart.abort();
    let _ = restart.await;
    for single in [
        policy("first", "one", 1, &[(1, Duration::from_secs(60))]),
        policy("second", "two", 1, &[(2, Duration::from_secs(60))]),
    ] {
        assert!(
            !admits_now(&limiter, &Arc::new(plan(vec![single]))).await,
            "every policy of the committed plan remains debited"
        );
    }
}

// ---- V3-R08: local algorithm with an injected clock ---------------------

#[tokio::test(start_paused = true)]
async fn continuous_refill_is_capacity_per_duration_with_exact_boundaries() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
        "refill",
        "north",
        1,
        &[(10, Duration::from_secs(10))],
    )]));
    assert_eq!(drain(&limiter, &plan).await, 10, "initial full burst");

    let waiting = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(999)).await;
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished(), "one token needs exactly one second");
    tokio::time::advance(Duration::from_millis(1)).await;
    waiting
        .await
        .expect("refill task")
        .expect("admitted at the exact refill boundary");

    tokio::time::advance(Duration::from_secs(3)).await;
    assert_eq!(drain(&limiter, &plan).await, 3, "continuous partial refill");

    tokio::time::advance(Duration::from_secs(1_000)).await;
    assert_eq!(
        drain(&limiter, &plan).await,
        10,
        "refill never overshoots the capacity"
    );
}

#[tokio::test(start_paused = true)]
async fn conjunctive_windows_bind_at_each_window_boundary() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
        "windows",
        "north",
        1,
        &[(2, Duration::from_secs(1)), (3, Duration::from_secs(30))],
    )]));
    assert_eq!(drain(&limiter, &plan).await, 2, "short window binds first");
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        drain(&limiter, &plan).await,
        1,
        "long window leaves 3 - 2 = 1 plus a 1/10-second refill"
    );
    tokio::time::advance(Duration::from_secs(9)).await;
    assert_eq!(drain(&limiter, &plan).await, 1, "long window refill 3/30s");
    tokio::time::advance(Duration::from_secs(30)).await;
    assert_eq!(
        drain(&limiter, &plan).await,
        2,
        "full refill of both windows"
    );
}

#[tokio::test(start_paused = true)]
async fn cooldown_waits_exactly_the_remaining_time() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let plan = Arc::new(plan(vec![policy(
        "cooldown",
        "north",
        1,
        &[(100, Duration::from_secs(1))],
    )]));
    observe_feedback(
        &limiter,
        &plan,
        Some("example.test"),
        StatusCode::TOO_MANY_REQUESTS,
        Some(Duration::from_secs(10)),
        Duration::from_secs(60),
    )
    .await;
    tokio::time::advance(Duration::from_secs(4)).await;
    let waiting = spawn_acquire(
        Arc::clone(&limiter),
        Arc::clone(&plan),
        Some("example.test"),
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(5_999)).await;
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished(), "six seconds of cooldown remain");
    tokio::time::advance(Duration::from_millis(1)).await;
    waiting
        .await
        .expect("cooldown task")
        .expect("admitted when the cooldown expires");
}

#[tokio::test(start_paused = true)]
async fn replenished_but_recently_used_bucket_is_not_pruned_before_ttl() {
    let limiter = LocalRateLimiter::new()
        .quota_capacity(1)
        .expect("quota capacity")
        .idle_ttl(Duration::from_secs(60))
        .expect("idle TTL");
    let first = plan(vec![policy(
        "first",
        "one",
        1,
        &[(1, Duration::from_secs(1))],
    )]);
    let second = plan(vec![policy(
        "second",
        "two",
        1,
        &[(1, Duration::from_secs(1))],
    )]);
    acquire(&limiter, &first, Some("example.test")).await;
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(
        rate_error(
            try_acquire(&limiter, &second, Some("example.test")).await,
            "a replenished bucket was pruned before its idle TTL",
        )
        .kind(),
        RateLimitErrorKind::StateCapacity
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    acquire(&limiter, &second, Some("example.test")).await;
    assert_eq!(limiter.test_quota_entry_count(), 1);
}

// ---- V3-R03: bucket identity ---------------------------------------------

#[tokio::test(start_paused = true)]
async fn same_windows_under_different_ids_are_separate_buckets() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let windows = [(2, Duration::from_secs(60)), (5, Duration::from_secs(600))];
    let riot_21 = Arc::new(plan(vec![policy("concord.riot.21", "", 1, &windows)]));
    let riot_22 = Arc::new(plan(vec![policy("concord.riot.22", "", 1, &windows)]));
    assert_eq!(drain(&limiter, &riot_21).await, 2);
    assert_eq!(
        drain(&limiter, &riot_22).await,
        2,
        "numeric window reuse grants no shared identity"
    );
}

#[tokio::test(start_paused = true)]
async fn labels_are_part_of_bucket_identity_and_cost_is_not() {
    let limiter = Arc::new(LocalRateLimiter::new());
    let window = [(3, Duration::from_secs(60))];
    let tenant = Arc::new(plan(vec![labelled_policy(
        "catalog.tenant.read",
        "tenant",
        "tenant-a",
        1,
        &window,
    )]));
    let client_tenant = Arc::new(plan(vec![labelled_policy(
        "catalog.tenant.read",
        "client.tenant",
        "tenant-a",
        1,
        &window,
    )]));
    assert_eq!(drain(&limiter, &tenant).await, 3);
    assert_eq!(
        drain(&limiter, &client_tenant).await,
        3,
        "a different label is a different bucket even for equal bytes"
    );

    let costly = Arc::new(plan(vec![labelled_policy(
        "shared.cost",
        "tenant",
        "b",
        2,
        &window,
    )]));
    let cheap = Arc::new(plan(vec![labelled_policy(
        "shared.cost",
        "tenant",
        "b",
        1,
        &window,
    )]));
    assert!(admits_now(&limiter, &costly).await);
    assert!(admits_now(&limiter, &cheap).await, "one token remains");
    assert!(
        !admits_now(&limiter, &cheap).await,
        "different per-attempt costs debit one shared bucket"
    );
}

#[tokio::test(start_paused = true)]
async fn same_id_key_and_limiter_share_across_plans_but_separate_limiters_do_not() {
    let shared = Arc::new(LocalRateLimiter::new());
    let separate = Arc::new(LocalRateLimiter::new());
    let window = [(2, Duration::from_secs(60))];
    let endpoint_a = Arc::new(plan(vec![
        policy("catalog.tenant.read", "tenant-a", 1, &window),
        policy(
            "catalog.item.write",
            "tenant-a",
            1,
            &[(9, Duration::from_secs(60))],
        ),
    ]));
    let endpoint_b = Arc::new(plan(vec![policy(
        "catalog.tenant.read",
        "tenant-a",
        1,
        &window,
    )]));
    assert!(admits_now(&shared, &endpoint_a).await);
    assert_eq!(
        drain(&shared, &endpoint_b).await,
        1,
        "the second endpoint shares the policy bucket through one limiter"
    );
    assert_eq!(
        drain(&separate, &endpoint_b).await,
        2,
        "a separately built limiter object has independent counters"
    );
}
