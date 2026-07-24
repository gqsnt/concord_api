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
mod tests;
