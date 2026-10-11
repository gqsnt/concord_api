use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::rate_limit::encode_canonical;

const ID_SENTINEL: &str = "rate-id-private-sentinel-0f3a";
const KEY_SENTINEL: &str = "rate-key-private-sentinel-7b2c";

fn window(capacity: u32, seconds: u64) -> RateLimitWindow {
    RateLimitWindow::new(
        NonZeroU32::new(capacity).expect("positive test capacity"),
        Duration::from_secs(seconds),
    )
    .expect("valid test window")
}

fn rate_error<T>(result: Result<T, RateLimitError>, message: &str) -> RateLimitError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

fn tenant_key(value: &str) -> RateLimitKey {
    RateLimitKey::from_canonical_values([("tenant", value)]).expect("tenant key")
}

fn rule(id: impl AsRef<str>) -> RateLimitPolicy {
    RateLimitPolicy::new(
        RateLimitId::new(id).expect("valid policy ID"),
        tenant_key("north"),
        NonZeroU32::MIN,
        [window(1, 1)],
    )
    .expect("valid rule")
}

fn assert_redacted(error: &RateLimitError, sentinels: &[&str]) {
    let display = error.to_string();
    let debug = format!("{error:?}");
    for sentinel in sentinels {
        assert!(!display.contains(sentinel));
        assert!(!debug.contains(sentinel));
    }
}

struct PartLimitIterator {
    calls: Arc<AtomicUsize>,
}

impl Iterator for PartLimitIterator {
    type Item = (String, &'static str);

    fn next(&mut self) -> Option<Self::Item> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call <= MAX_RATE_LIMIT_KEY_PARTS,
            "key iterator consumed beyond the first excess"
        );
        Some((format!("part-{call}"), "value"))
    }
}

struct DuplicatePartIterator {
    calls: Arc<AtomicUsize>,
}

impl Iterator for DuplicatePartIterator {
    type Item = (&'static str, &'static str);

    fn next(&mut self) -> Option<Self::Item> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(call < 2, "duplicate-key iterator tail was consumed");
        Some(("duplicate", KEY_SENTINEL))
    }
}

struct WindowLimitIterator {
    calls: Arc<AtomicUsize>,
}

impl Iterator for WindowLimitIterator {
    type Item = RateLimitWindow;

    fn next(&mut self) -> Option<Self::Item> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call <= MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
            "window iterator consumed beyond the first excess"
        );
        Some(window(100, u64::try_from(call + 1).expect("test index")))
    }
}

struct RuleLimitIterator {
    calls: Arc<AtomicUsize>,
}

impl Iterator for RuleLimitIterator {
    type Item = RateLimitPolicy;

    fn next(&mut self) -> Option<Self::Item> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call <= MAX_RATE_LIMIT_POLICIES_PER_PLAN,
            "plan iterator consumed beyond the first excess"
        );
        Some(rule(format!("rule-{call}")))
    }
}

#[test]
fn constructors_and_accessors_preserve_order_labels_encodings_and_checked_bytes() {
    let key = RateLimitKey::from_encoded_parts([
        (
            "host",
            encode_canonical("api.example.test", 1_024).expect("host"),
        ),
        (
            "client.tenant",
            encode_canonical(&7_u32, 1_024).expect("id"),
        ),
    ])
    .expect("valid key");
    assert_eq!(key.parts()[0].name(), "host");
    assert_eq!(key.parts()[0].kind(), RateDimensionKind::Custom);
    assert_eq!(key.parts()[0].encoding_id(), "concord.utf8/1");
    assert_eq!(
        key.parts()[0].value(),
        encode_canonical("api.example.test", 1_024)
            .expect("host")
            .as_bytes()
    );
    assert_eq!(key.parts()[1].name(), "client.tenant");
    assert_eq!(key.parts()[1].encoding_id(), "concord.u32/1");
    let host_value = 1 + 4 + 8 + 16;
    let id_value = 1 + 3 + 8 + 4;
    assert_eq!(
        key.total_bytes(),
        (12 + 4 + host_value) + (12 + 13 + id_value)
    );
    assert!(!key.uses_origin());

    let rule = RateLimitPolicy::new(
        RateLimitId::new("tenant").expect("ID"),
        key,
        NonZeroU32::new(2).expect("cost"),
        [window(1_000, 600), window(100, 60)],
    )
    .expect("rule");
    assert_eq!(rule.id().as_str(), "tenant");
    assert_eq!(rule.cost().get(), 2);
    assert_eq!(rule.windows().len(), 2);
    assert_eq!(
        rule.windows()[0].duration(),
        Duration::from_secs(600),
        "source order is retained"
    );
    let schema = rule.schema();
    assert_eq!(schema.id().as_str(), "tenant");
    assert_eq!(
        schema.windows()[0].duration(),
        Duration::from_secs(60),
        "schema windows are canonical"
    );
    assert_eq!(schema.dimensions()[1].encoding_id(), "concord.u32/1");

    let plan = RateLimitPlan::try_from_policies([rule]).expect("valid plan");
    assert!(!plan.is_empty());
    assert_eq!(plan.policies()[0].id().as_str(), "tenant");
}

#[test]
fn rate_limit_id_max_and_first_excess() {
    let boundary = "i".repeat(MAX_RATE_LIMIT_ID_BYTES);
    let id = RateLimitId::new(&boundary).expect("ID byte boundary");
    assert_eq!(id.as_str(), boundary);

    let excessive = format!(
        "{}{ID_SENTINEL}",
        "i".repeat(MAX_RATE_LIMIT_ID_BYTES + 1 - ID_SENTINEL.len())
    );
    let error = rate_error(
        RateLimitId::new(&excessive),
        "first excess ID byte unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    assert_redacted(&error, &[ID_SENTINEL]);

    let error = rate_error(RateLimitId::new(""), "empty ID unexpectedly succeeded");
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
}

/// V3-O07: generated declarations borrow their literal ID. The static and
/// shared forms are the same identity (equality, ordering, hashing and
/// `Debug` use the text) and obey the same bounds.
#[test]
fn static_and_shared_rate_limit_ids_are_one_identity() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let hash = |id: &RateLimitId| {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        hasher.finish()
    };
    let shared = RateLimitId::new("catalog.reads").expect("shared ID");
    let borrowed = RateLimitId::from_static("catalog.reads").expect("static ID");
    assert!(borrowed.is_static());
    assert!(!shared.is_static());
    assert_eq!(shared, borrowed);
    assert_eq!(hash(&shared), hash(&borrowed));
    assert_eq!(shared.cmp(&borrowed), std::cmp::Ordering::Equal);
    assert_eq!(format!("{shared:?}"), format!("{borrowed:?}"));
    assert_eq!(
        format!("{borrowed:?}"),
        "RateLimitId { value: \"catalog.reads\" }"
    );
    assert!(RateLimitId::from_static("a").expect("ID") < RateLimitId::new("b").expect("ID"));

    let error = rate_error(RateLimitId::from_static(""), "empty static ID");
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    let excessive: &'static str = Box::leak("i".repeat(MAX_RATE_LIMIT_ID_BYTES + 1).into());
    let error = rate_error(RateLimitId::from_static(excessive), "overlong static ID");
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);

    // A generated declaration keeps the static form through resolution, and
    // resolves to exactly the policy the copying constructor produces.
    let generated = crate::rate_limit::RateLimitDeclaration::new_static("catalog.reads")
        .window(10, Duration::from_secs(1))
        .origin();
    let runtime = crate::rate_limit::RateLimitDeclaration::new("catalog.reads")
        .window(10, Duration::from_secs(1))
        .origin();
    assert!(generated.id().is_some_and(RateLimitId::is_static));
    let endpoint = crate::endpoint::EndpointId::new("catalog.item").expect("endpoint ID");
    let generated = generated
        .resolve_for("https://api.example.test/v1", &endpoint)
        .expect("generated policy");
    let runtime = runtime
        .resolve_for("https://api.example.test/v1", &endpoint)
        .expect("runtime policy");
    assert!(generated.id().is_static());
    assert_eq!(generated.id(), runtime.id());
    assert_eq!(generated.schema(), runtime.schema());
    assert!(generated.is_exact_duplicate_of(&runtime));
}

#[test]
fn key_labels_are_explicit_bounded_unique_and_never_reserved() {
    let label = "n".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
    RateLimitKey::from_canonical_values([(label.as_str(), "v")]).expect("label boundary");

    let excessive_label = format!(
        "{}{KEY_SENTINEL}",
        "n".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES + 1 - KEY_SENTINEL.len())
    );
    for invalid in [excessive_label.as_str(), "", "origin", "endpoint"] {
        let error = rate_error(
            RateLimitKey::from_canonical_values([(invalid, KEY_SENTINEL)]),
            "invalid custom label unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&error, &[KEY_SENTINEL]);
    }
    let error = rate_error(
        RateLimitKey::from_canonical_values(std::iter::empty::<(&str, &str)>()),
        "a key without dimensions unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    // Label case and spelling are state-affecting.
    let a = RateLimitKey::from_canonical_values([("tenant", "x")]).expect("a");
    let b = RateLimitKey::from_canonical_values([("client.tenant", "x")]).expect("b");
    assert_eq!(a.parts()[0].value(), b.parts()[0].value());
    assert_ne!(a.parts()[0].name(), b.parts()[0].name());
}

#[test]
fn aggregate_key_bytes_stop_at_the_first_excess() {
    let boundary_value = "v".repeat(MAX_RATE_LIMIT_KEY_BYTES - 12 - 1 - 13);
    let boundary =
        RateLimitKey::from_canonical_values([("a", boundary_value.as_str())]).expect("boundary");
    assert_eq!(boundary.total_bytes(), MAX_RATE_LIMIT_KEY_BYTES);

    let excessive = format!("{boundary_value}{KEY_SENTINEL}");
    let error = rate_error(
        RateLimitKey::from_canonical_values([("a", excessive.as_str())]),
        "first excess aggregate key byte unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert_redacted(&error, &[KEY_SENTINEL]);

    let encoded = encode_canonical(boundary_value.as_str(), MAX_RATE_LIMIT_KEY_BYTES)
        .expect("encoded boundary");
    let error = rate_error(
        RateLimitKey::from_encoded_parts([("ab", encoded)]),
        "pre-encoded part above the remaining budget unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
}

#[test]
fn rate_limit_key_iterator_stops_at_first_excess() {
    let boundary = RateLimitKey::from_canonical_values(
        (0..MAX_RATE_LIMIT_KEY_PARTS).map(|index| (format!("part-{index}"), "value")),
    )
    .expect("key-part count boundary");
    assert_eq!(boundary.parts().len(), MAX_RATE_LIMIT_KEY_PARTS);

    let calls = Arc::new(AtomicUsize::new(0));
    let error = rate_error(
        RateLimitKey::from_canonical_values(PartLimitIterator {
            calls: Arc::clone(&calls),
        }),
        "first excess key part unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert_eq!(calls.load(Ordering::SeqCst), MAX_RATE_LIMIT_KEY_PARTS + 1);
}

#[test]
fn rate_limit_key_duplicate_stops_without_tail_consumption() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = rate_error(
        RateLimitKey::from_canonical_values(DuplicatePartIterator {
            calls: Arc::clone(&calls),
        }),
        "duplicate key-part name unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_redacted(&error, &[KEY_SENTINEL, "duplicate"]);
}

#[test]
fn rate_limit_rule_window_iterator_stops_at_first_excess() {
    let boundary = RateLimitPolicy::new(
        RateLimitId::new("window-boundary").expect("ID"),
        tenant_key("north"),
        NonZeroU32::MIN,
        (0..MAX_RATE_LIMIT_WINDOWS_PER_POLICY)
            .map(|index| window(100, u64::try_from(index + 1).expect("test index"))),
    )
    .expect("window count boundary");
    assert_eq!(boundary.windows().len(), MAX_RATE_LIMIT_WINDOWS_PER_POLICY);

    let calls = Arc::new(AtomicUsize::new(0));
    let error = rate_error(
        RateLimitPolicy::new(
            RateLimitId::new(ID_SENTINEL).expect("ID"),
            tenant_key("north"),
            NonZeroU32::MIN,
            WindowLimitIterator {
                calls: Arc::clone(&calls),
            },
        ),
        "first excess window unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        MAX_RATE_LIMIT_WINDOWS_PER_POLICY + 1
    );
    assert_redacted(&error, &[ID_SENTINEL]);
}

#[test]
fn policy_windows_reject_repeated_durations_unadmittable_cost_and_empty_sets() {
    let policy = |cost: u32, windows: Vec<RateLimitWindow>| {
        RateLimitPolicy::new(
            RateLimitId::new("windows").expect("ID"),
            tenant_key("north"),
            NonZeroU32::new(cost).expect("cost"),
            windows,
        )
    };
    for (cost, windows) in [
        (1, vec![window(10, 1), window(20, 1)]),
        (1, vec![window(10, 1), window(10, 1)]),
        (3, vec![window(10, 1), window(2, 60)]),
        (1, Vec::new()),
    ] {
        assert_eq!(
            rate_error(
                policy(cost, windows),
                "invalid windows unexpectedly succeeded"
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
    }
    policy(2, vec![window(2, 1), window(10, 60)]).expect("cost equal to the smallest capacity");
    assert_eq!(
        rate_error(
            RateLimitWindow::new(NonZeroU32::MIN, Duration::ZERO),
            "zero duration unexpectedly succeeded",
        )
        .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
}

#[test]
fn rate_limit_plan_rule_iterator_stops_at_first_excess() {
    let boundary = RateLimitPlan::try_from_policies(
        (0..MAX_RATE_LIMIT_POLICIES_PER_PLAN).map(|index| rule(format!("rule-{index}"))),
    )
    .expect("rule count boundary");
    assert_eq!(boundary.policies().len(), MAX_RATE_LIMIT_POLICIES_PER_PLAN);

    let calls = Arc::new(AtomicUsize::new(0));
    let error = rate_error(
        RateLimitPlan::try_from_policies(RuleLimitIterator {
            calls: Arc::clone(&calls),
        }),
        "first excess plan rule unexpectedly succeeded",
    );
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        MAX_RATE_LIMIT_POLICIES_PER_PLAN + 1
    );
}

#[test]
fn rate_limit_plan_dedups_exact_duplicates_and_rejects_conflicting_ids() {
    let exact = RateLimitPlan::try_from_policies([rule(ID_SENTINEL), rule(ID_SENTINEL)])
        .expect("exact duplicate deduplicates");
    assert_eq!(exact.policies().len(), 1);

    let variants = [
        RateLimitPolicy::new(
            RateLimitId::new(ID_SENTINEL).expect("ID"),
            tenant_key(KEY_SENTINEL),
            NonZeroU32::MIN,
            [window(1, 1)],
        ),
        RateLimitPolicy::new(
            RateLimitId::new(ID_SENTINEL).expect("ID"),
            RateLimitKey::from_canonical_values([("client.tenant", "north")]).expect("key"),
            NonZeroU32::MIN,
            [window(1, 1)],
        ),
        RateLimitPolicy::new(
            RateLimitId::new(ID_SENTINEL).expect("ID"),
            tenant_key("north"),
            NonZeroU32::MIN,
            [window(2, 1)],
        ),
        RateLimitPolicy::new(
            RateLimitId::new(ID_SENTINEL).expect("ID"),
            tenant_key("north"),
            NonZeroU32::new(1).expect("cost"),
            [window(1, 1), window(5, 10)],
        ),
    ];
    for variant in variants {
        let error = rate_error(
            RateLimitPlan::try_from_policies([rule(ID_SENTINEL), variant.expect("variant")]),
            "conflicting repeated policy ID unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert_redacted(&error, &[ID_SENTINEL, KEY_SENTINEL]);
    }
}

#[test]
fn plan_key_limit_is_key_safe_and_zero_admits_no_policy() {
    let keyed = RateLimitPolicy::new(
        RateLimitId::new("keyed").expect("ID"),
        tenant_key(KEY_SENTINEL),
        NonZeroU32::MIN,
        [window(1, 1)],
    )
    .expect("keyed rule");
    let total = keyed.key().total_bytes();
    let plan = RateLimitPlan::try_from_policies([keyed]).expect("keyed plan");
    plan.validate_key_limit(total).expect("exact key limit");
    for limit in [0, total - 1] {
        let error = plan
            .validate_key_limit(limit)
            .expect_err("key exceeds the preparation limit");
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&error, &[KEY_SENTINEL]);
    }
    RateLimitPlan::empty()
        .validate_key_limit(0)
        .expect("an empty plan needs no key bytes");

    let hard_error = RateLimitPlan::empty()
        .validate_key_limit(MAX_RATE_LIMIT_KEY_BYTES + 1)
        .expect_err("operational cap above hard limit");
    assert_eq!(hard_error.kind(), RateLimitErrorKind::InvalidConfiguration);
}
