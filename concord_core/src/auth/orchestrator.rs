use super::{
    AuthApplication, AuthApplicationRequest, AuthAppliedCredential, AuthError, AuthErrorKind,
    AuthFuture, AuthHttpExecutor, AuthPreparationReuse, AuthRejectionAction, AuthRequirement,
    AuthStepPolicy, BasicCredential, CredentialContext, CredentialLease, CredentialProvider,
    CredentialRefreshReason, CredentialSlot, InvalidateReason, SecretCredential,
    apply_basic_credential, apply_secret_credential, auth_decision_for_status,
};
use crate::client::ClientContext;
use std::any::Any;

struct ErasedCredentialLease {
    value: Box<dyn Any + Send + Sync>,
    generation: u64,
}

trait ErasedCredentialSlot<Cx: ClientContext>: Send + Sync {
    fn id(&self) -> super::CredentialId;

    fn get_or_refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        policy: AuthStepPolicy,
    ) -> AuthFuture<'a, Result<ErasedCredentialLease, AuthError>>;

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        generation: Option<u64>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>>;

    fn invalidate_local(&self, generation: Option<u64>) -> Result<(), AuthError>;
}

impl<Cx, P> ErasedCredentialSlot<Cx> for CredentialSlot<Cx, P>
where
    Cx: ClientContext,
    P: CredentialProvider<Cx>,
{
    fn id(&self) -> super::CredentialId {
        self.id()
    }

    fn get_or_refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        policy: AuthStepPolicy,
    ) -> AuthFuture<'a, Result<ErasedCredentialLease, AuthError>> {
        Box::pin(async move {
            let CredentialLease { value, generation } = self.get_or_refresh(ctx, policy).await?;
            Ok(ErasedCredentialLease {
                value: Box::new(value),
                generation,
            })
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        generation: Option<u64>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move { self.invalidate_generation(ctx, generation, reason).await })
    }

    fn invalidate_local(&self, generation: Option<u64>) -> Result<(), AuthError> {
        self.invalidate_generation_local(generation)
    }
}

type Materializer = fn(
    &dyn Any,
    &mut AuthApplicationRequest<'_>,
    &AuthRequirement,
) -> Result<AuthApplication, AuthError>;

fn materialize_secret<M: SecretCredential>(
    value: &dyn Any,
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
) -> Result<AuthApplication, AuthError> {
    let value = value.downcast_ref::<M>().ok_or_else(|| {
        AuthError::new(
            AuthErrorKind::InvalidConfiguration,
            "credential binding produced an incompatible secret material type",
        )
    })?;
    apply_secret_credential(request, requirement, value)
}

fn materialize_basic(
    value: &dyn Any,
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
) -> Result<AuthApplication, AuthError> {
    let value = value.downcast_ref::<BasicCredential>().ok_or_else(|| {
        AuthError::new(
            AuthErrorKind::InvalidConfiguration,
            "credential binding produced an incompatible basic material type",
        )
    })?;
    apply_basic_credential(request, requirement, value)
}

/// Opaque generated binding between one typed provider slot and core's
/// authentication lifecycle engine.
///
/// The adapter borrows existing state. It contains no credential value,
/// provider instance, cache entry, lock, future, or HTTP executor.
#[doc(hidden)]
pub struct AuthProviderBinding<'a, Cx: ClientContext> {
    slot: &'a dyn ErasedCredentialSlot<Cx>,
    materializer: Materializer,
    reuse: AuthPreparationReuse,
    refresh_on_challenge: bool,
}

impl<Cx: ClientContext> Copy for AuthProviderBinding<'_, Cx> {}

impl<Cx: ClientContext> Clone for AuthProviderBinding<'_, Cx> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Secret-free request-local credential reuse metadata emitted by macros.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthPreparationMode {
    PerAttempt,
    RequestLocal,
}

/// Secret-free challenge capability for one generated provider binding.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengeMode {
    InvalidateOnly,
    Refresh,
}

impl<'a, Cx: ClientContext> AuthProviderBinding<'a, Cx> {
    /// Binds a provider whose material is inserted as a bearer, custom-header,
    /// or query secret.
    #[doc(hidden)]
    pub fn secret<P>(
        slot: &'a CredentialSlot<Cx, P>,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> Self
    where
        P: CredentialProvider<Cx>,
        P::Credential: SecretCredential,
    {
        Self {
            slot,
            materializer: materialize_secret::<P::Credential>,
            reuse: match preparation {
                AuthPreparationMode::PerAttempt => AuthPreparationReuse::Never,
                AuthPreparationMode::RequestLocal => AuthPreparationReuse::RequestLocal,
            },
            refresh_on_challenge: matches!(challenge, AuthChallengeMode::Refresh),
        }
    }

    /// Binds a provider whose material is inserted as Basic authorization.
    #[doc(hidden)]
    pub fn basic<P>(
        slot: &'a CredentialSlot<Cx, P>,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> Self
    where
        P: CredentialProvider<Cx, Credential = BasicCredential>,
    {
        Self {
            slot,
            materializer: materialize_basic,
            reuse: match preparation {
                AuthPreparationMode::PerAttempt => AuthPreparationReuse::Never,
                AuthPreparationMode::RequestLocal => AuthPreparationReuse::RequestLocal,
            },
            refresh_on_challenge: matches!(challenge, AuthChallengeMode::Refresh),
        }
    }

    fn validate_requirement(&self, requirement: &AuthRequirement) -> Result<(), AuthError> {
        if self.slot.id() != requirement.credential.id {
            return Err(AuthError::new(
                AuthErrorKind::InvalidConfiguration,
                "authentication provider binding does not match its requirement",
            ));
        }
        Ok(())
    }
}

async fn prepare_binding<Cx: ClientContext>(
    binding: AuthProviderBinding<'_, Cx>,
    requirement: &AuthRequirement,
    request: &mut AuthApplicationRequest<'_>,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    executor: &dyn AuthHttpExecutor,
) -> Result<super::PreparedAuthCredential, AuthError> {
    binding.validate_requirement(requirement)?;
    let credential_ctx = CredentialContext {
        vars,
        auth,
        auth_state,
        executor,
        credential_id: requirement.credential.id.clone(),
        reason: CredentialRefreshReason::Missing,
    };
    let lease = binding
        .slot
        .get_or_refresh(credential_ctx, AuthStepPolicy::default())
        .await?;
    let application = (binding.materializer)(lease.value.as_ref(), request, requirement)?;
    let applied = AuthAppliedCredential {
        credential_id: requirement.credential.id.clone(),
        usage_id: requirement.usage_id.clone(),
        step_id: requirement.step_id,
        generation: Some(lease.generation),
        provenance: requirement.provenance.clone(),
    };
    Ok(super::PreparedAuthCredential::new(applied, application).with_reuse(binding.reuse))
}

fn plan_binding_rejection<Cx: ClientContext>(
    binding: AuthProviderBinding<'_, Cx>,
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    status: http::StatusCode,
) -> Result<AuthRejectionAction, AuthError> {
    binding.validate_requirement(requirement)?;
    let Some(decision) =
        auth_decision_for_status(status, requirement, applied, AuthStepPolicy::default())
    else {
        return Ok(AuthRejectionAction::terminal(requirement, applied, None));
    };
    if binding.refresh_on_challenge
        && let Some(retry_reason) = decision.retry_reason
    {
        return Ok(AuthRejectionAction::refresh(
            requirement,
            applied,
            retry_reason,
            decision.invalidate_reason,
        ));
    }
    Ok(AuthRejectionAction::terminal(
        requirement,
        applied,
        decision.invalidate_reason,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn apply_binding_rejection<Cx: ClientContext>(
    binding: AuthProviderBinding<'_, Cx>,
    action: &AuthRejectionAction,
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    executor: &dyn AuthHttpExecutor,
    status: http::StatusCode,
) -> Result<(), AuthError> {
    binding.validate_requirement(requirement)?;
    if !action.matches(requirement, applied) {
        return Err(AuthError::new(
            AuthErrorKind::InvalidConfiguration,
            "authentication rejection action does not match its provider binding",
        ));
    }
    let Some(reason) = rejection_invalidation_reason(action, status) else {
        return Ok(());
    };
    if action.requests_refresh() {
        let credential_ctx = CredentialContext {
            vars,
            auth,
            auth_state,
            executor,
            credential_id: applied.credential_id.clone(),
            reason: CredentialRefreshReason::Rejected,
        };
        binding
            .slot
            .invalidate(credential_ctx, applied.generation, reason)
            .await
    } else {
        binding.slot.invalidate_local(applied.generation)
    }
}

fn rejection_invalidation_reason(
    action: &AuthRejectionAction,
    status: http::StatusCode,
) -> Option<InvalidateReason> {
    action.invalidate_reason().or_else(|| {
        action.requests_refresh().then_some(match status {
            http::StatusCode::FORBIDDEN => InvalidateReason::Forbidden,
            _ => InvalidateReason::Unauthorized,
        })
    })
}

fn apply_binding_rejection_invalidation_only<Cx: ClientContext>(
    binding: AuthProviderBinding<'_, Cx>,
    action: &AuthRejectionAction,
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    status: http::StatusCode,
) -> Result<(), AuthError> {
    binding.validate_requirement(requirement)?;
    if !action.matches(requirement, applied) {
        return Err(AuthError::new(
            AuthErrorKind::InvalidConfiguration,
            "authentication rejection action does not match its provider binding",
        ));
    }
    if rejection_invalidation_reason(action, status).is_some() {
        binding.slot.invalidate_local(applied.generation)?;
    }
    Ok(())
}

/// The single protected-request authentication preparation entry point.
/// Generated contexts always resolve a versioned binding. The callback branch
/// is retained only for hand-written `ClientContext` compatibility and is not
/// emitted by `concord_macros`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare<Cx: ClientContext>(
    requirement: &AuthRequirement,
    request: &mut AuthApplicationRequest<'_>,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    executor: &dyn AuthHttpExecutor,
    meta: &crate::transport::RequestMeta,
) -> Result<super::PreparedAuthCredential, AuthError> {
    match Cx::auth_provider_binding(&requirement.credential.id, auth_state) {
        Some(binding) => {
            prepare_binding(
                binding,
                requirement,
                request,
                vars,
                auth,
                auth_state,
                executor,
            )
            .await
        }
        None => {
            Cx::prepare_auth_requirement(
                requirement,
                request,
                vars,
                auth,
                auth_state,
                executor,
                meta,
            )
            .await
        }
    }
}

/// Applies only generation-conditional local invalidation for a challenged
/// credential. This path has no provider executor and therefore cannot perform
/// provider I/O or acquire a replacement credential.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_rejection_invalidation_only<Cx: ClientContext>(
    action: &AuthRejectionAction,
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    meta: &crate::transport::RequestMeta,
    status: http::StatusCode,
) -> Result<(), AuthError> {
    match Cx::auth_provider_binding(&requirement.credential.id, auth_state) {
        Some(binding) => {
            apply_binding_rejection_invalidation_only(binding, action, requirement, applied, status)
        }
        None => {
            let terminal = AuthRejectionAction::terminal(
                requirement,
                applied,
                rejection_invalidation_reason(action, status),
            );
            Cx::apply_terminal_auth_action(
                &terminal,
                requirement,
                applied,
                vars,
                auth,
                auth_state,
                meta,
                status,
            )
            .await
        }
    }
}

/// The single side-effect-free authentication challenge planning entry point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_rejection<Cx: ClientContext>(
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    meta: &crate::transport::RequestMeta,
    status: http::StatusCode,
    headers: &http::HeaderMap,
) -> Result<AuthRejectionAction, AuthError> {
    match Cx::auth_provider_binding(&requirement.credential.id, auth_state) {
        Some(binding) => plan_binding_rejection(binding, requirement, applied, status),
        None => Cx::plan_auth_response(requirement, applied, vars, auth, meta, status, headers),
    }
}

/// The single generation-aware authentication rejection application entry
/// point. Core selects local terminal invalidation versus provider-capable
/// invalidation before consulting the compatibility adapter.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_rejection<Cx: ClientContext>(
    action: &AuthRejectionAction,
    requirement: &AuthRequirement,
    applied: &AuthAppliedCredential,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    executor: &dyn AuthHttpExecutor,
    meta: &crate::transport::RequestMeta,
    status: http::StatusCode,
) -> Result<(), AuthError> {
    match Cx::auth_provider_binding(&requirement.credential.id, auth_state) {
        Some(binding) => {
            apply_binding_rejection(
                binding,
                action,
                requirement,
                applied,
                vars,
                auth,
                auth_state,
                executor,
                status,
            )
            .await
        }
        None if action.requests_refresh() => {
            Cx::apply_refresh_auth_action(
                action,
                requirement,
                applied,
                vars,
                auth,
                auth_state,
                executor,
                meta,
                status,
            )
            .await
        }
        None => {
            Cx::apply_terminal_auth_action(
                action,
                requirement,
                applied,
                vars,
                auth,
                auth_state,
                meta,
                status,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        ApiKey, AuthChallengeMode, AuthPlan, AuthPreparationMode, AuthProvenance, AuthUsageId,
        CredentialId, CredentialRef,
    };
    use crate::types::RouteBuilder;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct TestCx;

    #[derive(Clone)]
    struct TestState {
        slot: Arc<CredentialSlot<TestCx, TestProvider>>,
    }

    impl ClientContext for TestCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = TestState;
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.test";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
            unreachable!("tests construct state with observable counters")
        }

        fn base_route(_vars: &Self::Vars, _auth: &Self::AuthVars) -> RouteBuilder {
            RouteBuilder::new()
        }
    }

    #[derive(Clone)]
    struct TestProvider {
        acquired: Arc<AtomicUsize>,
        invalidated: Arc<AtomicUsize>,
    }

    impl CredentialProvider<TestCx> for TestProvider {
        type Credential = ApiKey;

        fn id(&self) -> CredentialId {
            CredentialId::new("test", "token")
        }

        fn acquire<'a>(
            &'a self,
            _ctx: CredentialContext<'a, TestCx>,
        ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
            Box::pin(async move {
                let generation = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(ApiKey::new(format!("secret-{generation}")))
            })
        }

        fn invalidate<'a>(
            &'a self,
            _ctx: CredentialContext<'a, TestCx>,
            _current: Option<&'a Self::Credential>,
            _reason: InvalidateReason,
        ) -> AuthFuture<'a, Result<(), AuthError>> {
            Box::pin(async move {
                self.invalidated.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    struct NoHttp;

    impl AuthHttpExecutor for NoHttp {
        fn send<'a>(
            &'a self,
            _req: super::super::AuthHttpRequest,
        ) -> AuthFuture<'a, Result<super::super::AuthHttpResponse, AuthError>> {
            Box::pin(async {
                Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    "unexpected provider HTTP",
                ))
            })
        }
    }

    fn requirement() -> AuthRequirement {
        AuthRequirement {
            credential: CredentialRef {
                id: CredentialId::new("test", "token"),
            },
            placement: super::super::AuthPlacement::Bearer,
            usage_id: AuthUsageId::new("bearer"),
            step_id: Some("test:0:token"),
            provenance: AuthProvenance::new("endpoint"),
            challenge: super::super::AuthChallengePolicy::Default,
        }
    }

    #[tokio::test]
    async fn core_sequences_cache_hit_acquisition_invalidation_and_reacquisition() {
        let acquired = Arc::new(AtomicUsize::new(0));
        let invalidated = Arc::new(AtomicUsize::new(0));
        let state = TestState {
            slot: Arc::new(CredentialSlot::new(TestProvider {
                acquired: acquired.clone(),
                invalidated: invalidated.clone(),
            })),
        };
        let requirement = requirement();
        let placement = super::super::AuthPlacementPlan::from_auth_plan(&AuthPlan {
            requirements: vec![requirement.clone()],
        })
        .expect("valid placement");
        let binding = AuthProviderBinding::secret(
            state.slot.as_ref(),
            AuthPreparationMode::RequestLocal,
            AuthChallengeMode::Refresh,
        );

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let first = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("first acquisition");
        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let second = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("cache hit");
        assert_eq!(acquired.load(Ordering::SeqCst), 1);
        assert_eq!(first.applied.generation, second.applied.generation);

        let action = plan_binding_rejection(
            binding,
            &requirement,
            &second.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("challenge plan");
        assert!(action.requests_refresh());
        apply_binding_rejection(
            binding,
            &action,
            &requirement,
            &second.applied,
            &(),
            &(),
            &state,
            &NoHttp,
            http::StatusCode::UNAUTHORIZED,
        )
        .await
        .expect("provider invalidation");
        assert_eq!(invalidated.load(Ordering::SeqCst), 1);

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let third = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("reacquisition");
        assert_eq!(acquired.load(Ordering::SeqCst), 2);
        assert_ne!(second.applied.generation, third.applied.generation);
    }

    #[tokio::test]
    async fn generation_terminal_invalidation_is_local_and_forces_later_reacquisition() {
        let acquired = Arc::new(AtomicUsize::new(0));
        let invalidated = Arc::new(AtomicUsize::new(0));
        let state = TestState {
            slot: Arc::new(CredentialSlot::new(TestProvider {
                acquired: acquired.clone(),
                invalidated: invalidated.clone(),
            })),
        };
        let requirement = requirement();
        let placement = super::super::AuthPlacementPlan::from_auth_plan(&AuthPlan {
            requirements: vec![requirement.clone()],
        })
        .expect("valid placement");
        let binding = AuthProviderBinding::secret(
            state.slot.as_ref(),
            AuthPreparationMode::RequestLocal,
            AuthChallengeMode::Refresh,
        );

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let initial = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("initial generation");
        let first_challenge = plan_binding_rejection(
            binding,
            &requirement,
            &initial.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("first challenge");
        apply_binding_rejection(
            binding,
            &first_challenge,
            &requirement,
            &initial.applied,
            &(),
            &(),
            &state,
            &NoHttp,
            http::StatusCode::UNAUTHORIZED,
        )
        .await
        .expect("first provider-capable invalidation");

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let replacement = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("replacement generation");
        let second_challenge = plan_binding_rejection(
            binding,
            &requirement,
            &replacement.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("second challenge");
        apply_binding_rejection_invalidation_only(
            binding,
            &second_challenge,
            &requirement,
            &replacement.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("terminal local invalidation");

        assert_eq!(acquired.load(Ordering::SeqCst), 2);
        assert_eq!(invalidated.load(Ordering::SeqCst), 1);
        assert!(!state.slot.has_value().await);

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let later = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("later top-level generation");
        assert_eq!(acquired.load(Ordering::SeqCst), 3);
        assert!(later.applied.generation > replacement.applied.generation);
    }

    #[tokio::test]
    async fn generation_late_terminal_invalidation_preserves_newer_and_unrelated_slots() {
        let state = TestState {
            slot: Arc::new(CredentialSlot::new(TestProvider {
                acquired: Arc::new(AtomicUsize::new(0)),
                invalidated: Arc::new(AtomicUsize::new(0)),
            })),
        };
        let unrelated = CredentialSlot::new(TestProvider {
            acquired: Arc::new(AtomicUsize::new(0)),
            invalidated: Arc::new(AtomicUsize::new(0)),
        });
        let requirement = requirement();
        let placement = super::super::AuthPlacementPlan::from_auth_plan(&AuthPlan {
            requirements: vec![requirement.clone()],
        })
        .expect("valid placement");
        let binding = AuthProviderBinding::secret(
            state.slot.as_ref(),
            AuthPreparationMode::RequestLocal,
            AuthChallengeMode::Refresh,
        );

        let mut request = AuthApplicationRequest::new(&placement.slots[0]);
        let challenged = prepare_binding(
            binding,
            &requirement,
            &mut request,
            &(),
            &(),
            &state,
            &NoHttp,
        )
        .await
        .expect("challenged generation");
        let action = plan_binding_rejection(
            binding,
            &requirement,
            &challenged.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("delayed challenge plan");

        state
            .slot
            .set_manual(ApiKey::new("newer-generation"))
            .await
            .expect("install newer generation");
        unrelated
            .set_manual(ApiKey::new("unrelated-generation"))
            .await
            .expect("install unrelated generation");
        let newer = state.slot.get_cached().await.expect("newer cached value");
        let unrelated_before = unrelated
            .get_cached()
            .await
            .expect("unrelated cached value");

        apply_binding_rejection_invalidation_only(
            binding,
            &action,
            &requirement,
            &challenged.applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("stale terminal invalidation is conditional");

        assert_eq!(
            state
                .slot
                .get_cached()
                .await
                .expect("newer remains")
                .generation,
            newer.generation
        );
        assert_eq!(
            unrelated
                .get_cached()
                .await
                .expect("unrelated remains")
                .generation,
            unrelated_before.generation
        );
    }

    #[test]
    fn manual_binding_never_grants_challenge_refresh() {
        let state = TestState {
            slot: Arc::new(CredentialSlot::new(TestProvider {
                acquired: Arc::new(AtomicUsize::new(0)),
                invalidated: Arc::new(AtomicUsize::new(0)),
            })),
        };
        let requirement = requirement();
        let applied = AuthAppliedCredential {
            credential_id: requirement.credential.id.clone(),
            usage_id: requirement.usage_id.clone(),
            step_id: requirement.step_id,
            generation: Some(1),
            provenance: requirement.provenance.clone(),
        };
        let binding = AuthProviderBinding::secret(
            state.slot.as_ref(),
            AuthPreparationMode::PerAttempt,
            AuthChallengeMode::InvalidateOnly,
        );
        let action = plan_binding_rejection(
            binding,
            &requirement,
            &applied,
            http::StatusCode::UNAUTHORIZED,
        )
        .expect("manual challenge plan");
        assert!(!action.requests_refresh());
        assert_eq!(
            action.invalidate_reason(),
            Some(InvalidateReason::Unauthorized)
        );
    }
}
