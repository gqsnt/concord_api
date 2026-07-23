//! Bounded exhaustive scheduler for the credential transition model.
//!
//! Each test enumerates every order of a small event set. Invalid orders are
//! counted as pruned explicitly; this is deterministic model checking, not a
//! probabilistic thread stress test.

use super::model::{
    CohortTerminal, ErrorIdentity, LogicalTime, MaterialIdentity, ModelError, PublicSlotRef, Slot,
    SlotIdentity, SlotState, SourceIdentity, SourceKind, Version, WaiterIdentity,
};
use super::transition::{ClearOutcome, FinalizeDecision, PrepareDecision};

fn permutations<T: Copy>(events: &[T]) -> Vec<Vec<T>> {
    fn visit<T: Copy>(remaining: &mut Vec<T>, prefix: &mut Vec<T>, output: &mut Vec<Vec<T>>) {
        if remaining.is_empty() {
            output.push(prefix.clone());
            return;
        }
        for index in 0..remaining.len() {
            let event = remaining.remove(index);
            prefix.push(event);
            visit(remaining, prefix, output);
            prefix.pop();
            remaining.insert(index, event);
        }
    }

    let mut output = Vec::new();
    visit(&mut events.to_vec(), &mut Vec::new(), &mut output);
    output
}

fn empty_provider_slot() -> Slot {
    Slot::empty(
        SlotIdentity(1),
        SourceIdentity(11),
        SourceKind::CustomProvider,
    )
}

fn ready_slot(kind: SourceKind, expiry: u64) -> Slot {
    Slot::ready(
        SlotIdentity(1),
        SourceIdentity(11),
        kind,
        Version(1),
        MaterialIdentity(101),
        LogicalTime(expiry),
    )
}

#[test]
fn auth_model_systematic_single_flight_two_callers_bound_two_schedules() {
    #[derive(Clone, Copy)]
    enum Event {
        Prepare(WaiterIdentity),
    }

    let schedules = permutations(&[
        Event::Prepare(WaiterIdentity(1)),
        Event::Prepare(WaiterIdentity(2)),
    ]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = empty_provider_slot();
        let mut leader = None;
        let mut cohorts = Vec::new();
        for event in schedule {
            let Event::Prepare(waiter) = event;
            match slot.preflight(waiter, LogicalTime(0)) {
                PrepareDecision::Leader(ticket) => {
                    assert!(leader.replace(ticket).is_none());
                    cohorts.push(slot.operation_subscription(ticket, waiter).unwrap());
                }
                PrepareDecision::Joined(subscription) => cohorts.push(subscription),
                decision => panic!("unexpected decision: {decision:?}"),
            }
            assert!(slot.invariant());
        }
        let ticket = leader.expect("one leader");
        let prepared = slot
            .provider_succeeds(ticket, MaterialIdentity(202), LogicalTime(100))
            .unwrap()
            .unwrap();
        assert!(slot.invariant());
        assert_eq!(slot.provider_operations, 1);
        for subscription in cohorts {
            assert_eq!(subscription.publication_count(), 1);
            let terminal = subscription.observe().expect("cohort terminal");
            assert!(
                matches!(terminal, super::model::CohortTerminal::Success(value) if value.version == prepared.version && value.material == prepared.material)
            );
        }
    }
}

#[test]
fn auth_model_systematic_leader_cancellation_bound_two_schedules() {
    #[derive(Clone, Copy)]
    enum Event {
        WaiterPrepare,
        LeaderCancel,
    }

    let schedules = permutations(&[Event::WaiterPrepare, Event::LeaderCancel]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = empty_provider_slot();
        let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), LogicalTime(0))
        else {
            panic!("leader");
        };
        let mut second = None;
        for event in schedule {
            match event {
                Event::WaiterPrepare => match slot.preflight(WaiterIdentity(2), LogicalTime(0)) {
                    PrepareDecision::Joined(subscription) => {
                        assert_eq!(subscription.identity(), first.cohort);
                    }
                    PrepareDecision::Leader(ticket) => second = Some(ticket),
                    decision => panic!("unexpected decision: {decision:?}"),
                },
                Event::LeaderCancel => {
                    slot.cancel_leader(first).unwrap();
                }
            }
            assert!(slot.invariant());
        }
        let progress = match second {
            Some(ticket) => ticket,
            None => match slot.preflight(WaiterIdentity(2), LogicalTime(0)) {
                PrepareDecision::Leader(ticket) => ticket,
                decision => panic!("canceled waiter did not progress: {decision:?}"),
            },
        };
        assert!(
            slot.provider_succeeds(progress, MaterialIdentity(202), LogicalTime(100))
                .unwrap()
                .is_some()
        );
        assert!(!matches!(slot.state, SlotState::InFlight(_)));
        assert!(slot.invariant());
    }
}

#[derive(Clone, Copy)]
enum ReplacementEvent {
    Mutate,
    Complete,
}

#[test]
fn auth_model_systematic_manual_replacement_and_stale_completion_bound_two_schedules() {
    let schedules = permutations(&[ReplacementEvent::Mutate, ReplacementEvent::Complete]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = empty_provider_slot();
        let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), LogicalTime(0))
        else {
            panic!("leader");
        };
        for event in schedule {
            match event {
                ReplacementEvent::Mutate => {
                    slot.manual_set(MaterialIdentity(303), LogicalTime(100))
                        .unwrap();
                }
                ReplacementEvent::Complete => {
                    slot.provider_succeeds(ticket, MaterialIdentity(202), LogicalTime(100))
                        .unwrap();
                }
            }
            assert!(slot.invariant());
        }
        assert!(
            matches!(slot.state, SlotState::Ready(value) if value.material == MaterialIdentity(303))
        );
    }
}

#[test]
fn auth_model_systematic_clear_and_stale_completion_bound_two_schedules() {
    let schedules = permutations(&[ReplacementEvent::Mutate, ReplacementEvent::Complete]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = empty_provider_slot();
        let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), LogicalTime(0))
        else {
            panic!("leader");
        };
        for event in schedule {
            match event {
                ReplacementEvent::Mutate => {
                    slot.clear();
                }
                ReplacementEvent::Complete => {
                    slot.provider_succeeds(ticket, MaterialIdentity(202), LogicalTime(100))
                        .unwrap();
                }
            }
            assert!(slot.invariant());
        }
        assert!(matches!(slot.state, SlotState::Empty));
    }
}

#[test]
fn auth_model_systematic_exact_rejection_during_refresh_bound_two_schedules() {
    #[derive(Clone, Copy)]
    enum Event {
        Reject,
        Cancel,
    }

    let schedules = permutations(&[Event::Reject, Event::Cancel]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = ready_slot(SourceKind::RefreshableBearer, 1);
        let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), LogicalTime(2))
        else {
            panic!("refresh leader");
        };
        for event in schedule {
            match event {
                Event::Reject => {
                    slot.reject_exact(Version(1), false);
                }
                Event::Cancel => {
                    slot.cancel_leader(ticket).unwrap();
                }
            }
            assert!(slot.invariant());
        }
        assert!(matches!(slot.state, SlotState::Empty));
    }
}

#[test]
fn auth_model_systematic_physical_source_replacement_bound_six_prunes_three() {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum Event {
        Replace,
        OldComplete,
        Finalize,
    }

    let schedules = permutations(&[Event::Replace, Event::OldComplete, Event::Finalize]);
    assert_eq!(schedules.len(), 6);
    let mut explored = 0;
    let mut pruned = 0;
    for schedule in schedules {
        let replace_at = schedule
            .iter()
            .position(|event| *event == Event::Replace)
            .unwrap();
        let finalize_at = schedule
            .iter()
            .position(|event| *event == Event::Finalize)
            .unwrap();
        if replace_at > finalize_at {
            pruned += 1;
            continue;
        }
        explored += 1;
        let mut current = ready_slot(SourceKind::RefreshableBearer, 100);
        let prepared = match current.preflight(WaiterIdentity(9), LogicalTime(0)) {
            PrepareDecision::Ready(prepared) => prepared,
            decision => panic!("prepared: {decision:?}"),
        };
        let PrepareDecision::Leader(ticket) = current
            .begin_exact_refresh(Version(1), WaiterIdentity(1))
            .unwrap()
        else {
            panic!("refresh leader");
        };
        let mut public = PublicSlotRef::new(current);
        let mut old = None;
        let mut finalized = None;
        for event in schedule {
            match event {
                Event::Replace => {
                    old = Some(
                        public
                            .replace_source(SourceIdentity(22), SourceKind::StaticBearer)
                            .unwrap(),
                    );
                }
                Event::OldComplete => {
                    let target = old.as_mut().unwrap_or(&mut public.current);
                    target
                        .provider_succeeds(ticket, MaterialIdentity(202), LogicalTime(100))
                        .unwrap();
                    assert!(target.invariant());
                }
                Event::Finalize => {
                    finalized = Some(public.finalize(prepared, WaiterIdentity(2), LogicalTime(10)));
                }
            }
            assert!(public.current.invariant());
        }
        assert_eq!(public.current.identity, SlotIdentity(2));
        assert_eq!(
            finalized,
            Some(FinalizeDecision::Failed(ModelError::Unavailable))
        );
    }
    assert_eq!((explored, pruned), (3, 3));
}

#[derive(Clone, Copy)]
enum BoundaryEvent {
    Mutation,
    Finalize,
}

#[test]
fn auth_model_systematic_limiter_wait_mutations_bound_eight_schedules() {
    #[derive(Clone, Copy)]
    enum Mutation {
        Clear,
        Reject,
        Expire,
        Newer,
    }

    let mut explored = 0;
    for mutation in [
        Mutation::Clear,
        Mutation::Reject,
        Mutation::Expire,
        Mutation::Newer,
    ] {
        for schedule in permutations(&[BoundaryEvent::Mutation, BoundaryEvent::Finalize]) {
            explored += 1;
            let mutation_first = matches!(schedule[0], BoundaryEvent::Mutation);
            let kind = if matches!(mutation, Mutation::Expire) {
                SourceKind::RefreshableBearer
            } else {
                SourceKind::StaticBearer
            };
            let expiry = if matches!(mutation, Mutation::Expire) {
                5
            } else {
                100
            };
            let mut slot = ready_slot(kind, expiry);
            let prepared = match slot.preflight(WaiterIdentity(1), LogicalTime(0)) {
                PrepareDecision::Ready(prepared) => prepared,
                decision => panic!("prepared: {decision:?}"),
            };
            let mut now = LogicalTime(1);
            let mut decision = None;
            for event in schedule {
                match event {
                    BoundaryEvent::Mutation => match mutation {
                        Mutation::Clear => {
                            slot.clear();
                        }
                        Mutation::Reject => {
                            slot.reject_exact(Version(1), true);
                        }
                        Mutation::Expire => now = LogicalTime(10),
                        Mutation::Newer => {
                            slot.manual_set(MaterialIdentity(303), LogicalTime(100))
                                .unwrap();
                        }
                    },
                    BoundaryEvent::Finalize => {
                        decision = Some(slot.finalize(prepared, WaiterIdentity(2), now));
                    }
                }
                assert!(slot.invariant());
            }
            let result = decision.expect("finalization event");
            if mutation_first {
                match mutation {
                    Mutation::Clear => {
                        assert_eq!(result, FinalizeDecision::Failed(ModelError::Unavailable));
                    }
                    Mutation::Reject => assert_eq!(
                        result,
                        FinalizeDecision::Failed(ModelError::AuthenticationRejected)
                    ),
                    Mutation::Expire => assert!(matches!(result, FinalizeDecision::Leader(_))),
                    Mutation::Newer => assert!(matches!(
                        result,
                        FinalizeDecision::Final(finalized)
                            if finalized.material == MaterialIdentity(303)
                    )),
                }
            } else {
                assert!(matches!(
                    result,
                    FinalizeDecision::Final(finalized)
                        if finalized.material == MaterialIdentity(101)
                ));
            }
        }
    }
    assert_eq!(explored, 8);
}

#[test]
fn auth_model_systematic_finalization_boundary_clear_and_replacement_four_schedules() {
    #[derive(Clone, Copy)]
    enum Mutation {
        Clear,
        Replace,
    }

    let mut explored = 0;
    for mutation in [Mutation::Clear, Mutation::Replace] {
        for schedule in permutations(&[BoundaryEvent::Mutation, BoundaryEvent::Finalize]) {
            explored += 1;
            let mut public = PublicSlotRef::new(ready_slot(SourceKind::StaticBearer, 100));
            let prepared = match public.current.preflight(WaiterIdentity(1), LogicalTime(0)) {
                PrepareDecision::Ready(prepared) => prepared,
                decision => panic!("prepared: {decision:?}"),
            };
            let mutation_first = matches!(schedule[0], BoundaryEvent::Mutation);
            let mut finalized = None;
            for event in schedule {
                match event {
                    BoundaryEvent::Mutation => match mutation {
                        Mutation::Clear => {
                            public.current.clear();
                        }
                        Mutation::Replace => {
                            public
                                .replace_source(SourceIdentity(22), SourceKind::StaticBearer)
                                .unwrap();
                        }
                    },
                    BoundaryEvent::Finalize => {
                        finalized =
                            Some(public.finalize(prepared, WaiterIdentity(2), LogicalTime(10)));
                    }
                }
                assert!(public.current.invariant());
            }
            if mutation_first {
                assert_eq!(
                    finalized,
                    Some(FinalizeDecision::Failed(ModelError::Unavailable))
                );
            } else {
                assert!(matches!(
                    finalized,
                    Some(FinalizeDecision::Final(finalized))
                        if finalized.material == MaterialIdentity(101)
                ));
            }
        }
    }
    assert_eq!(explored, 4);
}

#[test]
fn auth_model_systematic_clear_rejection_replacement_bound_two_schedules() {
    #[derive(Clone, Copy)]
    enum Event {
        Clear,
        Replace,
    }

    let schedules = permutations(&[Event::Clear, Event::Replace]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut rejected = ready_slot(SourceKind::StaticBearer, 100);
        assert!(rejected.reject_exact(Version(1), true));
        let mut public = PublicSlotRef::new(rejected);
        let mut old = None;
        for event in schedule {
            match event {
                Event::Clear => {
                    let outcome = public.current.clear();
                    assert!(matches!(
                        outcome,
                        ClearOutcome::PersistentRejectionPreserved | ClearOutcome::AlreadyEmpty
                    ));
                }
                Event::Replace => {
                    old = Some(
                        public
                            .replace_source(SourceIdentity(22), SourceKind::StaticBearer)
                            .unwrap(),
                    );
                }
            }
            assert!(public.current.invariant());
        }
        let old = old.expect("replacement returns isolated old slot");
        assert!(matches!(
            old.state,
            SlotState::Rejected {
                exact_version: Version(1)
            }
        ));
        assert!(old.invariant());
        assert_eq!(public.current.identity, SlotIdentity(2));
        assert!(matches!(public.current.state, SlotState::Empty));
    }
}

#[test]
fn auth_model_systematic_delayed_observation_across_two_cohorts_bound_four_schedules() {
    #[derive(Clone, Copy)]
    enum FirstTerminal {
        Success,
        Failure,
    }
    #[derive(Clone, Copy)]
    enum Event {
        CompleteSecond,
        ObserveFirst,
    }

    let mut explored = 0;
    for first_terminal in [FirstTerminal::Success, FirstTerminal::Failure] {
        for schedule in permutations(&[Event::CompleteSecond, Event::ObserveFirst]) {
            explored += 1;
            let mut slot = empty_provider_slot();
            let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), LogicalTime(0))
            else {
                panic!("first leader");
            };
            let PrepareDecision::Joined(delayed) =
                slot.preflight(WaiterIdentity(2), LogicalTime(0))
            else {
                panic!("delayed waiter");
            };
            let expected = match first_terminal {
                FirstTerminal::Success => {
                    let prepared = slot
                        .provider_succeeds(first, MaterialIdentity(101), LogicalTime(100))
                        .unwrap()
                        .unwrap();
                    assert_eq!(slot.clear(), ClearOutcome::Cleared);
                    CohortTerminal::Success(super::model::ReadyValue {
                        slot: prepared.slot,
                        source: prepared.source,
                        version: prepared.version,
                        material: prepared.material,
                        expires_at: prepared.expires_at,
                    })
                }
                FirstTerminal::Failure => {
                    slot.provider_fails(first, ErrorIdentity(9), LogicalTime(0), None)
                        .unwrap();
                    CohortTerminal::Failure(ModelError::Provider(ErrorIdentity(9)))
                }
            };
            let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(3), LogicalTime(0))
            else {
                panic!("second leader");
            };
            for event in schedule {
                match event {
                    Event::CompleteSecond => {
                        slot.provider_succeeds(second, MaterialIdentity(202), LogicalTime(100))
                            .unwrap()
                            .unwrap();
                    }
                    Event::ObserveFirst => assert_eq!(delayed.observe(), Some(expected)),
                }
                assert!(slot.invariant());
            }
            assert_eq!(delayed.observe(), Some(expected));
            assert_eq!(delayed.publication_count(), 1);
        }
    }
    assert_eq!(explored, 4);
}

#[test]
fn auth_model_systematic_terminal_overflow_and_progress_bound_two_schedules() {
    #[derive(Clone, Copy)]
    enum Event {
        CompleteLater,
        ObserveOverflow,
    }

    let schedules = permutations(&[Event::CompleteLater, Event::ObserveOverflow]);
    assert_eq!(schedules.len(), 2);
    for schedule in schedules {
        let mut slot = empty_provider_slot();
        let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), LogicalTime(0))
        else {
            panic!("first leader");
        };
        let PrepareDecision::Joined(delayed) = slot.preflight(WaiterIdentity(2), LogicalTime(0))
        else {
            panic!("waiter");
        };
        assert_eq!(
            slot.provider_fails(first, ErrorIdentity(9), LogicalTime(u64::MAX), Some(1),),
            Err(ModelError::Overflow)
        );
        assert!(matches!(slot.state, SlotState::Empty));
        let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(3), LogicalTime(0))
        else {
            panic!("later progress");
        };
        for event in schedule {
            match event {
                Event::CompleteLater => {
                    slot.provider_succeeds(second, MaterialIdentity(202), LogicalTime(100))
                        .unwrap()
                        .unwrap();
                }
                Event::ObserveOverflow => assert_eq!(
                    delayed.observe(),
                    Some(CohortTerminal::Failure(ModelError::Overflow))
                ),
            }
            assert!(slot.invariant());
        }
        assert_eq!(
            delayed.observe(),
            Some(CohortTerminal::Failure(ModelError::Overflow))
        );
        assert!(!matches!(slot.state, SlotState::InFlight(_)));
    }
}
