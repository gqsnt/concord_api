use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use crate::error::AuthErrorKind;
use crate::http::provider::ProviderHttp;

use super::{
    AuthError, CredentialMaterial, CredentialProvider, CredentialReason, CredentialRequest,
    RecoveryMode,
};

const REFRESH_SKEW: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialSource {
    Static,
    Endpoint,
    Provider,
}

#[derive(Clone)]
struct Ready<M> {
    version: u64,
    value: Arc<M>,
    expires_at: Option<Instant>,
}

struct Cooldown {
    started_at: Instant,
    duration: Duration,
    error: AuthError,
}

struct SlotState<M> {
    next_version: u64,
    ready: Option<Ready<M>>,
    rejected_version: Option<u64>,
    in_flight: Option<ActiveFlight<M>>,
    cooldown: Option<Cooldown>,
}

struct ActiveFlight<M> {
    flight: Arc<Flight<M>>,
    previous_version: Option<u64>,
}

struct CredentialSlot<M> {
    source: CredentialSource,
    provider: Option<Arc<dyn CredentialProvider<Material = M>>>,
    recovery_mode: RecoveryMode,
    cooldown_cap: Duration,
    state: Mutex<SlotState<M>>,
}

struct Flight<M> {
    result: Mutex<Option<FlightResult<M>>>,
    notify: Notify,
}

enum FlightResult<M> {
    Ready(PreparedCredential<M>),
    Failed(AuthError),
    Reevaluate,
}

enum FlightOutcome<M> {
    Ready(PreparedCredential<M>),
    Failed(AuthError),
    Reevaluate,
}

#[derive(Clone)]
pub(crate) struct PreparedCredential<M> {
    slot: Arc<CredentialSlot<M>>,
    version: u64,
    value: Arc<M>,
}

impl<M> PreparedCredential<M> {
    pub(crate) fn value(&self) -> &M {
        self.value.as_ref()
    }
}

enum PrepareAction<M> {
    Ready(PreparedCredential<M>),
    Join(Arc<Flight<M>>),
    Lead {
        flight: Arc<Flight<M>>,
        provider: Arc<dyn CredentialProvider<Material = M>>,
        current: Option<Arc<M>>,
        reason: CredentialReason,
    },
    Failed(AuthError),
}

/// Private public-handle selector. Complete source replacement swaps the
/// selected slot while every previously started operation retains its opaque
/// old slot identity.
pub(crate) struct SlotSelector<M> {
    selected: Mutex<Arc<CredentialSlot<M>>>,
}

impl<M> SlotSelector<M>
where
    M: CredentialMaterial,
{
    pub(crate) fn static_value(value: M, cooldown_cap: Duration) -> Arc<Self> {
        Arc::new(Self {
            selected: Mutex::new(CredentialSlot::new(
                CredentialSource::Static,
                None,
                Some(value),
                cooldown_cap,
            )),
        })
    }

    pub(crate) fn endpoint(cooldown_cap: Duration) -> Arc<Self> {
        Arc::new(Self {
            selected: Mutex::new(CredentialSlot::new(
                CredentialSource::Endpoint,
                None,
                None,
                cooldown_cap,
            )),
        })
    }

    pub(crate) fn provider<P>(provider: P, cooldown_cap: Duration) -> Arc<Self>
    where
        P: CredentialProvider<Material = M>,
    {
        Arc::new(Self {
            selected: Mutex::new(CredentialSlot::new(
                CredentialSource::Provider,
                Some(Arc::new(provider)),
                None,
                cooldown_cap,
            )),
        })
    }

    pub(crate) fn provider_arc(
        provider: Arc<dyn CredentialProvider<Material = M>>,
        cooldown_cap: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            selected: Mutex::new(CredentialSlot::new(
                CredentialSource::Provider,
                Some(provider),
                None,
                cooldown_cap,
            )),
        })
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
        self.selected()?.ensure(http, false).await
    }

    pub(crate) async fn finalize(
        &self,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        let mut candidate = prepared.clone();
        loop {
            let selected = self.selected()?;
            if !Arc::ptr_eq(&selected, &candidate.slot) {
                candidate = Arc::clone(&selected).ensure(http, false).await?;
                continue;
            }

            let finalized = Arc::clone(&selected).finalize(&candidate, http).await?;
            // A complete source replacement that finished while provider work
            // was awaited wins before this finalization linearization point.
            if Arc::ptr_eq(&self.selected()?, &finalized.slot) {
                return Ok(finalized);
            }
            candidate = finalized;
        }
    }

    pub(crate) async fn recover(
        &self,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<Option<PreparedCredential<M>>, AuthError> {
        let selected = self.selected()?;
        if !Arc::ptr_eq(&selected, &prepared.slot) {
            return selected.ensure(http, false).await.map(Some);
        }
        selected.recover(prepared, http).await
    }

    pub(crate) fn reject_static_exact(
        &self,
        prepared: &PreparedCredential<M>,
    ) -> Result<(), AuthError> {
        let selected = self.selected()?;
        if Arc::ptr_eq(&selected, &prepared.slot) {
            selected.reject_exact(prepared.version)?;
        }
        Ok(())
    }

    pub(crate) fn clear_exact(&self, prepared: &PreparedCredential<M>) -> Result<(), AuthError> {
        let selected = self.selected()?;
        if Arc::ptr_eq(&selected, &prepared.slot) {
            selected.clear_exact(prepared.version)?;
        }
        Ok(())
    }

    pub(crate) fn replace_static(&self, value: M) -> Result<(), AuthError> {
        let cooldown_cap = self.selected()?.cooldown_cap;
        let replacement =
            CredentialSlot::new(CredentialSource::Static, None, Some(value), cooldown_cap);
        let previous = {
            let mut selected = lock(&self.selected)?;
            std::mem::replace(&mut *selected, replacement)
        };
        drop(previous);
        Ok(())
    }

    pub(crate) fn replace_provider<P>(&self, provider: P) -> Result<(), AuthError>
    where
        P: CredentialProvider<Material = M>,
    {
        let cooldown_cap = self.selected()?.cooldown_cap;
        let replacement = CredentialSlot::new(
            CredentialSource::Provider,
            Some(Arc::new(provider)),
            None,
            cooldown_cap,
        );
        let previous = {
            let mut selected = lock(&self.selected)?;
            std::mem::replace(&mut *selected, replacement)
        };
        drop(previous);
        Ok(())
    }

    pub(crate) fn set(&self, value: M) -> Result<(), AuthError> {
        self.selected()?.set(value)
    }

    pub(crate) fn clear(&self) -> Result<(), AuthError> {
        self.selected()?.clear()
    }

    pub(crate) fn is_set(&self) -> bool {
        self.selected()
            .ok()
            .is_some_and(|slot| slot.is_set(Instant::now()))
    }
}

impl<M> CredentialSlot<M>
where
    M: CredentialMaterial,
{
    fn new(
        source: CredentialSource,
        provider: Option<Arc<dyn CredentialProvider<Material = M>>>,
        value: Option<M>,
        cooldown_cap: Duration,
    ) -> Arc<Self> {
        let recovery_mode = provider
            .as_ref()
            .map_or(RecoveryMode::None, |provider| provider.recovery_mode());
        let ready = value.map(|value| Ready {
            expires_at: value.expires_at(),
            value: Arc::new(value),
            version: 1,
        });
        Arc::new(Self {
            source,
            provider,
            recovery_mode,
            cooldown_cap,
            state: Mutex::new(SlotState {
                next_version: u64::from(ready.is_some()),
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
        rejected_refresh: bool,
    ) -> Result<PreparedCredential<M>, AuthError> {
        loop {
            let action = self.prepare_action(Instant::now(), rejected_refresh)?;
            match self.run_action(action, http).await? {
                FlightOutcome::Ready(ready) => return Ok(ready),
                FlightOutcome::Failed(error) => return Err(error),
                FlightOutcome::Reevaluate => {}
            }
        }
    }

    async fn run_action(
        self: &Arc<Self>,
        action: PrepareAction<M>,
        http: ProviderHttp<'_>,
    ) -> Result<FlightOutcome<M>, AuthError> {
        match action {
            PrepareAction::Ready(ready) => Ok(FlightOutcome::Ready(ready)),
            PrepareAction::Join(flight) => flight.wait().await,
            PrepareAction::Failed(error) => Ok(FlightOutcome::Failed(error)),
            PrepareAction::Lead {
                flight,
                provider,
                current,
                reason,
            } => {
                let mut guard = LeaderGuard::new(Arc::clone(self), Arc::clone(&flight));
                let provider_work = async {
                    let context = CredentialRequest { http, reason };
                    match current.as_deref() {
                        Some(current) => provider.refresh(current, context).await,
                        None => provider.acquire(context).await,
                    }
                };
                tokio::pin!(provider_work);
                let outcome = tokio::select! {
                    result = &mut provider_work => self.finish_provider(&flight, result)?,
                    outcome = flight.wait() => outcome?,
                };
                guard.finished = true;
                Ok(outcome)
            }
        }
    }

    fn prepare_action(
        self: &Arc<Self>,
        now: Instant,
        rejected_refresh: bool,
    ) -> Result<PrepareAction<M>, AuthError> {
        let mut state = lock(&self.state)?;
        let mut rejected_refresh = rejected_refresh;
        if let Some(ready) = &state.ready {
            let rejected = state.rejected_version == Some(ready.version);
            if rejected && !rejected_refresh {
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
        let previous_version = state.ready.as_ref().map(|ready| ready.version);
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
        let flight = Arc::new(Flight::new());
        state.in_flight = Some(ActiveFlight {
            flight: Arc::clone(&flight),
            previous_version,
        });
        Ok(PrepareAction::Lead {
            flight,
            provider,
            current,
            reason,
        })
    }

    fn finish_provider(
        self: &Arc<Self>,
        flight: &Arc<Flight<M>>,
        result: Result<M, AuthError>,
    ) -> Result<FlightOutcome<M>, AuthError> {
        let result = result.map(|value| {
            let expires_at = value.expires_at();
            (Arc::new(value), expires_at)
        });
        let (terminal, retired, discarded) = {
            let mut state = lock(&self.state)?;
            let current = state
                .in_flight
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(&active.flight, flight));
            if !current {
                return Ok(FlightOutcome::Reevaluate);
            }
            state.in_flight = None;
            match result {
                Ok((value, expires_at)) => match state.next_version.checked_add(1) {
                    Some(version) => {
                        state.next_version = version;
                        let retired = state.ready.replace(Ready {
                            version,
                            value: Arc::clone(&value),
                            expires_at,
                        });
                        state.rejected_version = None;
                        state.cooldown = None;
                        (
                            FlightResult::Ready(PreparedCredential {
                                slot: Arc::clone(self),
                                version,
                                value,
                            }),
                            retired,
                            None,
                        )
                    }
                    None => {
                        let retired = state.ready.take();
                        state.rejected_version = None;
                        (
                            FlightResult::Failed(AuthError::new(AuthErrorKind::Internal)),
                            retired,
                            Some(value),
                        )
                    }
                },
                Err(error) => {
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
                    (FlightResult::Failed(error), retired, None)
                }
            }
        };
        drop(retired);
        drop(discarded);
        flight.publish(terminal)?;
        flight
            .result_now()?
            .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))
    }

    async fn finalize(
        self: Arc<Self>,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedCredential<M>, AuthError> {
        let mut provider_result = None;
        loop {
            let candidate = {
                let state = lock(&self.state)?;
                state.ready.as_ref().and_then(|ready| {
                    (state.rejected_version != Some(ready.version)
                        && ready.version >= prepared.version)
                        .then(|| (ready.version, Arc::clone(&ready.value)))
                })
            };

            if let Some((version, value)) = candidate {
                // CredentialMaterial is application-defined. Query expiration
                // without either credential mutex, then verify that the exact
                // candidate is still current before selecting it.
                let expires_at = value.expires_at();
                let current = {
                    let mut state = lock(&self.state)?;
                    let current = state.rejected_version != Some(version)
                        && state.in_flight.is_none()
                        && state.ready.as_ref().is_some_and(|ready| {
                            ready.version == version && Arc::ptr_eq(&ready.value, &value)
                        });
                    if current {
                        state.ready.as_mut().expect("checked ready").expires_at = expires_at;
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

            let resolved = Arc::clone(&self).ensure(http, false).await?;
            provider_result = Some(resolved.version);
        }
    }

    async fn recover(
        self: Arc<Self>,
        prepared: &PreparedCredential<M>,
        http: ProviderHttp<'_>,
    ) -> Result<Option<PreparedCredential<M>>, AuthError> {
        match self.recovery_mode {
            RecoveryMode::Refresh => loop {
                let action = self.prepare_recovery_action(prepared, Instant::now())?;
                match self.run_action(action, http).await? {
                    FlightOutcome::Ready(ready) => return Ok(Some(ready)),
                    FlightOutcome::Failed(error) => return Err(error),
                    FlightOutcome::Reevaluate => {}
                }
            },
            RecoveryMode::InvalidateOnly => {
                self.clear_exact(prepared.version)?;
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
        if let Some(active) = &state.in_flight {
            let challenged_is_previous = active.previous_version == Some(prepared.version);
            let flight = Arc::clone(&active.flight);
            if challenged_is_previous {
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
        let previous_version = state.ready.as_ref().map(|ready| ready.version);
        let exact = previous_version == Some(prepared.version);
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
        let flight = Arc::new(Flight::new());
        state.in_flight = Some(ActiveFlight {
            flight: Arc::clone(&flight),
            previous_version,
        });
        Ok(PrepareAction::Lead {
            flight,
            provider,
            current,
            reason,
        })
    }

    fn set(&self, value: M) -> Result<(), AuthError> {
        let expires_at = value.expires_at();
        let value = Arc::new(value);
        let (flight, retired) = {
            let mut state = lock(&self.state)?;
            let version = state
                .next_version
                .checked_add(1)
                .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
            state.next_version = version;
            let retired = state.ready.replace(Ready {
                version,
                value,
                expires_at,
            });
            state.rejected_version = None;
            state.cooldown = None;
            (state.in_flight.take().map(|active| active.flight), retired)
        };
        drop(retired);
        supersede_flight(flight)
    }

    fn clear(&self) -> Result<(), AuthError> {
        let (flight, retired) = {
            let mut state = lock(&self.state)?;
            state.next_version = state
                .next_version
                .checked_add(1)
                .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
            let retired = state.ready.take();
            state.rejected_version = None;
            state.cooldown = None;
            (state.in_flight.take().map(|active| active.flight), retired)
        };
        drop(retired);
        supersede_flight(flight)
    }

    fn clear_exact(&self, version: u64) -> Result<(), AuthError> {
        let (flight, retired) = {
            let mut state = lock(&self.state)?;
            if state.ready.as_ref().map(|ready| ready.version) != Some(version) {
                return Ok(());
            }
            state.next_version = state
                .next_version
                .checked_add(1)
                .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
            let retired = state.ready.take();
            state.rejected_version = None;
            state.cooldown = None;
            (state.in_flight.take().map(|active| active.flight), retired)
        };
        drop(retired);
        supersede_flight(flight)
    }

    fn reject_exact(&self, version: u64) -> Result<(), AuthError> {
        let retired = {
            let mut state = lock(&self.state)?;
            if state.ready.as_ref().map(|ready| ready.version) != Some(version) {
                return Ok(());
            }
            state.rejected_version = Some(version);
            (self.source == CredentialSource::Static)
                .then(|| state.ready.take())
                .flatten()
        };
        drop(retired);
        Ok(())
    }

    fn is_set(&self, now: Instant) -> bool {
        lock(&self.state).ok().is_some_and(|state| {
            state.in_flight.is_none()
                && state.cooldown.is_none()
                && state.ready.as_ref().is_some_and(|ready| {
                    state.rejected_version != Some(ready.version)
                        && ready.expires_at.is_none_or(|expires| expires > now)
                })
        })
    }

    fn cancel_leader(&self, flight: &Arc<Flight<M>>) {
        let removed = lock(&self.state).ok().and_then(|mut state| {
            let active = state
                .in_flight
                .as_ref()
                .filter(|active| Arc::ptr_eq(&active.flight, flight))?;
            let previous_rejected = active
                .previous_version
                .is_some_and(|version| state.rejected_version == Some(version));
            let active = state.in_flight.take()?;
            let retired = if previous_rejected {
                state.rejected_version = None;
                state.ready.take()
            } else {
                None
            };
            Some((active.flight, retired))
        });
        if let Some((removed, retired)) = removed {
            drop(removed);
            drop(retired);
            let _ = flight.publish(FlightResult::Reevaluate);
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
            if let Some(result) = self.result_now()? {
                return Ok(result);
            }
            notified.await;
        }
    }
}

fn flight_result<M>(result: &FlightResult<M>) -> FlightOutcome<M>
where
    M: CredentialMaterial,
{
    match result {
        FlightResult::Ready(ready) => FlightOutcome::Ready(ready.clone()),
        FlightResult::Failed(error) => FlightOutcome::Failed(error.clone_internal()),
        FlightResult::Reevaluate => FlightOutcome::Reevaluate,
    }
}

fn supersede_flight<M>(flight: Option<Arc<Flight<M>>>) -> Result<(), AuthError>
where
    M: CredentialMaterial,
{
    match flight {
        Some(flight) => flight.publish(FlightResult::Reevaluate),
        None => Ok(()),
    }
}

struct LeaderGuard<M>
where
    M: CredentialMaterial,
{
    slot: Arc<CredentialSlot<M>>,
    flight: Arc<Flight<M>>,
    finished: bool,
}

impl<M> LeaderGuard<M>
where
    M: CredentialMaterial,
{
    fn new(slot: Arc<CredentialSlot<M>>, flight: Arc<Flight<M>>) -> Self {
        Self {
            slot,
            flight,
            finished: false,
        }
    }
}

impl<M> Drop for LeaderGuard<M>
where
    M: CredentialMaterial,
{
    fn drop(&mut self) {
        if !self.finished {
            self.slot.cancel_leader(&self.flight);
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

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AuthError> {
    mutex
        .lock()
        .map_err(|_| AuthError::new(AuthErrorKind::Internal))
}

/// A typed read-only credential capability.
#[derive(Clone)]
pub struct CredentialRef<M> {
    pub(crate) selector: Arc<SlotSelector<M>>,
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
#[derive(Clone)]
pub struct CredentialHandle<M> {
    selector: Arc<SlotSelector<M>>,
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

    /// Atomically stores endpoint-acquired material.
    pub async fn set(&self, value: M) -> Result<(), AuthError> {
        self.selector.set(value)
    }

    /// Clears endpoint-acquired material.
    pub async fn clear(&self) -> Result<(), AuthError> {
        self.selector.clear()
    }

    /// Observes whether non-expired ready material is currently stored.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.selector.is_set()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;

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

        let unrepresentable =
            SlotSelector::provider(CooldownProvider(Duration::MAX), Duration::MAX);
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
}
