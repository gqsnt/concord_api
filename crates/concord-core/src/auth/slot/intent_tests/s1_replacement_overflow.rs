//! Native AUTH-03/V3-E15 probe over actual production slot state.
use super::*;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

async fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
}

#[tokio::test]
async fn s1_overflow_existing_mutations_and_near_boundary_provider_replace_controls() {
    for mutation in ["clear", "replace", "reservation"] {
        let selector = if mutation == "reservation" {
            store()
        } else {
            SlotSelector::provider(GatedProvider::new(None), Duration::ZERO)
        };
        selector.replace(Session(7)).unwrap();
        selector.set_counters_for_test(u64::MAX, 1);
        let error = match mutation {
            "clear" => selector.clear().unwrap_err(),
            "replace" => selector.replace(Session(9)).unwrap_err(),
            _ => match selector.reserve_store_intent() {
                Ok(_) => panic!("overflow reservation must fail"),
                Err(error) => error,
            },
        };
        let actual = current(&selector).await;
        println!(
            "S1_COUNTER_CONTROL mutation={mutation} error={:?} is_set={} version={:?} preflight={actual:?} counters={:?}",
            error.kind(),
            selector.is_set(),
            selector.installed_version(),
            selector.counters_for_test()
        );
        assert_eq!(error.kind(), AuthErrorKind::Internal);
        assert_eq!(error.to_string(), "authentication failed (Internal)");
        assert!(!selector.is_set());
        assert_eq!(selector.installed_version(), None);
        assert_eq!(actual, Err(AuthErrorKind::Internal));
        assert_eq!(selector.counters_for_test().0, u64::MAX);
    }
    // A counter AT max is not automatically failed: a legitimately completed
    // last reservation may still have ready material. The next attempted
    // allocation that cannot advance must trigger the fail-closed transition.
    let selector = SlotSelector::provider(GatedProvider::new(None), Duration::ZERO);
    assert_eq!(current(&selector).await, Ok(1));
    let old_version = selector.installed_version().unwrap();
    selector.set_counters_for_test(u64::MAX - 2, 1);
    selector.replace_provider(GatedProvider::new(None)).unwrap();
    assert_eq!(current(&selector).await, Ok(1));
    assert_ne!(selector.installed_version(), Some(old_version));
    assert_eq!(selector.counters_for_test(), (u64::MAX, 2));
    println!(
        "S1_COUNTER_CONTROL mutation=near_boundary_provider_replace ok=true is_set=true distinct_version=true counters={:?}",
        selector.counters_for_test()
    );
}

#[tokio::test]
async fn s1_provider_replacement_overflow_removes_ready_material_and_refuses_use() {
    let selector = SlotSelector::provider(GatedProvider::new(None), Duration::ZERO);
    selector.replace(Session(7)).unwrap();
    selector.set_counters_for_test(u64::MAX, 1);
    let error = selector
        .replace_provider(GatedProvider::new(None))
        .unwrap_err();
    let actual = current(&selector).await;
    println!(
        "S1_COUNTER_READY mutation=provider_replace error={:?} is_set={} version={:?} preflight={actual:?} counters={:?}",
        error.kind(),
        selector.is_set(),
        selector.installed_version(),
        selector.counters_for_test()
    );
    assert_eq!(error.kind(), AuthErrorKind::Internal);
    assert_eq!(error.to_string(), "authentication failed (Internal)");
    assert_eq!(selector.counters_for_test().0, u64::MAX);
    assert!(
        !selector.is_set(),
        "AUTH-03 exhausted slot must remove ready material"
    );
    assert_eq!(selector.installed_version(), None);
    assert_eq!(actual, Err(AuthErrorKind::Internal));
}

#[tokio::test]
async fn s1_provider_replacement_overflow_retires_live_leader_and_follower_without_install() {
    // Clear is an existing positive control on the identical last-epoch flight.
    for mutation in ["clear", "provider_replace"] {
        let gate = Gate::closed();
        let provider = GatedProvider::new(Some(gate.clone()));
        let calls = provider.calls.clone();
        let selector = SlotSelector::provider(provider, Duration::ZERO);
        selector.set_counters_for_test(u64::MAX - 1, 0);
        let mut leader = Box::pin(current(&selector));
        let mut follower = Box::pin(current(&selector));
        assert!(poll_once(leader.as_mut()).await.is_pending());
        assert_eq!(gate.arrivals(), 1);
        assert!(poll_once(follower.as_mut()).await.is_pending());
        let slot = selector.selected().unwrap();
        let flight = slot
            .state
            .lock()
            .unwrap()
            .in_flight
            .as_ref()
            .unwrap()
            .flight
            .clone();
        assert_eq!(flight.waiters.load(Ordering::SeqCst), 1);
        assert_eq!(selector.counters_for_test(), (u64::MAX, 0));
        let error = if mutation == "clear" {
            selector.clear().unwrap_err()
        } else {
            selector
                .replace_provider(GatedProvider::new(None))
                .unwrap_err()
        };
        let before_leader = poll_once(leader.as_mut()).await;
        let before_follower = poll_once(follower.as_mut()).await;
        let leader_pending = before_leader.is_pending();
        let follower_pending = before_follower.is_pending();
        let waiters_before_release = flight.waiters.load(Ordering::SeqCst);
        gate.open();
        let leader_result = match before_leader {
            Poll::Ready(result) => result,
            Poll::Pending => match poll_once(leader.as_mut()).await {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("released leader still pending"),
            },
        };
        let follower_result = match before_follower {
            Poll::Ready(result) => result,
            Poll::Pending => match poll_once(follower.as_mut()).await {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("settled follower still pending"),
            },
        };
        println!(
            "S1_COUNTER_FLIGHT mutation={mutation} error={:?} before_release_leader_pending={leader_pending} before_release_follower_pending={follower_pending} waiters_before_release={waiters_before_release} terminal_leader={leader_result:?} terminal_follower={follower_result:?} is_set={} version={:?} provider_calls={} final_waiters={} counters={:?}",
            error.kind(),
            selector.is_set(),
            selector.installed_version(),
            calls.load(Ordering::SeqCst),
            flight.waiters.load(Ordering::SeqCst),
            selector.counters_for_test()
        );
        assert_eq!(error.kind(), AuthErrorKind::Internal);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(flight.waiters.load(Ordering::SeqCst), 0);
        assert!(
            !leader_pending && !follower_pending,
            "overflow must retire the flight without waiting for provider I/O"
        );
        assert_eq!(leader_result, Err(AuthErrorKind::Internal));
        assert_eq!(follower_result, Err(AuthErrorKind::Internal));
        assert!(
            !selector.is_set(),
            "late provider work must never install after exhaustion"
        );
        assert_eq!(selector.installed_version(), None);
        assert_eq!(selector.counters_for_test().0, u64::MAX);
    }
}
