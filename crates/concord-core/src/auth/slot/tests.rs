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
            .set(TestMaterial {
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
        .replace_static_result(ApiKey::try_new(&exact))
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
        .replace_static_result(ApiKey::try_new(&excessive))
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
        .replace_static_result(BasicCredential::try_new("foreign:user", "replacement"))
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
                .set(TestMaterial {
                    id: 99,
                    expires_at: None,
                })
                .expect("manual set");
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
                .replace_static(TestMaterial {
                    id: 77,
                    expires_at: None,
                })
                .expect("complete source replacement");
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
        .set(TestMaterial {
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
        .set(TestMaterial {
            id: 1,
            expires_at: None,
        })
        .expect("endpoint set");
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
        .replace_static(TestMaterial {
            id: 11,
            expires_at: None,
        })
        .expect("complete replacement");
    let current = static_slot
        .finalize(&old, core.provider_http(None))
        .await
        .expect("finalization follows new slot");
    assert_eq!(current.value.id, 11);
    assert!(!Arc::ptr_eq(&old.slot, &current.slot));

    let same_slot = SlotSelector::endpoint(Duration::ZERO);
    same_slot
        .set(TestMaterial {
            id: 20,
            expires_at: None,
        })
        .expect("first same-slot value");
    let old = same_slot
        .preflight(core.provider_http(None))
        .await
        .expect("same-slot preflight");
    same_slot
        .set(TestMaterial {
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
async fn source_replacement_during_awaited_finalization_wins_selection() {
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

    let replace = {
        let selector = Arc::clone(&selector);
        async move {
            refresh_gate.wait_until_reached().await;
            selector
                .replace_static(TestMaterial {
                    id: 99,
                    expires_at: None,
                })
                .expect("complete replacement during finalization");
            refresh_gate.open();
        }
    };
    let (finalized, ()) = tokio::join!(
        selector.finalize(&prepared, core.provider_http(None)),
        replace,
    );
    let finalized = finalized.expect("finalization follows replacement slot");
    assert_eq!(finalized.value.id, 99);
    assert!(!Arc::ptr_eq(&finalized.slot, &prepared.slot));
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
        (Duration::from_secs(30), 2, true),
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
    lock(&slot.state).expect("slot state").next_version = u64::MAX;

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
        .reject_static_exact(&applied)
        .expect("exact rejection");
    assert!(!static_slot.is_set());
    let error = auth_error(
        static_slot.preflight(core.provider_http(None)).await,
        "rejection persists",
    );
    assert_eq!(error.kind(), AuthErrorKind::Rejected);
    static_slot
        .replace_static(TestMaterial {
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
