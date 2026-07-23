use super::challenge::{
    AppliedCredential, AuthPlan, ChallengeAction, ChallengePolicy, Placement, PolicySetting,
    apply_attributed_challenge, resolve_policy,
};
use super::model::{
    AttemptCache, CohortTerminal, ErrorIdentity, LogicalTime, MaterialIdentity, ModelError,
    PlacementIdentity, PublicSlotRef, Slot, SlotIdentity, SlotState, SourceIdentity, SourceKind,
    Version, WaiterIdentity,
};
use super::transition::{
    ClearOutcome, ExecutionModel, ExecutionPhase, FinalizeDecision, PrepareDecision,
};

const NOW: LogicalTime = LogicalTime(10);
const LATER: LogicalTime = LogicalTime(20);
const FAR: LogicalTime = LogicalTime(100);

fn empty(kind: SourceKind) -> Slot {
    Slot::empty(SlotIdentity(1), SourceIdentity(11), kind)
}

fn ready(kind: SourceKind, version: u64, material: u64, expiry: u64) -> Slot {
    Slot::ready(
        SlotIdentity(1),
        SourceIdentity(11),
        kind,
        Version(version),
        MaterialIdentity(material),
        LogicalTime(expiry),
    )
}

fn prepared(slot: &mut Slot, waiter: u64, now: LogicalTime) -> super::model::PreparedCredential {
    let PrepareDecision::Ready(prepared) = slot.preflight(WaiterIdentity(waiter), now) else {
        panic!("expected ready preflight");
    };
    prepared
}

fn placement(identity: u64, slot: u64, source: SourceKind, setting: PolicySetting) -> Placement {
    Placement {
        identity: PlacementIdentity(identity),
        slot: SlotIdentity(slot),
        source,
        setting,
    }
}

#[test]
fn auth_model_static_source_challenge_defaults_are_never() {
    for source in [
        SourceKind::StaticApiKey,
        SourceKind::StaticBearer,
        SourceKind::StaticBasic,
    ] {
        assert_eq!(
            resolve_policy(source, PolicySetting::Omitted),
            Ok(ChallengePolicy::Never)
        );
    }
}

#[test]
fn auth_model_endpoint_backed_challenge_default_is_never() {
    assert_eq!(
        resolve_policy(SourceKind::EndpointBacked, PolicySetting::Omitted),
        Ok(ChallengePolicy::Never)
    );
}

#[test]
fn auth_model_refreshable_challenge_default_is_unauthorized() {
    assert_eq!(
        resolve_policy(SourceKind::RefreshableBearer, PolicySetting::Omitted),
        Ok(ChallengePolicy::Unauthorized)
    );
}

#[test]
fn auth_model_custom_provider_requires_explicit_policy() {
    assert_eq!(
        resolve_policy(SourceKind::CustomProvider, PolicySetting::Omitted),
        Err(ModelError::InvalidPolicy)
    );
    assert_eq!(
        resolve_policy(
            SourceKind::CustomProvider,
            PolicySetting::Explicit(ChallengePolicy::Never)
        ),
        Ok(ChallengePolicy::Never)
    );
}

#[test]
fn auth_model_rejects_two_challenge_enabled_placements() {
    let plan = AuthPlan {
        placements: vec![
            placement(
                1,
                1,
                SourceKind::StaticApiKey,
                PolicySetting::Explicit(ChallengePolicy::Unauthorized),
            ),
            placement(2, 2, SourceKind::RefreshableBearer, PolicySetting::Omitted),
        ],
    };
    assert_eq!(plan.validate(), Err(ModelError::InvalidPlan));
}

#[test]
fn auth_model_challenge_status_classification_is_exact() {
    assert!(ChallengePolicy::Unauthorized.classifies(401));
    assert!(!ChallengePolicy::Unauthorized.classifies(403));
    assert!(ChallengePolicy::UnauthorizedOrForbidden.classifies(401));
    assert!(ChallengePolicy::UnauthorizedOrForbidden.classifies(403));
    assert!(!ChallengePolicy::UnauthorizedOrForbidden.classifies(400));
    assert!(!ChallengePolicy::Never.classifies(401));
    assert!(!ChallengePolicy::Never.classifies(403));
}

#[test]
fn auth_model_single_attribution_preserves_unrelated_credential() {
    let mut slots = vec![
        ready(SourceKind::StaticBearer, 1, 101, 100),
        Slot::ready(
            SlotIdentity(2),
            SourceIdentity(12),
            SourceKind::RefreshableBearer,
            Version(7),
            MaterialIdentity(202),
            FAR,
        ),
    ];
    let unrelated = slots[1].clone();
    let plan = AuthPlan {
        placements: vec![
            placement(
                1,
                1,
                SourceKind::StaticBearer,
                PolicySetting::Explicit(ChallengePolicy::Unauthorized),
            ),
            placement(
                2,
                2,
                SourceKind::RefreshableBearer,
                PolicySetting::Explicit(ChallengePolicy::Never),
            ),
        ],
    };
    let action = apply_attributed_challenge(
        &plan,
        401,
        AppliedCredential {
            placement: PlacementIdentity(1),
            slot: SlotIdentity(1),
            version: Version(1),
            material: MaterialIdentity(101),
        },
        &mut slots,
        WaiterIdentity(1),
    );
    assert_eq!(action, Ok(ChallengeAction::StaticRejected));
    assert_eq!(slots[1], unrelated);
    assert_eq!(slots[1].provider_operations, 0);
}

#[test]
fn auth_model_plan_without_challenger_mutates_nothing() {
    let mut slots = vec![ready(SourceKind::StaticApiKey, 1, 101, 100)];
    let before = slots.clone();
    let plan = AuthPlan {
        placements: vec![placement(
            1,
            1,
            SourceKind::StaticApiKey,
            PolicySetting::Omitted,
        )],
    };
    let action = apply_attributed_challenge(
        &plan,
        401,
        AppliedCredential {
            placement: PlacementIdentity(1),
            slot: SlotIdentity(1),
            version: Version(1),
            material: MaterialIdentity(101),
        },
        &mut slots,
        WaiterIdentity(1),
    );
    assert_eq!(action, Ok(ChallengeAction::None));
    assert_eq!(slots, before);
}

#[test]
fn auth_model_one_provider_operation_and_one_cohort_result() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("first caller must lead");
    };
    let leader = slot
        .operation_subscription(ticket, WaiterIdentity(1))
        .unwrap();
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("second caller must join");
    };
    assert_eq!(waiter.identity(), ticket.cohort);
    let result = slot
        .provider_succeeds(ticket, MaterialIdentity(101), FAR)
        .expect("checked counters")
        .expect("current completion");
    let expected = CohortTerminal::Success(super::model::ReadyValue {
        slot: SlotIdentity(1),
        source: SourceIdentity(11),
        version: result.version,
        material: result.material,
        expires_at: FAR,
    });
    assert_eq!(slot.provider_operations, 1);
    assert_eq!(leader.publication_count(), 1);
    assert_eq!(waiter.publication_count(), 1);
    assert_eq!(leader.observe(), Some(expected));
    assert_eq!(waiter.observe(), Some(expected));
    assert!(slot.invariant());
}

#[test]
fn auth_model_attempt_cache_reuses_one_prepared_result_for_same_slot() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let first = slot
        .provider_succeeds(ticket, MaterialIdentity(101), FAR)
        .unwrap()
        .unwrap();
    let mut cache = AttemptCache::with_capacity(2);
    cache.remember(first).unwrap();
    let second_placement = cache.get(slot.identity).expect("attempt-local reuse");
    assert_eq!(second_placement, first);
    assert_eq!(slot.provider_operations, 1);
}

#[test]
fn auth_model_no_cooldown_failure_is_shared_but_later_call_may_lead() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let leader = slot
        .operation_subscription(ticket, WaiterIdentity(1))
        .unwrap();
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("waiter joins");
    };
    assert_eq!(waiter.identity(), ticket.cohort);
    assert_eq!(
        slot.provider_fails(ticket, ErrorIdentity(9), NOW, None),
        Ok(true)
    );
    let terminal = Some(CohortTerminal::Failure(ModelError::Provider(
        ErrorIdentity(9),
    )));
    assert_eq!(leader.observe(), terminal);
    assert_eq!(waiter.observe(), terminal);
    assert!(matches!(
        slot.preflight(WaiterIdentity(3), NOW),
        PrepareDecision::Leader(_)
    ));
    assert_eq!(slot.provider_operations, 2);
}

#[test]
fn auth_model_manual_set_wins_over_stale_completion() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let manual = slot.manual_set(MaterialIdentity(202), FAR).unwrap();
    assert_eq!(
        slot.provider_succeeds(ticket, MaterialIdentity(101), FAR),
        Ok(None)
    );
    assert_eq!(slot.discarded_completions, 1);
    assert_eq!(prepared(&mut slot, 2, NOW), manual);
}

#[test]
fn auth_model_clear_wins_over_stale_completion() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    slot.clear();
    assert_eq!(
        slot.provider_succeeds(ticket, MaterialIdentity(101), FAR),
        Ok(None)
    );
    assert!(matches!(slot.state, SlotState::Empty));
    assert_eq!(slot.discarded_completions, 1);
}

#[test]
fn auth_model_source_replacement_physically_isolates_old_completion() {
    let mut public = PublicSlotRef::new(empty(SourceKind::CustomProvider));
    let PrepareDecision::Leader(ticket) = public.current.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let mut old = public
        .replace_source(SourceIdentity(22), SourceKind::CustomProvider)
        .unwrap();
    assert_eq!(
        old.provider_succeeds(ticket, MaterialIdentity(101), FAR)
            .unwrap()
            .unwrap()
            .slot,
        SlotIdentity(1)
    );
    assert_eq!(public.current.identity, SlotIdentity(2));
    assert!(matches!(public.current.state, SlotState::Empty));
    assert_eq!(public.current.provider_operations, 0);
}

#[test]
fn auth_model_leader_cancellation_wakes_waiters_and_permits_progress() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("waiter joins");
    };
    assert_eq!(waiter.identity(), first.cohort);
    assert_eq!(slot.cancel_leader(first), Ok(true));
    assert_eq!(waiter.observe(), Some(CohortTerminal::Canceled));
    let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("reevaluating waiter becomes leader");
    };
    assert_ne!(first.operation, second.operation);
    assert!(
        slot.provider_succeeds(second, MaterialIdentity(303), FAR)
            .unwrap()
            .is_some()
    );
    assert_eq!(slot.provider_operations, 2);
}

#[test]
fn auth_model_waiter_cancellation_does_not_cancel_leader() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("waiter joins");
    };
    let leader = slot
        .operation_subscription(ticket, WaiterIdentity(1))
        .unwrap();
    assert!(slot.cancel_waiter(WaiterIdentity(2)));
    assert!(matches!(slot.state, SlotState::InFlight(_)));
    assert!(
        slot.provider_succeeds(ticket, MaterialIdentity(101), FAR)
            .unwrap()
            .is_some()
    );
    assert!(waiter.observe().is_none());
    assert!(leader.observe().is_some());
}

#[test]
fn auth_model_exact_rejection_prevents_previous_restoration() {
    let mut slot = ready(SourceKind::RefreshableBearer, 4, 404, 9);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("expired credential refreshes");
    };
    assert_eq!(ticket.refresh_input, Some(MaterialIdentity(404)));
    assert!(slot.reject_exact(Version(4), false));
    assert_eq!(slot.cancel_leader(ticket), Ok(true));
    assert!(matches!(slot.state, SlotState::Empty));
}

#[test]
fn auth_model_static_rejection_is_persistent_without_provider_call() {
    let mut slots = vec![ready(SourceKind::StaticBasic, 1, 101, 100)];
    let plan = AuthPlan {
        placements: vec![placement(
            1,
            1,
            SourceKind::StaticBasic,
            PolicySetting::Explicit(ChallengePolicy::Unauthorized),
        )],
    };
    assert_eq!(
        apply_attributed_challenge(
            &plan,
            401,
            AppliedCredential {
                placement: PlacementIdentity(1),
                slot: SlotIdentity(1),
                version: Version(1),
                material: MaterialIdentity(101),
            },
            &mut slots,
            WaiterIdentity(1),
        ),
        Ok(ChallengeAction::StaticRejected)
    );
    assert_eq!(
        slots[0].preflight(WaiterIdentity(2), NOW),
        PrepareDecision::Failed(ModelError::AuthenticationRejected)
    );
    assert_eq!(slots[0].provider_operations, 0);
}

#[test]
fn auth_model_only_source_replacement_clears_rejected_slot() {
    let mut slot = ready(SourceKind::StaticApiKey, 1, 101, 100);
    assert!(slot.reject_exact(Version(1), true));
    let mut public = PublicSlotRef::new(slot);
    assert_eq!(
        public.current.clear(),
        ClearOutcome::PersistentRejectionPreserved
    );
    assert_eq!(
        public.current.manual_set(MaterialIdentity(202), FAR),
        Err(ModelError::AuthenticationRejected)
    );
    let old = public
        .replace_source(SourceIdentity(22), SourceKind::StaticApiKey)
        .unwrap();
    assert!(matches!(
        old.state,
        SlotState::Rejected {
            exact_version: Version(1)
        }
    ));
    assert_eq!(public.current.identity, SlotIdentity(2));
    assert!(matches!(public.current.state, SlotState::Empty));
}

#[test]
fn auth_model_clear_cannot_remove_persistent_static_rejection() {
    let mut slots = vec![ready(SourceKind::StaticBearer, 1, 101, 100)];
    let plan = AuthPlan {
        placements: vec![placement(
            1,
            1,
            SourceKind::StaticBearer,
            PolicySetting::Explicit(ChallengePolicy::Unauthorized),
        )],
    };
    assert_eq!(
        apply_attributed_challenge(
            &plan,
            401,
            AppliedCredential {
                placement: PlacementIdentity(1),
                slot: SlotIdentity(1),
                version: Version(1),
                material: MaterialIdentity(101),
            },
            &mut slots,
            WaiterIdentity(1),
        ),
        Ok(ChallengeAction::StaticRejected)
    );
    assert_eq!(slots[0].clear(), ClearOutcome::PersistentRejectionPreserved);
    assert!(matches!(
        slots[0].state,
        SlotState::Rejected {
            exact_version: Version(1)
        }
    ));
    assert_eq!(
        slots[0].preflight(WaiterIdentity(2), NOW),
        PrepareDecision::Failed(ModelError::AuthenticationRejected)
    );
    assert_eq!(slots[0].provider_operations, 0);
    assert!(slots[0].invariant());
}

#[test]
fn auth_model_delayed_waiter_keeps_success_after_later_cohort() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("first leader");
    };
    let PrepareDecision::Joined(delayed) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("first waiter");
    };
    let first_ready = slot
        .provider_succeeds(first, MaterialIdentity(101), FAR)
        .unwrap()
        .unwrap();
    assert_eq!(slot.clear(), ClearOutcome::Cleared);
    let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(3), NOW) else {
        panic!("second leader");
    };
    slot.provider_succeeds(second, MaterialIdentity(202), FAR)
        .unwrap()
        .unwrap();
    assert_eq!(slot.provider_operations, 2);
    assert_eq!(delayed.publication_count(), 1);
    assert_eq!(
        delayed.observe(),
        Some(CohortTerminal::Success(super::model::ReadyValue {
            slot: first_ready.slot,
            source: first_ready.source,
            version: first_ready.version,
            material: MaterialIdentity(101),
            expires_at: FAR,
        }))
    );
}

#[test]
fn auth_model_delayed_waiter_keeps_failure_after_later_cohort() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("first leader");
    };
    let PrepareDecision::Joined(delayed) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("first waiter");
    };
    slot.provider_fails(first, ErrorIdentity(9), NOW, None)
        .unwrap();
    let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(3), NOW) else {
        panic!("second leader");
    };
    slot.provider_succeeds(second, MaterialIdentity(202), FAR)
        .unwrap()
        .unwrap();
    assert_eq!(slot.provider_operations, 2);
    assert_eq!(delayed.publication_count(), 1);
    assert_eq!(
        delayed.observe(),
        Some(CohortTerminal::Failure(ModelError::Provider(
            ErrorIdentity(9)
        )))
    );
}

#[test]
fn auth_model_provider_success_version_overflow_wakes_cohort() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let leader = slot
        .operation_subscription(ticket, WaiterIdentity(1))
        .unwrap();
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("waiter");
    };
    slot.next_version = u64::MAX;
    assert_eq!(
        slot.provider_succeeds(ticket, MaterialIdentity(101), FAR),
        Err(ModelError::Overflow)
    );
    assert!(!matches!(
        slot.state,
        SlotState::InFlight(_) | SlotState::Ready(_)
    ));
    assert_eq!(
        leader.observe(),
        Some(CohortTerminal::Failure(ModelError::Overflow))
    );
    assert_eq!(
        waiter.observe(),
        Some(CohortTerminal::Failure(ModelError::Overflow))
    );
    assert_eq!(waiter.publication_count(), 1);
    assert!(slot.invariant());

    let mut public = PublicSlotRef::new(slot);
    let old = public
        .replace_source(SourceIdentity(22), SourceKind::CustomProvider)
        .unwrap();
    assert!(!matches!(
        old.state,
        SlotState::InFlight(_) | SlotState::Ready(_)
    ));
    assert!(matches!(
        public.current.preflight(WaiterIdentity(3), NOW),
        PrepareDecision::Leader(_)
    ));
}

#[test]
fn auth_model_provider_failure_retry_overflow_wakes_cohort() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(first) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let leader = slot
        .operation_subscription(first, WaiterIdentity(1))
        .unwrap();
    let PrepareDecision::Joined(waiter) = slot.preflight(WaiterIdentity(2), NOW) else {
        panic!("waiter");
    };
    assert_eq!(
        slot.provider_fails(first, ErrorIdentity(9), LogicalTime(u64::MAX - 1), Some(2)),
        Err(ModelError::Overflow)
    );
    assert!(matches!(slot.state, SlotState::Empty));
    assert_eq!(
        leader.observe(),
        Some(CohortTerminal::Failure(ModelError::Overflow))
    );
    assert_eq!(
        waiter.observe(),
        Some(CohortTerminal::Failure(ModelError::Overflow))
    );

    let PrepareDecision::Leader(second) = slot.preflight(WaiterIdentity(3), NOW) else {
        panic!("later caller progresses");
    };
    assert!(
        slot.provider_succeeds(second, MaterialIdentity(202), FAR)
            .unwrap()
            .is_some()
    );
    assert_eq!(slot.provider_operations, 2);
    assert!(slot.invariant());
}

#[test]
fn auth_model_terminal_overflow_leaves_no_inflight_state() {
    let mut success = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(success_ticket) = success.preflight(WaiterIdentity(1), NOW) else {
        panic!("success leader");
    };
    success.next_version = u64::MAX;
    assert_eq!(
        success.provider_succeeds(success_ticket, MaterialIdentity(101), FAR),
        Err(ModelError::Overflow)
    );
    assert!(!matches!(success.state, SlotState::InFlight(_)));

    let mut failure = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(failure_ticket) = failure.preflight(WaiterIdentity(1), NOW) else {
        panic!("failure leader");
    };
    assert_eq!(
        failure.provider_fails(
            failure_ticket,
            ErrorIdentity(9),
            LogicalTime(u64::MAX),
            Some(1)
        ),
        Err(ModelError::Overflow)
    );
    assert!(!matches!(failure.state, SlotState::InFlight(_)));
    assert!(success.invariant());
    assert!(failure.invariant());
}

#[test]
fn auth_model_strengthened_invariant_rejects_inconsistent_states() {
    let mut version_regression = ready(SourceKind::StaticBearer, 2, 101, 100);
    version_regression.next_version = 2;
    assert!(!version_regression.invariant());

    let mut ineligible_rejection = ready(SourceKind::CustomProvider, 1, 101, 100);
    ineligible_rejection.state = SlotState::Rejected {
        exact_version: Version(1),
    };
    assert!(!ineligible_rejection.invariant());

    let mut mismatched = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(_) = mismatched.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    let SlotState::InFlight(active) = &mut mismatched.state else {
        panic!("in flight");
    };
    active.ticket.cohort = super::model::CohortIdentity(999);
    assert!(!mismatched.invariant());
}

#[test]
fn auth_model_replacement_before_finalization_can_acquire_from_new_slot() {
    let slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let mut public = PublicSlotRef::new(slot);
    let candidate = prepared(&mut public.current, 1, NOW);
    public
        .replace_source(SourceIdentity(22), SourceKind::CustomProvider)
        .unwrap();
    let FinalizeDecision::Leader(ticket) = public.finalize(candidate, WaiterIdentity(2), LATER)
    else {
        panic!("replacement provider acquisition");
    };
    assert_eq!(ticket.slot, SlotIdentity(2));
    assert_eq!(public.current.provider_operations, 1);
}

#[test]
fn auth_model_cooldown_blocks_provider_until_logical_retry_instant() {
    let mut slot = empty(SourceKind::CustomProvider);
    let PrepareDecision::Leader(ticket) = slot.preflight(WaiterIdentity(1), NOW) else {
        panic!("leader");
    };
    slot.provider_fails(ticket, ErrorIdentity(9), NOW, Some(5))
        .unwrap();
    assert_eq!(
        slot.preflight(WaiterIdentity(2), LogicalTime(14)),
        PrepareDecision::Failed(ModelError::Cooldown(ErrorIdentity(9)))
    );
    assert_eq!(slot.provider_operations, 1);
    assert!(matches!(
        slot.preflight(WaiterIdentity(3), LogicalTime(15)),
        PrepareDecision::Leader(_)
    ));
    assert_eq!(slot.provider_operations, 2);
}

#[test]
fn auth_model_endpoint_challenge_clears_without_reacquisition() {
    let mut slots = vec![ready(SourceKind::EndpointBacked, 1, 101, 100)];
    let plan = AuthPlan {
        placements: vec![placement(
            1,
            1,
            SourceKind::EndpointBacked,
            PolicySetting::Explicit(ChallengePolicy::Unauthorized),
        )],
    };
    assert_eq!(
        apply_attributed_challenge(
            &plan,
            401,
            AppliedCredential {
                placement: PlacementIdentity(1),
                slot: SlotIdentity(1),
                version: Version(1),
                material: MaterialIdentity(101),
            },
            &mut slots,
            WaiterIdentity(1),
        ),
        Ok(ChallengeAction::EndpointCleared)
    );
    assert!(matches!(slots[0].state, SlotState::Empty));
    assert_eq!(slots[0].provider_operations, 0);
}

#[test]
fn auth_model_refresh_challenge_uses_exact_input_and_preserves_newer_value() {
    let mut slots = vec![ready(SourceKind::RefreshableBearer, 3, 303, 100)];
    let plan = AuthPlan {
        placements: vec![placement(
            1,
            1,
            SourceKind::RefreshableBearer,
            PolicySetting::Omitted,
        )],
    };
    let action = apply_attributed_challenge(
        &plan,
        401,
        AppliedCredential {
            placement: PlacementIdentity(1),
            slot: SlotIdentity(1),
            version: Version(3),
            material: MaterialIdentity(303),
        },
        &mut slots,
        WaiterIdentity(1),
    )
    .unwrap();
    let ChallengeAction::Refresh(PrepareDecision::Leader(ticket)) = action else {
        panic!("exact challenged value starts refresh");
    };
    assert_eq!(ticket.refresh_input, Some(MaterialIdentity(303)));

    let mut newer = ready(SourceKind::RefreshableBearer, 4, 404, 100);
    assert_eq!(
        newer.begin_exact_refresh(Version(3), WaiterIdentity(2)),
        Ok(PrepareDecision::Ready(super::model::PreparedCredential {
            slot: SlotIdentity(1),
            source: SourceIdentity(11),
            version: Version(4),
            material: MaterialIdentity(404),
            expires_at: FAR,
        }))
    );
    assert_eq!(newer.provider_operations, 0);
}

#[test]
fn auth_model_preflight_precedes_limiter_wait_and_reservation() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let prepared = prepared(&mut slot, 1, NOW);
    let mut execution = ExecutionModel::new();
    assert_eq!(
        execution.begin_limiter_wait(),
        Err(ModelError::InvalidTransition)
    );
    execution.preflight_complete(prepared).unwrap();
    execution.begin_limiter_wait().unwrap();
    assert_eq!(execution.reservations, 0);
    execution.grant_reservation().unwrap();
    assert_eq!(execution.reservations, 1);
}

#[test]
fn auth_model_preflight_failure_has_no_limiter_reservation_or_send() {
    let mut rejected = ready(SourceKind::StaticBearer, 1, 101, 100);
    assert!(rejected.reject_exact(Version(1), true));
    assert_eq!(
        rejected.preflight(WaiterIdentity(1), NOW),
        PrepareDecision::Failed(ModelError::AuthenticationRejected)
    );
    let execution = ExecutionModel::new();
    assert_eq!(execution.reservations, 0);
    assert_eq!(execution.sends, 0);
    assert_eq!(execution.phase, ExecutionPhase::Initial);
}

fn reserved_execution(slot: &mut Slot) -> (ExecutionModel, super::model::PreparedCredential) {
    let prepared = prepared(slot, 1, NOW);
    let mut execution = ExecutionModel::new();
    execution.preflight_complete(prepared).unwrap();
    execution.begin_limiter_wait().unwrap();
    execution.grant_reservation().unwrap();
    (execution, prepared)
}

#[test]
fn auth_model_clear_during_limiter_wait_prevents_stale_finalization() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let (mut execution, candidate) = reserved_execution(&mut slot);
    slot.clear();
    let decision = slot.finalize(candidate, WaiterIdentity(2), LATER);
    assert_eq!(decision, FinalizeDecision::Failed(ModelError::Unavailable));
    assert_eq!(execution.record_finalization(decision), Ok(None));
    assert_eq!(execution.sends, 0);
    assert_eq!(execution.reservations, 1);
}

#[test]
fn auth_model_source_replacement_during_wait_prevents_old_slot_finalization() {
    let slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let mut public = PublicSlotRef::new(slot);
    let (mut execution, candidate) = reserved_execution(&mut public.current);
    let old = public
        .replace_source(SourceIdentity(22), SourceKind::StaticBearer)
        .unwrap();
    let decision = public.finalize(candidate, WaiterIdentity(2), LATER);
    assert_eq!(decision, FinalizeDecision::Failed(ModelError::Unavailable));
    assert_eq!(execution.record_finalization(decision), Ok(None));
    assert_eq!(old.identity, SlotIdentity(1));
    assert_eq!(public.current.identity, SlotIdentity(2));
    assert_eq!(execution.sends, 0);
}

#[test]
fn auth_model_rejection_during_wait_prevents_stale_finalization() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let (mut execution, candidate) = reserved_execution(&mut slot);
    assert!(slot.reject_exact(Version(1), true));
    let decision = slot.finalize(candidate, WaiterIdentity(2), LATER);
    assert_eq!(
        decision,
        FinalizeDecision::Failed(ModelError::AuthenticationRejected)
    );
    assert_eq!(execution.record_finalization(decision), Ok(None));
    assert_eq!(execution.reservations, 1);
    assert_eq!(execution.sends, 0);
}

#[test]
fn auth_model_expiry_during_wait_refreshes_or_fails_closed() {
    let mut refreshable = ready(SourceKind::RefreshableBearer, 1, 101, 15);
    let (_, candidate) = reserved_execution(&mut refreshable);
    let FinalizeDecision::Leader(ticket) =
        refreshable.finalize(candidate, WaiterIdentity(2), LATER)
    else {
        panic!("expired refreshable begins refresh");
    };
    assert_eq!(ticket.refresh_input, Some(MaterialIdentity(101)));
    assert_eq!(refreshable.provider_operations, 1);
    refreshable
        .provider_succeeds(ticket, MaterialIdentity(202), FAR)
        .unwrap()
        .unwrap();
    assert!(matches!(
        refreshable.finalize(candidate, WaiterIdentity(2), LATER),
        FinalizeDecision::Final(finalized)
            if finalized.version == Version(2) && finalized.material == MaterialIdentity(202)
    ));

    let mut static_slot = ready(SourceKind::StaticBearer, 1, 101, 15);
    let (_, static_candidate) = reserved_execution(&mut static_slot);
    assert_eq!(
        static_slot.finalize(static_candidate, WaiterIdentity(2), LATER),
        FinalizeDecision::Failed(ModelError::Expired)
    );
}

#[test]
fn auth_model_newer_valid_version_replaces_preflight_version() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let (_, candidate) = reserved_execution(&mut slot);
    let newer = slot.manual_set(MaterialIdentity(202), FAR).unwrap();
    let FinalizeDecision::Final(finalized) = slot.finalize(candidate, WaiterIdentity(2), LATER)
    else {
        panic!("newer ready value finalizes");
    };
    assert_eq!(finalized.version, newer.version);
    assert_eq!(finalized.material, MaterialIdentity(202));
    assert_ne!(finalized.material, candidate.material);
}

#[test]
fn auth_model_finalization_failure_produces_no_send_and_retains_reservation() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let (mut execution, candidate) = reserved_execution(&mut slot);
    slot.clear();
    let decision = slot.finalize(candidate, WaiterIdentity(2), LATER);
    execution.record_finalization(decision).unwrap();
    assert!(matches!(
        execution.phase,
        ExecutionPhase::FinalizationFailed(ModelError::Unavailable)
    ));
    assert_eq!(execution.sends, 0);
    assert_eq!(execution.reservations, 1);
}

#[test]
fn auth_model_post_finalization_clear_cannot_retract_request_local_material() {
    let mut slot = ready(SourceKind::StaticBearer, 1, 101, 100);
    let (mut execution, candidate) = reserved_execution(&mut slot);
    let decision = slot.finalize(candidate, WaiterIdentity(2), LATER);
    let finalized = execution
        .record_finalization(decision)
        .unwrap()
        .expect("final credential");
    slot.clear();
    execution.send(finalized).unwrap();
    assert_eq!(execution.sends, 1);
    assert_eq!(finalized.material, MaterialIdentity(101));
    assert!(matches!(slot.state, SlotState::Empty));
}

#[test]
fn auth_model_checked_counter_overflow_fails_closed() {
    let mut slot = empty(SourceKind::CustomProvider);
    slot.next_operation = u64::MAX;
    assert_eq!(
        slot.preflight(WaiterIdentity(1), NOW),
        PrepareDecision::Failed(ModelError::Overflow)
    );
    assert!(matches!(slot.state, SlotState::Empty));
    assert_eq!(slot.provider_operations, 0);
}
