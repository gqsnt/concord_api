//! V3-R06 shared-adapter conformance tests.

use concord_core::contracts::{AttemptInfo, EndpointId, Method};
use concord_core::rate_limit::{
    CanonicalKeyWriter, RateKeyEncoder, RateKeyPart, RateLimitDeclaration, RateLimitPolicy,
};

use super::*;

const DESTINATION: &str = "https://EU.api.catalog.example.test/root/v2/items";

struct UppercaseTenant;

impl RateKeyEncoder<str> for UppercaseTenant {
    const ENCODING_ID: &'static str = "com.example.tenant-upper/1";

    fn write_canonical(value: &str, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        value.to_ascii_uppercase().write_canonical(out)
    }
}

fn endpoint() -> EndpointId {
    EndpointId::new("catalog.items.list").expect("endpoint ID")
}

fn tenant_reads() -> RateLimitDeclaration {
    RateLimitDeclaration::new("catalog.tenant.read")
        .window(3, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .origin()
        .part("tenant", "tenant-a")
}

fn resolve(declaration: RateLimitDeclaration) -> RateLimitPolicy {
    declaration
        .resolve_for(DESTINATION, &endpoint())
        .expect("resolved policy")
}

fn plan_of(policies: Vec<RateLimitPolicy>) -> RateLimitPlan {
    RateLimitPlan::try_from_policies(policies).expect("plan")
}

async fn try_acquire(
    limiter: &InMemorySharedLimiter,
    plan: &RateLimitPlan,
) -> Result<RateLimitPermit, RateLimitError> {
    limiter
        .acquire(RateLimitRequest {
            cooldown_origin: None,
            endpoint: "catalog.items.list",
            method: &Method::GET,
            logical_origin: plan
                .uses_origin()
                .then_some("https://eu.api.catalog.example.test:443"),
            page_index: 0,
            attempt: AttemptInfo {
                index: 0,
                policy_retry_index: 0,
                authentication_recovery: false,
            },
            plan,
            capability: limiter.capability(),
        })
        .await
}

fn kind<T>(result: Result<T, RateLimitError>) -> RateLimitErrorKind {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error.kind(),
    }
}

struct Snapshot {
    registrations: usize,
    buckets: usize,
    admissions: u64,
    schema: Option<(u32, RatePolicySchema)>,
}

fn snapshot(store: &InMemorySharedStore) -> Snapshot {
    Snapshot {
        registrations: store.registration_count().expect("registrations"),
        buckets: store.bucket_count().expect("buckets"),
        admissions: store.admission_count().expect("admissions"),
        schema: store
            .registered_schema("catalog.tenant.read")
            .expect("registered schema"),
    }
}

fn assert_unchanged(before: &Snapshot, store: &InMemorySharedStore) {
    let after = snapshot(store);
    assert_eq!(after.registrations, before.registrations);
    assert_eq!(after.buckets, before.buckets);
    assert_eq!(after.admissions, before.admissions);
    assert_eq!(after.schema, before.schema);
}

#[tokio::test(start_paused = true)]
async fn adapters_report_shared_persistent_capability_and_validate_configuration() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    let capability = limiter.capability();
    assert_eq!(capability.enforcement(), Enforcement::SharedPersistent);
    assert_eq!(capability.namespace(), "test.shared-memory");
    assert_eq!(
        capability.algorithm_version(),
        FIXED_WINDOW_ALGORITHM_VERSION
    );
    capability.validate().expect("valid capability");
    assert_eq!(
        store
            .limiter_with_algorithm(9)
            .capability()
            .algorithm_version(),
        9
    );
    for namespace in ["", "has space"] {
        assert_eq!(
            kind(InMemorySharedStore::new(namespace)),
            RateLimitErrorKind::InvalidConfiguration
        );
    }
    assert_eq!(
        kind(InMemorySharedStore::with_capacities("test.shared", 0, 1)),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(
        kind(InMemorySharedStore::with_capacities("test.shared", 1, 0)),
        RateLimitErrorKind::InvalidConfiguration
    );
    assert_eq!(store.registration_count().expect("count"), 0);
    assert_eq!(DEFAULT_RATE_SCHEMA_REGISTRATIONS, 4_096);
}

#[tokio::test(start_paused = true)]
async fn compatible_instances_register_once_and_share_state_across_processes() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let first_process = store.limiter();
    let second_process = store.limiter();
    let plan = plan_of(vec![resolve(tenant_reads())]);
    for _ in 0..2 {
        try_acquire(&first_process, &plan).await.expect("admitted");
    }
    let (algorithm, schema) = store
        .registered_schema("catalog.tenant.read")
        .expect("lookup")
        .expect("registered on first admission");
    assert_eq!(algorithm, FIXED_WINDOW_ALGORITHM_VERSION);
    assert_eq!(&schema, plan.policies()[0].schema());
    try_acquire(&second_process, &plan)
        .await
        .expect("the second process shares the registered contract");
    assert_eq!(
        kind(try_acquire(&second_process, &plan).await),
        RateLimitErrorKind::Acquire,
        "both processes debit one shared bucket"
    );
    assert_eq!(store.registration_count().expect("count"), 1);
    assert_eq!(store.admission_count().expect("admissions"), 3);

    // Cost is not schema identity: a different per-call cost shares the
    // registration and the bucket.
    let costly = plan_of(vec![resolve(
        RateLimitDeclaration::new("catalog.tenant.read")
            .cost(2)
            .window(3, Duration::from_secs(60))
            .window(50, Duration::from_secs(3_600))
            .origin()
            .part("tenant", "tenant-b"),
    )]);
    try_acquire(&first_process, &costly)
        .await
        .expect("same schema with a different cost and value");
    assert_eq!(store.registration_count().expect("count"), 1);

    tokio::time::advance(Duration::from_secs(60)).await;
    try_acquire(&second_process, &plan)
        .await
        .expect("the fixed window resets after its duration");
}

#[tokio::test(start_paused = true)]
async fn every_schema_change_under_one_id_fails_closed_before_state_use() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    let baseline = plan_of(vec![resolve(tenant_reads())]);
    try_acquire(&limiter, &baseline)
        .await
        .expect("baseline registration");

    let changed_encoding = RateLimitDeclaration::new("catalog.tenant.read")
        .window(3, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .origin()
        .part_with::<UppercaseTenant, str>("tenant", "tenant-a");
    let changed_label = RateLimitDeclaration::new("catalog.tenant.read")
        .window(3, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .origin()
        .part("client.tenant", "tenant-a");
    let changed_order = RateLimitDeclaration::new("catalog.tenant.read")
        .window(3, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .part("tenant", "tenant-a")
        .origin();
    let changed_window = RateLimitDeclaration::new("catalog.tenant.read")
        .window(4, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .origin()
        .part("tenant", "tenant-a");
    let added_window = tenant_reads().window(1_000, Duration::from_secs(86_400));
    let added_dimension = tenant_reads().endpoint();
    let changed_type = RateLimitDeclaration::new("catalog.tenant.read")
        .window(3, Duration::from_secs(60))
        .window(50, Duration::from_secs(3_600))
        .origin()
        .part("tenant", &7_u64);
    for (name, declaration) in [
        ("encoding ID", changed_encoding),
        ("label", changed_label),
        ("dimension order", changed_order),
        ("window capacity", changed_window),
        ("window set", added_window),
        ("dimension set", added_dimension),
        ("value type", changed_type),
    ] {
        let before = snapshot(&store);
        let changed = plan_of(vec![resolve(declaration)]);
        assert_ne!(
            changed.policies()[0].schema(),
            baseline.policies()[0].schema()
        );
        assert_eq!(
            kind(try_acquire(&limiter, &changed).await),
            RateLimitErrorKind::SchemaConflict,
            "{name} change must fail closed"
        );
        assert_unchanged(&before, &store);
    }

    let before = snapshot(&store);
    let other_algorithm = store.limiter_with_algorithm(FIXED_WINDOW_ALGORITHM_VERSION + 1);
    assert_eq!(
        kind(try_acquire(&other_algorithm, &baseline).await),
        RateLimitErrorKind::SchemaConflict,
        "a different algorithm interpretation must fail closed"
    );
    assert_unchanged(&before, &store);

    try_acquire(&limiter, &baseline)
        .await
        .expect("the unchanged contract keeps working");
}

#[tokio::test(start_paused = true)]
async fn a_conflict_anywhere_in_the_plan_registers_and_debits_nothing() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    try_acquire(&limiter, &plan_of(vec![resolve(tenant_reads())]))
        .await
        .expect("baseline");
    let new_policy = || {
        resolve(
            RateLimitDeclaration::new("catalog.item.write")
                .cost(5)
                .window(30, Duration::from_secs(60))
                .origin()
                .endpoint()
                .part("client.tenant", "tenant-a"),
        )
    };
    let conflicting = resolve(tenant_reads().window(9, Duration::from_secs(1)));
    let before = snapshot(&store);
    assert_eq!(
        kind(try_acquire(&limiter, &plan_of(vec![new_policy(), conflicting])).await),
        RateLimitErrorKind::SchemaConflict
    );
    assert_unchanged(&before, &store);
    assert!(
        store
            .registered_schema("catalog.item.write")
            .expect("lookup")
            .is_none(),
        "the compatible prefix was not registered"
    );
    try_acquire(&limiter, &plan_of(vec![new_policy()]))
        .await
        .expect("a later compatible admission registers normally");
    for _ in 0..5 {
        try_acquire(&limiter, &plan_of(vec![new_policy()]))
            .await
            .expect("the failed plan did not debit the write bucket");
    }
    assert_eq!(
        kind(try_acquire(&limiter, &plan_of(vec![new_policy()])).await),
        RateLimitErrorKind::Acquire
    );
}

#[tokio::test(start_paused = true)]
async fn quota_exhaustion_is_atomic_across_policies() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    let scarce = || {
        resolve(
            RateLimitDeclaration::new("scarce")
                .window(1, Duration::from_secs(60))
                .origin(),
        )
    };
    let ample = || {
        resolve(
            RateLimitDeclaration::new("ample")
                .window(2, Duration::from_secs(60))
                .origin(),
        )
    };
    try_acquire(&limiter, &plan_of(vec![scarce()]))
        .await
        .expect("scarce");
    let before = store.admission_count().expect("admissions");
    for _ in 0..3 {
        assert_eq!(
            kind(try_acquire(&limiter, &plan_of(vec![ample(), scarce()])).await),
            RateLimitErrorKind::Acquire
        );
    }
    assert_eq!(store.admission_count().expect("admissions"), before);
    assert!(
        store.registered_schema("ample").expect("lookup").is_none(),
        "a denied plan registers nothing"
    );
    for _ in 0..2 {
        try_acquire(&limiter, &plan_of(vec![ample()]))
            .await
            .expect("the denied plans debited nothing from ample");
    }
}

#[tokio::test(start_paused = true)]
async fn registration_capacity_is_finite_and_never_evicts_active_contracts() {
    let store = InMemorySharedStore::with_capacities("test.shared-memory", 2, 64).expect("store");
    let limiter = store.limiter();
    let policy = |id: &str| {
        plan_of(vec![resolve(
            RateLimitDeclaration::new(id)
                .window(10, Duration::from_secs(60))
                .origin(),
        )])
    };
    try_acquire(&limiter, &policy("first"))
        .await
        .expect("first");
    try_acquire(&limiter, &policy("second"))
        .await
        .expect("second");
    assert_eq!(
        kind(try_acquire(&limiter, &policy("third")).await),
        RateLimitErrorKind::StateCapacity
    );
    assert_eq!(store.registration_count().expect("count"), 2);
    try_acquire(&limiter, &policy("first"))
        .await
        .expect("active contracts remain registered");
    try_acquire(&limiter, &policy("second"))
        .await
        .expect("active contracts remain registered");

    assert!(store.retire_schema("first").expect("retire"));
    assert!(!store.retire_schema("never-registered").expect("retire"));
    try_acquire(&limiter, &policy("third"))
        .await
        .expect("an explicit retirement frees capacity");
    assert!(
        store.registered_schema("first").expect("lookup").is_none(),
        "retirement is a reviewed reset"
    );
}

#[tokio::test(start_paused = true)]
async fn quota_bucket_capacity_prunes_only_elapsed_windows() {
    let store = InMemorySharedStore::with_capacities("test.shared-memory", 16, 1).expect("store");
    let limiter = store.limiter();
    let policy = |id: &str| {
        plan_of(vec![resolve(
            RateLimitDeclaration::new(id)
                .window(10, Duration::from_secs(60))
                .origin(),
        )])
    };
    try_acquire(&limiter, &policy("first"))
        .await
        .expect("first");
    assert_eq!(
        kind(try_acquire(&limiter, &policy("second")).await),
        RateLimitErrorKind::StateCapacity,
        "an active window is never evicted"
    );
    tokio::time::advance(Duration::from_secs(60)).await;
    try_acquire(&limiter, &policy("second"))
        .await
        .expect("an elapsed window is prunable");
    assert_eq!(store.bucket_count().expect("buckets"), 1);
}

#[tokio::test(start_paused = true)]
async fn legacy_state_is_not_handshake_compatible_without_an_explicit_lock() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    let plan = plan_of(vec![resolve(tenant_reads())]);
    store
        .mark_legacy_state("catalog.tenant.read")
        .expect("legacy peer state");
    let before = snapshot(&store);
    assert_eq!(
        kind(try_acquire(&limiter, &plan).await),
        RateLimitErrorKind::SchemaConflict,
        "an unregistered v2 peer's state is not assumed compatible"
    );
    assert_unchanged(&before, &store);

    let wrong = resolve(tenant_reads().window(1, Duration::from_secs(1)));
    store
        .install_schema_lock(
            FIXED_WINDOW_ALGORITHM_VERSION,
            plan.policies()[0].schema().clone(),
        )
        .expect("operator schema lock");
    store
        .install_schema_lock(
            FIXED_WINDOW_ALGORITHM_VERSION,
            plan.policies()[0].schema().clone(),
        )
        .expect("an identical lock is idempotent");
    assert_eq!(
        kind(store.install_schema_lock(FIXED_WINDOW_ALGORITHM_VERSION, wrong.schema().clone())),
        RateLimitErrorKind::SchemaConflict
    );
    assert_eq!(
        kind(store.mark_legacy_state("catalog.tenant.read")),
        RateLimitErrorKind::SchemaConflict
    );
    try_acquire(&limiter, &plan)
        .await
        .expect("admitted under the explicit lock");
}

#[tokio::test(start_paused = true)]
async fn mismatched_core_snapshot_and_unpolled_acquisition_change_nothing() {
    let store = InMemorySharedStore::new("test.shared-memory").expect("store");
    let limiter = store.limiter();
    let plan = plan_of(vec![resolve(tenant_reads())]);
    let request = |capability| RateLimitRequest {
        cooldown_origin: None,
        endpoint: "catalog.items.list",
        method: &Method::GET,
        logical_origin: Some("https://eu.api.catalog.example.test:443"),
        page_index: 0,
        attempt: AttemptInfo {
            index: 0,
            policy_retry_index: 0,
            authentication_recovery: false,
        },
        plan: &plan,
        capability,
    };
    let foreign = LimiterCapability::new(Enforcement::Local, "test.other", 1);
    assert_eq!(
        kind(limiter.acquire(request(foreign)).await),
        RateLimitErrorKind::InvalidConfiguration
    );
    drop(limiter.acquire(request(limiter.capability())));
    assert_eq!(store.registration_count().expect("count"), 0);
    assert_eq!(store.admission_count().expect("admissions"), 0);
    assert_eq!(store.bucket_count().expect("buckets"), 0);

    let oversized = plan_of(vec![resolve(
        RateLimitDeclaration::new("oversized")
            .window(1, Duration::from_secs(1))
            .part("tenant", &"x".repeat(DEFAULT_RATE_LIMIT_KEY_BYTES)),
    )]);
    assert_eq!(
        kind(try_acquire(&limiter, &oversized).await),
        RateLimitErrorKind::InvalidKey
    );
}

#[test]
fn legacy_and_registered_identities_share_capacity_and_conversion_reuses_slot() {
    let store =
        InMemorySharedStore::with_capacities("review.capacity", 1, 1).expect("bounded store");
    store.mark_legacy_state("review.a").expect("first marker");
    store
        .mark_legacy_state("review.a")
        .expect("duplicate at capacity");
    assert_eq!(
        kind(store.mark_legacy_state("review.b")),
        RateLimitErrorKind::StateCapacity
    );
    assert_eq!(store.lock().expect("state").legacy.len(), 1);
    let a = resolve(
        RateLimitDeclaration::new("review.a")
            .origin()
            .window(1, Duration::from_secs(1)),
    );
    let b = resolve(
        RateLimitDeclaration::new("review.b")
            .origin()
            .window(1, Duration::from_secs(1)),
    );
    assert_eq!(
        kind(store.install_schema_lock(1, b.schema().clone())),
        RateLimitErrorKind::StateCapacity
    );
    store
        .install_schema_lock(1, a.schema().clone())
        .expect("legacy conversion reuses slot");
    assert_eq!(store.registration_count().expect("schemas"), 1);
    assert_eq!(store.lock().expect("state").legacy.len(), 0);
    assert_eq!(
        kind(store.mark_legacy_state("review.b")),
        RateLimitErrorKind::StateCapacity
    );
    assert_eq!(
        kind(store.mark_legacy_state("review.a")),
        RateLimitErrorKind::SchemaConflict
    );
    assert_eq!(store.bucket_count().expect("buckets"), 0);
    assert_eq!(store.admission_count().expect("admissions"), 0);
    assert!(store.retire_schema("review.a").expect("explicit reset"));
    store
        .mark_legacy_state("review.b")
        .expect("retirement frees capacity");
}

#[tokio::test(start_paused = true)]
async fn unrelated_legacy_identity_blocks_new_admission_without_partial_effects() {
    let store =
        InMemorySharedStore::with_capacities("review.admission", 1, 2).expect("bounded store");
    store
        .mark_legacy_state("review.legacy")
        .expect("legacy slot");
    let policy = resolve(
        RateLimitDeclaration::new("review.new")
            .origin()
            .window(1, Duration::from_secs(1)),
    );
    let plan = plan_of(vec![policy]);
    assert_eq!(
        kind(try_acquire(&store.limiter(), &plan).await),
        RateLimitErrorKind::StateCapacity
    );
    assert_eq!(store.registration_count().expect("schemas"), 0);
    assert_eq!(store.bucket_count().expect("buckets"), 0);
    assert_eq!(store.admission_count().expect("admissions"), 0);
    assert_eq!(store.lock().expect("state").legacy.len(), 1);
    store
        .retire_schema("review.legacy")
        .expect("explicit retirement");
    try_acquire(&store.limiter(), &plan)
        .await
        .expect("same plan after freeing slot");
    assert_eq!(store.registration_count().expect("schemas"), 1);
    assert_eq!(store.bucket_count().expect("buckets"), 1);
    assert_eq!(store.admission_count().expect("admissions"), 1);
}
