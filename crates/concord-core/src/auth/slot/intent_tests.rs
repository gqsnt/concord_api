//! Installation intent versus installed version (AUTH-03, AUTH-04) and
//! provider single-flight (V3-E12..V3-E15) at the credential-store level.
//!
//! Every interleaving is forced explicitly: storage "start" is the intent
//! reservation performed on first poll, "succeeds" is the install attempt
//! after complete decoding, and provider concurrency is driven by gates.

use std::sync::atomic::{AtomicUsize, Ordering};

use concord_test_support::gate::Gate;

use crate::hooks::BoxFuture;

use super::*;

mod s1_replacement_overflow;

#[derive(Debug)]
struct Session(u32);

impl CredentialMaterial for Session {}

fn http() -> ProviderHttp<'static> {
    ProviderHttp::unavailable_for_test()
}

fn store() -> Arc<SlotSelector<Session>> {
    SlotSelector::endpoint(Duration::ZERO)
}

async fn current(selector: &SlotSelector<Session>) -> Result<u32, AuthErrorKind> {
    selector
        .preflight(http())
        .await
        .map(|prepared| prepared.value().0)
        .map_err(|error| error.kind())
}

fn installed(outcome: StoreOutcome) -> CredentialVersion {
    match outcome {
        StoreOutcome::Installed { version } => version,
        StoreOutcome::Superseded => panic!("expected an installed outcome"),
    }
}

// V3-E12 row 1: Start A; clear; A succeeds => A Superseded; slot empty.
#[tokio::test]
async fn start_a_clear_a_succeeds_is_superseded_and_store_stays_empty() {
    let selector = store();
    let a = selector.reserve_store_intent().expect("start A");
    selector.clear().expect("clear");
    assert_eq!(
        selector.install(a, Session(1)).expect("A completes"),
        StoreOutcome::Superseded
    );
    assert!(!selector.is_set());
    assert_eq!(selector.installed_version(), None);
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Unavailable));
}

// V3-E12 row 2: Start A; manual replace B; A succeeds => A Superseded; B remains.
#[tokio::test]
async fn start_a_replace_b_a_succeeds_is_superseded_and_b_remains() {
    let selector = store();
    let a = selector.reserve_store_intent().expect("start A");
    let b = selector.replace(Session(2)).expect("replace B");
    assert_eq!(
        selector.install(a, Session(1)).expect("A completes"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.installed_version(), Some(b));
    assert_eq!(current(&selector).await, Ok(2));
}

// V3-E12 row 3: Start A; start B; A succeeds; B succeeds => A Superseded; B installs.
#[tokio::test]
async fn start_a_start_b_a_succeeds_b_succeeds_b_installs() {
    let selector = store();
    let a = selector.reserve_store_intent().expect("start A");
    let b = selector.reserve_store_intent().expect("start B");
    assert_eq!(
        selector.install(a, Session(1)).expect("A completes"),
        StoreOutcome::Superseded
    );
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Unavailable));
    let version = installed(selector.install(b, Session(2)).expect("B completes"));
    assert_eq!(selector.installed_version(), Some(version));
    assert_eq!(current(&selector).await, Ok(2));
}

// V3-E12 row 4: Start A; start B; B fails; A succeeds => A Superseded;
// pre-existing ready value remains; no revival.
#[tokio::test]
async fn start_a_start_b_b_fails_a_succeeds_no_revival_and_existing_value_remains() {
    let selector = store();
    let existing = selector.replace(Session(7)).expect("pre-existing value");
    let a = selector.reserve_store_intent().expect("start A");
    let b = selector.reserve_store_intent().expect("start B");
    // B fails: its ticket is dropped without installing. Nothing rolls back.
    drop(b);
    assert_eq!(
        selector.install(a, Session(1)).expect("A completes"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.installed_version(), Some(existing));
    assert_eq!(current(&selector).await, Ok(7));
    // The failure did not revive A: a fresh start is required to install.
    let c = selector.reserve_store_intent().expect("start C");
    installed(selector.install(c, Session(3)).expect("C completes"));
    assert_eq!(current(&selector).await, Ok(3));
}

// V3-E12 row 5 / V3-E13: Old version V rejected while a newer intent is
// pending => V is rejected; the newer intent remains eligible.
#[tokio::test]
async fn old_version_rejected_while_newer_intent_pending_keeps_the_intent_eligible() {
    let selector = store();
    selector.replace(Session(1)).expect("version V");
    let applied = selector.preflight(http()).await.expect("attempt applies V");
    let pending = selector.reserve_store_intent().expect("newer intent");
    selector
        .reject_installed(&applied, false)
        .expect("challenge rejects exactly V");
    assert!(!selector.is_set());
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Rejected));
    let version = installed(
        selector
            .install(pending, Session(2))
            .expect("newer install"),
    );
    assert_eq!(selector.installed_version(), Some(version));
    assert_eq!(current(&selector).await, Ok(2));
}

// V3-E12 row 6 at store level: an intent reserved after a clear is a new,
// eligible post-clear intent (the executor-level laziness test proves the
// reservation happens on first poll).
#[tokio::test]
async fn intent_reserved_after_clear_is_a_new_eligible_intent() {
    let selector = store();
    selector.replace(Session(1)).expect("initial");
    selector.clear().expect("clear");
    let a = selector.reserve_store_intent().expect("post-clear start");
    installed(selector.install(a, Session(2)).expect("post-clear install"));
    assert_eq!(current(&selector).await, Ok(2));
}

// V3-E13: a late challenge for an old version cannot clear a newer version.
#[tokio::test]
async fn late_challenge_for_old_version_cannot_clear_a_newer_installed_version() {
    let selector = store();
    selector.replace(Session(1)).expect("version 1");
    let old = selector.preflight(http()).await.expect("old attempt");
    let pending = selector.reserve_store_intent().expect("newer storage");
    let newer = installed(
        selector
            .install(pending, Session(2))
            .expect("newer install"),
    );
    selector
        .reject_installed(&old, false)
        .expect("late challenge for version 1");
    assert!(selector.is_set());
    assert_eq!(selector.installed_version(), Some(newer));
    assert_eq!(current(&selector).await, Ok(2));

    // The same holds for a manual replacement and for provider-retained
    // (refresh-input) rejection.
    let before = selector
        .preflight(http())
        .await
        .expect("attempt on version 2");
    let replaced = selector.replace(Session(3)).expect("manual replacement");
    selector
        .reject_installed(&before, true)
        .expect("late challenge for version 2");
    assert_eq!(selector.installed_version(), Some(replaced));
    assert_eq!(current(&selector).await, Ok(3));
}

#[tokio::test]
async fn reserving_an_intent_does_not_clear_installed_material() {
    let selector = store();
    let version = selector.replace(Session(1)).expect("installed");
    let _pending = selector.reserve_store_intent().expect("pending intent");
    assert!(selector.is_set());
    assert_eq!(selector.installed_version(), Some(version));
    assert_eq!(current(&selector).await, Ok(1));
}

#[tokio::test]
async fn intent_epoch_and_installed_version_are_separate_counters() {
    let selector = store();
    assert_eq!(selector.counters_for_test(), (0, 0));
    let a = selector.reserve_store_intent().expect("reservation");
    assert_eq!(
        selector.counters_for_test(),
        (1, 0),
        "reservation: intent only"
    );
    selector.clear().expect("clear of an empty store");
    assert_eq!(selector.counters_for_test(), (2, 0), "clear: intent only");
    selector.replace(Session(1)).expect("replace");
    assert_eq!(
        selector.counters_for_test(),
        (3, 1),
        "replace: intent and version"
    );
    assert_eq!(
        selector.install(a, Session(9)).expect("superseded install"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.counters_for_test(), (3, 1), "superseded: nothing");
    let b = selector.reserve_store_intent().expect("reservation");
    installed(selector.install(b, Session(2)).expect("current install"));
    assert_eq!(
        selector.counters_for_test(),
        (4, 2),
        "install: version only"
    );
    let applied = selector.preflight(http()).await.expect("applied");
    selector
        .reject_installed(&applied, false)
        .expect("challenge rejection");
    assert_eq!(
        selector.counters_for_test(),
        (4, 2),
        "challenge rejection advances neither counter"
    );
}

#[tokio::test]
async fn explicit_storage_requires_an_endpoint_backed_store() {
    let selector = SlotSelector::static_value(Session(1), Duration::ZERO);
    let error = match selector.reserve_store_intent() {
        Ok(_) => panic!("static source must refuse explicit storage"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
    assert_eq!(current(&selector).await, Ok(1));
}

// V3-E15: counter exhaustion fails closed without wrapping.
#[tokio::test]
async fn intent_epoch_exhaustion_fails_closed_and_removes_material() {
    let selector = store();
    selector.replace(Session(1)).expect("installed");
    selector.set_counters_for_test(u64::MAX, 1);
    let error = match selector.reserve_store_intent() {
        Ok(_) => panic!("an exhausted intent epoch must not wrap"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), AuthErrorKind::Internal);
    assert_eq!(error.to_string(), "authentication failed (Internal)");
    assert!(!selector.is_set());
    assert_eq!(selector.installed_version(), None);
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Internal));
    assert_eq!(
        selector
            .replace(Session(2))
            .expect_err("exhausted replace")
            .kind(),
        AuthErrorKind::Internal
    );
    assert_eq!(
        selector.clear().expect_err("exhausted clear").kind(),
        AuthErrorKind::Internal
    );
    assert_eq!(selector.counters_for_test().0, u64::MAX);
}

#[tokio::test]
async fn clear_with_exhausted_intent_epoch_still_removes_material() {
    let selector = store();
    selector.replace(Session(1)).expect("installed");
    selector.set_counters_for_test(u64::MAX, 1);
    assert_eq!(
        selector.clear().expect_err("clear cannot advance").kind(),
        AuthErrorKind::Internal
    );
    assert!(!selector.is_set());
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Internal));
}

#[tokio::test]
async fn installed_version_exhaustion_fails_closed_on_install() {
    let selector = store();
    let ticket = selector.reserve_store_intent().expect("reservation");
    selector.set_counters_for_test(1, u64::MAX);
    assert_eq!(
        selector
            .install(ticket, Session(1))
            .expect_err("exhausted version")
            .kind(),
        AuthErrorKind::Internal
    );
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test().1, u64::MAX);
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Internal));
}

// V3-E15: a poisoned credential store fails closed with a value-free error.
#[tokio::test]
async fn poisoned_store_fails_closed_with_value_free_errors() {
    let selector = store();
    selector.replace(Session(1)).expect("installed");
    let ticket = selector.reserve_store_intent().expect("reservation");
    selector.poison_for_test();
    assert!(!selector.is_set());
    assert_eq!(selector.installed_version(), None);
    assert_eq!(current(&selector).await, Err(AuthErrorKind::Internal));
    let error = selector
        .install(ticket, Session(2))
        .expect_err("poisoned install");
    assert_eq!(error.kind(), AuthErrorKind::Internal);
    assert_eq!(error.to_string(), "authentication failed (Internal)");
    assert!(selector.reserve_store_intent().is_err());
    assert!(selector.clear().is_err());
    assert!(selector.replace(Session(3)).is_err());
}

#[tokio::test]
async fn provider_replacement_cannot_change_the_source_kind() {
    let selector = store();
    let error = selector
        .replace_provider(GatedProvider::new(None))
        .expect_err("an endpoint-backed store refuses provider replacement");
    assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
    let ticket = selector
        .reserve_store_intent()
        .expect("the store remains endpoint-backed");
    installed(selector.install(ticket, Session(1)).expect("install"));
    assert_eq!(current(&selector).await, Ok(1));
}

struct GatedProvider {
    calls: Arc<AtomicUsize>,
    gate: Option<Gate>,
    gated_call: usize,
}

impl GatedProvider {
    fn new(gate: Option<Gate>) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            gate,
            gated_call: 0,
        }
    }
}

impl CredentialProvider for GatedProvider {
    type Material = Session;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let gate = (call == self.gated_call)
            .then(|| self.gate.clone())
            .flatten();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            Ok(Session(u32::try_from(call + 1).expect("small call count")))
        })
    }
}

// V3-E14: followers join the current acquisition; the leader reserves one
// intent epoch and one installed version for the whole cohort.
#[tokio::test]
async fn provider_leader_reserves_one_epoch_and_followers_join() {
    let gate = Gate::closed();
    let provider = GatedProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let open = async {
        gate.wait_until_reached().await;
        gate.open();
    };
    let (first, second, third, fourth, ()) = tokio::join!(
        selector.preflight(http()),
        selector.preflight(http()),
        selector.preflight(http()),
        selector.preflight(http()),
        open,
    );
    let versions = [first, second, third, fourth].map(|result| {
        let prepared = result.expect("cohort member");
        assert_eq!(prepared.value().0, 1);
        prepared.version()
    });
    assert!(versions.iter().all(|version| *version == versions[0]));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(selector.counters_for_test(), (1, 1));
}

// V3-E14: a manual clear supersedes the leader; its late result is never
// installed and the follower re-evaluates into a new flight.
#[tokio::test]
async fn provider_flight_superseded_by_manual_clear_never_installs_its_result() {
    let gate = Gate::closed();
    let provider = GatedProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let clear = {
        let selector = Arc::clone(&selector);
        async move {
            gate.wait_until_reached().await;
            selector
                .clear()
                .expect("manual clear supersedes the flight");
            gate.open();
        }
    };
    let (prepared, ()) = tokio::join!(selector.preflight(http()), clear);
    assert_eq!(prepared.expect("re-evaluated acquisition").value().0, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    // Leader 1 epoch, clear epoch, leader 2 epoch; one version installed.
    assert_eq!(selector.counters_for_test(), (3, 1));
}

// V3-E14: a cancelled leader retires its flight so a waiting follower does
// not hang; the follower becomes the next leader. The cancelled leader's
// epoch is not rolled back.
#[tokio::test]
async fn cancelled_leader_retires_its_flight_and_the_follower_takes_over() {
    let gate = Gate::closed();
    let provider = GatedProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    let leader = {
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { selector.preflight(http()).await.map(|p| p.value().0) })
    };
    gate.wait_until_reached().await;
    let follower = {
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { selector.preflight(http()).await.map(|p| p.value().0) })
    };
    tokio::task::yield_now().await;
    leader.abort();
    assert!(leader.await.expect_err("aborted leader").is_cancelled());
    let value = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .expect("follower must not hang")
        .expect("follower task")
        .expect("follower acquisition");
    assert_eq!(value, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(selector.counters_for_test(), (2, 1));
}

// V3-E14: waiters are bounded; a cancelled follower releases its slot and
// never cancels the live leader.
#[tokio::test]
async fn follower_waiters_are_bounded_and_cancellation_releases_the_slot() {
    let gate = Gate::closed();
    let provider = GatedProvider::new(Some(gate.clone()));
    let calls = Arc::clone(&provider.calls);
    let selector = SlotSelector::provider(provider, Duration::ZERO);
    selector.set_waiter_limit_for_test(1);
    let leader = {
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { selector.preflight(http()).await.map(|p| p.value().0) })
    };
    gate.wait_until_reached().await;
    let first_follower = {
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { selector.preflight(http()).await.map(|p| p.value().0) })
    };
    tokio::task::yield_now().await;
    let over_limit = selector
        .preflight(http())
        .await
        .map(|prepared| prepared.value().0)
        .expect_err("second follower exceeds the waiter limit");
    assert_eq!(over_limit.kind(), AuthErrorKind::Unavailable);
    assert!(!over_limit.to_string().contains("Session"));

    first_follower.abort();
    assert!(
        first_follower
            .await
            .expect_err("aborted follower")
            .is_cancelled()
    );
    let replacement = {
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { selector.preflight(http()).await.map(|p| p.value().0) })
    };
    tokio::task::yield_now().await;
    gate.open();
    assert_eq!(leader.await.expect("leader task").expect("leader"), 1);
    assert_eq!(
        replacement
            .await
            .expect("replacement follower task")
            .expect("replacement follower"),
        1
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// The previous finalization loop could spin without yielding while a
// proactive refresh was in flight; finalization now joins the flight.
#[tokio::test]
async fn finalization_joins_an_in_flight_proactive_refresh_instead_of_spinning() {
    struct ExpiringProvider {
        calls: Arc<AtomicUsize>,
        gate: Gate,
    }

    #[derive(Debug)]
    struct Expiring {
        id: usize,
        expires_at: Option<Instant>,
    }

    impl CredentialMaterial for Expiring {
        fn expires_at(&self) -> Option<Instant> {
            self.expires_at
        }
    }

    impl CredentialProvider for ExpiringProvider {
        type Material = Expiring;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let gate = self.gate.clone();
            Box::pin(async move {
                if call == 0 {
                    // Valid, but inside the proactive refresh skew.
                    return Ok(Expiring {
                        id: 1,
                        expires_at: Instant::now().checked_add(Duration::from_secs(30)),
                    });
                }
                gate.wait().await;
                Ok(Expiring {
                    id: 2,
                    expires_at: None,
                })
            })
        }
    }

    let gate = Gate::closed();
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        ExpiringProvider {
            calls: Arc::clone(&calls),
            gate: gate.clone(),
        },
        Duration::ZERO,
    );
    let prepared = selector.preflight(http()).await.expect("initial");
    assert_eq!(prepared.value().id, 1);
    let open = async {
        gate.wait_until_reached().await;
        gate.open();
    };
    let (proactive, finalized, ()) = tokio::join!(
        selector.preflight(http()),
        selector.finalize(&prepared, http()),
        open,
    );
    assert_eq!(proactive.expect("proactive refresh").value().id, 2);
    assert_eq!(finalized.expect("finalization joins").value().id, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
