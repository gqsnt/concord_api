//! Private credential slot state: installation intents versus installed
//! versions (AUTH-03, AUTH-04) and provider single-flight acquisition.
//!
//! Every slot owns two separate checked, never-wrapping counters:
//!
//! * `intent_epoch` — the latest-started installation intent. It advances on
//!   the first poll of an explicit storage operation, when a provider leader
//!   starts one acquisition, and on every manual `clear`/`replace`.
//! * the installed version — the last allocated installed version. It
//!   advances only when material is actually published. The counter is owned
//!   by the credential store and shared across complete provider
//!   replacement, so two installations in one store never share a version.
//!
//! An attributed challenge *rejects one installed version*; it never touches
//! the intent epoch, so a late challenge for an old version cannot clear a
//! newer version or invalidate a newer pending intent. A user `clear` or
//! `replace` advances the intent epoch and therefore supersedes every earlier
//! pending intent. The two meanings are never folded into one generation.
//!
//! No mutex in this module is held across an `.await`, provider call,
//! application `Drop` or other downstream work. Material and flights retired
//! under a lock are released only after the lock is dropped, and a retired
//! flight is told to re-evaluate before retired material is dropped. The
//! storage installation hands its released material to the caller instead
//! (dropped after the storage result is fixed and observed). Counter
//! exhaustion and mutex poisoning fail closed with a value-free
//! [`AuthError`] of kind `Internal`; an exhausted slot removes its material
//! and refuses every later use.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use crate::error::{AuthErrorKind, ProviderAcquisitionFacts, ProviderAcquisitionOutcome};
use crate::http::provider::{ProviderHttp, ProviderJournal, ProviderScope, ProviderSendBudget};
use crate::operation::OperationId;

use super::origin::{OriginSetError, ProviderOrigin, validated_origin_set};
use super::version::{CredentialVersion, StoreOutcome};
use super::{
    AuthError, CredentialMaterial, CredentialMaterialError, CredentialProvider, CredentialReason,
    CredentialRequest, RecoveryMode,
};

const REFRESH_SKEW: Duration = Duration::from_secs(60);

/// Maximum concurrent followers that may join one provider acquisition.
///
/// A follower beyond this bound fails immediately with an `Unavailable`
/// authentication error instead of queueing without limit.
pub const MAX_CREDENTIAL_FLIGHT_WAITERS: usize = 4_096;

/// Maximum re-evaluations of slot state within one preparation, finalization
/// or recovery step. Repeated churn (for example repeated clears racing an
/// acquisition) ends with an `Unavailable` error instead of looping.
const MAX_REEVALUATIONS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialSource {
    Static,
    Endpoint,
    Provider,
}

struct Ready<M> {
    version: u64,
    value: Arc<M>,
    expires_at: Option<Instant>,
}

impl<M> Clone for Ready<M> {
    fn clone(&self) -> Self {
        Self {
            version: self.version,
            value: Arc::clone(&self.value),
            expires_at: self.expires_at,
        }
    }
}

struct Cooldown {
    started_at: Instant,
    duration: Duration,
    error: AuthError,
}

/// The store-wide installed-version counter. Checked, never wrapping.
///
/// Allocation happens while the allocating slot's state lock is held; the
/// atomic only makes allocation unique across an old slot that is still
/// finishing work and its complete-replacement successor.
struct VersionCounter {
    last: AtomicU64,
}

impl VersionCounter {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            last: AtomicU64::new(0),
        })
    }

    fn allocate(&self) -> Option<u64> {
        self.last
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |last| {
                last.checked_add(1)
            })
            .ok()
            .and_then(|previous| previous.checked_add(1))
    }
}

struct SlotState<M> {
    /// Latest-started installation intent. Checked, never wrapping.
    intent_epoch: u64,
    /// Set when either counter is exhausted; the slot then fails closed.
    exhausted: bool,
    ready: Option<Ready<M>>,
    rejected_version: Option<u64>,
    in_flight: Option<ActiveFlight<M>>,
    cooldown: Option<Cooldown>,
}

impl<M> SlotState<M> {
    fn advance_intent(&mut self) -> Option<u64> {
        let epoch = self.intent_epoch.checked_add(1)?;
        self.intent_epoch = epoch;
        Some(epoch)
    }

    /// Marks the slot permanently unusable and retires everything it holds.
    fn exhaust(&mut self) -> Retired<M> {
        self.exhausted = true;
        self.rejected_version = None;
        self.cooldown = None;
        Retired {
            ready: self.ready.take(),
            value: None,
            flight: self.in_flight.take().map(|active| active.flight),
        }
    }

    fn take_flight(&mut self) -> Option<Arc<Flight<M>>> {
        self.in_flight.take().map(|active| active.flight)
    }
}

/// Values retired while a lock was held, released after it is dropped.
struct Retired<M> {
    ready: Option<Ready<M>>,
    value: Option<Arc<M>>,
    flight: Option<Arc<Flight<M>>>,
}

impl<M> Retired<M>
where
    M: CredentialMaterial,
{
    const fn none() -> Self {
        Self {
            ready: None,
            value: None,
            flight: None,
        }
    }

    fn release(self) -> Result<(), AuthError> {
        let (published, material) = self.hand_out();
        drop(material);
        published
    }

    /// Publishes the retired flight's re-evaluation and hands the retired
    /// credential material out undropped, so that its application-defined
    /// `Drop` runs wherever the caller decides.
    fn hand_out(self) -> (Result<(), AuthError>, ReleasedMaterial<M>) {
        let Self {
            ready,
            value,
            flight,
        } = self;
        let published = flight
            .as_ref()
            .map_or(Ok(()), |flight| flight.publish(FlightResult::Reevaluate));
        (
            published,
            ReleasedMaterial {
                _ready: ready,
                _value: value,
                _flight: flight,
            },
        )
    }
}

/// Credential material an installation decision no longer holds: the value
/// it did not install, or the installed value it replaced (with any retired
/// flight). It is handed out of the decision undropped, so that its
/// application-defined `Drop` runs outside every lock and, for explicit
/// storage, only after the storage result is fixed and finally observed
/// (EXEC-04). Dropping it releases the material.
#[must_use = "dropping it runs the application-defined Drop of credential material"]
pub(crate) struct ReleasedMaterial<M> {
    _ready: Option<Ready<M>>,
    _value: Option<Arc<M>>,
    _flight: Option<Arc<Flight<M>>>,
}

struct ActiveFlight<M> {
    flight: Arc<Flight<M>>,
    /// Installation intent reserved by this flight's leader.
    epoch: u64,
    /// Installed version that was current when the flight started.
    source_version: Option<u64>,
}

/// The immutable source description of one slot. Provider-authored values
/// (recovery mode, declared origins) are read when this is built, before any
/// credential lock is taken.
struct SourceSpec<M> {
    source: CredentialSource,
    provider: Option<Arc<dyn CredentialProvider<Material = M>>>,
    recovery_mode: RecoveryMode,
    origins: Arc<[ProviderOrigin]>,
}

impl<M> SourceSpec<M>
where
    M: CredentialMaterial,
{
    fn fixed(source: CredentialSource) -> Self {
        Self {
            source,
            provider: None,
            recovery_mode: RecoveryMode::None,
            origins: Arc::from([]),
        }
    }

    fn provider(
        provider: Arc<dyn CredentialProvider<Material = M>>,
        max_origins: usize,
    ) -> Result<Self, OriginSetError> {
        // Declared origins are read exactly once, here.
        let origins = validated_origin_set(provider.http_origins(), max_origins)?;
        let recovery_mode = provider.recovery_mode();
        Ok(Self {
            source: CredentialSource::Provider,
            provider: Some(provider),
            recovery_mode,
            origins,
        })
    }
}

struct CredentialSlot<M> {
    versions: Arc<VersionCounter>,
    source: CredentialSource,
    provider: Option<Arc<dyn CredentialProvider<Material = M>>>,
    origins: Arc<[ProviderOrigin]>,
    recovery_mode: RecoveryMode,
    cooldown_cap: Duration,
    waiter_limit: AtomicUsize,
    state: Mutex<SlotState<M>>,
}

struct Flight<M> {
    result: Mutex<Option<FlightResult<M>>>,
    notify: Notify,
    waiters: AtomicUsize,
}

/// The published terminal of one provider flight. A settled acquisition
/// carries its provider journal facts so every follower reports the same
/// provider operation identity (EXEC-01); a failure carries them inside its
/// [`AuthError`].
enum FlightResult<M> {
    Ready(PreparedCredential<M>, ProviderAcquisitionFacts),
    Failed(AuthError),
    Reevaluate,
}

enum FlightOutcome<M> {
    /// Usable material, with the facts of the provider acquisition that
    /// produced it when it came from a flight.
    Ready(PreparedCredential<M>, Option<ProviderAcquisitionFacts>),
    Failed(AuthError),
    Reevaluate,
}

impl<M> FlightOutcome<M> {
    /// Facts of the settled provider acquisition behind this outcome.
    fn provider_facts(&self) -> Option<ProviderAcquisitionFacts> {
        match self {
            Self::Ready(_, facts) => *facts,
            Self::Failed(error) => error.provider_acquisition(),
            Self::Reevaluate => None,
        }
    }

    const fn auth_kind(&self) -> Option<AuthErrorKind> {
        match self {
            Self::Failed(error) => Some(error.kind()),
            Self::Ready(..) | Self::Reevaluate => None,
        }
    }
}

pub(crate) struct PreparedCredential<M> {
    slot: Arc<CredentialSlot<M>>,
    version: u64,
    value: Arc<M>,
}

impl<M> Clone for PreparedCredential<M> {
    fn clone(&self) -> Self {
        Self {
            slot: Arc::clone(&self.slot),
            version: self.version,
            value: Arc::clone(&self.value),
        }
    }
}

impl<M> PreparedCredential<M> {
    pub(crate) fn value(&self) -> &M {
        self.value.as_ref()
    }

    /// Returns the exact installed version applied by this preparation.
    #[cfg(test)]
    pub(crate) const fn version(&self) -> CredentialVersion {
        CredentialVersion::new(self.version)
    }

    /// Returns the value-free identity of the applied material: the slot it
    /// came from and its installed version. Two preparations with equal
    /// identities apply exactly the same installed material; the executor's
    /// phase-6 freshness check compares identities without provider work.
    pub(crate) fn applied_identity(&self) -> (usize, u64) {
        (Arc::as_ptr(&self.slot).cast::<()>() as usize, self.version)
    }
}

enum PrepareAction<M> {
    Ready(PreparedCredential<M>),
    Join(Arc<Flight<M>>),
    Lead {
        flight: Arc<Flight<M>>,
        epoch: u64,
        provider: Arc<dyn CredentialProvider<Material = M>>,
        current: Option<Arc<M>>,
        reason: CredentialReason,
        /// The provider acquisition's own operation identity (EXEC-01).
        operation_id: OperationId,
    },
    Failed(AuthError),
}

/// One reserved explicit installation intent.
///
/// Created on the first poll of an explicit storage operation. Consumed by
/// exactly one install attempt; dropping it (failure or cancellation) never
/// rolls the epoch back and never revives an earlier intent.
pub(crate) struct IntentTicket<M> {
    slot: Arc<CredentialSlot<M>>,
    epoch: u64,
}

/// Private public-handle selector. Complete provider replacement swaps the
/// selected slot while every previously started operation retains its opaque
/// old slot identity.
pub(crate) struct SlotSelector<M> {
    context: CredentialContext,
    max_origins: usize,
    selected: Mutex<Arc<CredentialSlot<M>>>,
}

/// Unforgeable identity for the client runtime authorized to use a credential
/// selector.
///
/// The marker is deliberately opaque and has no diagnostic representation.
/// Generated credentials receive the marker owned by their `ClientCore`;
/// direct selector constructors used by the retained state-machine tests are
/// explicitly unbound.
#[derive(Clone)]
pub(crate) enum CredentialContext {
    Bound(Arc<()>),
    #[cfg(test)]
    Unbound,
}

impl CredentialContext {
    pub(crate) fn new() -> Self {
        Self::Bound(Arc::new(()))
    }

    #[cfg(test)]
    const fn unbound() -> Self {
        Self::Unbound
    }

    pub(crate) fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bound(left), Self::Bound(right)) => Arc::ptr_eq(left, right),
            #[cfg(test)]
            (Self::Unbound, _) | (_, Self::Unbound) => true,
        }
    }
}

impl<M> SlotSelector<M>
where
    M: CredentialMaterial,
{
    #[cfg(test)]
    pub(crate) fn static_value(value: M, cooldown_cap: Duration) -> Arc<Self> {
        Self::static_value_in_context(value, cooldown_cap, CredentialContext::unbound())
    }

    pub(crate) fn static_value_in_context(
        value: M,
        cooldown_cap: Duration,
        context: CredentialContext,
    ) -> Arc<Self> {
        let expires_at = value.expires_at();
        Arc::new(Self {
            context,
            max_origins: 0,
            selected: Mutex::new(CredentialSlot::new(
                SourceSpec::fixed(CredentialSource::Static),
                cooldown_cap,
                Some((value, expires_at)),
                0,
                VersionCounter::new(),
            )),
        })
    }

    #[cfg(test)]
    pub(crate) fn endpoint(cooldown_cap: Duration) -> Arc<Self> {
        Self::endpoint_in_context(cooldown_cap, CredentialContext::unbound())
    }

    pub(crate) fn endpoint_in_context(
        cooldown_cap: Duration,
        context: CredentialContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            context,
            max_origins: 0,
            selected: Mutex::new(CredentialSlot::new(
                SourceSpec::fixed(CredentialSource::Endpoint),
                cooldown_cap,
                None,
                0,
                VersionCounter::new(),
            )),
        })
    }

    #[cfg(test)]
    pub(crate) fn provider<P>(provider: P, cooldown_cap: Duration) -> Arc<Self>
    where
        P: CredentialProvider<Material = M>,
    {
        Self::provider_in_context(
            provider,
            cooldown_cap,
            CredentialContext::unbound(),
            crate::limits::MAX_PROVIDER_ORIGINS,
        )
        .expect("test providers declare a bounded HTTPS origin set")
    }

    pub(crate) fn provider_in_context<P>(
        provider: P,
        cooldown_cap: Duration,
        context: CredentialContext,
        max_origins: usize,
    ) -> Result<Arc<Self>, OriginSetError>
    where
        P: CredentialProvider<Material = M>,
    {
        Self::provider_arc_in_context(Arc::new(provider), cooldown_cap, context, max_origins)
    }

    pub(crate) fn provider_arc_in_context(
        provider: Arc<dyn CredentialProvider<Material = M>>,
        cooldown_cap: Duration,
        context: CredentialContext,
        max_origins: usize,
    ) -> Result<Arc<Self>, OriginSetError> {
        let spec = SourceSpec::provider(provider, max_origins)?;
        Ok(Arc::new(Self {
            context,
            max_origins,
            selected: Mutex::new(CredentialSlot::new(
                spec,
                cooldown_cap,
                None,
                0,
                VersionCounter::new(),
            )),
        }))
    }

    pub(crate) const fn context(&self) -> &CredentialContext {
        &self.context
    }

    fn selected(&self) -> Result<Arc<CredentialSlot<M>>, AuthError> {
        Ok(Arc::clone(&*lock(&self.selected)?))
    }

    pub(crate) fn source(&self) -> Result<CredentialSource, AuthError> {
        Ok(self.selected()?.source)
    }

    pub(crate) fn recovery_mode(&self) -> Result<RecoveryMode, AuthError> {
        Ok(self.selected()?.recovery_mode)
    }

    pub(crate) async fn preflight(
        &self,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        self.selected()?.ensure(http).await
    }

    pub(crate) async fn finalize(
        &self,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        let mut candidate = prepared.clone();
        for _ in 0..MAX_REEVALUATIONS {
            let selected = self.selected()?;
            if !Arc::ptr_eq(&selected, &candidate.slot) {
                candidate = Arc::clone(&selected).ensure(http).await?;
                continue;
            }

            let finalized = Arc::clone(&selected).finalize(&candidate, http).await?;
            // A complete provider replacement that finished while provider
            // work was awaited wins before this finalization linearization
            // point.
            if Arc::ptr_eq(&self.selected()?, &finalized.slot) {
                return Ok(finalized);
            }
            candidate = finalized;
        }
        Err(churn_error())
    }

    pub(crate) async fn recover(
        &self,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<Option<PreparedCredential<M>>, AuthError> {
        let selected = self.selected()?;
        if !Arc::ptr_eq(&selected, &prepared.slot) {
            return selected.ensure(http).await.map(Some);
        }
        selected.recover(prepared, http).await
    }

    /// Rejects exactly the installed version applied by `prepared`.
    ///
    /// This is challenge attribution, not a user clear: the intent epoch is
    /// untouched, pending intents stay eligible and a newer installed version
    /// is never affected. With `retain`, a provider keeps the rejected value
    /// only as refresh input.
    pub(crate) fn reject_installed(
        &self,
        prepared: &PreparedCredential<M>,
        retain: bool,
    ) -> Result<(), AuthError> {
        let selected = self.selected()?;
        if Arc::ptr_eq(&selected, &prepared.slot) {
            selected.reject_installed(prepared.version, retain)?;
        }
        Ok(())
    }

    /// Reserves one new explicit installation intent for endpoint-backed
    /// storage. It supersedes every earlier pending intent and does not clear
    /// currently installed material.
    pub(crate) fn reserve_store_intent(&self) -> Result<IntentTicket<M>, AuthError> {
        let slot = self.selected()?;
        if slot.source != CredentialSource::Endpoint {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        let epoch = slot.reserve_intent()?;
        Ok(IntentTicket { slot, epoch })
    }

    /// Installs `value` atomically only if `ticket` is still the latest
    /// started intent of the currently selected slot.
    #[cfg(test)]
    pub(crate) fn install(
        &self,
        ticket: IntentTicket<M>,
        value: M,
    ) -> Result<StoreOutcome, AuthError> {
        let expires_at = value.expires_at();
        let (outcome, released) = self.install_with_expiry(ticket, value, expires_at);
        drop(released);
        outcome
    }

    /// Installs `value` atomically only if `ticket` is still the latest
    /// started intent of the currently selected slot.
    ///
    /// `expires_at` is the value's application-defined expiration, read by
    /// the caller before any lock is taken (a synchronous extension call the
    /// storage terminal brackets with its cancellation and deadline checks,
    /// so that the lock-guarded installation decision runs no application
    /// code). The material this decision no longer holds — `value` when it is
    /// not installed, or the installed value it replaced — is returned
    /// undropped with the outcome: the storage terminal drops it only after
    /// its result is fixed and finally observed.
    pub(crate) fn install_with_expiry(
        &self,
        ticket: IntentTicket<M>,
        value: M,
        expires_at: Option<Instant>,
    ) -> (Result<StoreOutcome, AuthError>, ReleasedMaterial<M>) {
        let value = Arc::new(value);
        let (outcome, retired) =
            match lock(&self.selected) {
                Ok(selected) if Arc::ptr_eq(&selected, &ticket.slot) => ticket
                    .slot
                    .install_if_current(ticket.epoch, value, expires_at),
                // A replaced slot supersedes the ticket; a poisoned selection
                // fails closed. Neither installs `value`.
                selected => (
                    selected.map(|_| StoreOutcome::Superseded),
                    Retired {
                        value: Some(value),
                        ..Retired::none()
                    },
                ),
            };
        let (published, released) = retired.hand_out();
        (published.and(outcome), released)
    }

    /// Manual local clear: advances the intent epoch (even when empty),
    /// removes ready material and supersedes in-flight acquisition.
    pub(crate) fn clear(&self) -> Result<(), AuthError> {
        self.selected()?.clear()
    }

    /// Manual local replacement: advances the intent epoch and installs a new
    /// version.
    pub(crate) fn replace(&self, value: M) -> Result<CredentialVersion, AuthError> {
        self.selected()?.replace(value)
    }

    pub(crate) fn replace_result(
        &self,
        value: Result<M, CredentialMaterialError>,
    ) -> Result<CredentialVersion, AuthError> {
        let value = value.map_err(AuthError::from)?;
        self.replace(value)
    }

    /// Complete provider replacement of a provider-backed store.
    ///
    /// The replacement's declared origins are validated against the client's
    /// origin capacity before any lock is taken; the source kind cannot change
    /// (a static or endpoint-backed store refuses). Counters continue
    /// monotonically; storage intents of the old slot are superseded and
    /// operations already holding the old slot re-select the new one at
    /// finalization.
    pub(crate) fn replace_provider<P>(&self, provider: P) -> Result<(), AuthError>
    where
        P: CredentialProvider<Material = M>,
    {
        let spec = SourceSpec::provider(Arc::new(provider), self.max_origins)
            .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?;
        let previous = {
            let mut selected = lock(&self.selected)?;
            let current = Arc::clone(&selected);
            if current.source != CredentialSource::Provider {
                return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
            }
            let state = lock(&current.state)?;
            if state.exhausted {
                return Err(internal_error());
            }
            let epoch = state
                .intent_epoch
                .checked_add(1)
                .ok_or_else(internal_error)?;
            let replacement = CredentialSlot::new(
                spec,
                current.cooldown_cap,
                None,
                epoch,
                Arc::clone(&current.versions),
            );
            drop(state);
            std::mem::replace(&mut *selected, replacement)
        };
        drop(previous);
        Ok(())
    }

    pub(crate) fn is_set(&self) -> bool {
        self.selected()
            .ok()
            .is_some_and(|slot| slot.is_set(Instant::now()))
    }

    pub(crate) fn installed_version(&self) -> Option<CredentialVersion> {
        self.selected()
            .ok()
            .and_then(|slot| slot.installed_version())
    }

    #[cfg(test)]
    pub(crate) fn set_waiter_limit_for_test(&self, limit: usize) {
        self.selected()
            .expect("selected")
            .waiter_limit
            .store(limit, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn counters_for_test(&self) -> (u64, u64) {
        let slot = self.selected().expect("selected");
        let state = lock(&slot.state).expect("state");
        (
            state.intent_epoch,
            slot.versions.last.load(Ordering::SeqCst),
        )
    }

    #[cfg(test)]
    pub(crate) fn set_counters_for_test(&self, intent_epoch: u64, installed_version: u64) {
        let slot = self.selected().expect("selected");
        let mut state = lock(&slot.state).expect("state");
        state.intent_epoch = intent_epoch;
        slot.versions
            .last
            .store(installed_version, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(self: &Arc<Self>) {
        let slot = self.selected().expect("selected");
        let result = std::thread::spawn(move || {
            let _guard = slot.state.lock().expect("state");
            panic!("deliberate credential-state poisoning for a fail-closed test");
        })
        .join();
        assert!(result.is_err());
    }
}

impl<M> CredentialSlot<M>
where
    M: CredentialMaterial,
{
    fn new(
        spec: SourceSpec<M>,
        cooldown_cap: Duration,
        value: Option<(M, Option<Instant>)>,
        intent_epoch: u64,
        versions: Arc<VersionCounter>,
    ) -> Arc<Self> {
        let SourceSpec {
            source,
            provider,
            recovery_mode,
            origins,
        } = spec;
        // The initial static installation is version 1 of a fresh store.
        let ready = value.and_then(|(value, expires_at)| {
            versions.allocate().map(|version| Ready {
                expires_at,
                value: Arc::new(value),
                version,
            })
        });
        Arc::new(Self {
            versions,
            source,
            provider,
            origins,
            recovery_mode,
            cooldown_cap,
            waiter_limit: AtomicUsize::new(MAX_CREDENTIAL_FLIGHT_WAITERS),
            state: Mutex::new(SlotState {
                intent_epoch,
                exhausted: false,
                ready,
                rejected_version: None,
                in_flight: None,
                cooldown: None,
            }),
        })
    }

    async fn ensure(
        self: Arc<Self>,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        for _ in 0..MAX_REEVALUATIONS {
            let action = self.prepare_action(Instant::now())?;
            match self.run_action(action, http).await? {
                FlightOutcome::Ready(ready, _) => return Ok(ready),
                FlightOutcome::Failed(error) => return Err(error),
                FlightOutcome::Reevaluate => {}
            }
        }
        Err(churn_error())
    }

    async fn run_action(
        self: &Arc<Self>,
        action: PrepareAction<M>,
        http: ProviderHttp<'_>,
    ) -> Result<FlightOutcome<M>, AuthError> {
        match action {
            PrepareAction::Ready(ready) => Ok(FlightOutcome::Ready(ready, None)),
            PrepareAction::Join(flight) => {
                // A follower never owns provider work; dropping this future
                // only releases its waiter slot. It links the settled
                // acquisition it joined into its own operation journal.
                let waiter = FlightWaiter::join(flight, self.waiter_limit.load(Ordering::Acquire))?;
                let outcome = waiter.wait().await?;
                if let (Some(scope), Some(facts)) = (http.scope(), outcome.provider_facts()) {
                    scope.link(facts);
                }
                Ok(outcome)
            }
            PrepareAction::Failed(error) => Ok(FlightOutcome::Failed(error)),
            PrepareAction::Lead {
                flight,
                epoch,
                provider,
                current,
                reason,
                operation_id,
            } => {
                // EXEC-01: the acquisition is its own operation with a
                // bounded journal linked to the leading application operation.
                let scope = http.scope();
                let journal =
                    ProviderJournal::new(operation_id, scope.map(|scope| scope.parent()), reason);
                let mut guard =
                    LeaderGuard::new(Arc::clone(self), Arc::clone(&flight), &journal, scope);
                let budget = ProviderSendBudget::for_http(&http);
                let (bound, acquisition_deadline) =
                    http.for_acquisition(&self.origins, &budget, &journal);
                let provider_work = async {
                    let context = CredentialRequest {
                        http: bound,
                        reason,
                    };
                    let work = async {
                        match current.as_deref() {
                            Some(current) => provider.refresh(current, context).await,
                            None => provider.acquire(context).await,
                        }
                    };
                    match acquisition_deadline {
                        Some(deadline) => deadline
                            .run(work)
                            .await
                            .unwrap_or_else(|_| Err(provider_timeout_error())),
                        None => work.await,
                    }
                };
                tokio::pin!(provider_work);
                let (outcome, facts) = tokio::select! {
                    result = &mut provider_work => {
                        self.finish_provider(&flight, epoch, result, &journal)?
                    }
                    // Only a superseding clear, replace or later intent
                    // retires a live leader's flight.
                    outcome = flight.wait() => {
                        (outcome?, journal.facts(ProviderAcquisitionOutcome::Superseded))
                    }
                };
                guard.finished = true;
                if let Some(scope) = scope {
                    scope.settle(&journal, facts, outcome.auth_kind());
                }
                Ok(outcome)
            }
        }
    }

    fn prepare_action(self: &Arc<Self>, now: Instant) -> Result<PrepareAction<M>, AuthError> {
        let mut state = lock(&self.state)?;
        if state.exhausted {
            return Ok(PrepareAction::Failed(internal_error()));
        }
        let mut rejected_refresh = false;
        if let Some(ready) = &state.ready {
            if state.rejected_version == Some(ready.version) {
                if self.provider.is_some() {
                    rejected_refresh = true;
                } else {
                    return Ok(PrepareAction::Failed(AuthError::new(
                        AuthErrorKind::Rejected,
                    )));
                }
            }
            if !rejected_refresh {
                match freshness(ready.expires_at, now) {
                    Freshness::Fresh => {
                        return Ok(PrepareAction::Ready(PreparedCredential {
                            slot: Arc::clone(self),
                            version: ready.version,
                            value: Arc::clone(&ready.value),
                        }));
                    }
                    Freshness::Expired | Freshness::ExpiringSoon => {}
                }
            }
        }
        if let Some(active) = &state.in_flight {
            return Ok(PrepareAction::Join(Arc::clone(&active.flight)));
        }
        if let Some(cooldown) = &state.cooldown {
            if now.saturating_duration_since(cooldown.started_at) < cooldown.duration {
                return Ok(PrepareAction::Failed(cooldown.error.clone_internal()));
            }
            state.cooldown = None;
        }
        let Some(provider) = self.provider.as_ref().map(Arc::clone) else {
            let kind = if state.rejected_version.is_some() {
                AuthErrorKind::Rejected
            } else {
                AuthErrorKind::Unavailable
            };
            return Ok(PrepareAction::Failed(AuthError::new(kind)));
        };
        let current = state.ready.as_ref().map(|ready| Arc::clone(&ready.value));
        let source_version = state.ready.as_ref().map(|ready| ready.version);
        let reason = if rejected_refresh {
            CredentialReason::Rejected
        } else {
            match state
                .ready
                .as_ref()
                .map(|ready| freshness(ready.expires_at, now))
            {
                None => CredentialReason::Missing,
                Some(Freshness::Expired) => CredentialReason::Expired,
                Some(Freshness::ExpiringSoon | Freshness::Fresh) => CredentialReason::ExpiringSoon,
            }
        };
        self.lead(state, provider, current, source_version, reason)
    }

    /// Starts one provider flight: the leader reserves exactly one intent
    /// epoch, retains the exact source version and allocates the
    /// acquisition's own operation identity.
    fn lead(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, SlotState<M>>,
        provider: Arc<dyn CredentialProvider<Material = M>>,
        current: Option<Arc<M>>,
        source_version: Option<u64>,
        reason: CredentialReason,
    ) -> Result<PrepareAction<M>, AuthError> {
        let Ok(operation_id) = OperationId::allocate() else {
            drop(state);
            drop(current);
            return Ok(PrepareAction::Failed(internal_error()));
        };
        let Some(epoch) = state.advance_intent() else {
            let retired = state.exhaust();
            drop(state);
            drop(current);
            retired.release()?;
            return Ok(PrepareAction::Failed(internal_error()));
        };
        let flight = Arc::new(Flight::new());
        state.in_flight = Some(ActiveFlight {
            flight: Arc::clone(&flight),
            epoch,
            source_version,
        });
        Ok(PrepareAction::Lead {
            flight,
            epoch,
            provider,
            current,
            reason,
            operation_id,
        })
    }

    /// Settles the leader's provider result and publishes it to followers.
    /// Returns the outcome with the acquisition's settled journal facts.
    fn finish_provider(
        self: &Arc<Self>,
        flight: &Arc<Flight<M>>,
        epoch: u64,
        result: Result<M, AuthError>,
        journal: &ProviderJournal,
    ) -> Result<(FlightOutcome<M>, ProviderAcquisitionFacts), AuthError> {
        // Application-defined expiration is read before the slot lock.
        let result = result.map(|value| {
            let expires_at = value.expires_at();
            (Arc::new(value), expires_at)
        });
        let (terminal, retired) = {
            let mut state = lock(&self.state)?;
            let current = state.intent_epoch == epoch
                && state.in_flight.as_ref().is_some_and(|active| {
                    Arc::ptr_eq(&active.flight, flight) && active.epoch == epoch
                });
            if !current {
                // Superseded by a manual or explicit later intent: the
                // produced material is dropped after the lock is released.
                drop(state);
                drop(result);
                return Ok((
                    FlightOutcome::Reevaluate,
                    journal.facts(ProviderAcquisitionOutcome::Superseded),
                ));
            }
            state.in_flight = None;
            match result {
                Ok((value, expires_at)) => match self.versions.allocate() {
                    Some(version) => {
                        let retired = state.ready.replace(Ready {
                            version,
                            value: Arc::clone(&value),
                            expires_at,
                        });
                        state.rejected_version = None;
                        state.cooldown = None;
                        (
                            FlightResult::Ready(
                                PreparedCredential {
                                    slot: Arc::clone(self),
                                    version,
                                    value,
                                },
                                journal.facts(ProviderAcquisitionOutcome::Installed),
                            ),
                            Retired {
                                ready: retired,
                                ..Retired::none()
                            },
                        )
                    }
                    None => {
                        let mut retired = state.exhaust();
                        retired.value = Some(value);
                        let facts = journal.facts(ProviderAcquisitionOutcome::Failed);
                        (
                            FlightResult::Failed(internal_error().with_provider_acquisition(facts)),
                            retired,
                        )
                    }
                },
                Err(error) => {
                    let error = error.with_provider_acquisition(
                        journal.facts(ProviderAcquisitionOutcome::Failed),
                    );
                    let retired = state.ready.take();
                    state.rejected_version = None;
                    if let Some(delay) = error.retry_after().filter(|delay| !delay.is_zero()) {
                        let delay = delay.min(self.cooldown_cap);
                        if !delay.is_zero() {
                            state.cooldown = Some(Cooldown {
                                started_at: Instant::now(),
                                duration: delay,
                                error: error.clone_internal(),
                            });
                        }
                    }
                    (
                        FlightResult::Failed(error),
                        Retired {
                            ready: retired,
                            ..Retired::none()
                        },
                    )
                }
            }
        };
        // Followers are woken with this flight's terminal before any material
        // the slot no longer holds (the replaced installed value, or the
        // produced value after version exhaustion) runs its
        // application-defined `Drop`.
        let (retired_published, material) = retired.hand_out();
        let published = flight.publish(terminal);
        drop(material);
        retired_published?;
        published?;
        let outcome = flight.result_now()?.ok_or_else(internal_error)?;
        let facts = outcome
            .provider_facts()
            .unwrap_or_else(|| journal.facts(ProviderAcquisitionOutcome::Failed));
        Ok((outcome, facts))
    }

    async fn finalize(
        self: Arc<Self>,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        let mut provider_result = None;
        for _ in 0..MAX_REEVALUATIONS {
            let candidate = {
                let state = lock(&self.state)?;
                if state.exhausted {
                    return Err(internal_error());
                }
                state.ready.as_ref().and_then(|ready| {
                    (state.rejected_version != Some(ready.version)
                        && ready.version >= prepared.version)
                        .then(|| (ready.version, Arc::clone(&ready.value)))
                })
            };

            if let Some((version, value)) = candidate {
                // CredentialMaterial is application-defined. Query expiration
                // without the slot mutex, then verify that the exact candidate
                // is still installed and unrejected before selecting it.
                let expires_at = value.expires_at();
                let current = {
                    let mut state = lock(&self.state)?;
                    let current = !state.exhausted
                        && state.rejected_version != Some(version)
                        && state.ready.as_ref().is_some_and(|ready| {
                            ready.version == version && Arc::ptr_eq(&ready.value, &value)
                        });
                    if current {
                        if let Some(ready) = state.ready.as_mut() {
                            ready.expires_at = expires_at;
                        }
                    }
                    current
                };
                if !current {
                    continue;
                }

                match freshness(expires_at, Instant::now()) {
                    Freshness::Fresh => {
                        return Ok(PreparedCredential {
                            slot: Arc::clone(&self),
                            version,
                            value,
                        });
                    }
                    Freshness::ExpiringSoon if provider_result == Some(version) => {
                        // A provider may deliberately return valid material
                        // with less than the proactive skew remaining. Accept
                        // it once at this immediate send boundary.
                        return Ok(PreparedCredential {
                            slot: Arc::clone(&self),
                            version,
                            value,
                        });
                    }
                    Freshness::Expired if provider_result == Some(version) => {
                        return Err(AuthError::new(AuthErrorKind::Unavailable));
                    }
                    Freshness::ExpiringSoon | Freshness::Expired => {}
                }
            }

            let resolved = Arc::clone(&self).ensure(http).await?;
            provider_result = Some(resolved.version);
        }
        Err(churn_error())
    }

    async fn recover(
        self: Arc<Self>,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<Option<PreparedCredential<M>>, AuthError> {
        match self.recovery_mode {
            RecoveryMode::Refresh => {
                for _ in 0..MAX_REEVALUATIONS {
                    let action = self.prepare_recovery_action(prepared, Instant::now())?;
                    match self.run_action(action, http).await? {
                        FlightOutcome::Ready(ready, _) => return Ok(Some(ready)),
                        FlightOutcome::Failed(error) => return Err(error),
                        FlightOutcome::Reevaluate => {}
                    }
                }
                Err(churn_error())
            }
            RecoveryMode::InvalidateOnly => {
                self.reject_installed(prepared.version, false)?;
                Ok(None)
            }
            RecoveryMode::None => Ok(None),
        }
    }

    fn prepare_recovery_action(
        self: &Arc<Self>,
        prepared: &PreparedCredential<M>,
        now: Instant,
    ) -> Result<PrepareAction<M>, AuthError> {
        let mut state = lock(&self.state)?;
        if state.exhausted {
            return Ok(PrepareAction::Failed(internal_error()));
        }
        if let Some(active) = &state.in_flight {
            let challenged_is_source = active.source_version == Some(prepared.version);
            let flight = Arc::clone(&active.flight);
            if challenged_is_source {
                state.rejected_version = Some(prepared.version);
            }
            return Ok(PrepareAction::Join(flight));
        }

        if let Some(cooldown) = &state.cooldown {
            if now.saturating_duration_since(cooldown.started_at) < cooldown.duration {
                return Ok(PrepareAction::Failed(cooldown.error.clone_internal()));
            }
            state.cooldown = None;
        }

        if let Some(ready) = &state.ready {
            if ready.version != prepared.version
                && state.rejected_version != Some(ready.version)
                && freshness(ready.expires_at, now) != Freshness::Expired
            {
                return Ok(PrepareAction::Ready(PreparedCredential {
                    slot: Arc::clone(self),
                    version: ready.version,
                    value: Arc::clone(&ready.value),
                }));
            }
        }

        let Some(provider) = self.provider.as_ref().map(Arc::clone) else {
            return Ok(PrepareAction::Failed(AuthError::new(
                AuthErrorKind::Unavailable,
            )));
        };
        let current = state.ready.as_ref().map(|ready| Arc::clone(&ready.value));
        let source_version = state.ready.as_ref().map(|ready| ready.version);
        let exact = source_version == Some(prepared.version);
        if exact {
            state.rejected_version = Some(prepared.version);
        }
        let reason = if exact {
            CredentialReason::Rejected
        } else {
            match state
                .ready
                .as_ref()
                .map(|ready| freshness(ready.expires_at, now))
            {
                None => CredentialReason::Missing,
                Some(Freshness::Expired) => CredentialReason::Expired,
                Some(Freshness::ExpiringSoon | Freshness::Fresh) => CredentialReason::ExpiringSoon,
            }
        };
        self.lead(state, provider, current, source_version, reason)
    }

    fn reserve_intent(&self) -> Result<u64, AuthError> {
        let (result, retired) = {
            let mut state = lock(&self.state)?;
            if state.exhausted {
                return Err(internal_error());
            }
            match state.advance_intent() {
                Some(epoch) => {
                    // Latest-started intent: an earlier pending acquisition
                    // is superseded; installed material stays usable.
                    let flight = state.take_flight();
                    (
                        Ok(epoch),
                        Retired {
                            flight,
                            ..Retired::none()
                        },
                    )
                }
                None => (Err(internal_error()), state.exhaust()),
            }
        };
        retired.release()?;
        result
    }

    fn install_if_current(
        &self,
        epoch: u64,
        value: Arc<M>,
        expires_at: Option<Instant>,
    ) -> (Result<StoreOutcome, AuthError>, Retired<M>) {
        let mut state = match lock(&self.state) {
            Ok(state) => state,
            Err(error) => {
                return (
                    Err(error),
                    Retired {
                        value: Some(value),
                        ..Retired::none()
                    },
                );
            }
        };
        if state.exhausted {
            return (
                Err(internal_error()),
                Retired {
                    value: Some(value),
                    ..Retired::none()
                },
            );
        }
        if state.intent_epoch != epoch {
            return (
                Ok(StoreOutcome::Superseded),
                Retired {
                    value: Some(value),
                    ..Retired::none()
                },
            );
        }
        let Some(version) = self.versions.allocate() else {
            let mut retired = state.exhaust();
            retired.value = Some(value);
            return (Err(internal_error()), retired);
        };
        let ready = state.ready.replace(Ready {
            version,
            value,
            expires_at,
        });
        state.rejected_version = None;
        state.cooldown = None;
        // No later flight can exist while this epoch is current; retire any
        // defensive leftover so it re-evaluates against the new version.
        let flight = state.take_flight();
        (
            Ok(StoreOutcome::Installed {
                version: CredentialVersion::new(version),
            }),
            Retired {
                ready,
                value: None,
                flight,
            },
        )
    }

    fn clear(&self) -> Result<(), AuthError> {
        let (result, retired) = {
            let mut state = lock(&self.state)?;
            let advanced = if state.exhausted {
                None
            } else {
                state.advance_intent()
            };
            match advanced {
                Some(_) => {
                    let ready = state.ready.take();
                    state.rejected_version = None;
                    state.cooldown = None;
                    let flight = state.take_flight();
                    (
                        Ok(()),
                        Retired {
                            ready,
                            value: None,
                            flight,
                        },
                    )
                }
                // Fail closed: material is removed even when the intent
                // epoch cannot advance, and the slot refuses later use.
                None => (Err(internal_error()), state.exhaust()),
            }
        };
        retired.release()?;
        result
    }

    fn replace(&self, value: M) -> Result<CredentialVersion, AuthError> {
        let expires_at = value.expires_at();
        let value = Arc::new(value);
        let (result, retired) = {
            let mut state = match lock(&self.state) {
                Ok(state) => state,
                Err(error) => {
                    drop(value);
                    return Err(error);
                }
            };
            if state.exhausted {
                (
                    Err(internal_error()),
                    Retired {
                        value: Some(value),
                        ..Retired::none()
                    },
                )
            } else {
                match (state.advance_intent(), self.versions.allocate()) {
                    (Some(_), Some(version)) => {
                        let ready = state.ready.replace(Ready {
                            version,
                            value,
                            expires_at,
                        });
                        state.rejected_version = None;
                        state.cooldown = None;
                        let flight = state.take_flight();
                        (
                            Ok(CredentialVersion::new(version)),
                            Retired {
                                ready,
                                value: None,
                                flight,
                            },
                        )
                    }
                    _ => {
                        let mut retired = state.exhaust();
                        retired.value = Some(value);
                        (Err(internal_error()), retired)
                    }
                }
            }
        };
        retired.release()?;
        result
    }

    fn reject_installed(&self, version: u64, retain: bool) -> Result<(), AuthError> {
        let retired = {
            let mut state = lock(&self.state)?;
            if state.ready.as_ref().map(|ready| ready.version) != Some(version) {
                // A late challenge for an older version never affects a newer
                // installed version or a pending intent.
                return Ok(());
            }
            state.rejected_version = Some(version);
            if retain { None } else { state.ready.take() }
        };
        drop(retired);
        Ok(())
    }

    fn is_set(&self, now: Instant) -> bool {
        lock(&self.state).ok().is_some_and(|state| {
            !state.exhausted
                && state.in_flight.is_none()
                && state.cooldown.is_none()
                && state.ready.as_ref().is_some_and(|ready| {
                    state.rejected_version != Some(ready.version)
                        && ready.expires_at.is_none_or(|expires| expires > now)
                })
        })
    }

    fn installed_version(&self) -> Option<CredentialVersion> {
        lock(&self.state).ok().and_then(|state| {
            if state.exhausted {
                return None;
            }
            state
                .ready
                .as_ref()
                .filter(|ready| state.rejected_version != Some(ready.version))
                .map(|ready| CredentialVersion::new(ready.version))
        })
    }

    fn cancel_leader(&self, flight: &Arc<Flight<M>>) {
        let removed = lock(&self.state).ok().and_then(|mut state| {
            let active = state
                .in_flight
                .as_ref()
                .filter(|active| Arc::ptr_eq(&active.flight, flight))?;
            let source_rejected = active
                .source_version
                .is_some_and(|version| state.rejected_version == Some(version));
            let active = state.in_flight.take()?;
            // The epoch reserved by the cancelled leader is not rolled back.
            let retired = if source_rejected {
                state.rejected_version = None;
                state.ready.take()
            } else {
                None
            };
            Some((active.flight, retired))
        });
        if let Some((removed, retired)) = removed {
            // Followers are told to re-evaluate before the retired material's
            // application-defined `Drop` runs, so a panicking or slow `Drop`
            // cannot leave them waiting on an unpublished flight.
            let _ = flight.publish(FlightResult::Reevaluate);
            drop(removed);
            drop(retired);
        }
    }
}

impl<M> Flight<M>
where
    M: CredentialMaterial,
{
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            notify: Notify::new(),
            waiters: AtomicUsize::new(0),
        }
    }

    fn publish(&self, result: FlightResult<M>) -> Result<(), AuthError> {
        let mut pending = Some(result);
        let published = {
            let mut terminal = lock(&self.result)?;
            if terminal.is_none() {
                *terminal = pending.take();
                true
            } else {
                false
            }
        };
        // A duplicate terminal value can retain user-defined credential
        // material. Drop it only after releasing the flight mutex.
        drop(pending);
        if published {
            self.notify.notify_waiters();
        }
        Ok(())
    }

    fn result_now(&self) -> Result<Option<FlightOutcome<M>>, AuthError> {
        Ok(lock(&self.result)?.as_ref().map(flight_result))
    }

    async fn wait(&self) -> Result<FlightOutcome<M>, AuthError> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = self.result_now()? {
                return Ok(result);
            }
            notified.await;
        }
    }
}

/// One bounded follower registration on a provider flight.
struct FlightWaiter<M>
where
    M: CredentialMaterial,
{
    flight: Arc<Flight<M>>,
}

impl<M> FlightWaiter<M>
where
    M: CredentialMaterial,
{
    fn join(flight: Arc<Flight<M>>, limit: usize) -> Result<Self, AuthError> {
        flight
            .waiters
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |waiters| {
                (waiters < limit).then_some(waiters + 1)
            })
            .map_err(|_| {
                AuthError::with_safe_message(
                    AuthErrorKind::Unavailable,
                    "credential acquisition waiter limit reached",
                )
            })?;
        Ok(Self { flight })
    }

    async fn wait(&self) -> Result<FlightOutcome<M>, AuthError> {
        self.flight.wait().await
    }
}

impl<M> Drop for FlightWaiter<M>
where
    M: CredentialMaterial,
{
    fn drop(&mut self) {
        self.flight.waiters.fetch_sub(1, Ordering::AcqRel);
    }
}

fn flight_result<M>(result: &FlightResult<M>) -> FlightOutcome<M>
where
    M: CredentialMaterial,
{
    match result {
        FlightResult::Ready(ready, facts) => FlightOutcome::Ready(ready.clone(), Some(*facts)),
        FlightResult::Failed(error) => FlightOutcome::Failed(error.clone_internal()),
        FlightResult::Reevaluate => FlightOutcome::Reevaluate,
    }
}

struct LeaderGuard<'j, M>
where
    M: CredentialMaterial,
{
    slot: Arc<CredentialSlot<M>>,
    flight: Arc<Flight<M>>,
    journal: &'j ProviderJournal,
    scope: Option<ProviderScope<'j>>,
    finished: bool,
}

impl<'j, M> LeaderGuard<'j, M>
where
    M: CredentialMaterial,
{
    const fn new(
        slot: Arc<CredentialSlot<M>>,
        flight: Arc<Flight<M>>,
        journal: &'j ProviderJournal,
        scope: Option<ProviderScope<'j>>,
    ) -> Self {
        Self {
            slot,
            flight,
            journal,
            scope,
            finished: false,
        }
    }
}

impl<M> Drop for LeaderGuard<'_, M>
where
    M: CredentialMaterial,
{
    fn drop(&mut self) {
        if !self.finished {
            // A cancelled leader retires its flight so followers re-evaluate
            // instead of waiting forever, then synchronously settles its
            // provider journal as cancelled (EXEC-01). Nothing is spawned.
            self.slot.cancel_leader(&self.flight);
            if let Some(scope) = self.scope {
                scope.settle(
                    self.journal,
                    self.journal.facts(ProviderAcquisitionOutcome::Cancelled),
                    None,
                );
            }
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Freshness {
    Fresh,
    ExpiringSoon,
    Expired,
}

fn freshness(expires_at: Option<Instant>, now: Instant) -> Freshness {
    let Some(expires) = expires_at else {
        return Freshness::Fresh;
    };
    if expires <= now {
        return Freshness::Expired;
    }
    match now.checked_add(REFRESH_SKEW) {
        Some(skewed) if expires <= skewed => Freshness::ExpiringSoon,
        Some(_) => Freshness::Fresh,
        None => Freshness::ExpiringSoon,
    }
}

fn internal_error() -> AuthError {
    AuthError::new(AuthErrorKind::Internal)
}

fn churn_error() -> AuthError {
    AuthError::with_safe_message(
        AuthErrorKind::Unavailable,
        "credential state changed repeatedly before it could be used",
    )
}

fn provider_timeout_error() -> AuthError {
    AuthError::with_safe_message(
        AuthErrorKind::Acquisition,
        "credential provider exceeded its helper time limit",
    )
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AuthError> {
    mutex.lock().map_err(|_| internal_error())
}

/// A typed read-only credential capability.
///
/// References produced by a generated client are bound to that built client's
/// private security context. They may be used through clones of that client,
/// but authentication preparation rejects use through an independent build.
pub struct CredentialRef<M> {
    pub(crate) selector: Arc<SlotSelector<M>>,
}

impl<M> Clone for CredentialRef<M> {
    fn clone(&self) -> Self {
        Self {
            selector: Arc::clone(&self.selector),
        }
    }
}

impl<M> CredentialRef<M>
where
    M: CredentialMaterial,
{
    pub(crate) fn new(selector: Arc<SlotSelector<M>>) -> Self {
        Self { selector }
    }
}

/// A typed mutable endpoint-backed credential capability.
///
/// The handle and every read-only reference obtained from it remain bound to
/// the originating built client's private security context. Clones share one
/// credential store. Every method is synchronous, local and performs no
/// network work: it neither revokes a remote session nor recalls requests
/// that were already dispatched with an earlier version. Methods never hold a
/// lock across application code; a poisoned or exhausted store fails closed
/// with a value-free [`AuthError`].
pub struct CredentialHandle<M> {
    selector: Arc<SlotSelector<M>>,
}

impl<M> Clone for CredentialHandle<M> {
    fn clone(&self) -> Self {
        Self {
            selector: Arc::clone(&self.selector),
        }
    }
}

impl<M> CredentialHandle<M>
where
    M: CredentialMaterial,
{
    pub(crate) fn new(selector: Arc<SlotSelector<M>>) -> Self {
        Self { selector }
    }

    /// Returns a read-only credential reference.
    #[must_use]
    pub fn credential(&self) -> CredentialRef<M> {
        CredentialRef::new(Arc::clone(&self.selector))
    }

    /// Manually installs `value` as a new version.
    ///
    /// This advances the installation intent, so every earlier pending
    /// explicit storage operation or provider acquisition is superseded and
    /// will report [`StoreOutcome::Superseded`] or re-evaluate.
    pub fn replace(&self, value: M) -> Result<CredentialVersion, AuthError> {
        self.selector.replace(value)
    }

    /// Manually clears local material (logout).
    ///
    /// This advances the installation intent even when the store is already
    /// empty, removes ready material and supersedes in-flight acquisition. An
    /// explicit storage operation first polled *after* this call is a new
    /// intent and may install normally.
    pub fn clear(&self) -> Result<(), AuthError> {
        self.selector.clear()
    }

    /// Observes whether non-expired, unrejected ready material is stored.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.selector.is_set()
    }

    /// Returns the currently installed, unrejected version, if any.
    #[must_use]
    pub fn installed_version(&self) -> Option<CredentialVersion> {
        self.selector.installed_version()
    }
}

#[cfg(test)]
mod intent_tests;
#[cfg(test)]
mod tests;
