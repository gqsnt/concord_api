use super::errors::{AuthError, AuthErrorKind, CredentialRefreshReason, InvalidateReason};
use super::future::AuthFuture;
use super::http::AuthHttpExecutor;
use super::ids::CredentialId;
use crate::client::ClientContext;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio::sync::futures::OwnedNotified;

pub trait CredentialMaterial: Clone + Send + Sync + 'static {
    fn expires_at(&self) -> Option<Instant> {
        None
    }
}

pub trait SecretCredential: CredentialMaterial {
    fn secret_value(&self) -> &str;
}

pub struct CredentialContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub executor: &'a dyn AuthHttpExecutor,
    pub credential_id: CredentialId,
    pub reason: CredentialRefreshReason,
}

impl<Cx: ClientContext> Clone for CredentialContext<'_, Cx> {
    fn clone(&self) -> Self {
        Self {
            vars: self.vars,
            auth: self.auth,
            auth_state: self.auth_state,
            executor: self.executor,
            credential_id: self.credential_id.clone(),
            reason: self.reason,
        }
    }
}

impl<'a, Cx: ClientContext> CredentialContext<'a, Cx> {
    #[inline]
    pub fn with_reason(&self, reason: CredentialRefreshReason) -> Self {
        Self {
            reason,
            ..self.clone()
        }
    }
}

pub trait CredentialProvider<Cx: ClientContext>: Send + Sync + 'static {
    type Credential: CredentialMaterial;

    fn id(&self) -> CredentialId;

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>>;

    fn refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: &'a Self::Credential,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let _ = current;
            self.acquire(ctx).await
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: Option<&'a Self::Credential>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            let _ = (ctx, current, reason);
            Ok(())
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AuthStepPolicy {
    pub refresh_skew: Duration,
    pub retry_on_unauthorized: bool,
    pub retry_on_forbidden: bool,
    pub invalidate_on_unauthorized: bool,
    pub invalidate_on_forbidden: bool,
}

impl Default for AuthStepPolicy {
    fn default() -> Self {
        Self {
            refresh_skew: Duration::from_secs(60),
            retry_on_unauthorized: true,
            retry_on_forbidden: true,
            invalidate_on_unauthorized: true,
            invalidate_on_forbidden: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CredentialLease<T> {
    pub value: T,
    pub generation: u64,
}

enum CredentialSlotState<T> {
    Empty {
        generation: u64,
    },
    Valid {
        value: T,
        generation: u64,
    },
    Refreshing {
        notify: Arc<Notify>,
        generation: u64,
        owner: RefreshOwnerId,
        previous: RefreshPrevious<T>,
    },
    Failed {
        generation: u64,
        error: AuthError,
        retry_after: Option<Instant>,
    },
}

struct CredentialSlotInner<T> {
    state: CredentialSlotState<T>,
    next_owner: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RefreshOwnerId(u64);

enum RefreshPrevious<T> {
    Empty,
    Valid {
        value: T,
        generation: u64,
    },
    Failed {
        error: AuthError,
        retry_after: Option<Instant>,
    },
}

pub struct CredentialSlot<Cx: ClientContext, P: CredentialProvider<Cx>> {
    id: CredentialId,
    provider: Option<P>,
    init_error: Option<AuthError>,
    inner: Arc<Mutex<CredentialSlotInner<P::Credential>>>,
    _cx: PhantomData<Cx>,
}

enum SlotAction<T> {
    Wait {
        notified: Pin<Box<OwnedNotified>>,
    },
    Acquire {
        generation: u64,
        guard: RefreshGuard<T>,
    },
    Refresh {
        current: T,
        generation: u64,
        guard: RefreshGuard<T>,
        reason: CredentialRefreshReason,
    },
}

enum CommitOutcome<T> {
    Stored(CredentialLease<T>),
    Failed(AuthError),
    StaleOwner,
}

struct RefreshGuard<T> {
    inner: Arc<Mutex<CredentialSlotInner<T>>>,
    owner: RefreshOwnerId,
    disarmed: bool,
}

impl<T> RefreshGuard<T> {
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl<T> Drop for RefreshGuard<T> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }

        let notify = {
            let mut inner = lock_slot_inner(&self.inner);
            match &inner.state {
                CredentialSlotState::Refreshing { owner, .. } if *owner == self.owner => {
                    match std::mem::replace(
                        &mut inner.state,
                        CredentialSlotState::Empty { generation: 0 },
                    ) {
                        CredentialSlotState::Refreshing {
                            notify,
                            generation,
                            previous,
                            ..
                        } => {
                            inner.state = previous.into_state_at(generation);
                            Some(notify)
                        }
                        state => {
                            inner.state = state;
                            None
                        }
                    }
                }
                _ => None,
            }
        };

        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }
}

impl<T> RefreshPrevious<T> {
    fn into_state_at(self, generation: u64) -> CredentialSlotState<T> {
        match self {
            Self::Empty => CredentialSlotState::Empty { generation },
            Self::Valid { value, .. } => CredentialSlotState::Valid { value, generation },
            Self::Failed { error, retry_after } => CredentialSlotState::Failed {
                generation,
                error,
                retry_after,
            },
        }
    }
}

impl<Cx, P> CredentialSlot<Cx, P>
where
    Cx: ClientContext,
    P: CredentialProvider<Cx>,
{
    fn provider_ref(&self) -> Result<&P, AuthError> {
        if let Some(error) = &self.init_error {
            return Err(error.clone());
        }

        self.provider
            .as_ref()
            .ok_or_else(|| AuthError::state_unavailable("credential slot provider missing"))
    }

    #[inline]
    pub fn new(provider: P) -> Self {
        Self {
            id: provider.id(),
            provider: Some(provider),
            init_error: None,
            inner: Arc::new(Mutex::new(CredentialSlotInner {
                state: CredentialSlotState::Empty { generation: 0 },
                next_owner: 1,
            })),
            _cx: PhantomData,
        }
    }

    #[inline]
    pub fn new_result(id: CredentialId, provider: Result<P, AuthError>) -> Self {
        let (provider, init_error) = match provider {
            Ok(provider) => (Some(provider), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            id,
            provider,
            init_error,
            inner: Arc::new(Mutex::new(CredentialSlotInner {
                state: CredentialSlotState::Empty { generation: 0 },
                next_owner: 1,
            })),
            _cx: PhantomData,
        }
    }

    #[inline]
    pub fn provider(&self) -> Option<&P> {
        self.provider.as_ref()
    }

    #[inline]
    pub fn id(&self) -> CredentialId {
        self.id.clone()
    }

    pub async fn get_or_refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        policy: AuthStepPolicy,
    ) -> Result<CredentialLease<P::Credential>, AuthError> {
        let provider = self.provider_ref()?;

        loop {
            let action = {
                let mut inner = lock_slot_inner(&self.inner);
                match &inner.state {
                    CredentialSlotState::Valid { value, generation }
                        if credential_refresh_reason(value, policy)?.is_none() =>
                    {
                        return Ok(CredentialLease {
                            value: value.clone(),
                            generation: *generation,
                        });
                    }
                    CredentialSlotState::Valid { value, generation } => {
                        let notify = Arc::new(Notify::new());
                        let current = value.clone();
                        let previous_generation = *generation;
                        let generation = next_generation(&inner.state)?;
                        let reason = credential_refresh_reason(value, policy)?
                            .unwrap_or(CredentialRefreshReason::ExpiringSoon);
                        let previous = RefreshPrevious::Valid {
                            value: value.clone(),
                            generation: previous_generation,
                        };
                        let owner = inner.next_refresh_owner()?;
                        inner.state = CredentialSlotState::Refreshing {
                            notify,
                            generation,
                            owner,
                            previous,
                        };
                        SlotAction::Refresh {
                            current,
                            generation,
                            guard: RefreshGuard {
                                inner: self.inner.clone(),
                                owner,
                                disarmed: false,
                            },
                            reason,
                        }
                    }
                    CredentialSlotState::Refreshing { notify, owner, .. } => {
                        let owner = *owner;
                        let mut notified = Box::pin(notify.clone().notified_owned());
                        notified.as_mut().enable();
                        let should_wait = matches!(
                            &inner.state,
                            CredentialSlotState::Refreshing {
                                owner: current_owner,
                                ..
                            } if *current_owner == owner
                        );
                        notify_refresh_wait_registered();
                        if should_wait {
                            SlotAction::Wait { notified }
                        } else {
                            continue;
                        }
                    }
                    CredentialSlotState::Failed {
                        generation: _,
                        error,
                        retry_after,
                    } => {
                        if retry_after.is_some_and(|retry_at| retry_at > Instant::now()) {
                            return Err(error.clone());
                        }
                        let notify = Arc::new(Notify::new());
                        let generation = next_generation(&inner.state)?;
                        let previous = RefreshPrevious::Failed {
                            error: error.clone(),
                            retry_after: *retry_after,
                        };
                        let owner = inner.next_refresh_owner()?;
                        inner.state = CredentialSlotState::Refreshing {
                            notify,
                            generation,
                            owner,
                            previous,
                        };
                        SlotAction::Acquire {
                            generation,
                            guard: RefreshGuard {
                                inner: self.inner.clone(),
                                owner,
                                disarmed: false,
                            },
                        }
                    }
                    CredentialSlotState::Empty { .. } => {
                        let notify = Arc::new(Notify::new());
                        let owner = inner.next_refresh_owner()?;
                        let generation = next_generation(&inner.state)?;
                        inner.state = CredentialSlotState::Refreshing {
                            notify,
                            generation,
                            owner,
                            previous: RefreshPrevious::Empty,
                        };
                        SlotAction::Acquire {
                            generation,
                            guard: RefreshGuard {
                                inner: self.inner.clone(),
                                owner,
                                disarmed: false,
                            },
                        }
                    }
                }
            };

            match action {
                SlotAction::Wait { notified } => {
                    notified.await;
                }
                SlotAction::Acquire {
                    generation,
                    mut guard,
                } => {
                    let result = provider.acquire(ctx.clone()).await;
                    match self.commit_slot_result(generation, &mut guard, result)? {
                        CommitOutcome::Stored(lease) => return Ok(lease),
                        CommitOutcome::Failed(error) => return Err(error),
                        CommitOutcome::StaleOwner => continue,
                    }
                }
                SlotAction::Refresh {
                    current,
                    generation,
                    mut guard,
                    reason,
                } => {
                    let result = provider.refresh(ctx.with_reason(reason), &current).await;
                    match self.commit_slot_result(generation, &mut guard, result)? {
                        CommitOutcome::Stored(lease) => return Ok(lease),
                        CommitOutcome::Failed(error) => return Err(error),
                        CommitOutcome::StaleOwner => continue,
                    }
                }
            }
        }
    }

    pub async fn invalidate_generation<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        generation: Option<u64>,
        reason: InvalidateReason,
    ) -> Result<(), AuthError> {
        let provider = self.provider_ref()?;
        let current = self.invalidate_generation_state(generation)?;
        provider.invalidate(ctx, current.as_ref(), reason).await
    }

    /// Invalidates only the local slot state. This is intentionally separate
    /// from provider invalidation so terminal response handling cannot access
    /// an executor or initiate network/provider work.
    pub fn invalidate_generation_local(&self, generation: Option<u64>) -> Result<(), AuthError> {
        let _ = self.invalidate_generation_state(generation)?;
        Ok(())
    }

    fn invalidate_generation_state(
        &self,
        generation: Option<u64>,
    ) -> Result<Option<P::Credential>, AuthError> {
        Ok({
            let mut inner = lock_slot_inner(&self.inner);
            match &mut inner.state {
                CredentialSlotState::Valid {
                    value,
                    generation: current_generation,
                } if generation.is_none_or(|expected| expected == *current_generation) => {
                    let current = value.clone();
                    let next_generation = current_generation.checked_add(1).ok_or_else(|| {
                        AuthError::new(
                            AuthErrorKind::AcquireFailed,
                            "credential generation counter overflowed",
                        )
                    })?;
                    inner.state = CredentialSlotState::Empty {
                        generation: next_generation,
                    };
                    Some(current)
                }
                CredentialSlotState::Refreshing {
                    generation: _,
                    previous,
                    ..
                } => {
                    let current = match previous {
                        RefreshPrevious::Valid {
                            value,
                            generation: previous_generation,
                        } if generation.is_none_or(|expected| expected == *previous_generation) => {
                            Some(value.clone())
                        }
                        _ => None,
                    };
                    if current.is_some() {
                        *previous = RefreshPrevious::Empty;
                    }
                    current
                }
                _ => None,
            }
        })
    }

    pub async fn set_manual(&self, value: P::Credential) -> Result<(), AuthError> {
        if let Some(error) = &self.init_error {
            return Err(error.clone());
        }

        let notify = {
            let mut inner = lock_slot_inner(&self.inner);
            let generation = next_generation(&inner.state)?;
            let notify = match &inner.state {
                CredentialSlotState::Refreshing { notify, .. } => Some(notify.clone()),
                _ => None,
            };
            inner.state = CredentialSlotState::Valid { value, generation };
            notify
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
        Ok(())
    }

    pub async fn clear_manual(&self) -> Result<(), AuthError> {
        if let Some(error) = &self.init_error {
            return Err(error.clone());
        }

        let notify = {
            let mut inner = lock_slot_inner(&self.inner);
            let generation = next_generation(&inner.state)?;
            let notify = match &inner.state {
                CredentialSlotState::Refreshing { notify, .. } => Some(notify.clone()),
                _ => None,
            };
            inner.state = CredentialSlotState::Empty { generation };
            notify
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
        Ok(())
    }

    pub async fn has_value(&self) -> bool {
        if self.init_error.is_some() {
            return false;
        }
        let inner = lock_slot_inner(&self.inner);
        matches!(inner.state, CredentialSlotState::Valid { .. })
    }

    pub async fn get_cached(&self) -> Option<CredentialLease<P::Credential>> {
        if self.init_error.is_some() {
            return None;
        }
        let inner = lock_slot_inner(&self.inner);
        match &inner.state {
            CredentialSlotState::Valid { value, generation } => Some(CredentialLease {
                value: value.clone(),
                generation: *generation,
            }),
            _ => None,
        }
    }

    fn commit_slot_result(
        &self,
        attempt_generation: u64,
        guard: &mut RefreshGuard<P::Credential>,
        result: Result<P::Credential, AuthError>,
    ) -> Result<CommitOutcome<P::Credential>, AuthError> {
        let notify = {
            let inner = lock_slot_inner(&self.inner);
            match &inner.state {
                CredentialSlotState::Refreshing {
                    notify,
                    owner,
                    generation,
                    ..
                } if *owner == guard.owner && *generation == attempt_generation => notify.clone(),
                _ => {
                    guard.disarm();
                    return Ok(CommitOutcome::StaleOwner);
                }
            }
        };

        let mut inner = lock_slot_inner(&self.inner);
        match &inner.state {
            CredentialSlotState::Refreshing {
                owner, generation, ..
            } if *owner == guard.owner && *generation == attempt_generation => {}
            _ => {
                guard.disarm();
                return Ok(CommitOutcome::StaleOwner);
            }
        }

        match result {
            Ok(value) => {
                inner.state = CredentialSlotState::Valid {
                    value: value.clone(),
                    generation: attempt_generation,
                };
                guard.disarm();
                notify.notify_waiters();
                Ok(CommitOutcome::Stored(CredentialLease {
                    value,
                    generation: attempt_generation,
                }))
            }
            Err(error) => {
                inner.state = CredentialSlotState::Failed {
                    generation: attempt_generation,
                    error: error.clone(),
                    retry_after: error
                        .retry_after()
                        .map(|wait| {
                            checked_auth_instant_add(
                                Instant::now(),
                                wait,
                                "auth retry-after overflowed",
                            )
                        })
                        .transpose()?,
                };
                guard.disarm();
                notify.notify_waiters();
                Ok(CommitOutcome::Failed(error))
            }
        }
    }
}

impl<T> CredentialSlotInner<T> {
    fn next_refresh_owner(&mut self) -> Result<RefreshOwnerId, AuthError> {
        let owner = self.next_owner;
        self.next_owner = self.next_owner.checked_add(1).ok_or_else(|| {
            AuthError::new(
                AuthErrorKind::AcquireFailed,
                "credential refresh owner counter overflowed",
            )
        })?;
        Ok(RefreshOwnerId(owner))
    }
}

fn lock_slot_inner<T>(
    inner: &Arc<Mutex<CredentialSlotInner<T>>>,
) -> MutexGuard<'_, CredentialSlotInner<T>> {
    inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
type RefreshWaitHook = Arc<dyn Fn() + Send + Sync>;

#[cfg(test)]
static REFRESH_WAIT_HOOK: Mutex<Option<RefreshWaitHook>> = Mutex::new(None);

#[cfg(test)]
fn notify_refresh_wait_registered() {
    let hook = REFRESH_WAIT_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(not(test))]
fn notify_refresh_wait_registered() {}

fn credential_refresh_reason<T: CredentialMaterial>(
    value: &T,
    policy: AuthStepPolicy,
) -> Result<Option<CredentialRefreshReason>, AuthError> {
    value
        .expires_at()
        .map(|expires_at| {
            let now = Instant::now();
            if expires_at <= now {
                Ok(Some(CredentialRefreshReason::Expired))
            } else if expires_at
                <= checked_auth_instant_add(
                    now,
                    policy.refresh_skew,
                    "auth refresh_skew overflowed",
                )?
            {
                Ok(Some(CredentialRefreshReason::ExpiringSoon))
            } else {
                Ok(None)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn checked_auth_instant_add(
    base: Instant,
    duration: Duration,
    context: &'static str,
) -> Result<Instant, AuthError> {
    base.checked_add(duration)
        .ok_or_else(|| AuthError::new(AuthErrorKind::InvalidConfiguration, context))
}

fn next_generation<T>(state: &CredentialSlotState<T>) -> Result<u64, AuthError> {
    match state {
        CredentialSlotState::Empty { generation }
        | CredentialSlotState::Valid { generation, .. }
        | CredentialSlotState::Refreshing { generation, .. }
        | CredentialSlotState::Failed { generation, .. } => {
            generation.checked_add(1).ok_or_else(|| {
                AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    "credential generation counter overflowed",
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::{AuthHttpRequest, AuthHttpResponse};
    use super::*;
    use http::uri::Scheme;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex as AsyncMutex, Notify as AsyncNotify, oneshot};
    use tokio::time::{Duration as TokioDuration, timeout};

    static TEST_UNIT: () = ();
    static NOOP_EXECUTOR: NoopExecutor = NoopExecutor;
    static HOOK_TEST_LOCK: AsyncMutex<()> = AsyncMutex::const_new(());

    #[test]
    fn credential_generation_overflow_returns_auth_error() {
        let state = CredentialSlotState::<()>::Valid {
            value: (),
            generation: u64::MAX,
        };

        let err = next_generation(&state)
            .expect_err("overflowing credential generation should return auth error");
        assert_eq!(err.kind, AuthErrorKind::AcquireFailed);
        assert!(
            err.to_string()
                .contains("credential generation counter overflowed")
        );
    }

    #[tokio::test]
    async fn leader_cancellation_during_acquire_recovers() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));

        let first = provider.enqueue_acquire().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;
        leader.abort();
        let _ = leader.await;
        assert!(first.send(Ok(TestCredential::fresh("cancelled"))).is_err());

        let second = provider.enqueue_acquire().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(2).await;
        second
            .send(Ok(TestCredential::fresh("second")))
            .expect("second acquire should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("later caller should not hang")
            .expect("caller task should not panic")
            .expect("later acquire should succeed");
        assert_eq!(lease.value.value, "second");
        assert_eq!(lease.generation, 2);
        assert!(slot.has_value().await);
        assert_eq!(provider.acquire_count(), 2);
    }

    #[tokio::test]
    async fn leader_cancellation_during_refresh_restores_previous_valid_and_recovers() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        slot.set_manual(TestCredential::expired("old"))
            .await
            .expect("manual credential should be stored");

        let first = provider.enqueue_refresh().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_refresh_count(1).await;
        leader.abort();
        let _ = leader.await;
        assert!(first.send(Ok(TestCredential::fresh("cancelled"))).is_err());

        let cached = slot
            .get_cached()
            .await
            .expect("cancelled refresh should restore previous credential");
        assert_eq!(cached.value.value, "old");

        let second = provider.enqueue_refresh().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_refresh_count(2).await;
        second
            .send(Ok(TestCredential::fresh("new")))
            .expect("second refresh should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("later caller should not hang")
            .expect("caller task should not panic")
            .expect("refresh retry should succeed");
        assert_eq!(lease.value.value, "new");
        assert!(slot.has_value().await);
        assert_eq!(provider.refresh_count(), 2);
    }

    #[tokio::test]
    async fn invalidate_generation_during_refresh_prevents_cancelled_refresh_from_restoring_invalidated_valid()
     {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        slot.set_manual(TestCredential::expired("old"))
            .await
            .expect("manual credential should be stored");
        let old_generation = slot
            .get_cached()
            .await
            .expect("manual credential should be cached")
            .generation;

        let refresh_release = provider.enqueue_refresh().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_refresh_count(1).await;

        slot.invalidate_generation(
            test_context(),
            Some(old_generation),
            InvalidateReason::Unauthorized,
        )
        .await
        .expect("invalidation should succeed");

        leader.abort();
        let _ = leader.await;
        assert!(
            refresh_release
                .send(Ok(TestCredential::fresh("cancelled")))
                .is_err()
        );
        assert!(
            slot.get_cached().await.is_none(),
            "cancelled refresh must not restore invalidated old credential"
        );

        let acquire_release = provider.enqueue_acquire().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;
        acquire_release
            .send(Ok(TestCredential::fresh("new")))
            .expect("acquire after invalidation should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("caller should not hang after invalidation rollback")
            .expect("caller task should not panic")
            .expect("caller should acquire replacement credential");
        assert_eq!(lease.value.value, "new");
        assert_ne!(lease.value.value, "old");
        assert!(
            lease.generation > old_generation,
            "replacement credential must not reuse invalidated generation"
        );
    }

    #[tokio::test]
    async fn invalidated_previous_generation_is_not_resurrected_by_refresh_cancellation() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        slot.set_manual(TestCredential::expired("OLD_TOKEN_MUST_NOT_RESURRECT"))
            .await
            .expect("old credential should be stored");
        let old_generation = slot
            .get_cached()
            .await
            .expect("old credential should be cached")
            .generation;

        let refresh_release = provider.enqueue_refresh().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_refresh_count(1).await;

        slot.invalidate_generation(
            test_context(),
            Some(old_generation),
            InvalidateReason::Unauthorized,
        )
        .await
        .expect("in-flight previous generation invalidation should succeed");

        leader.abort();
        let _ = leader.await;
        assert!(
            refresh_release
                .send(Ok(TestCredential::fresh(
                    "STALE_REFRESH_AFTER_CANCEL_MUST_NOT_INSTALL"
                )))
                .is_err()
        );
        assert!(
            slot.get_cached().await.is_none(),
            "cancelled refresh rollback must not resurrect invalidated old credential"
        );

        let acquire_release = provider.enqueue_acquire().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;
        acquire_release
            .send(Ok(TestCredential::fresh(
                "NEW_TOKEN_AFTER_CANCELLED_REFRESH",
            )))
            .expect("replacement acquire should be waiting");

        let lease = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("replacement acquire should not hang")
            .expect("caller task should not panic")
            .expect("replacement acquire should succeed");
        assert_eq!(lease.value.value, "NEW_TOKEN_AFTER_CANCELLED_REFRESH");
        assert!(
            lease.generation > old_generation + 1,
            "replacement generation must advance past the cancelled refresh attempt"
        );
    }

    #[tokio::test]
    async fn generation_increases_across_invalidation_and_regeneration() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        slot.set_manual(TestCredential::fresh("first"))
            .await
            .expect("manual credential should be stored");
        let first = slot
            .get_cached()
            .await
            .expect("first credential should be cached");
        assert_eq!(first.generation, 1);

        slot.invalidate_generation(
            test_context(),
            Some(first.generation),
            InvalidateReason::Unauthorized,
        )
        .await
        .expect("matching generation invalidation should succeed");
        assert!(slot.get_cached().await.is_none());

        let release = provider.enqueue_acquire().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;
        release
            .send(Ok(TestCredential::fresh("second")))
            .expect("replacement acquire should be waiting");
        let second = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("replacement acquire should not hang")
            .expect("caller task should not panic")
            .expect("replacement acquire should succeed");

        assert_eq!(second.value.value, "second");
        assert!(
            second.generation > first.generation,
            "slot generation must increase after invalidation"
        );
    }

    #[tokio::test]
    async fn stale_invalidation_does_not_clear_newer_credential() {
        let provider = TestProvider::new();
        let slot = CredentialSlot::<TestCx, _>::new(provider);
        slot.set_manual(TestCredential::fresh("OLD_TOKEN_SHOULD_NOT_SURVIVE"))
            .await
            .expect("old credential should be stored");
        let old_generation = slot
            .get_cached()
            .await
            .expect("old credential should be cached")
            .generation;

        slot.set_manual(TestCredential::fresh("NEW_TOKEN_SHOULD_REMAIN"))
            .await
            .expect("new credential should be stored");
        let new_generation = slot
            .get_cached()
            .await
            .expect("new credential should be cached")
            .generation;
        assert!(new_generation > old_generation);

        slot.invalidate_generation(
            test_context(),
            Some(old_generation),
            InvalidateReason::Unauthorized,
        )
        .await
        .expect("stale invalidation should be ignored safely");

        let cached = slot
            .get_cached()
            .await
            .expect("newer credential must remain cached");
        assert_eq!(cached.value.value, "NEW_TOKEN_SHOULD_REMAIN");
        assert_eq!(cached.generation, new_generation);
    }

    #[tokio::test]
    async fn leader_cancellation_during_retry_from_failed_restores_failed_and_recovers() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));

        let first = provider.enqueue_acquire().await;
        let initial = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;
        first
            .send(Err(AuthError::new(
                AuthErrorKind::AcquireFailed,
                "first failed",
            )))
            .expect("initial acquire should still be waiting");
        let err = initial
            .await
            .expect("initial task should not panic")
            .expect_err("initial acquire should fail");
        assert_eq!(err.kind, AuthErrorKind::AcquireFailed);

        let retry = provider.enqueue_acquire().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(2).await;
        leader.abort();
        let _ = leader.await;
        assert!(retry.send(Ok(TestCredential::fresh("cancelled"))).is_err());

        let recovery = provider.enqueue_acquire().await;
        let caller = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(3).await;
        recovery
            .send(Ok(TestCredential::fresh("recovered")))
            .expect("recovery acquire should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), caller)
            .await
            .expect("caller should not hang after failed retry cancellation")
            .expect("caller task should not panic")
            .expect("caller should recover from restored failed state");
        assert_eq!(lease.value.value, "recovered");
        assert!(slot.has_value().await);
        assert_eq!(provider.acquire_count(), 3);
    }

    #[tokio::test]
    async fn waiter_cannot_miss_refresh_completion_notification() {
        let _hook_guard = HOOK_TEST_LOCK.lock().await;
        set_refresh_wait_hook_none();
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        let release = provider.enqueue_acquire().await;

        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;

        let (registered_tx, registered_rx) = oneshot::channel();
        let registered_tx = Arc::new(Mutex::new(Some(registered_tx)));
        set_refresh_wait_hook({
            let registered_tx = registered_tx.clone();
            Arc::new(move || {
                if let Some(tx) = registered_tx
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = tx.send(());
                }
            })
        });

        let follower = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        registered_rx
            .await
            .expect("follower should reach registered wait path");
        release
            .send(Ok(TestCredential::fresh("shared")))
            .expect("leader acquire should still be waiting");
        set_refresh_wait_hook_none();

        let leader = timeout(TokioDuration::from_secs(1), leader)
            .await
            .expect("leader should complete")
            .expect("leader task should not panic")
            .expect("leader should acquire");
        let follower = timeout(TokioDuration::from_secs(1), follower)
            .await
            .expect("follower should complete")
            .expect("follower task should not panic")
            .expect("follower should observe stored credential");

        assert_eq!(leader.value.value, "shared");
        assert_eq!(follower.value.value, "shared");
        assert_eq!(provider.acquire_count(), 1);
    }

    #[tokio::test]
    async fn waiters_are_notified_on_cancellation_rollback() {
        let _hook_guard = HOOK_TEST_LOCK.lock().await;
        set_refresh_wait_hook_none();
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        let first = provider.enqueue_acquire().await;

        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;

        let (registered_tx, registered_rx) = oneshot::channel();
        let registered_tx = Arc::new(Mutex::new(Some(registered_tx)));
        set_refresh_wait_hook({
            let registered_tx = registered_tx.clone();
            Arc::new(move || {
                if let Some(tx) = registered_tx
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = tx.send(());
                }
            })
        });

        let follower = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        registered_rx
            .await
            .expect("follower should reach registered wait path");

        let second = provider.enqueue_acquire().await;
        leader.abort();
        let _ = leader.await;
        assert!(first.send(Ok(TestCredential::fresh("cancelled"))).is_err());
        set_refresh_wait_hook_none();

        provider.wait_for_acquire_count(2).await;
        second
            .send(Ok(TestCredential::fresh("recovered")))
            .expect("follower retry acquire should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), follower)
            .await
            .expect("follower should not hang after rollback")
            .expect("follower task should not panic")
            .expect("follower retry should succeed");
        assert_eq!(lease.value.value, "recovered");
        assert!(slot.has_value().await);
    }

    #[tokio::test]
    async fn stale_acquire_owner_cannot_overwrite_manual_set() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        let release = provider.enqueue_acquire().await;

        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_acquire_count(1).await;

        slot.set_manual(TestCredential::fresh("manual"))
            .await
            .expect("manual set should succeed");
        release
            .send(Ok(TestCredential::fresh("stale")))
            .expect("stale acquire should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), leader)
            .await
            .expect("stale owner should not hang")
            .expect("leader task should not panic")
            .expect("leader should observe manual credential after stale commit");
        assert_eq!(lease.value.value, "manual");

        let cached = slot
            .get_cached()
            .await
            .expect("manual credential should remain cached");
        assert_eq!(cached.value.value, "manual");
    }

    #[tokio::test]
    async fn stale_refresh_owner_cannot_overwrite_newer_manual_set() {
        let provider = TestProvider::new();
        let slot = Arc::new(CredentialSlot::<TestCx, _>::new(provider.clone()));
        slot.set_manual(TestCredential::expired("old"))
            .await
            .expect("old credential should be stored");

        let release = provider.enqueue_refresh().await;
        let leader = {
            let slot = slot.clone();
            tokio::spawn(async move {
                slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                    .await
            })
        };
        provider.wait_for_refresh_count(1).await;

        slot.set_manual(TestCredential::fresh("FRESH_TOKEN_MUST_INSTALL"))
            .await
            .expect("new manual credential should be stored");
        release
            .send(Ok(TestCredential::fresh("STALE_TOKEN_MUST_NOT_INSTALL")))
            .expect("stale refresh should still be waiting");

        let lease = timeout(TokioDuration::from_secs(1), leader)
            .await
            .expect("stale refresh owner should not hang")
            .expect("leader task should not panic")
            .expect("leader should observe newer manual credential after stale commit");
        assert_eq!(lease.value.value, "FRESH_TOKEN_MUST_INSTALL");

        let cached = slot
            .get_cached()
            .await
            .expect("newer manual credential should remain cached");
        assert_eq!(cached.value.value, "FRESH_TOKEN_MUST_INSTALL");
    }

    #[tokio::test]
    async fn auth_retry_after_overflow_returns_typed_error() {
        let provider = TestProvider::new();
        let slot = CredentialSlot::<TestCx, _>::new(provider.clone());
        let release = provider.enqueue_acquire().await;

        let caller = tokio::spawn(async move {
            slot.get_or_refresh(test_context(), AuthStepPolicy::default())
                .await
        });
        provider.wait_for_acquire_count(1).await;
        release
            .send(Err(AuthError::new(AuthErrorKind::AcquireFailed, "wait")
                .with_retry_after(Duration::MAX)))
            .expect("acquire should be waiting");

        let err = caller
            .await
            .expect("task should not panic")
            .expect_err("overflowing retry-after should fail");
        assert_eq!(err.kind, AuthErrorKind::InvalidConfiguration);
        assert!(err.to_string().contains("auth retry-after overflowed"));
    }

    #[tokio::test]
    async fn refresh_skew_overflow_returns_typed_error() {
        let provider = TestProvider::new();
        let slot = CredentialSlot::<TestCx, _>::new(provider);
        slot.set_manual(TestCredential::fresh("cached"))
            .await
            .expect("manual credential should be stored");

        let err = slot
            .get_or_refresh(
                test_context(),
                AuthStepPolicy {
                    refresh_skew: Duration::MAX,
                    ..AuthStepPolicy::default()
                },
            )
            .await
            .expect_err("overflowing refresh skew should fail");
        assert_eq!(err.kind, AuthErrorKind::InvalidConfiguration);
        assert!(err.to_string().contains("auth refresh_skew overflowed"));
    }

    #[derive(Clone, Debug)]
    struct TestCredential {
        value: &'static str,
        expires_at: Option<Instant>,
    }

    impl TestCredential {
        fn fresh(value: &'static str) -> Self {
            Self {
                value,
                expires_at: Some(Instant::now() + Duration::from_secs(3600)),
            }
        }

        fn expired(value: &'static str) -> Self {
            Self {
                value,
                expires_at: Some(Instant::now() - Duration::from_secs(1)),
            }
        }
    }

    impl CredentialMaterial for TestCredential {
        fn expires_at(&self) -> Option<Instant> {
            self.expires_at
        }
    }

    #[derive(Clone)]
    struct TestProvider {
        inner: Arc<TestProviderInner>,
    }

    struct TestProviderInner {
        acquire_count: AtomicUsize,
        refresh_count: AtomicUsize,
        acquire_started: AsyncNotify,
        refresh_started: AsyncNotify,
        acquire_releases:
            AsyncMutex<VecDeque<oneshot::Receiver<Result<TestCredential, AuthError>>>>,
        refresh_releases:
            AsyncMutex<VecDeque<oneshot::Receiver<Result<TestCredential, AuthError>>>>,
    }

    impl TestProvider {
        fn new() -> Self {
            Self {
                inner: Arc::new(TestProviderInner {
                    acquire_count: AtomicUsize::new(0),
                    refresh_count: AtomicUsize::new(0),
                    acquire_started: AsyncNotify::new(),
                    refresh_started: AsyncNotify::new(),
                    acquire_releases: AsyncMutex::new(VecDeque::new()),
                    refresh_releases: AsyncMutex::new(VecDeque::new()),
                }),
            }
        }

        async fn enqueue_acquire(&self) -> oneshot::Sender<Result<TestCredential, AuthError>> {
            let (tx, rx) = oneshot::channel();
            self.inner.acquire_releases.lock().await.push_back(rx);
            tx
        }

        async fn enqueue_refresh(&self) -> oneshot::Sender<Result<TestCredential, AuthError>> {
            let (tx, rx) = oneshot::channel();
            self.inner.refresh_releases.lock().await.push_back(rx);
            tx
        }

        fn acquire_count(&self) -> usize {
            self.inner.acquire_count.load(Ordering::SeqCst)
        }

        fn refresh_count(&self) -> usize {
            self.inner.refresh_count.load(Ordering::SeqCst)
        }

        async fn wait_for_acquire_count(&self, expected: usize) {
            loop {
                if self.acquire_count() >= expected {
                    return;
                }
                self.inner.acquire_started.notified().await;
            }
        }

        async fn wait_for_refresh_count(&self, expected: usize) {
            loop {
                if self.refresh_count() >= expected {
                    return;
                }
                self.inner.refresh_started.notified().await;
            }
        }
    }

    impl CredentialProvider<TestCx> for TestProvider {
        type Credential = TestCredential;

        fn id(&self) -> CredentialId {
            CredentialId::new("test", "credential")
        }

        fn acquire<'a>(
            &'a self,
            _ctx: CredentialContext<'a, TestCx>,
        ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
            Box::pin(async move {
                self.inner.acquire_count.fetch_add(1, Ordering::SeqCst);
                self.inner.acquire_started.notify_waiters();
                let release = self
                    .inner
                    .acquire_releases
                    .lock()
                    .await
                    .pop_front()
                    .expect("test acquire release should be queued");
                release
                    .await
                    .expect("test acquire sender should not be dropped")
            })
        }

        fn refresh<'a>(
            &'a self,
            _ctx: CredentialContext<'a, TestCx>,
            _current: &'a Self::Credential,
        ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
            Box::pin(async move {
                self.inner.refresh_count.fetch_add(1, Ordering::SeqCst);
                self.inner.refresh_started.notify_waiters();
                let release = self
                    .inner
                    .refresh_releases
                    .lock()
                    .await
                    .pop_front()
                    .expect("test refresh release should be queued");
                release
                    .await
                    .expect("test refresh sender should not be dropped")
            })
        }
    }

    #[derive(Clone)]
    struct TestCx;

    impl ClientContext for TestCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = ();
        const SCHEME: Scheme = Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
    }

    struct NoopExecutor;

    impl AuthHttpExecutor for NoopExecutor {
        fn send<'a>(
            &'a self,
            _req: AuthHttpRequest,
        ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>> {
            Box::pin(async {
                Err(AuthError::new(
                    AuthErrorKind::UnsupportedScheme,
                    "test executor does not send requests",
                ))
            })
        }
    }

    fn test_context() -> CredentialContext<'static, TestCx> {
        CredentialContext {
            vars: &TEST_UNIT,
            auth: &TEST_UNIT,
            auth_state: &TEST_UNIT,
            executor: &NOOP_EXECUTOR,
            credential_id: CredentialId::new("test", "credential"),
            reason: CredentialRefreshReason::Missing,
        }
    }

    fn set_refresh_wait_hook(hook: RefreshWaitHook) {
        *REFRESH_WAIT_HOOK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(hook);
    }

    fn set_refresh_wait_hook_none() {
        *REFRESH_WAIT_HOOK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}
