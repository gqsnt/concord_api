use super::{AuthProvenance, AuthStepPolicy, AuthUsageId, CredentialId, InvalidateReason};
use crate::secret::SecretString;
use http::HeaderName;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_AUTH_SLOT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AuthSlotId(u64);

impl AuthSlotId {
    #[inline]
    pub(crate) fn next() -> Result<Self, crate::auth::AuthError> {
        NEXT_AUTH_SLOT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(Self)
            .map_err(|_| {
                crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::InvalidConfiguration,
                    "auth slot id counter overflowed",
                )
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedAuthPlacement {
    Bearer,
    Header(HeaderName),
    Query(String),
    Basic,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PlannedAuthSlot {
    pub id: AuthSlotId,
    pub credential: CredentialRef,
    pub usage_id: AuthUsageId,
    pub step_id: Option<&'static str>,
    pub provenance: AuthProvenance,
    pub placement: PlannedAuthPlacement,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthPlacementPlan {
    pub slots: Vec<PlannedAuthSlot>,
    pub sensitive_query_keys: Vec<String>,
}

impl AuthPlacementPlan {
    pub(crate) fn from_auth_plan(plan: &AuthPlan) -> Result<Self, crate::auth::AuthError> {
        let mut planned = Self::default();
        for requirement in &plan.requirements {
            let placement = match requirement.placement {
                AuthPlacement::Bearer => PlannedAuthPlacement::Bearer,
                AuthPlacement::Basic => PlannedAuthPlacement::Basic,
                AuthPlacement::Header(name) => PlannedAuthPlacement::Header(
                    HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                        crate::auth::AuthError::new(
                            crate::auth::AuthErrorKind::InvalidConfiguration,
                            "invalid auth header name",
                        )
                    })?,
                ),
                AuthPlacement::Query(name) => {
                    if !planned
                        .sensitive_query_keys
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(name))
                    {
                        planned.sensitive_query_keys.push(name.to_string());
                    }
                    PlannedAuthPlacement::Query(name.to_string())
                }
            };
            if let PlannedAuthPlacement::Header(name) = &placement
                && crate::header_ownership::is_forbidden_auth_placement(name)
            {
                return Err(crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::InvalidConfiguration,
                    format!("authentication may not own reserved header `{name}`"),
                ));
            }
            if planned
                .slots
                .iter()
                .any(|existing| placements_collide(&existing.placement, &placement))
            {
                return Err(crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::InvalidConfiguration,
                    "duplicate authentication placement target",
                ));
            }
            planned.slots.push(PlannedAuthSlot {
                id: AuthSlotId::next()?,
                credential: requirement.credential.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                provenance: requirement.provenance.clone(),
                placement,
            });
        }
        Ok(planned)
    }

    pub(crate) fn validate_public_request(
        &self,
        headers: &http::HeaderMap,
        url: &url::Url,
    ) -> Result<(), crate::auth::AuthError> {
        self.validate_public_request_with_reserved_headers(headers, url, &[])
    }

    pub(crate) fn validate_public_request_with_reserved_headers(
        &self,
        headers: &http::HeaderMap,
        url: &url::Url,
        reserved_headers: &[HeaderName],
    ) -> Result<(), crate::auth::AuthError> {
        use http::header::AUTHORIZATION;
        for slot in &self.slots {
            match &slot.placement {
                PlannedAuthPlacement::Bearer => {
                    if headers.contains_key(AUTHORIZATION) {
                        return Err(crate::auth::AuthError::new(
                            crate::auth::AuthErrorKind::InvalidConfiguration,
                            "bearer auth collides with an existing public Authorization header",
                        ));
                    }
                }
                PlannedAuthPlacement::Basic => {
                    if headers.contains_key(AUTHORIZATION) {
                        return Err(crate::auth::AuthError::new(
                            crate::auth::AuthErrorKind::InvalidConfiguration,
                            "basic auth collides with an existing public Authorization header",
                        ));
                    }
                }
                PlannedAuthPlacement::Header(name) => {
                    if headers.contains_key(name)
                        || reserved_headers.iter().any(|reserved| reserved == name)
                    {
                        return Err(crate::auth::AuthError::new(
                            crate::auth::AuthErrorKind::InvalidConfiguration,
                            format!(
                                "header auth key `{name}` collides with an existing public header"
                            ),
                        ));
                    }
                }
                PlannedAuthPlacement::Query(name) => {
                    if url
                        .query_pairs()
                        .any(|(existing, _)| existing == name.as_str())
                    {
                        return Err(crate::auth::AuthError::new(
                            crate::auth::AuthErrorKind::InvalidConfiguration,
                            format!(
                                "query auth key `{name}` collides with an existing public query parameter"
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn placements_collide(a: &PlannedAuthPlacement, b: &PlannedAuthPlacement) -> bool {
    use PlannedAuthPlacement::{Basic, Bearer, Header, Query};
    match (a, b) {
        (Bearer | Basic, Bearer | Basic) => true,
        (Header(name), Bearer | Basic) | (Bearer | Basic, Header(name)) => {
            *name == http::header::AUTHORIZATION
        }
        (Header(a), Header(b)) => a == b,
        (Query(a), Query(b)) => a == b,
        _ => false,
    }
}

pub struct AuthApplicationRequest<'a> {
    planned: &'a PlannedAuthSlot,
}

impl<'a> AuthApplicationRequest<'a> {
    #[inline]
    pub(crate) fn new(planned: &'a PlannedAuthSlot) -> Self {
        Self { planned }
    }

    fn validate_requirement(
        &self,
        requirement: &AuthRequirement,
    ) -> Result<(), crate::auth::AuthError> {
        if self.planned.credential != requirement.credential
            || self.planned.usage_id != requirement.usage_id
            || self.planned.step_id != requirement.step_id
            || self.planned.provenance != requirement.provenance
        {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::InvalidConfiguration,
                "credential application does not match the preplanned authentication slot",
            ));
        }
        if !placement_matches_requirement(&self.planned.placement, requirement.placement) {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::InvalidConfiguration,
                "credential application placement does not match the preplanned authentication slot",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for PlannedAuthSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlannedAuthSlot")
            .field("id", &self.id)
            .field("credential", &self.credential)
            .field("usage_id", &self.usage_id)
            .field("step_id", &self.step_id)
            .field("provenance", &self.provenance)
            .field("placement", &self.placement)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) enum AuthTransportMaterial {
    Secret {
        slot_id: AuthSlotId,
        secret: SecretString,
    },
    Basic {
        slot_id: AuthSlotId,
        username: SecretString,
        password: SecretString,
    },
}

impl fmt::Debug for AuthTransportMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret { slot_id, .. } => f
                .debug_struct("AuthTransportMaterial::Secret")
                .field("slot_id", slot_id)
                .field("secret", &"<redacted>")
                .finish(),
            Self::Basic { slot_id, .. } => f
                .debug_struct("AuthTransportMaterial::Basic")
                .field("slot_id", slot_id)
                .field("username", &"<redacted>")
                .field("password", &"<redacted>")
                .finish(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthApplication {
    material: AuthTransportMaterial,
}

#[derive(Clone, Debug)]
pub struct PreparedAuthCredential {
    pub applied: AuthAppliedCredential,
    pub(crate) reuse: AuthPreparationReuse,
    pub(crate) material: AuthTransportMaterial,
    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) lifecycle_observation_target: Option<super::CredentialLifecycleObservationTarget>,
}

impl PreparedAuthCredential {
    #[inline]
    pub fn new(applied: AuthAppliedCredential, application: AuthApplication) -> Self {
        Self {
            applied,
            reuse: AuthPreparationReuse::Never,
            material: application.material,
            #[cfg(feature = "dangerous-dev-tools")]
            lifecycle_observation_target: None,
        }
    }

    #[inline]
    pub fn with_reuse(mut self, reuse: AuthPreparationReuse) -> Self {
        self.reuse = reuse;
        self
    }

    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) fn with_lifecycle_observation_target(
        mut self,
        target: Option<super::CredentialLifecycleObservationTarget>,
    ) -> Self {
        self.lifecycle_observation_target = target;
        self
    }

    pub(crate) fn validate_binding(
        &self,
        slot: &PlannedAuthSlot,
    ) -> Result<(), crate::auth::AuthError> {
        if self.material.slot_id() != slot.id || !material_matches_slot(&self.material, slot) {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::InvalidConfiguration,
                "credential material does not match the preplanned authentication slot",
            ));
        }
        if self.applied.credential_id != slot.credential.id
            || self.applied.usage_id != slot.usage_id
            || self.applied.step_id != slot.step_id
            || self.applied.provenance != slot.provenance
        {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::InvalidConfiguration,
                "applied credential metadata does not match the preplanned authentication slot",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthPreparationReuse {
    #[default]
    Never,
    RequestLocal,
}

impl AuthTransportMaterial {
    fn slot_id(&self) -> AuthSlotId {
        match self {
            Self::Secret { slot_id, .. } | Self::Basic { slot_id, .. } => *slot_id,
        }
    }
}

fn material_matches_slot(material: &AuthTransportMaterial, slot: &PlannedAuthSlot) -> bool {
    matches!(
        (material, &slot.placement),
        (
            AuthTransportMaterial::Secret { .. },
            PlannedAuthPlacement::Bearer
                | PlannedAuthPlacement::Header(_)
                | PlannedAuthPlacement::Query(_)
        ) | (
            AuthTransportMaterial::Basic { .. },
            PlannedAuthPlacement::Basic
        )
    )
}

fn placement_matches_requirement(
    planned: &PlannedAuthPlacement,
    requirement: AuthPlacement,
) -> bool {
    match (planned, requirement) {
        (PlannedAuthPlacement::Bearer, AuthPlacement::Bearer)
        | (PlannedAuthPlacement::Basic, AuthPlacement::Basic) => true,
        (PlannedAuthPlacement::Header(planned), AuthPlacement::Header(requirement)) => {
            planned.as_str().eq_ignore_ascii_case(requirement)
        }
        (PlannedAuthPlacement::Query(planned), AuthPlacement::Query(requirement)) => {
            planned == requirement
        }
        _ => false,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthPlan {
    pub requirements: Vec<AuthRequirement>,
}

impl AuthPlan {
    pub fn none() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthRequirement {
    pub credential: CredentialRef,
    pub placement: AuthPlacement,
    pub usage_id: AuthUsageId,
    pub step_id: Option<&'static str>,
    pub provenance: AuthProvenance,
    pub challenge: AuthChallengePolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialRef {
    pub id: CredentialId,
}

#[derive(Clone, Copy)]
pub enum AuthPlacement {
    Bearer,
    Header(&'static str),
    Query(&'static str),
    Basic,
}

impl fmt::Debug for AuthPlacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer => f.write_str("Bearer"),
            Self::Header(name) => f.debug_tuple("Header").field(name).finish(),
            Self::Query(name) => f.debug_tuple("Query").field(name).finish(),
            Self::Basic => f.write_str("Basic"),
        }
    }
}

impl PartialEq for AuthPlacement {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bearer, Self::Bearer) | (Self::Basic, Self::Basic) => true,
            (Self::Header(a), Self::Header(b)) | (Self::Query(a), Self::Query(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for AuthPlacement {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthChallengePolicy {
    #[default]
    Default,
    NeverRefresh,
}

/// One side-effect-free action for one exact applied authentication use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthRejectionAction {
    Terminal {
        credential: CredentialRef,
        usage_id: AuthUsageId,
        step_id: Option<&'static str>,
        generation: Option<u64>,
        invalidate_reason: Option<InvalidateReason>,
    },
    Refresh {
        credential: CredentialRef,
        usage_id: AuthUsageId,
        step_id: Option<&'static str>,
        generation: Option<u64>,
        reason: AuthRetryReason,
        invalidate_reason: Option<InvalidateReason>,
    },
}

/// Aggregate rejection plan for one response. Terminal actions are retained
/// in order; at most one refresh action is admitted by the runtime.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AuthRejectionPlan {
    actions: Vec<AuthRejectionAction>,
}

impl AuthRejectionAction {
    pub fn terminal(
        requirement: &AuthRequirement,
        applied: &AuthAppliedCredential,
        invalidate_reason: Option<InvalidateReason>,
    ) -> Self {
        Self::Terminal {
            credential: requirement.credential.clone(),
            usage_id: applied.usage_id.clone(),
            step_id: applied.step_id,
            generation: applied.generation,
            invalidate_reason,
        }
    }

    pub fn refresh(
        requirement: &AuthRequirement,
        applied: &AuthAppliedCredential,
        reason: AuthRetryReason,
        invalidate_reason: Option<InvalidateReason>,
    ) -> Self {
        Self::Refresh {
            credential: requirement.credential.clone(),
            usage_id: applied.usage_id.clone(),
            step_id: applied.step_id,
            generation: applied.generation,
            reason,
            invalidate_reason,
        }
    }

    pub fn requests_refresh(&self) -> bool {
        matches!(self, Self::Refresh { .. })
    }

    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) fn matches_use_identity(
        &self,
        credential_id: &CredentialId,
        usage_id: &AuthUsageId,
        step_id: Option<&'static str>,
    ) -> bool {
        match self {
            Self::Terminal {
                credential,
                usage_id: action_usage_id,
                step_id: action_step_id,
                ..
            }
            | Self::Refresh {
                credential,
                usage_id: action_usage_id,
                step_id: action_step_id,
                ..
            } => {
                &credential.id == credential_id
                    && action_usage_id == usage_id
                    && *action_step_id == step_id
            }
        }
    }

    pub fn matches(&self, requirement: &AuthRequirement, applied: &AuthAppliedCredential) -> bool {
        match self {
            Self::Terminal {
                credential,
                usage_id,
                step_id,
                generation,
                ..
            }
            | Self::Refresh {
                credential,
                usage_id,
                step_id,
                generation,
                ..
            } => {
                credential == &requirement.credential
                    && credential.id == applied.credential_id
                    && usage_id == &applied.usage_id
                    && step_id == &applied.step_id
                    && generation == &applied.generation
            }
        }
    }

    pub fn invalidate_reason(&self) -> Option<InvalidateReason> {
        match self {
            Self::Terminal {
                invalidate_reason, ..
            }
            | Self::Refresh {
                invalidate_reason, ..
            } => *invalidate_reason,
        }
    }
}

impl AuthRejectionPlan {
    pub(crate) fn actions(&self) -> &[AuthRejectionAction] {
        &self.actions
    }

    pub(crate) fn requests_refresh(&self) -> bool {
        self.actions
            .iter()
            .any(AuthRejectionAction::requests_refresh)
    }

    pub(crate) fn append_validated(&mut self, action: AuthRejectionAction) {
        match action {
            AuthRejectionAction::Terminal { .. } => self.actions.push(action),
            AuthRejectionAction::Refresh { .. } if !self.requests_refresh() => {
                self.actions.push(action)
            }
            AuthRejectionAction::Refresh { .. } => {}
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthAppliedCredential {
    pub credential_id: CredentialId,
    pub usage_id: AuthUsageId,
    pub step_id: Option<&'static str>,
    pub generation: Option<u64>,
    pub provenance: AuthProvenance,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthAttemptSummary {
    pub applied: Vec<AuthAppliedCredential>,
    pub refreshed_credentials: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthRetryReason {
    Unauthorized,
    Forbidden,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthRejectionDecision {
    pub invalidate_reason: Option<InvalidateReason>,
    pub retry_reason: Option<AuthRetryReason>,
}

pub fn auth_decision_for_status(
    status: http::StatusCode,
    requirement: &AuthRequirement,
    _applied: &AuthAppliedCredential,
    policy: AuthStepPolicy,
) -> Option<AuthRejectionDecision> {
    if matches!(requirement.challenge, AuthChallengePolicy::NeverRefresh) {
        return None;
    }

    if status == http::StatusCode::UNAUTHORIZED {
        let retry_reason = policy
            .retry_on_unauthorized
            .then_some(AuthRetryReason::Unauthorized);
        let invalidate_reason = policy
            .invalidate_on_unauthorized
            .then_some(InvalidateReason::Unauthorized);
        if retry_reason.is_some() || invalidate_reason.is_some() {
            return Some(AuthRejectionDecision {
                invalidate_reason,
                retry_reason,
            });
        }
    } else if status == http::StatusCode::FORBIDDEN {
        let retry_reason = policy
            .retry_on_forbidden
            .then_some(AuthRetryReason::Forbidden);
        let invalidate_reason = policy
            .invalidate_on_forbidden
            .then_some(InvalidateReason::Forbidden);
        if retry_reason.is_some() || invalidate_reason.is_some() {
            return Some(AuthRejectionDecision {
                invalidate_reason,
                retry_reason,
            });
        }
    }

    None
}

pub fn apply_secret_credential<M: crate::auth::SecretCredential>(
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
    material: &M,
) -> Result<AuthApplication, crate::auth::AuthError> {
    request.validate_requirement(requirement)?;
    if !matches!(
        request.planned.placement,
        PlannedAuthPlacement::Bearer
            | PlannedAuthPlacement::Header(_)
            | PlannedAuthPlacement::Query(_)
    ) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "credential material does not support preplanned auth placement",
        ));
    }
    Ok(AuthApplication {
        material: AuthTransportMaterial::Secret {
            slot_id: request.planned.id,
            secret: SecretString::new(material.secret_value().to_string()),
        },
    })
}

pub fn apply_basic_credential(
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
    material: &crate::auth::BasicCredential,
) -> Result<AuthApplication, crate::auth::AuthError> {
    request.validate_requirement(requirement)?;
    if !matches!(request.planned.placement, PlannedAuthPlacement::Basic) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "basic credential requires basic auth placement",
        ));
    }
    Ok(AuthApplication {
        material: AuthTransportMaterial::Basic {
            slot_id: request.planned.id,
            username: material.username.clone(),
            password: material.password.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    fn requirement(challenge: AuthChallengePolicy) -> AuthRequirement {
        AuthRequirement {
            credential: CredentialRef {
                id: CredentialId::new("test", "token"),
            },
            placement: AuthPlacement::Bearer,
            usage_id: AuthUsageId::new("test.token"),
            step_id: None,
            provenance: AuthProvenance::default(),
            challenge,
        }
    }

    fn applied() -> AuthAppliedCredential {
        AuthAppliedCredential {
            credential_id: CredentialId::new("test", "token"),
            usage_id: AuthUsageId::new("test.token"),
            step_id: None,
            generation: Some(1),
            provenance: AuthProvenance::default(),
        }
    }

    #[test]
    fn auth_decision_default_unauthorized_invalidates_and_retries() {
        let decision = auth_decision_for_status(
            StatusCode::UNAUTHORIZED,
            &requirement(AuthChallengePolicy::Default),
            &applied(),
            AuthStepPolicy::default(),
        )
        .expect("default 401 should request auth handling");

        assert_eq!(
            decision,
            AuthRejectionDecision {
                invalidate_reason: Some(InvalidateReason::Unauthorized),
                retry_reason: Some(AuthRetryReason::Unauthorized),
            }
        );
    }

    #[test]
    fn reserved_protocol_and_body_headers_cannot_be_authentication_placements() {
        for name in [
            "content-length",
            "transfer-encoding",
            "accept-encoding",
            "host",
            "proxy-authorization",
            "content-type",
        ] {
            let mut requirement = requirement(AuthChallengePolicy::Default);
            requirement.placement = AuthPlacement::Header(name);
            let error = AuthPlacementPlan::from_auth_plan(&AuthPlan {
                requirements: vec![requirement],
            })
            .expect_err("reserved headers must not become authentication targets");
            assert_eq!(error.kind, crate::auth::AuthErrorKind::InvalidConfiguration);
        }
    }

    #[test]
    fn auth_decision_default_forbidden_invalidates_and_retries() {
        let decision = auth_decision_for_status(
            StatusCode::FORBIDDEN,
            &requirement(AuthChallengePolicy::Default),
            &applied(),
            AuthStepPolicy::default(),
        )
        .expect("default 403 should request auth handling");

        assert_eq!(
            decision,
            AuthRejectionDecision {
                invalidate_reason: Some(InvalidateReason::Forbidden),
                retry_reason: Some(AuthRetryReason::Forbidden),
            }
        );
    }

    #[test]
    fn auth_decision_never_refresh_does_nothing_for_unauthorized() {
        assert_eq!(
            auth_decision_for_status(
                StatusCode::UNAUTHORIZED,
                &requirement(AuthChallengePolicy::NeverRefresh),
                &applied(),
                AuthStepPolicy::default(),
            ),
            None
        );
    }

    #[test]
    fn auth_decision_can_invalidate_unauthorized_without_retrying() {
        let policy = AuthStepPolicy {
            retry_on_unauthorized: false,
            invalidate_on_unauthorized: true,
            ..AuthStepPolicy::default()
        };

        let decision = auth_decision_for_status(
            StatusCode::UNAUTHORIZED,
            &requirement(AuthChallengePolicy::Default),
            &applied(),
            policy,
        )
        .expect("invalidate-only 401 should request auth handling");

        assert_eq!(
            decision,
            AuthRejectionDecision {
                invalidate_reason: Some(InvalidateReason::Unauthorized),
                retry_reason: None,
            }
        );
    }

    #[test]
    fn auth_decision_can_retry_unauthorized_without_invalidating() {
        let policy = AuthStepPolicy {
            retry_on_unauthorized: true,
            invalidate_on_unauthorized: false,
            ..AuthStepPolicy::default()
        };

        let decision = auth_decision_for_status(
            StatusCode::UNAUTHORIZED,
            &requirement(AuthChallengePolicy::Default),
            &applied(),
            policy,
        )
        .expect("retry-only 401 should request auth handling");

        assert_eq!(
            decision,
            AuthRejectionDecision {
                invalidate_reason: None,
                retry_reason: Some(AuthRetryReason::Unauthorized),
            }
        );
    }

    #[test]
    fn auth_decision_forbidden_follows_explicit_retry_and_invalidation_policy() {
        let policy = AuthStepPolicy {
            retry_on_forbidden: true,
            invalidate_on_forbidden: true,
            ..AuthStepPolicy::default()
        };

        let decision = auth_decision_for_status(
            StatusCode::FORBIDDEN,
            &requirement(AuthChallengePolicy::Default),
            &applied(),
            policy,
        )
        .expect("custom 403 policy should request auth handling");

        assert_eq!(
            decision,
            AuthRejectionDecision {
                invalidate_reason: Some(InvalidateReason::Forbidden),
                retry_reason: Some(AuthRetryReason::Forbidden),
            }
        );
    }

    #[test]
    fn aggregate_plan_keeps_all_terminals_and_one_refresh() {
        let req = requirement(AuthChallengePolicy::Default);
        let first = applied();
        let mut second = applied();
        second.credential_id = CredentialId::new("test", "other");
        second.usage_id = AuthUsageId::new("test.other");

        let mut plan = AuthRejectionPlan::default();
        plan.append_validated(AuthRejectionAction::terminal(
            &req,
            &first,
            Some(InvalidateReason::Unauthorized),
        ));
        plan.append_validated(AuthRejectionAction::refresh(
            &req,
            &second,
            AuthRetryReason::Unauthorized,
            None,
        ));
        plan.append_validated(AuthRejectionAction::terminal(
            &req,
            &first,
            Some(InvalidateReason::Forbidden),
        ));
        plan.append_validated(AuthRejectionAction::refresh(
            &req,
            &second,
            AuthRetryReason::Forbidden,
            None,
        ));

        assert_eq!(plan.actions().len(), 3);
        assert!(plan.actions()[0].invalidate_reason().is_some());
        assert!(plan.actions()[1].requests_refresh());
        assert_eq!(
            plan.actions()[2].invalidate_reason(),
            Some(InvalidateReason::Forbidden)
        );
    }

    #[test]
    fn action_matching_requires_exact_use_and_generation() {
        let req = requirement(AuthChallengePolicy::Default);
        let action =
            AuthRejectionAction::refresh(&req, &applied(), AuthRetryReason::Unauthorized, None);
        assert!(action.matches(&req, &applied()));

        let mut mismatched = applied();
        mismatched.generation = Some(2);
        assert!(!action.matches(&req, &mismatched));
        mismatched.generation = Some(1);
        mismatched.usage_id = AuthUsageId::new("test.other-use");
        assert!(!action.matches(&req, &mismatched));
        mismatched.usage_id = applied().usage_id;
        mismatched.step_id = Some("other");
        assert!(!action.matches(&req, &mismatched));

        let mut other_req = req.clone();
        other_req.usage_id = AuthUsageId::new("test.other-use");
        let mut other_applied = applied();
        other_applied.usage_id = other_req.usage_id.clone();
        let other_action = AuthRejectionAction::terminal(&other_req, &other_applied, None);
        assert!(other_action.matches(&other_req, &other_applied));
        assert!(!other_action.matches(&req, &applied()));
    }
}
