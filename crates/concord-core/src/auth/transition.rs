use super::model::{
    Cohort, CohortIdentity, CohortSubscription, CohortTerminal, ErrorIdentity, FinalCredential,
    InFlight, LogicalTime, MaterialIdentity, ModelError, OperationIdentity, OperationKind,
    OperationTicket, PreparedCredential, PublicSlotRef, ReadyValue, Slot, SlotIdentity, SlotState,
    SourceIdentity, SourceKind, Version, WaiterIdentity,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PrepareDecision {
    Ready(PreparedCredential),
    Leader(OperationTicket),
    Joined(CohortSubscription),
    Failed(ModelError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FinalizeDecision {
    Final(FinalCredential),
    Leader(OperationTicket),
    Joined(CohortSubscription),
    Failed(ModelError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClearOutcome {
    Cleared,
    AlreadyEmpty,
    PersistentRejectionPreserved,
}

impl Slot {
    fn checked_increment(counter: &mut u64) -> Result<u64, ModelError> {
        let current = *counter;
        *counter = counter.checked_add(1).ok_or(ModelError::Overflow)?;
        Ok(current)
    }

    fn publish(active: &InFlight, terminal: CohortTerminal) {
        debug_assert!(active.cohort.publish(terminal));
    }

    fn begin_operation(
        &mut self,
        kind: OperationKind,
        previous: Option<ReadyValue>,
        previous_eligible: bool,
        leader: WaiterIdentity,
    ) -> Result<OperationTicket, ModelError> {
        let operation = OperationIdentity(Self::checked_increment(&mut self.next_operation)?);
        let cohort = CohortIdentity(Self::checked_increment(&mut self.next_cohort)?);
        Self::checked_increment(&mut self.provider_operations)?;
        let refresh_input = previous.map(|value| value.material);
        let ticket = OperationTicket {
            slot: self.identity,
            operation,
            cohort,
            kind,
            refresh_input,
        };
        let active = InFlight {
            ticket,
            previous,
            previous_eligible,
            cohort: Cohort::new(cohort, leader),
        };
        self.state = SlotState::InFlight(active);
        Ok(ticket)
    }

    pub(super) fn preflight(
        &mut self,
        waiter: WaiterIdentity,
        now: LogicalTime,
    ) -> PrepareDecision {
        match self.state.clone() {
            SlotState::Ready(value) if value.valid_at(now) => PrepareDecision::Ready(value.into()),
            SlotState::Ready(value) if self.source_kind.can_refresh() => self
                .begin_operation(OperationKind::Refresh, Some(value), true, waiter)
                .map_or_else(PrepareDecision::Failed, PrepareDecision::Leader),
            SlotState::Ready(_) => PrepareDecision::Failed(ModelError::Expired),
            SlotState::Empty if self.source_kind.can_acquire() => self
                .begin_operation(OperationKind::Acquire, None, false, waiter)
                .map_or_else(PrepareDecision::Failed, PrepareDecision::Leader),
            SlotState::Empty => PrepareDecision::Failed(ModelError::Unavailable),
            SlotState::InFlight(active) => PrepareDecision::Joined(active.cohort.subscribe(waiter)),
            SlotState::Failed {
                error,
                retry_at: Some(retry_at),
            } if now < retry_at => PrepareDecision::Failed(ModelError::Cooldown(error)),
            SlotState::Failed { .. } if self.source_kind.can_acquire() => self
                .begin_operation(OperationKind::Acquire, None, false, waiter)
                .map_or_else(PrepareDecision::Failed, PrepareDecision::Leader),
            SlotState::Failed { .. } => PrepareDecision::Failed(ModelError::Unavailable),
            SlotState::Rejected { .. } => {
                PrepareDecision::Failed(ModelError::AuthenticationRejected)
            }
        }
    }

    pub(super) fn operation_subscription(
        &self,
        ticket: OperationTicket,
        waiter: WaiterIdentity,
    ) -> Option<CohortSubscription> {
        let SlotState::InFlight(active) = &self.state else {
            return None;
        };
        (active.ticket == ticket).then(|| active.cohort.subscribe(waiter))
    }

    pub(super) fn provider_succeeds(
        &mut self,
        ticket: OperationTicket,
        material: MaterialIdentity,
        expires_at: LogicalTime,
    ) -> Result<Option<PreparedCredential>, ModelError> {
        let SlotState::InFlight(active) = self.state.clone() else {
            Self::checked_increment(&mut self.discarded_completions)?;
            return Ok(None);
        };
        if active.ticket != ticket {
            Self::checked_increment(&mut self.discarded_completions)?;
            return Ok(None);
        }
        let version = match Self::checked_increment(&mut self.next_version) {
            Ok(version) => Version(version),
            Err(ModelError::Overflow) => {
                self.state = SlotState::Empty;
                Self::publish(&active, CohortTerminal::Failure(ModelError::Overflow));
                return Err(ModelError::Overflow);
            }
            Err(error) => return Err(error),
        };
        let ready = ReadyValue {
            slot: self.identity,
            source: self.source,
            version,
            material,
            expires_at,
        };
        self.state = SlotState::Ready(ready);
        Self::publish(&active, CohortTerminal::Success(ready));
        Ok(Some(ready.into()))
    }

    pub(super) fn provider_fails(
        &mut self,
        ticket: OperationTicket,
        error: ErrorIdentity,
        now: LogicalTime,
        cooldown: Option<u64>,
    ) -> Result<bool, ModelError> {
        let SlotState::InFlight(active) = self.state.clone() else {
            Self::checked_increment(&mut self.discarded_completions)?;
            return Ok(false);
        };
        if active.ticket != ticket {
            Self::checked_increment(&mut self.discarded_completions)?;
            return Ok(false);
        }
        let retry_at = match cooldown {
            Some(delta) if delta > 0 => match now.checked_add(delta) {
                Ok(retry_at) => Some(retry_at),
                Err(ModelError::Overflow) => {
                    self.state = SlotState::Empty;
                    Self::publish(&active, CohortTerminal::Failure(ModelError::Overflow));
                    return Err(ModelError::Overflow);
                }
                Err(error) => return Err(error),
            },
            Some(_) | None => None,
        };
        self.state = if retry_at.is_some() {
            SlotState::Failed { error, retry_at }
        } else {
            SlotState::Empty
        };
        Self::publish(
            &active,
            CohortTerminal::Failure(ModelError::Provider(error)),
        );
        Ok(true)
    }

    pub(super) fn cancel_leader(&mut self, ticket: OperationTicket) -> Result<bool, ModelError> {
        let SlotState::InFlight(active) = self.state.clone() else {
            return Ok(false);
        };
        if active.ticket != ticket {
            return Ok(false);
        }
        self.state = match (active.previous, active.previous_eligible) {
            (Some(previous), true) => SlotState::Ready(previous),
            (Some(_) | None, false) | (None, true) => SlotState::Empty,
        };
        Self::publish(&active, CohortTerminal::Canceled);
        Ok(true)
    }

    pub(super) fn cancel_waiter(&mut self, waiter: WaiterIdentity) -> bool {
        let SlotState::InFlight(active) = self.state.clone() else {
            return false;
        };
        active.cohort.cancel(waiter)
    }

    pub(super) fn manual_set(
        &mut self,
        material: MaterialIdentity,
        expires_at: LogicalTime,
    ) -> Result<PreparedCredential, ModelError> {
        if matches!(self.state, SlotState::Rejected { .. }) && self.source_kind.is_static() {
            return Err(ModelError::AuthenticationRejected);
        }
        let superseded = match self.state.clone() {
            SlotState::InFlight(active) => Some(active),
            SlotState::Empty
            | SlotState::Ready(_)
            | SlotState::Failed { .. }
            | SlotState::Rejected { .. } => None,
        };
        let version = Version(Self::checked_increment(&mut self.next_version)?);
        let ready = ReadyValue {
            slot: self.identity,
            source: self.source,
            version,
            material,
            expires_at,
        };
        self.state = SlotState::Ready(ready);
        if let Some(active) = superseded {
            Self::publish(&active, CohortTerminal::Superseded);
        }
        Ok(ready.into())
    }

    pub(super) fn clear(&mut self) -> ClearOutcome {
        if matches!(self.state, SlotState::Rejected { .. }) && self.source_kind.is_static() {
            return ClearOutcome::PersistentRejectionPreserved;
        }
        let superseded = match self.state.clone() {
            SlotState::InFlight(active) => Some(active),
            SlotState::Empty => return ClearOutcome::AlreadyEmpty,
            SlotState::Ready(_) | SlotState::Failed { .. } | SlotState::Rejected { .. } => None,
        };
        self.state = SlotState::Empty;
        if let Some(active) = superseded {
            Self::publish(&active, CohortTerminal::Superseded);
        }
        ClearOutcome::Cleared
    }

    pub(super) fn reject_exact(&mut self, version: Version, persistent: bool) -> bool {
        match self.state.clone() {
            SlotState::Ready(value) if value.version == version => {
                self.state = if persistent {
                    SlotState::Rejected {
                        exact_version: version,
                    }
                } else {
                    SlotState::Empty
                };
                true
            }
            SlotState::InFlight(mut active)
                if active
                    .previous
                    .is_some_and(|value| value.version == version) =>
            {
                active.previous_eligible = false;
                self.state = SlotState::InFlight(active);
                true
            }
            SlotState::Empty
            | SlotState::Ready(_)
            | SlotState::InFlight(_)
            | SlotState::Failed { .. }
            | SlotState::Rejected { .. } => false,
        }
    }

    pub(super) fn begin_exact_refresh(
        &mut self,
        version: Version,
        waiter: WaiterIdentity,
    ) -> Result<PrepareDecision, ModelError> {
        match self.state.clone() {
            SlotState::Ready(value) if value.version == version => self
                .begin_operation(OperationKind::Refresh, Some(value), false, waiter)
                .map(PrepareDecision::Leader),
            SlotState::Ready(value) if value.version > version => {
                Ok(PrepareDecision::Ready(value.into()))
            }
            SlotState::InFlight(mut active)
                if active
                    .previous
                    .is_some_and(|value| value.version == version) =>
            {
                active.previous_eligible = false;
                let subscription = active.cohort.subscribe(waiter);
                self.state = SlotState::InFlight(active);
                Ok(PrepareDecision::Joined(subscription))
            }
            SlotState::Rejected { .. } => {
                Ok(PrepareDecision::Failed(ModelError::AuthenticationRejected))
            }
            SlotState::Empty
            | SlotState::Ready(_)
            | SlotState::InFlight(_)
            | SlotState::Failed { .. } => Ok(PrepareDecision::Failed(ModelError::StalePrepared)),
        }
    }

    pub(super) fn finalize(
        &mut self,
        prepared: PreparedCredential,
        waiter: WaiterIdentity,
        now: LogicalTime,
    ) -> FinalizeDecision {
        if prepared.slot != self.identity {
            return FinalizeDecision::Failed(ModelError::StalePrepared);
        }
        match self.state.clone() {
            SlotState::Ready(value) if value.version >= prepared.version && value.valid_at(now) => {
                FinalizeDecision::Final(FinalCredential {
                    slot: value.slot,
                    version: value.version,
                    material: value.material,
                    finalized_at: now,
                })
            }
            SlotState::Ready(value) if self.source_kind.can_refresh() => self
                .begin_operation(OperationKind::Refresh, Some(value), true, waiter)
                .map_or_else(FinalizeDecision::Failed, FinalizeDecision::Leader),
            SlotState::Ready(_) => FinalizeDecision::Failed(ModelError::Expired),
            SlotState::Empty if self.source_kind.can_acquire() => self
                .begin_operation(OperationKind::Acquire, None, false, waiter)
                .map_or_else(FinalizeDecision::Failed, FinalizeDecision::Leader),
            SlotState::Empty => FinalizeDecision::Failed(ModelError::Unavailable),
            SlotState::InFlight(active) => {
                FinalizeDecision::Joined(active.cohort.subscribe(waiter))
            }
            SlotState::Failed {
                error,
                retry_at: Some(retry_at),
            } if now < retry_at => FinalizeDecision::Failed(ModelError::Cooldown(error)),
            SlotState::Failed { .. } if self.source_kind.can_acquire() => self
                .begin_operation(OperationKind::Acquire, None, false, waiter)
                .map_or_else(FinalizeDecision::Failed, FinalizeDecision::Leader),
            SlotState::Failed { .. } => FinalizeDecision::Failed(ModelError::Unavailable),
            SlotState::Rejected { .. } => {
                FinalizeDecision::Failed(ModelError::AuthenticationRejected)
            }
        }
    }
}

impl PublicSlotRef {
    pub(super) fn replace_source(
        &mut self,
        source: SourceIdentity,
        source_kind: SourceKind,
    ) -> Result<Slot, ModelError> {
        let identity = SlotIdentity(self.next_slot_identity);
        self.next_slot_identity = self
            .next_slot_identity
            .checked_add(1)
            .ok_or(ModelError::Overflow)?;
        let replacement = Slot::empty(identity, source, source_kind);
        Ok(std::mem::replace(&mut self.current, replacement))
    }

    pub(super) fn finalize(
        &mut self,
        prepared: PreparedCredential,
        waiter: WaiterIdentity,
        now: LogicalTime,
    ) -> FinalizeDecision {
        if self.current.identity != prepared.slot {
            return match self.current.preflight(waiter, now) {
                PrepareDecision::Ready(current) => self.current.finalize(current, waiter, now),
                PrepareDecision::Leader(ticket) => FinalizeDecision::Leader(ticket),
                PrepareDecision::Joined(cohort) => FinalizeDecision::Joined(cohort),
                PrepareDecision::Failed(error) => FinalizeDecision::Failed(error),
            };
        }
        self.current.finalize(prepared, waiter, now)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExecutionPhase {
    Initial,
    PreflightComplete(PreparedCredential),
    LimiterWaiting(PreparedCredential),
    Reserved(PreparedCredential),
    Finalized(FinalCredential),
    FinalizationFailed(ModelError),
    Sent(FinalCredential),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExecutionModel {
    pub(super) phase: ExecutionPhase,
    pub(super) reservations: u64,
    pub(super) sends: u64,
}

impl ExecutionModel {
    pub(super) fn new() -> Self {
        Self {
            phase: ExecutionPhase::Initial,
            reservations: 0,
            sends: 0,
        }
    }

    pub(super) fn preflight_complete(
        &mut self,
        prepared: PreparedCredential,
    ) -> Result<(), ModelError> {
        if self.phase != ExecutionPhase::Initial {
            return Err(ModelError::InvalidTransition);
        }
        self.phase = ExecutionPhase::PreflightComplete(prepared);
        Ok(())
    }

    pub(super) fn begin_limiter_wait(&mut self) -> Result<(), ModelError> {
        let ExecutionPhase::PreflightComplete(prepared) = self.phase else {
            return Err(ModelError::InvalidTransition);
        };
        self.phase = ExecutionPhase::LimiterWaiting(prepared);
        Ok(())
    }

    pub(super) fn grant_reservation(&mut self) -> Result<(), ModelError> {
        let ExecutionPhase::LimiterWaiting(prepared) = self.phase else {
            return Err(ModelError::InvalidTransition);
        };
        self.reservations = self
            .reservations
            .checked_add(1)
            .ok_or(ModelError::Overflow)?;
        self.phase = ExecutionPhase::Reserved(prepared);
        Ok(())
    }

    pub(super) fn prepared(&self) -> Result<PreparedCredential, ModelError> {
        match self.phase {
            ExecutionPhase::Reserved(prepared) => Ok(prepared),
            ExecutionPhase::Initial
            | ExecutionPhase::PreflightComplete(_)
            | ExecutionPhase::LimiterWaiting(_)
            | ExecutionPhase::Finalized(_)
            | ExecutionPhase::FinalizationFailed(_)
            | ExecutionPhase::Sent(_) => Err(ModelError::InvalidTransition),
        }
    }

    pub(super) fn record_finalization(
        &mut self,
        decision: FinalizeDecision,
    ) -> Result<Option<FinalCredential>, ModelError> {
        if !matches!(self.phase, ExecutionPhase::Reserved(_)) {
            return Err(ModelError::InvalidTransition);
        }
        match decision {
            FinalizeDecision::Final(finalized) => {
                self.phase = ExecutionPhase::Finalized(finalized);
                Ok(Some(finalized))
            }
            FinalizeDecision::Failed(error) => {
                self.phase = ExecutionPhase::FinalizationFailed(error);
                Ok(None)
            }
            FinalizeDecision::Leader(_) | FinalizeDecision::Joined(_) => {
                Err(ModelError::InvalidTransition)
            }
        }
    }

    pub(super) fn send(&mut self, credential: FinalCredential) -> Result<(), ModelError> {
        if self.phase != ExecutionPhase::Finalized(credential) {
            return Err(ModelError::InvalidTransition);
        }
        self.sends = self.sends.checked_add(1).ok_or(ModelError::Overflow)?;
        self.phase = ExecutionPhase::Sent(credential);
        Ok(())
    }
}
