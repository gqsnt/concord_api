use super::errors::{AuthError, CredentialRefreshReason, InvalidateReason};
use super::future::AuthFuture;
use super::http::AuthHttpExecutor;
use super::ids::{AuthIdentity, CredentialId};
use crate::client::ClientContext;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};

pub trait CredentialMaterial: Clone + Send + Sync + 'static {
    fn expires_at(&self) -> Option<Instant> {
        None
    }

    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::Anonymous
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
    pub max_auth_retries: u8,
    pub retry_on_forbidden: bool,
    pub retry_on_challenge_rejection: bool,
    pub invalidate_on_unauthorized: bool,
    pub invalidate_on_forbidden: bool,
    pub invalidate_on_challenge_rejection: bool,
}

impl Default for AuthStepPolicy {
    fn default() -> Self {
        Self {
            refresh_skew: Duration::from_secs(60),
            retry_on_unauthorized: true,
            max_auth_retries: 1,
            retry_on_forbidden: false,
            retry_on_challenge_rejection: true,
            invalidate_on_unauthorized: true,
            invalidate_on_forbidden: false,
            invalidate_on_challenge_rejection: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CredentialLease<T> {
    pub value: T,
    pub generation: u64,
}

enum CredentialSlotState<T> {
    Empty,
    Valid {
        value: T,
        generation: u64,
    },
    Refreshing {
        notify: Arc<Notify>,
        generation: u64,
    },
    Failed {
        generation: u64,
        error: AuthError,
        retry_after: Option<Instant>,
    },
}

pub struct CredentialSlot<Cx: ClientContext, P: CredentialProvider<Cx>> {
    provider: P,
    state: Mutex<CredentialSlotState<P::Credential>>,
    _cx: PhantomData<Cx>,
}

enum SlotAction<T> {
    Wait(Arc<Notify>),
    Acquire {
        generation: u64,
        notify: Arc<Notify>,
    },
    Refresh {
        current: T,
        generation: u64,
        notify: Arc<Notify>,
        reason: CredentialRefreshReason,
    },
}

impl<Cx, P> CredentialSlot<Cx, P>
where
    Cx: ClientContext,
    P: CredentialProvider<Cx>,
{
    #[inline]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            state: Mutex::new(CredentialSlotState::Empty),
            _cx: PhantomData,
        }
    }

    #[inline]
    pub fn provider(&self) -> &P {
        &self.provider
    }

    #[inline]
    pub fn id(&self) -> CredentialId {
        self.provider.id()
    }

    pub async fn get_or_refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        policy: AuthStepPolicy,
    ) -> Result<CredentialLease<P::Credential>, AuthError> {
        loop {
            let action = {
                let mut state = self.state.lock().await;
                match &*state {
                    CredentialSlotState::Valid { value, generation }
                        if credential_refresh_reason(value, policy).is_none() =>
                    {
                        return Ok(CredentialLease {
                            value: value.clone(),
                            generation: *generation,
                        });
                    }
                    CredentialSlotState::Valid { value, generation } => {
                        let notify = Arc::new(Notify::new());
                        let current = value.clone();
                        let generation = *generation;
                        let reason = credential_refresh_reason(value, policy)
                            .unwrap_or(CredentialRefreshReason::ExpiringSoon);
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                            generation,
                        };
                        SlotAction::Refresh {
                            current,
                            generation,
                            notify,
                            reason,
                        }
                    }
                    CredentialSlotState::Refreshing { notify, .. } => {
                        SlotAction::Wait(notify.clone())
                    }
                    CredentialSlotState::Failed {
                        generation,
                        error,
                        retry_after,
                    } => {
                        if retry_after.is_some_and(|retry_at| retry_at > Instant::now()) {
                            return Err(error.clone());
                        }
                        let notify = Arc::new(Notify::new());
                        let generation = *generation;
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                            generation,
                        };
                        SlotAction::Acquire { generation, notify }
                    }
                    CredentialSlotState::Empty => {
                        let notify = Arc::new(Notify::new());
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                            generation: 0,
                        };
                        SlotAction::Acquire {
                            generation: 0,
                            notify,
                        }
                    }
                }
            };

            match action {
                SlotAction::Wait(notify) => {
                    notify.notified().await;
                }
                SlotAction::Acquire { generation, notify } => {
                    let result = self.provider.acquire(ctx.clone()).await;
                    return self.commit_slot_result(generation, notify, result).await;
                }
                SlotAction::Refresh {
                    current,
                    generation,
                    notify,
                    reason,
                } => {
                    let result = self
                        .provider
                        .refresh(ctx.with_reason(reason), &current)
                        .await;
                    return self.commit_slot_result(generation, notify, result).await;
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
        let current = {
            let mut state = self.state.lock().await;
            match &*state {
                CredentialSlotState::Valid {
                    value,
                    generation: current_generation,
                } if generation.is_none_or(|expected| expected == *current_generation) => {
                    let current = value.clone();
                    *state = CredentialSlotState::Empty;
                    Some(current)
                }
                _ => None,
            }
        };
        self.provider
            .invalidate(ctx, current.as_ref(), reason)
            .await
    }

    pub async fn set_manual(&self, value: P::Credential) {
        let notify = {
            let mut state = self.state.lock().await;
            let generation = next_generation(&*state);
            let notify = match &*state {
                CredentialSlotState::Refreshing { notify, .. } => Some(notify.clone()),
                _ => None,
            };
            *state = CredentialSlotState::Valid { value, generation };
            notify
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    pub async fn clear_manual(&self) {
        let notify = {
            let mut state = self.state.lock().await;
            let notify = match &*state {
                CredentialSlotState::Refreshing { notify, .. } => Some(notify.clone()),
                _ => None,
            };
            *state = CredentialSlotState::Empty;
            notify
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    pub async fn has_value(&self) -> bool {
        let state = self.state.lock().await;
        matches!(*state, CredentialSlotState::Valid { .. })
    }

    pub async fn get_cached(&self) -> Option<CredentialLease<P::Credential>> {
        let state = self.state.lock().await;
        match &*state {
            CredentialSlotState::Valid { value, generation } => Some(CredentialLease {
                value: value.clone(),
                generation: *generation,
            }),
            _ => None,
        }
    }

    async fn commit_slot_result(
        &self,
        previous_generation: u64,
        notify: Arc<Notify>,
        result: Result<P::Credential, AuthError>,
    ) -> Result<CredentialLease<P::Credential>, AuthError> {
        let mut state = self.state.lock().await;
        match result {
            Ok(value) => {
                let generation = previous_generation.saturating_add(1);
                *state = CredentialSlotState::Valid {
                    value: value.clone(),
                    generation,
                };
                notify.notify_waiters();
                Ok(CredentialLease { value, generation })
            }
            Err(error) => {
                *state = CredentialSlotState::Failed {
                    generation: previous_generation,
                    error: error.clone(),
                    retry_after: error.retry_after().map(|wait| Instant::now() + wait),
                };
                notify.notify_waiters();
                Err(error)
            }
        }
    }
}

fn credential_refresh_reason<T: CredentialMaterial>(
    value: &T,
    policy: AuthStepPolicy,
) -> Option<CredentialRefreshReason> {
    value.expires_at().and_then(|expires_at| {
        let now = Instant::now();
        if expires_at <= now {
            Some(CredentialRefreshReason::Expired)
        } else if expires_at <= now + policy.refresh_skew {
            Some(CredentialRefreshReason::ExpiringSoon)
        } else {
            None
        }
    })
}

fn next_generation<T>(state: &CredentialSlotState<T>) -> u64 {
    match state {
        CredentialSlotState::Empty => 1,
        CredentialSlotState::Valid { generation, .. }
        | CredentialSlotState::Refreshing { generation, .. }
        | CredentialSlotState::Failed { generation, .. } => generation.saturating_add(1),
    }
}
