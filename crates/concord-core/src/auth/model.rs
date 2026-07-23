use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub(super) struct $name(pub(super) u64);
    };
}

identity!(SlotIdentity);
identity!(SourceIdentity);
identity!(Version);
identity!(OperationIdentity);
identity!(CohortIdentity);
identity!(WaiterIdentity);
identity!(MaterialIdentity);
identity!(PlacementIdentity);
identity!(ErrorIdentity);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct LogicalTime(pub(super) u64);

impl LogicalTime {
    pub(super) fn checked_add(self, delta: u64) -> Result<Self, ModelError> {
        self.0
            .checked_add(delta)
            .map(Self)
            .ok_or(ModelError::Overflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceKind {
    StaticApiKey,
    StaticBearer,
    StaticBasic,
    RefreshableBearer,
    EndpointBacked,
    CustomProvider,
}

impl SourceKind {
    pub(super) fn is_static(self) -> bool {
        matches!(
            self,
            Self::StaticApiKey | Self::StaticBearer | Self::StaticBasic
        )
    }

    pub(super) fn can_acquire(self) -> bool {
        matches!(self, Self::RefreshableBearer | Self::CustomProvider)
    }

    pub(super) fn can_refresh(self) -> bool {
        matches!(self, Self::RefreshableBearer | Self::CustomProvider)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OperationKind {
    Acquire,
    Refresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelError {
    AuthenticationRejected,
    Cooldown(ErrorIdentity),
    Provider(ErrorIdentity),
    Unavailable,
    Expired,
    StalePrepared,
    Overflow,
    InvalidPlan,
    InvalidPolicy,
    InvalidTransition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReadyValue {
    pub(super) slot: SlotIdentity,
    pub(super) source: SourceIdentity,
    pub(super) version: Version,
    pub(super) material: MaterialIdentity,
    pub(super) expires_at: LogicalTime,
}

impl ReadyValue {
    pub(super) fn valid_at(self, now: LogicalTime) -> bool {
        self.expires_at > now
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PreparedCredential {
    pub(super) slot: SlotIdentity,
    pub(super) source: SourceIdentity,
    pub(super) version: Version,
    pub(super) material: MaterialIdentity,
    pub(super) expires_at: LogicalTime,
}

impl From<ReadyValue> for PreparedCredential {
    fn from(value: ReadyValue) -> Self {
        Self {
            slot: value.slot,
            source: value.source,
            version: value.version,
            material: value.material,
            expires_at: value.expires_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FinalCredential {
    pub(super) slot: SlotIdentity,
    pub(super) version: Version,
    pub(super) material: MaterialIdentity,
    pub(super) finalized_at: LogicalTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OperationTicket {
    pub(super) slot: SlotIdentity,
    pub(super) operation: OperationIdentity,
    pub(super) cohort: CohortIdentity,
    pub(super) kind: OperationKind,
    pub(super) refresh_input: Option<MaterialIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Cohort {
    handle: CohortHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CohortTerminal {
    Success(ReadyValue),
    Failure(ModelError),
    Superseded,
    Canceled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CohortCell {
    identity: CohortIdentity,
    subscribers: BTreeSet<WaiterIdentity>,
    terminal: Option<CohortTerminal>,
    publication_count: u8,
}

#[derive(Clone)]
struct CohortHandle(Rc<RefCell<CohortCell>>);

impl fmt::Debug for CohortHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.borrow().fmt(formatter)
    }
}

impl PartialEq for CohortHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for CohortHandle {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CohortSubscription {
    handle: CohortHandle,
    pub(super) waiter: WaiterIdentity,
}

impl Cohort {
    pub(super) fn new(identity: CohortIdentity, leader: WaiterIdentity) -> Self {
        Self {
            handle: CohortHandle(Rc::new(RefCell::new(CohortCell {
                identity,
                subscribers: BTreeSet::from([leader]),
                terminal: None,
                publication_count: 0,
            }))),
        }
    }

    pub(super) fn identity(&self) -> CohortIdentity {
        self.handle.0.borrow().identity
    }

    pub(super) fn subscribe(&self, waiter: WaiterIdentity) -> CohortSubscription {
        self.handle.0.borrow_mut().subscribers.insert(waiter);
        CohortSubscription {
            handle: self.handle.clone(),
            waiter,
        }
    }

    pub(super) fn cancel(&self, waiter: WaiterIdentity) -> bool {
        self.handle.0.borrow_mut().subscribers.remove(&waiter)
    }

    pub(super) fn publish(&self, terminal: CohortTerminal) -> bool {
        let mut cell = self.handle.0.borrow_mut();
        if cell.terminal.is_some() {
            return false;
        }
        cell.terminal = Some(terminal);
        cell.publication_count = 1;
        true
    }

    pub(super) fn terminal_for(&self, waiter: WaiterIdentity) -> Option<CohortTerminal> {
        let cell = self.handle.0.borrow();
        cell.subscribers
            .contains(&waiter)
            .then_some(cell.terminal)
            .flatten()
    }

    pub(super) fn publication_count(&self) -> u8 {
        self.handle.0.borrow().publication_count
    }

    pub(super) fn invariant(&self, active: bool) -> bool {
        let cell = self.handle.0.borrow();
        let publication_consistent = match cell.terminal {
            Some(_) => cell.publication_count == 1,
            None => cell.publication_count == 0,
        };
        publication_consistent && (!active || cell.terminal.is_none())
    }
}

impl CohortSubscription {
    pub(super) fn identity(&self) -> CohortIdentity {
        self.handle.0.borrow().identity
    }

    pub(super) fn observe(&self) -> Option<CohortTerminal> {
        let cell = self.handle.0.borrow();
        cell.subscribers
            .contains(&self.waiter)
            .then_some(cell.terminal)
            .flatten()
    }

    pub(super) fn publication_count(&self) -> u8 {
        self.handle.0.borrow().publication_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct InFlight {
    pub(super) ticket: OperationTicket,
    pub(super) previous: Option<ReadyValue>,
    pub(super) previous_eligible: bool,
    pub(super) cohort: Cohort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SlotState {
    Empty,
    Ready(ReadyValue),
    InFlight(InFlight),
    Failed {
        error: ErrorIdentity,
        retry_at: Option<LogicalTime>,
    },
    Rejected {
        exact_version: Version,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Slot {
    pub(super) identity: SlotIdentity,
    pub(super) source: SourceIdentity,
    pub(super) source_kind: SourceKind,
    pub(super) state: SlotState,
    pub(super) next_version: u64,
    pub(super) next_operation: u64,
    pub(super) next_cohort: u64,
    pub(super) provider_operations: u64,
    pub(super) discarded_completions: u64,
}

impl Slot {
    pub(super) fn empty(
        identity: SlotIdentity,
        source: SourceIdentity,
        source_kind: SourceKind,
    ) -> Self {
        Self {
            identity,
            source,
            source_kind,
            state: SlotState::Empty,
            next_version: 1,
            next_operation: 1,
            next_cohort: 1,
            provider_operations: 0,
            discarded_completions: 0,
        }
    }

    pub(super) fn ready(
        identity: SlotIdentity,
        source: SourceIdentity,
        source_kind: SourceKind,
        version: Version,
        material: MaterialIdentity,
        expires_at: LogicalTime,
    ) -> Self {
        let mut slot = Self::empty(identity, source, source_kind);
        slot.next_version = version.0.saturating_add(1);
        slot.state = SlotState::Ready(ReadyValue {
            slot: identity,
            source,
            version,
            material,
            expires_at,
        });
        slot
    }

    pub(super) fn invariant(&self) -> bool {
        match &self.state {
            SlotState::Empty | SlotState::Failed { .. } => true,
            SlotState::Ready(value) => {
                value.slot == self.identity
                    && value.source == self.source
                    && value.version.0 < self.next_version
            }
            SlotState::InFlight(active) => {
                let previous_valid = active.previous.is_none_or(|previous| {
                    previous.slot == self.identity
                        && previous.source == self.source
                        && previous.version.0 < self.next_version
                });
                active.ticket.slot == self.identity
                    && active.ticket.cohort == active.cohort.identity()
                    && active.ticket.operation.0 < self.next_operation
                    && active.ticket.cohort.0 < self.next_cohort
                    && previous_valid
                    && active.cohort.invariant(true)
            }
            SlotState::Rejected { exact_version } => {
                self.source_kind.is_static() && exact_version.0 < self.next_version
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicSlotRef {
    pub(super) current: Slot,
    pub(super) next_slot_identity: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AttemptCache {
    capacity: usize,
    prepared: BTreeMap<SlotIdentity, PreparedCredential>,
}

impl AttemptCache {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            prepared: BTreeMap::new(),
        }
    }

    pub(super) fn get(&self, slot: SlotIdentity) -> Option<PreparedCredential> {
        self.prepared.get(&slot).copied()
    }

    pub(super) fn remember(&mut self, prepared: PreparedCredential) -> Result<(), ModelError> {
        if !self.prepared.contains_key(&prepared.slot) && self.prepared.len() == self.capacity {
            return Err(ModelError::InvalidPlan);
        }
        self.prepared.insert(prepared.slot, prepared);
        Ok(())
    }
}

impl PublicSlotRef {
    pub(super) fn new(current: Slot) -> Self {
        Self {
            next_slot_identity: current.identity.0.saturating_add(1),
            current,
        }
    }
}
