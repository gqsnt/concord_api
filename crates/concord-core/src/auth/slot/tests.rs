use std::sync::atomic::{AtomicUsize, Ordering};

use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;

use crate::auth::{ApiKey, BasicCredential, MAX_CREDENTIAL_SECRET_BYTES, SecretCredential};
use crate::hooks::BoxFuture;

use super::*;

#[derive(Clone)]
struct TestMaterial {
    id: usize,
    expires_at: Option<Instant>,
}

impl CredentialMaterial for TestMaterial {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }
}

struct SequenceProvider {
    calls: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
    first_gate: Option<Gate>,
    gated_call: usize,
    failures: usize,
    first_expiration: Option<Instant>,
    reasons: Arc<Mutex<Vec<CredentialReason>>>,
    refresh_inputs: Arc<Mutex<Vec<usize>>>,
}

impl SequenceProvider {
    fn new(first_gate: Option<Gate>) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::new(AtomicUsize::new(0)),
            first_gate,
            gated_call: 0,
            failures: 0,
            first_expiration: None,
            reasons: Arc::new(Mutex::new(Vec::new())),
            refresh_inputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn invoke(&self, context: CredentialRequest<'_>) -> Result<TestMaterial, AuthError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        lock(&self.reasons)?.push(context.reason);
        if call == self.gated_call {
            if let Some(gate) = &self.first_gate {
                gate.wait().await;
            }
        }
        if call < self.failures {
            return Err(AuthError::new(AuthErrorKind::Acquisition));
        }
        Ok(TestMaterial {
            id: call + 1,
            expires_at: (call == 0).then_some(self.first_expiration).flatten(),
        })
    }
}

impl CredentialProvider for SequenceProvider {
    type Material = TestMaterial;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(self.invoke(context))
    }

    fn refresh<'a>(
        &'a self,
        current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        let input = current.id;
        Box::pin(async move {
            lock(&self.refresh_inputs)?.push(input);
            self.invoke(context).await
        })
    }
}

/// A provider that always returns one fixed identity.
struct FixedProvider {
    id: usize,
}

impl CredentialProvider for FixedProvider {
    type Material = TestMaterial;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        let id = self.id;
        Box::pin(async move {
            Ok(TestMaterial {
                id,
                expires_at: None,
            })
        })
    }
}

struct TestCore;

impl TestCore {
    fn provider_http(
        &self,
        _deadline: Option<crate::operation::OperationDeadline>,
    ) -> ProviderHttp<'static> {
        ProviderHttp::unavailable_for_test()
    }
}

fn core(_executor: &DeterministicExecutor) -> TestCore {
    TestCore
}

fn auth_error<T>(result: Result<T, AuthError>, context: &str) -> AuthError {
    match result {
        Ok(_) => panic!("{context}: expected an authentication error"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn concurrent_missing_callers_share_one_success_cohort() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let gate = Gate::closed();
    let provider = SequenceProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::from_secs(60));
    let open = async {
        gate.wait_until_reached().await;
        gate.open();
    };
    let (first, second, ()) = tokio::join!(
        selector.preflight(core.provider_http(None)),
        selector.preflight(core.provider_http(None)),
        open,
    );
    let first = first.expect("leader success");
    let second = second.expect("waiter success");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.version, second.version);
    assert_eq!(first.value.id, second.value.id);
}

#[tokio::test]
async fn credential_versions_remain_monotonic_across_deterministic_mutations() {
    const SEED: u64 = 0x6372_6564_5f32_3032;
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let mut state = SEED;
    let mut previous = 0_u64;
    for case_index in 0..256_usize {
        state = state
            .wrapping_mul(2_862_933_555_777_941_757)
            .wrapping_add(3_037_000_493);
        if state & 1 == 0 && selector.is_set() {
            selector
                .clear()
                .unwrap_or_else(|_| panic!("seed={SEED:#x} clear case={case_index}"));
        }
        selector
            .replace(TestMaterial {
                id: case_index + 1,
                expires_at: None,
            })
            .unwrap_or_else(|_| panic!("seed={SEED:#x} set case={case_index}"));
        let prepared = selector
            .preflight(core.provider_http(None))
            .await
            .unwrap_or_else(|_| panic!("seed={SEED:#x} preflight case={case_index}"));
        assert!(
            prepared.version > previous,
            "seed={SEED:#x} case={case_index} previous={previous} current={}",
            prepared.version
        );
        previous = prepared.version;
    }
}

#[tokio::test]
async fn built_in_replacement_exact_limit_succeeds_and_failure_preserves_material_and_version() {
    const INITIAL: &str = "preserved-slot-api-key";
    const FIRST_EXCESS_SENTINEL: &str = "\u{7f}";

    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let selector = SlotSelector::static_value(
        ApiKey::try_new(INITIAL).expect("bounded initial API key"),
        Duration::ZERO,
    );
    let before = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial ready material");
    let exact = "e".repeat(MAX_CREDENTIAL_SECRET_BYTES);
    selector
        .replace_result(ApiKey::try_new(&exact))
        .expect("exact replacement limit");
    let replaced = selector
        .preflight(core.provider_http(None))
        .await
        .expect("exact replacement material");
    assert!(replaced.version > before.version);
    assert_eq!(replaced.value.expose_secret(), exact);

    let excessive = format!(
        "{}{FIRST_EXCESS_SENTINEL}",
        "x".repeat(MAX_CREDENTIAL_SECRET_BYTES)
    );
    let error = selector
        .replace_result(ApiKey::try_new(&excessive))
        .expect_err("first excess replacement");
    assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);
    assert!(!error.to_string().contains(FIRST_EXCESS_SENTINEL));
    assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));

    let after = selector
        .preflight(core.provider_http(None))
        .await
        .expect("old material remains ready");
    assert_eq!(after.version, replaced.version);
    assert_eq!(after.value.expose_secret(), exact);
    assert!(Arc::ptr_eq(&after.value, &replaced.value));
}

#[tokio::test]
async fn invalid_basic_username_replacement_preserves_material_and_version() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let selector = SlotSelector::static_value(
        BasicCredential::try_new("initial-user", "initial-password")
            .expect("initial Basic credential"),
        Duration::ZERO,
    );
    let before = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial Basic material");

    let error = selector
        .replace_result(BasicCredential::try_new("foreign:user", "replacement"))
        .expect_err("invalid Basic replacement");
    assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);

    let after = selector
        .preflight(core.provider_http(None))
        .await
        .expect("preserved Basic material");
    assert_eq!(after.version, before.version);
    assert_eq!(after.value.username(), "initial-user");
    assert_eq!(after.value.password(), "initial-password");
    assert!(Arc::ptr_eq(&after.value, &before.value));
}

#[tokio::test]
async fn failure_cohort_is_shared_and_later_no_cooldown_call_retries() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let gate = Gate::closed();
    let mut provider = SequenceProvider::new(Some(gate.clone()));
    provider.failures = 1;
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let open = async {
        gate.wait_until_reached().await;
        gate.open();
    };
    let (first, second, ()) = tokio::join!(
        selector.preflight(core.provider_http(None)),
        selector.preflight(core.provider_http(None)),
        open,
    );
    assert_eq!(
        auth_error(first, "leader").kind(),
        AuthErrorKind::Acquisition
    );
    assert_eq!(
        auth_error(second, "waiter").kind(),
        AuthErrorKind::Acquisition
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let later = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later independent retry");
    assert_eq!(later.value.id, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn manual_set_supersedes_and_cancels_blocked_acquisition() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let gate = Gate::closed();
    let provider = SequenceProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let mutate = {
        let selector = Arc::clone(&selector);
        async move {
            gate.wait_until_reached().await;
            selector
                .replace(TestMaterial {
                    id: 99,
                    expires_at: None,
                })
                .expect("manual replacement");
        }
    };
    let (prepared, ()) = tokio::join!(selector.preflight(core.provider_http(None)), mutate,);
    assert_eq!(prepared.expect("reevaluated set").value.id, 99);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(selector.is_set());
}

#[tokio::test]
async fn clear_supersedes_blocked_acquisition_and_stale_work_cannot_restore_it() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let gate = Gate::closed();
    let provider = SequenceProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let mutate = {
        let selector = Arc::clone(&selector);
        async move {
            gate.wait_until_reached().await;
            selector.clear().expect("clear blocked acquisition");
        }
    };
    let (prepared, ()) = tokio::join!(selector.preflight(core.provider_http(None)), mutate,);
    let prepared = prepared.expect("later acquisition progresses after clear");
    assert_eq!(prepared.value.id, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(selector.is_set());
}

#[tokio::test]
async fn complete_source_replacement_isolated_from_blocked_old_completion() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let gate = Gate::closed();
    let provider = SequenceProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let replace = {
        let selector = Arc::clone(&selector);
        async move {
            gate.wait_until_reached().await;
            selector
                .replace_provider(FixedProvider { id: 77 })
                .expect("complete provider replacement");
            gate.open();
        }
    };
    let (old, ()) = tokio::join!(selector.preflight(core.provider_http(None)), replace,);
    let old = old.expect("isolated old operation may complete");
    assert_eq!(old.value.id, 1);
    let current = selector
        .finalize(&old, core.provider_http(None))
        .await
        .expect("finalization follows replacement slot");
    assert_eq!(current.value.id, 77);
    assert!(!Arc::ptr_eq(&old.slot, &current.slot));
    assert!(
        current.version > old.version,
        "versions continue monotonically"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn waiter_cancellation_does_not_cancel_the_shared_leader() {
    let executor = DeterministicExecutor::new();
    let core = Arc::new(core(&executor));
    let first_gate = Gate::closed();
    let provider = SequenceProvider::new(Some(first_gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);

    let leader = {
        let selector = Arc::clone(&selector);
        let core = Arc::clone(&core);
        tokio::spawn(async move { selector.preflight(core.provider_http(None)).await })
    };
    first_gate.wait_until_reached().await;
    let waiter = {
        let selector = Arc::clone(&selector);
        let core = Arc::clone(&core);
        tokio::spawn(async move { selector.preflight(core.provider_http(None)).await })
    };
    tokio::task::yield_now().await;
    waiter.abort();
    assert!(match waiter.await {
        Ok(_) => false,
        Err(error) => error.is_cancelled(),
    });
    first_gate.open();
    let ready = leader.await.expect("leader task").expect("leader result");
    assert_eq!(ready.value.id, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let cached = selector
        .preflight(core.provider_http(None))
        .await
        .expect("waiter cancellation preserves ready state");
    assert_eq!(cached.value.id, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn leader_cancellation_leaves_progress_available() {
    let executor = DeterministicExecutor::new();
    let core = Arc::new(core(&executor));
    let first_gate = Gate::closed();
    let provider = SequenceProvider::new(Some(first_gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);

    let leader = {
        let selector = Arc::clone(&selector);
        let core = Arc::clone(&core);
        tokio::spawn(async move { selector.preflight(core.provider_http(None)).await })
    };
    first_gate.wait_until_reached().await;
    leader.abort();
    assert!(match leader.await {
        Ok(_) => false,
        Err(error) => error.is_cancelled(),
    });

    let later = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later caller progresses");
    assert_eq!(later.value.id, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn exact_recovery_joins_refresh_and_never_restores_challenged_value() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let refresh_gate = Gate::closed();
    let mut provider = SequenceProvider::new(Some(refresh_gate.clone()));
    provider.gated_call = 1;
    provider.first_expiration = Instant::now().checked_add(Duration::from_secs(30));
    let calls = Arc::clone(&provider.calls);
    let refreshes = Arc::clone(&provider.refreshes);
    let reasons = Arc::clone(&provider.reasons);
    let inputs = Arc::clone(&provider.refresh_inputs);
    let selector = SlotSelector::provider(provider, Duration::ZERO);

    let challenged = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial acquisition");
    let open = async {
        refresh_gate.wait_until_reached().await;
        refresh_gate.open();
    };
    let proactive = selector.preflight(core.provider_http(None));
    let recovery = selector.recover(&challenged, core.provider_http(None));
    let (proactive, recovery, ()) = tokio::join!(proactive, recovery, open);
    let proactive = proactive.expect("proactive refresh");
    let recovery = recovery
        .expect("recovery")
        .expect("refresh recovery material");
    assert_eq!(proactive.version, recovery.version);
    assert_ne!(recovery.value.id, challenged.value.id);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(&*lock(&inputs).expect("inputs"), &[challenged.value.id]);
    assert_eq!(
        &*lock(&reasons).expect("reasons"),
        &[CredentialReason::Missing, CredentialReason::ExpiringSoon]
    );
}

#[tokio::test]
async fn recovery_preserves_newer_valid_expiring_soon_material() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let provider = SequenceProvider::new(None);
    let calls = Arc::clone(&provider.calls);
    let refreshes = Arc::clone(&provider.refreshes);
    let selector = SlotSelector::provider(provider, Duration::ZERO);

    let challenged = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial acquisition");
    selector
        .replace(TestMaterial {
            id: 50,
            expires_at: Instant::now().checked_add(Duration::from_secs(30)),
        })
        .expect("newer valid material");

    let recovered = selector
        .recover(&challenged, core.provider_http(None))
        .await
        .expect("stale challenge recovery")
        .expect("newer material should be reusable");
    assert_eq!(recovered.value.id, 50);
    assert!(recovered.version > challenged.version);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn finalization_rechecks_clear_and_complete_source_replacement() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let endpoint = SlotSelector::endpoint(Duration::ZERO);
    endpoint
        .replace(TestMaterial {
            id: 1,
            expires_at: None,
        })
        .expect("endpoint replacement");
    let prepared = endpoint
        .preflight(core.provider_http(None))
        .await
        .expect("endpoint preflight");
    endpoint.clear().expect("endpoint clear");
    let error = auth_error(
        endpoint.finalize(&prepared, core.provider_http(None)).await,
        "clear wins",
    );
    assert_eq!(error.kind(), AuthErrorKind::Unavailable);

    let static_slot = SlotSelector::static_value(
        TestMaterial {
            id: 10,
            expires_at: None,
        },
        Duration::ZERO,
    );
    let old = static_slot
        .preflight(core.provider_http(None))
        .await
        .expect("old static");
    static_slot
        .replace(TestMaterial {
            id: 11,
            expires_at: None,
        })
        .expect("manual replacement");
    let current = static_slot
        .finalize(&old, core.provider_http(None))
        .await
        .expect("finalization selects the newer installed version");
    assert_eq!(current.value.id, 11);
    assert!(Arc::ptr_eq(&old.slot, &current.slot));
    assert!(current.version > old.version);

    let same_slot = SlotSelector::endpoint(Duration::ZERO);
    same_slot
        .replace(TestMaterial {
            id: 20,
            expires_at: None,
        })
        .expect("first same-slot value");
    let old = same_slot
        .preflight(core.provider_http(None))
        .await
        .expect("same-slot preflight");
    same_slot
        .replace(TestMaterial {
            id: 21,
            expires_at: None,
        })
        .expect("newer same-slot value");
    let newer = same_slot
        .finalize(&old, core.provider_http(None))
        .await
        .expect("newer same-slot finalization");
    assert_eq!(newer.value.id, 21);
    assert!(Arc::ptr_eq(&old.slot, &newer.slot));
    assert!(newer.version > old.version);
}

#[tokio::test]
async fn manual_replacement_during_awaited_finalization_wins_selection() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let refresh_gate = Gate::closed();
    let mut provider = SequenceProvider::new(Some(refresh_gate.clone()));
    provider.gated_call = 1;
    provider.first_expiration = Instant::now().checked_add(Duration::from_secs(30));
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let prepared = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial short-lived material");
    // A valid just-acquired short-lived token needs no second refresh (R7-01).
    // Install genuinely expired material to exercise the awaited refresh race.
    selector
        .replace(TestMaterial {
            id: 2,
            expires_at: Some(Instant::now() - Duration::from_secs(1)),
        })
        .expect("expired intervening installation requires refresh");

    let replace = {
        let selector = Arc::clone(&selector);
        async move {
            refresh_gate.wait_until_reached().await;
            selector
                .replace(TestMaterial {
                    id: 99,
                    expires_at: None,
                })
                .expect("manual replacement during finalization");
            refresh_gate.open();
        }
    };
    let (finalized, ()) = tokio::join!(
        selector.finalize(&prepared, core.provider_http(None)),
        replace,
    );
    let finalized = finalized.expect("finalization selects the manual replacement");
    assert_eq!(finalized.value.id, 99);
    assert!(finalized.version > prepared.version);
}

#[tokio::test]
async fn expired_and_expiring_values_select_exact_provider_reasons() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let mut provider = SequenceProvider::new(None);
    provider.first_expiration = Instant::now().checked_sub(Duration::from_secs(1));
    let reasons = Arc::clone(&provider.reasons);
    let refreshes = Arc::clone(&provider.refreshes);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let first = selector
        .preflight(core.provider_http(None))
        .await
        .expect("initial provider result");
    assert_eq!(first.value.id, 1);
    assert!(!selector.is_set());
    let second = selector
        .preflight(core.provider_http(None))
        .await
        .expect("expired refresh");
    assert_eq!(second.value.id, 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        &*lock(&reasons).expect("reasons"),
        &[CredentialReason::Missing, CredentialReason::Expired]
    );
}

#[tokio::test]
async fn finalization_rejects_expired_output_but_accepts_valid_short_lived_material() {
    struct NonFreshProvider {
        calls: Arc<AtomicUsize>,
        lifetime: Duration,
    }

    impl CredentialProvider for NonFreshProvider {
        type Material = TestMaterial;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let expiration = Instant::now().checked_add(self.lifetime);
            Box::pin(async move {
                Ok(TestMaterial {
                    id: 1,
                    expires_at: expiration,
                })
            })
        }
    }

    for (lifetime, expected_calls, succeeds) in [
        (Duration::ZERO, 2, false),
        (Duration::from_secs(30), 1, true),
    ] {
        let executor = DeterministicExecutor::new();
        let core = core(&executor);
        let calls = Arc::new(AtomicUsize::new(0));
        let selector = SlotSelector::provider(
            NonFreshProvider {
                calls: Arc::clone(&calls),
                lifetime,
            },
            Duration::ZERO,
        );
        let prepared = selector
            .preflight(core.provider_http(None))
            .await
            .expect("provider cohort result");
        let finalized = selector.finalize(&prepared, core.provider_http(None)).await;
        if succeeds {
            assert_eq!(finalized.expect("short-lived valid material").value.id, 1);
        } else {
            assert_eq!(
                auth_error(finalized, "expired final provider result").kind(),
                AuthErrorKind::Unavailable
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        assert_eq!(selector.is_set(), succeeds);
    }
}

#[tokio::test]
async fn provider_material_discarded_on_version_overflow_drops_outside_slot_lock() {
    struct LockAwareMaterial {
        slot: Arc<Mutex<Option<std::sync::Weak<CredentialSlot<LockAwareMaterial>>>>>,
        dropped_without_lock: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Clone for LockAwareMaterial {
        fn clone(&self) -> Self {
            Self {
                slot: Arc::clone(&self.slot),
                dropped_without_lock: Arc::clone(&self.dropped_without_lock),
            }
        }
    }

    impl CredentialMaterial for LockAwareMaterial {}

    impl Drop for LockAwareMaterial {
        fn drop(&mut self) {
            let slot = lock(&self.slot)
                .ok()
                .and_then(|slot| slot.as_ref().and_then(std::sync::Weak::upgrade));
            if slot.is_some_and(|slot| slot.state.try_lock().is_ok()) {
                self.dropped_without_lock.store(true, Ordering::SeqCst);
            }
        }
    }

    struct OverflowProvider {
        slot: Arc<Mutex<Option<std::sync::Weak<CredentialSlot<LockAwareMaterial>>>>>,
        dropped_without_lock: Arc<std::sync::atomic::AtomicBool>,
    }

    impl CredentialProvider for OverflowProvider {
        type Material = LockAwareMaterial;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            let material = LockAwareMaterial {
                slot: Arc::clone(&self.slot),
                dropped_without_lock: Arc::clone(&self.dropped_without_lock),
            };
            Box::pin(async move { Ok(material) })
        }
    }

    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let slot_probe = Arc::new(Mutex::new(None));
    let dropped_without_lock = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let selector = SlotSelector::provider(
        OverflowProvider {
            slot: Arc::clone(&slot_probe),
            dropped_without_lock: Arc::clone(&dropped_without_lock),
        },
        Duration::ZERO,
    );
    let slot = selector.selected().expect("selected slot");
    *lock(&slot_probe).expect("slot probe") = Some(Arc::downgrade(&slot));
    slot.versions.last.store(u64::MAX, Ordering::SeqCst);

    assert_eq!(
        auth_error(
            selector.preflight(core.provider_http(None)).await,
            "version overflow",
        )
        .kind(),
        AuthErrorKind::Internal
    );
    assert!(dropped_without_lock.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cooldown_is_capped_without_waiting_and_static_rejection_is_exact() {
    struct CooldownProvider(Duration);
    impl CredentialProvider for CooldownProvider {
        type Material = TestMaterial;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            let retry_after = self.0;
            Box::pin(async move {
                Err(AuthError::new(AuthErrorKind::Acquisition).with_retry_after(retry_after))
            })
        }
    }

    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let cap = Duration::from_secs(60);
    let selector = SlotSelector::provider(CooldownProvider(Duration::from_secs(3_600)), cap);
    auth_error(
        selector.preflight(core.provider_http(None)).await,
        "provider failure",
    );
    let remaining = {
        let slot = selector.selected().expect("selected slot");
        let state = lock(&slot.state).expect("slot state");
        let cooldown = state.cooldown.as_ref().expect("stored cooldown");
        cooldown
            .duration
            .saturating_sub(cooldown.started_at.elapsed())
    };
    assert!(remaining <= cap);
    assert!(remaining >= cap - Duration::from_secs(1));

    let unrepresentable = SlotSelector::provider(CooldownProvider(Duration::MAX), Duration::MAX);
    auth_error(
        unrepresentable.preflight(core.provider_http(None)).await,
        "unrepresentable retry instant",
    );
    {
        let slot = unrepresentable.selected().expect("selected slot");
        let state = lock(&slot.state).expect("slot state");
        let cooldown = state.cooldown.as_ref().expect("bounded cooldown state");
        assert_eq!(cooldown.duration, Duration::MAX);
    }

    let static_slot = SlotSelector::static_value(
        TestMaterial {
            id: 7,
            expires_at: None,
        },
        Duration::ZERO,
    );
    let applied = static_slot
        .preflight(core.provider_http(None))
        .await
        .expect("static ready");
    static_slot
        .reject_installed(&applied, false)
        .expect("exact rejection");
    assert!(!static_slot.is_set());
    let error = auth_error(
        static_slot.preflight(core.provider_http(None)).await,
        "rejection persists",
    );
    assert_eq!(error.kind(), AuthErrorKind::Rejected);
    static_slot
        .replace(TestMaterial {
            id: 8,
            expires_at: None,
        })
        .expect("replacement clears rejection");
    assert_eq!(
        static_slot
            .preflight(core.provider_http(None))
            .await
            .expect("replacement ready")
            .value
            .id,
        8
    );
}

/// Module invariant: a retired flight is told to re-evaluate before retired
/// material is dropped. A cancelled leader whose source version was rejected
/// retires the challenged installed material; its application `Drop` must
/// observe the flight already published, so a panicking or blocking `Drop`
/// can never leave followers waiting on an unpublished flight.
#[test]
fn cancelled_leader_publishes_reevaluation_before_dropping_retired_material() {
    type ProbeFlight = Arc<Mutex<Option<Arc<Flight<FlightProbeMaterial>>>>>;

    struct FlightProbeMaterial {
        flight: ProbeFlight,
        published_before_drop: Arc<Mutex<Option<bool>>>,
    }

    impl CredentialMaterial for FlightProbeMaterial {}

    impl Drop for FlightProbeMaterial {
        fn drop(&mut self) {
            let flight = lock(&self.flight).ok().and_then(|flight| flight.clone());
            let published = flight
                .map(|flight| matches!(flight.result_now(), Ok(Some(FlightOutcome::Reevaluate))));
            if let Ok(mut seen) = lock(&self.published_before_drop) {
                *seen = published;
            }
        }
    }

    struct UnusedProvider;

    impl CredentialProvider for UnusedProvider {
        type Material = FlightProbeMaterial;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            Box::pin(async { Err(AuthError::new(AuthErrorKind::Acquisition)) })
        }
    }

    let selector = SlotSelector::provider(UnusedProvider, Duration::ZERO);
    let slot = selector.selected().expect("selected slot");
    let flight = Arc::new(Flight::new());
    let probe_flight: ProbeFlight = Arc::new(Mutex::new(Some(Arc::clone(&flight))));
    let published_before_drop = Arc::new(Mutex::new(None));
    {
        let mut state = lock(&slot.state).expect("slot state");
        let epoch = state.intent_epoch;
        // The slot holds the only reference to the challenged material.
        state.ready = Some(Ready {
            version: 1,
            value: Arc::new(FlightProbeMaterial {
                flight: Arc::clone(&probe_flight),
                published_before_drop: Arc::clone(&published_before_drop),
            }),
            expires_at: None,
        });
        state.rejected_version = Some(1);
        state.in_flight = Some(ActiveFlight {
            flight: Arc::clone(&flight),
            epoch,
            source_version: Some(1),
        });
    }

    slot.cancel_leader(&flight);

    assert_eq!(
        *lock(&published_before_drop).expect("probe"),
        Some(true),
        "the retired material was dropped before the flight published Reevaluate"
    );
    let state = lock(&slot.state).expect("slot state");
    assert!(state.ready.is_none() && state.in_flight.is_none());
    assert_eq!(state.rejected_version, None);
    drop(state);
    *lock(&probe_flight).expect("probe flight") = None;
}

/// Module invariant for the leader's own terminal: `finish_provider` wakes
/// followers with the flight's terminal before any material the slot no
/// longer holds runs its application `Drop`. In the version-overflow branch
/// the produced value is the last reference to itself, so its `Drop` must
/// observe the flight already published (third post-fix review probe).
#[tokio::test]
async fn provider_overflow_publishes_the_terminal_before_dropping_the_produced_value() {
    type FlightProbe = Arc<Mutex<Option<Arc<Flight<OrderProbeMaterial>>>>>;
    type SlotProbe = Arc<Mutex<Option<std::sync::Weak<CredentialSlot<OrderProbeMaterial>>>>>;

    struct OrderProbeMaterial {
        flight: FlightProbe,
        published_at_drop: Arc<Mutex<Option<bool>>>,
    }

    impl CredentialMaterial for OrderProbeMaterial {}

    impl Drop for OrderProbeMaterial {
        fn drop(&mut self) {
            let flight = lock(&self.flight).ok().and_then(|flight| flight.clone());
            let published = flight.is_some_and(|flight| matches!(flight.result_now(), Ok(Some(_))));
            if let Ok(mut seen) = lock(&self.published_at_drop) {
                seen.get_or_insert(published);
            }
        }
    }

    struct CapturingProvider {
        flight: FlightProbe,
        published_at_drop: Arc<Mutex<Option<bool>>>,
        slot: SlotProbe,
    }

    impl CredentialProvider for CapturingProvider {
        type Material = OrderProbeMaterial;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            // Capture the leader's flight for the Drop probe.
            let slot = lock(&self.slot)
                .ok()
                .and_then(|slot| slot.as_ref().and_then(std::sync::Weak::upgrade));
            if let Some(slot) = slot {
                let flight = lock(&slot.state).ok().and_then(|state| {
                    state
                        .in_flight
                        .as_ref()
                        .map(|active| Arc::clone(&active.flight))
                });
                *lock(&self.flight).expect("flight probe") = flight;
            }
            let material = OrderProbeMaterial {
                flight: Arc::clone(&self.flight),
                published_at_drop: Arc::clone(&self.published_at_drop),
            };
            Box::pin(async move { Ok(material) })
        }
    }

    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let flight_probe: FlightProbe = Arc::new(Mutex::new(None));
    let slot_probe: SlotProbe = Arc::new(Mutex::new(None));
    let published_at_drop = Arc::new(Mutex::new(None));
    let selector = SlotSelector::provider(
        CapturingProvider {
            flight: Arc::clone(&flight_probe),
            published_at_drop: Arc::clone(&published_at_drop),
            slot: Arc::clone(&slot_probe),
        },
        Duration::ZERO,
    );
    let slot = selector.selected().expect("selected slot");
    *lock(&slot_probe).expect("slot probe") = Some(Arc::downgrade(&slot));
    slot.versions.last.store(u64::MAX, Ordering::SeqCst);

    let error = auth_error(
        selector.preflight(core.provider_http(None)).await,
        "version overflow",
    );
    assert_eq!(error.kind(), AuthErrorKind::Internal);
    assert_eq!(
        *lock(&published_at_drop).expect("probe"),
        Some(true),
        "the produced value was dropped before the flight published its terminal"
    );
    *lock(&flight_probe).expect("flight probe") = None;
}

/// A late metadata callback must not allocate a version. Its discarded result
/// is released only after the terminal reaches followers and outside the lock.
#[tokio::test]
async fn provider_expiration_timeout_publishes_before_dropping_material_outside_the_lock() {
    use crate::operation::{OperationDeadline, TestClockGuard};

    type SlotProbe = Arc<Mutex<Option<std::sync::Weak<CredentialSlot<DeadlineMaterial>>>>>;
    type FlightProbe = Arc<Mutex<Option<Arc<Flight<DeadlineMaterial>>>>>;

    struct DeadlineMaterial {
        slot: SlotProbe,
        flight: FlightProbe,
        observed: Arc<Mutex<Option<(bool, bool)>>>,
    }

    impl CredentialMaterial for DeadlineMaterial {
        fn expires_at(&self) -> Option<Instant> {
            TestClockGuard::advance(Duration::from_secs(2));
            None
        }
    }

    impl Drop for DeadlineMaterial {
        fn drop(&mut self) {
            let slot = lock(&self.slot)
                .expect("slot probe")
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .expect("live slot");
            let unlocked = slot.state.try_lock().is_ok();
            let flight = lock(&self.flight)
                .expect("flight probe")
                .clone()
                .expect("flight");
            let published = matches!(flight.result_now(), Ok(Some(FlightOutcome::Failed(_))));
            *lock(&self.observed).expect("Drop observation") = Some((unlocked, published));
        }
    }

    struct DeadlineProvider {
        slot: SlotProbe,
        flight: FlightProbe,
        observed: Arc<Mutex<Option<(bool, bool)>>>,
    }

    impl CredentialProvider for DeadlineProvider {
        type Material = DeadlineMaterial;

        fn acquire<'a>(
            &'a self,
            _: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            let slot = lock(&self.slot)
                .expect("slot probe")
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .expect("live slot");
            *lock(&self.flight).expect("flight probe") = lock(&slot.state)
                .expect("slot state")
                .in_flight
                .as_ref()
                .map(|active| Arc::clone(&active.flight));
            Box::pin(async move {
                Ok(DeadlineMaterial {
                    slot: Arc::clone(&self.slot),
                    flight: Arc::clone(&self.flight),
                    observed: Arc::clone(&self.observed),
                })
            })
        }
    }

    let _clock = TestClockGuard::reset();
    let slot_probe: SlotProbe = Arc::new(Mutex::new(None));
    let flight_probe: FlightProbe = Arc::new(Mutex::new(None));
    let observed = Arc::new(Mutex::new(None));
    let selector = SlotSelector::provider(
        DeadlineProvider {
            slot: Arc::clone(&slot_probe),
            flight: Arc::clone(&flight_probe),
            observed: Arc::clone(&observed),
        },
        Duration::ZERO,
    );
    let slot = selector.selected().expect("selected slot");
    *lock(&slot_probe).expect("slot probe") = Some(Arc::downgrade(&slot));
    let core = crate::client::ClientCore::build(
        "https://provider.example.test/",
        crate::http::HttpConfigBuilder::new(),
        crate::runtime::RuntimeConfigBuilder::new(),
    )
    .expect("provider core");
    let deadline = OperationDeadline::new(OperationDeadline::now(), Some(Duration::from_secs(1)))
        .expect("parent deadline");
    let error = auth_error(
        selector.preflight(core.provider_http(Some(deadline))).await,
        "metadata callback consumed acquisition budget",
    );
    assert_eq!(error.kind(), AuthErrorKind::Acquisition);
    assert_eq!(
        *lock(&observed).expect("Drop observation"),
        Some((true, true))
    );
    assert_eq!(selector.counters_for_test(), (1, 0));
    assert!(!selector.is_set());
    *lock(&flight_probe).expect("flight probe") = None;
}
