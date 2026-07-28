use std::error::Error as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use http::{HeaderMap, HeaderValue, Method, StatusCode};
use url::Url;

use super::budget::FixedRatio;
use super::policy::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryClassifier, RetryDecision, RetryFailure,
    RetryPolicy, RetryPolicyBuildError, RetryPolicyBuildErrorKind,
};
use crate::hooks::AttemptInfo;
use crate::redaction::SanitizedHeaders;

const INITIAL_ATTEMPT: AttemptInfo = AttemptInfo {
    index: 0,
    policy_retry_index: 0,
    authentication_recovery: false,
};

fn url(value: &str) -> Url {
    Url::parse(value).expect("test URL")
}

fn policy(ratio: f64) -> RetryPolicy {
    RetryPolicy::builder()
        .max_extra_load(ratio)
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("valid retry policy")
}

fn configured_policy(
    ratio: f64,
    capacity: usize,
    ttl: Duration,
) -> Result<RetryPolicy, RetryPolicyBuildError> {
    RetryPolicy::builder()
        .max_extra_load(ratio)
        .budget_scope_capacity(capacity)
        .budget_scope_ttl(ttl)
        .classify_fn(|_| RetryDecision::Retry)
        .build()
}

fn record_initials(policy: &RetryPolicy, target: &Url, count: usize) {
    for _ in 0..count {
        policy.record_initial_dispatch(target);
    }
}

#[test]
fn ratio_conversion_is_fixed_point_and_runtime_arithmetic_is_checked() {
    let zero = FixedRatio::from_f64(0.0).expect("zero ratio");
    assert_eq!(zero.units(), 0);
    assert_eq!(zero.allowed_retries(u64::MAX), Some(0));

    let default = FixedRatio::from_f64(0.20).expect("default ratio");
    assert_eq!(default.units(), 2_000_000_000_000_000);
    assert_eq!(default.allowed_retries(1), Some(1));
    assert_eq!(default.allowed_retries(5), Some(1));
    assert_eq!(default.allowed_retries(6), Some(2));
    assert_eq!(default.allowed_retries(100), Some(20));

    let third = FixedRatio::from_f64(1.0 / 3.0).expect("third");
    assert_eq!(third.units(), 3_333_333_333_333_333);
    assert_eq!(third.allowed_retries(3), Some(1));

    let tiny = FixedRatio::from_f64(f64::MIN_POSITIVE).expect("tiny positive ratio");
    assert_eq!(tiny.units(), 1);
    assert_eq!(tiny.allowed_retries(1), Some(1));

    let maximum = FixedRatio::from_f64(1_000.0).expect("maximum ratio");
    assert_eq!(maximum.units(), 10_000_000_000_000_000_000);
    assert!(maximum.allowed_retries(u64::MAX).is_some());
}

#[test]
fn every_invalid_builder_value_is_a_source_less_structural_error() {
    let missing = RetryPolicy::builder()
        .build()
        .expect_err("classifier is required");
    assert_eq!(missing.kind(), RetryPolicyBuildErrorKind::MissingClassifier);
    assert!(missing.source().is_none());

    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1_000.01] {
        let error = RetryPolicy::builder()
            .max_extra_load(ratio)
            .classify_fn(|_| RetryDecision::Stop)
            .build()
            .expect_err("invalid ratio");
        assert_eq!(error.kind(), RetryPolicyBuildErrorKind::InvalidMaxExtraLoad);
        assert_eq!(
            error.to_string(),
            "managed retry policy configuration failed: extra-load target ratio must be finite and within 0.0 through 1000.0"
        );
    }
    for maximum in [0, MAX_POLICY_RETRIES_PER_OPERATION + 1] {
        let error = RetryPolicy::builder()
            .max_retries_per_operation(maximum)
            .classify_fn(|_| RetryDecision::Stop)
            .build()
            .expect_err("invalid retry maximum");
        assert_eq!(
            error.kind(),
            RetryPolicyBuildErrorKind::InvalidMaxRetriesPerOperation
        );
    }
    assert_eq!(
        configured_policy(0.2, 0, Duration::from_secs(1))
            .expect_err("zero capacity")
            .kind(),
        RetryPolicyBuildErrorKind::InvalidBudgetScopeCapacity
    );
    assert_eq!(
        configured_policy(0.2, 1, Duration::ZERO)
            .expect_err("zero TTL")
            .kind(),
        RetryPolicyBuildErrorKind::InvalidBudgetScopeTtl
    );
}

#[test]
fn classifier_receives_only_the_finite_semantic_context() {
    struct InspectingClassifier;

    impl RetryClassifier for InspectingClassifier {
        fn classify(&self, attempt: RetryAttempt<'_>) -> RetryDecision {
            assert_eq!(attempt.endpoint(), "retry_api.get");
            assert_eq!(attempt.method(), Method::GET);
            assert_eq!(attempt.host(), Some("api.example.test"));
            assert_eq!(attempt.page_index(), 3);
            assert_eq!(attempt.attempt(), INITIAL_ATTEMPT);
            assert_eq!(attempt.retries_performed(), 0);
            assert!(matches!(
                attempt.failure(),
                RetryFailure::RejectedStatus {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                }
            ));
            RetryDecision::Retry
        }
    }

    let policy = RetryPolicy::builder()
        .classify(InspectingClassifier)
        .build()
        .expect("classifier policy");
    let headers = HeaderMap::new();
    let attempt = RetryAttempt::new(
        "retry_api.get",
        &Method::GET,
        Some("api.example.test"),
        3,
        INITIAL_ATTEMPT,
        0,
        RetryFailure::RejectedStatus {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: SanitizedHeaders::new(&headers),
        },
    );
    assert_eq!(policy.classify(attempt), RetryDecision::Retry);
}

#[tokio::test(start_paused = true)]
async fn zero_ratio_denies_every_budgeted_retry() {
    let policy = policy(0.0);
    let target = url("https://zero.example.test/data");
    record_initials(&policy, &target, 100);
    assert!(!policy.reserve_retry(&target));
}

#[tokio::test(start_paused = true)]
async fn default_ratio_uses_low_volume_ceiling() {
    let policy = policy(0.20);
    let target = url("https://low-volume.example.test/data");
    assert_eq!(policy.max_retries_per_operation(), 2);
    let budget = policy.budget().expect("default budget");
    assert_eq!(budget.ratio().units(), 2_000_000_000_000_000);
    assert_eq!(budget.scope_capacity(), 4_096);
    assert_eq!(budget.scope_ttl(), Duration::from_secs(15 * 60));
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));
    assert!(!policy.reserve_retry(&target));
}

#[tokio::test(start_paused = true)]
async fn high_volume_budget_converges_to_the_exact_ratio() {
    let policy = policy(0.20);
    let target = url("https://high-volume.example.test/data");
    record_initials(&policy, &target, 100);
    for _ in 0..20 {
        assert!(policy.reserve_retry(&target));
    }
    assert!(!policy.reserve_retry(&target));
}

#[tokio::test(start_paused = true)]
async fn one_second_rotation_retains_exactly_ten_bounded_buckets() {
    let policy = policy(1.0);
    let target = url("https://rotation.example.test/data");
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));
    assert_eq!(
        policy
            .budget()
            .expect("budget")
            .active_bucket_count(&target),
        1
    );

    tokio::time::advance(Duration::from_secs(1)).await;
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));
    assert_eq!(
        policy
            .budget()
            .expect("budget")
            .active_bucket_count(&target),
        2
    );

    for _ in 0..20 {
        tokio::time::advance(Duration::from_secs(1)).await;
        policy.record_initial_dispatch(&target);
    }
    assert_eq!(
        policy
            .budget()
            .expect("budget")
            .active_bucket_count(&target),
        10
    );
}

#[tokio::test(start_paused = true)]
async fn rolling_window_expires_old_dispatches_and_reservations() {
    let policy = policy(1.0);
    let target = url("https://expiration.example.test/data");
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));

    tokio::time::advance(Duration::from_secs(10)).await;
    assert!(!policy.reserve_retry(&target));
    assert_eq!(
        policy
            .budget()
            .expect("budget")
            .active_bucket_count(&target),
        0
    );
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));
}

#[tokio::test(start_paused = true)]
async fn origin_scope_ignores_path_query_and_user_information() {
    let policy = policy(0.5);
    let first = url("HTTPS://first:secret@SHARED.EXAMPLE.TEST/a?tenant=one");
    let second = url("https://shared.example.test/b?tenant=two");
    policy.record_initial_dispatch(&first);
    policy.record_initial_dispatch(&second);
    assert!(policy.reserve_retry(&first));
    assert!(!policy.reserve_retry(&second));
    assert_eq!(policy.budget().expect("budget").tracked_scope_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn normalized_ip_spellings_share_one_origin() {
    let policy = policy(0.5);
    let expanded = url("https://[2001:0db8:0000:0000:0000:0000:0000:0001]/a");
    let compressed = url("https://[2001:db8::1]/b");
    policy.record_initial_dispatch(&expanded);
    policy.record_initial_dispatch(&compressed);
    assert!(policy.reserve_retry(&expanded));
    assert!(!policy.reserve_retry(&compressed));
    assert_eq!(policy.budget().expect("budget").tracked_scope_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn independent_hosts_receive_independent_budget() {
    let policy = policy(0.20);
    let first = url("https://first.example.test/data");
    let second = url("https://second.example.test/data");
    policy.record_initial_dispatch(&first);
    policy.record_initial_dispatch(&second);
    assert!(policy.reserve_retry(&first));
    assert!(policy.reserve_retry(&second));
}

#[tokio::test(start_paused = true)]
async fn scheme_and_effective_port_partition_origins() {
    let policy = policy(0.20);
    let https = url("https://scope.example.test/data");
    let http = url("http://scope.example.test/data");
    let alternate_port = url("https://scope.example.test:8443/data");
    let explicit_default = url("https://scope.example.test:443/other");

    policy.record_initial_dispatch(&https);
    policy.record_initial_dispatch(&http);
    policy.record_initial_dispatch(&alternate_port);
    assert!(policy.reserve_retry(&explicit_default));
    assert!(policy.reserve_retry(&http));
    assert!(policy.reserve_retry(&alternate_port));
    assert_eq!(policy.budget().expect("budget").tracked_scope_count(), 3);
}

#[tokio::test(start_paused = true)]
async fn idle_pruning_frees_capacity_without_evicting_active_state() {
    let policy = configured_policy(0.20, 1, Duration::from_secs(5)).expect("bounded retry policy");
    let first = url("https://old.example.test/data");
    let second = url("https://new.example.test/data");
    policy.record_initial_dispatch(&first);

    tokio::time::advance(Duration::from_secs(5)).await;
    policy.record_initial_dispatch(&second);
    assert_eq!(policy.budget().expect("budget").tracked_scope_count(), 1);
    assert!(!policy.reserve_retry(&first));
    assert!(policy.reserve_retry(&second));
}

#[tokio::test(start_paused = true)]
async fn capacity_pressure_denies_untracked_origin_without_replacing_state() {
    let policy = configured_policy(0.20, 1, Duration::from_secs(60)).expect("bounded retry policy");
    let retained = url("https://retained.example.test/data");
    let denied = url("https://denied.example.test/data");
    policy.record_initial_dispatch(&retained);
    policy.record_initial_dispatch(&denied);
    assert_eq!(policy.budget().expect("budget").tracked_scope_count(), 1);
    assert!(!policy.reserve_retry(&denied));
    assert!(policy.reserve_retry(&retained));
}

#[tokio::test(start_paused = true)]
async fn idle_ttl_resets_an_existing_scope_on_next_use() {
    let policy = configured_policy(1.0, 1, Duration::from_secs(5)).expect("bounded retry policy");
    let target = url("https://ttl.example.test/data");
    policy.record_initial_dispatch(&target);

    tokio::time::advance(Duration::from_secs(5)).await;
    assert!(!policy.reserve_retry(&target));
    policy.record_initial_dispatch(&target);
    assert!(policy.reserve_retry(&target));
}

#[tokio::test(start_paused = true)]
async fn cloned_policy_shares_budget_and_independent_builds_do_not() {
    let shared = policy(1.0);
    let clone = shared.clone();
    assert!(shared.shares_state_with(&clone));
    let target = url("https://sharing.example.test/data");
    shared.record_initial_dispatch(&target);
    assert!(clone.reserve_retry(&target));
    assert!(!shared.reserve_retry(&target));

    let first = policy(1.0);
    let second = policy(1.0);
    assert!(!first.shares_state_with(&second));
    first.record_initial_dispatch(&target);
    second.record_initial_dispatch(&target);
    assert!(first.reserve_retry(&target));
    assert!(second.reserve_retry(&target));
}

#[test]
fn without_budget_retains_the_hard_per_operation_maximum() {
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(MAX_POLICY_RETRIES_PER_OPERATION)
        .without_budget()
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("unbudgeted bounded policy");
    let target = url("https://unbudgeted.example.test/data");
    assert_eq!(
        policy.max_retries_per_operation(),
        MAX_POLICY_RETRIES_PER_OPERATION
    );
    assert!(policy.budget().is_none());
    for _ in 0..100 {
        assert!(policy.reserve_retry(&target));
    }
}

#[test]
fn concurrent_reservation_never_exceeds_the_allowed_count() {
    let policy = policy(1.0);
    let target = url("https://concurrent.example.test/data");
    policy.record_initial_dispatch(&target);
    let admitted = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    for _ in 0..64 {
        let policy = policy.clone();
        let target = target.clone();
        let admitted = Arc::clone(&admitted);
        threads.push(std::thread::spawn(move || {
            if policy.reserve_retry(&target) {
                admitted.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for thread in threads {
        thread.join().expect("reservation thread");
    }
    assert_eq!(admitted.load(Ordering::SeqCst), 1);
}

#[test]
fn debug_and_build_error_formatting_exclude_host_header_and_classifier_state() {
    const HOST_SECRET: &str = "retry-host-private-sentinel-9ef213";
    const HEADER_SECRET: &str = "retry-header-private-sentinel-719c8e";
    const CLASSIFIER_SECRET: &str = "retry-classifier-private-sentinel-28bc41";

    let captured_classifier_state = CLASSIFIER_SECRET.to_owned();
    let policy = RetryPolicy::builder()
        .classify_fn(move |_| {
            let _ = captured_classifier_state.len();
            RetryDecision::Stop
        })
        .build()
        .expect("safe policy");
    let captured_builder_state = CLASSIFIER_SECRET.to_owned();
    let builder = RetryPolicy::builder().classify_fn(move |_| {
        let _ = captured_builder_state.len();
        RetryDecision::Stop
    });
    let mut headers = HeaderMap::new();
    headers.insert("x-public", HeaderValue::from_static(HEADER_SECRET));
    let attempt = RetryAttempt::new(
        "retry_api.safe",
        &Method::GET,
        Some(HOST_SECRET),
        0,
        INITIAL_ATTEMPT,
        0,
        RetryFailure::RejectedStatus {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: SanitizedHeaders::new(&headers),
        },
    );
    let error = RetryPolicy::builder()
        .max_extra_load(f64::NAN)
        .classify_fn(|_| RetryDecision::Stop)
        .build()
        .expect_err("invalid policy");
    let rendered = format!(
        "{builder:?}\n{policy:?}\n{attempt:?}\n{:?}\n{}",
        attempt.failure(),
        error
    );
    for forbidden in [HOST_SECRET, HEADER_SECRET, CLASSIFIER_SECRET, "NaN"] {
        assert!(
            !rendered.contains(forbidden),
            "retry formatting leaked {forbidden}"
        );
    }
}
