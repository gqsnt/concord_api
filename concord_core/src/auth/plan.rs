use super::{
    AuthIdentity, AuthProvenance, AuthStepPolicy, AuthUsageId, CredentialId, InvalidateReason,
};
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
pub enum PendingAuthPlacement {
    Bearer,
    Header(HeaderName),
    Query(String),
    Basic,
    Certificate,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PendingAuthSlot {
    pub id: AuthSlotId,
    pub credential: CredentialRef,
    pub usage_id: AuthUsageId,
    pub step_id: Option<&'static str>,
    pub generation: Option<u64>,
    pub identity: AuthIdentity,
    pub provenance: AuthProvenance,
    pub placement: PendingAuthPlacement,
}

pub struct AuthApplicationRequest<'a> {
    extensions: &'a mut crate::auth::RequestExtensions,
}

impl<'a> AuthApplicationRequest<'a> {
    #[inline]
    pub(crate) fn new(extensions: &'a mut crate::auth::RequestExtensions) -> Self {
        Self { extensions }
    }

    #[inline]
    fn push_pending_slot(&mut self, slot: PendingAuthSlot) {
        self.extensions.pending_auth_slots.push(slot);
    }

    fn mark_sensitive_query_key(&mut self, key: &str) {
        if !self
            .extensions
            .sensitive_query_keys
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(key))
        {
            self.extensions.sensitive_query_keys.push(key.to_string());
        }
    }
}

impl fmt::Debug for PendingAuthSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingAuthSlot")
            .field("id", &self.id)
            .field("credential", &self.credential)
            .field("usage_id", &self.usage_id)
            .field("step_id", &self.step_id)
            .field("generation", &self.generation)
            .field("identity", &"<redacted>")
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
    Certificate {
        slot_id: AuthSlotId,
        identity_id: String,
    },
}

impl AuthTransportMaterial {
    #[inline]
    pub(crate) fn slot_id(&self) -> AuthSlotId {
        match self {
            Self::Secret { slot_id, .. }
            | Self::Basic { slot_id, .. }
            | Self::Certificate { slot_id, .. } => *slot_id,
        }
    }
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
            Self::Certificate {
                slot_id,
                identity_id,
            } => f
                .debug_struct("AuthTransportMaterial::Certificate")
                .field("slot_id", slot_id)
                .field(
                    "identity_id",
                    &format_args!("<redacted:{}>", identity_id.len()),
                )
                .finish(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthApplication {
    identity: AuthIdentity,
    material: AuthTransportMaterial,
}

impl AuthApplication {
    #[inline]
    pub fn identity(&self) -> &AuthIdentity {
        &self.identity
    }
}

#[derive(Clone, Debug)]
pub struct PreparedAuthCredential {
    pub applied: AuthAppliedCredential,
    pub(crate) material: AuthTransportMaterial,
}

impl PreparedAuthCredential {
    #[inline]
    pub fn new(applied: AuthAppliedCredential, application: AuthApplication) -> Self {
        Self {
            applied,
            material: application.material,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PreparedInternalAuth {
    pub(crate) materials: Vec<AuthTransportMaterial>,
}

impl PreparedInternalAuth {
    #[inline]
    pub fn none() -> Self {
        Self::default()
    }

    #[inline]
    pub fn from_application(application: AuthApplication) -> Self {
        Self {
            materials: vec![application.material],
        }
    }

    #[inline]
    pub fn push_application(&mut self, application: AuthApplication) {
        self.materials.push(application.material);
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
    Certificate,
}

impl fmt::Debug for AuthPlacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer => f.write_str("Bearer"),
            Self::Header(name) => f.debug_tuple("Header").field(name).finish(),
            Self::Query(name) => f.debug_tuple("Query").field(name).finish(),
            Self::Basic => f.write_str("Basic"),
            Self::Certificate => f.write_str("Certificate"),
        }
    }
}

impl PartialEq for AuthPlacement {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bearer, Self::Bearer)
            | (Self::Basic, Self::Basic)
            | (Self::Certificate, Self::Certificate) => true,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthDecision {
    Continue,
    RetryAfterRefresh {
        credential: CredentialRef,
        generation: Option<u64>,
        reason: AuthRetryReason,
    },
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthAppliedCredential {
    pub credential_id: CredentialId,
    pub usage_id: AuthUsageId,
    pub step_id: Option<&'static str>,
    pub generation: Option<u64>,
    pub identity: AuthIdentity,
    pub provenance: AuthProvenance,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthAttemptSummary {
    pub applied: Vec<AuthAppliedCredential>,
    pub refreshed_credentials: usize,
}

impl AuthAttemptSummary {
    #[inline]
    pub fn applied_credentials(&self) -> usize {
        self.applied.len()
    }
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
    let slot_id = AuthSlotId::next()?;
    let placement = match requirement.placement {
        AuthPlacement::Bearer => PendingAuthPlacement::Bearer,
        AuthPlacement::Header(name) => {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::UnsupportedScheme,
                    "invalid auth header name",
                )
            })?;
            PendingAuthPlacement::Header(name)
        }
        AuthPlacement::Query(name) => {
            request.mark_sensitive_query_key(name);
            PendingAuthPlacement::Query(name.to_string())
        }
        _ => {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::UnsupportedScheme,
                "credential material does not support requested auth placement",
            ));
        }
    };
    let identity = crate::auth::CredentialMaterial::safe_identity(material);
    request.push_pending_slot(PendingAuthSlot {
        id: slot_id,
        credential: requirement.credential.clone(),
        usage_id: requirement.usage_id.clone(),
        step_id: requirement.step_id,
        generation: None,
        identity: identity.clone(),
        provenance: requirement.provenance.clone(),
        placement,
    });
    Ok(AuthApplication {
        identity,
        material: AuthTransportMaterial::Secret {
            slot_id,
            secret: SecretString::new(material.secret_value().to_string()),
        },
    })
}

pub fn apply_basic_credential(
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
    material: &crate::auth::BasicCredential,
) -> Result<AuthApplication, crate::auth::AuthError> {
    if !matches!(requirement.placement, AuthPlacement::Basic) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "basic credential requires basic auth placement",
        ));
    }
    let slot_id = AuthSlotId::next()?;
    let identity = crate::auth::CredentialMaterial::safe_identity(material);
    request.push_pending_slot(PendingAuthSlot {
        id: slot_id,
        credential: requirement.credential.clone(),
        usage_id: requirement.usage_id.clone(),
        step_id: requirement.step_id,
        generation: None,
        identity: identity.clone(),
        provenance: requirement.provenance.clone(),
        placement: PendingAuthPlacement::Basic,
    });
    Ok(AuthApplication {
        identity,
        material: AuthTransportMaterial::Basic {
            slot_id,
            username: material.username.clone(),
            password: material.password.clone(),
        },
    })
}

pub fn apply_certificate_credential(
    request: &mut AuthApplicationRequest<'_>,
    requirement: &AuthRequirement,
    material: &crate::auth::ClientCertificate,
) -> Result<AuthApplication, crate::auth::AuthError> {
    if !matches!(requirement.placement, AuthPlacement::Certificate) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "certificate credential requires certificate auth placement",
        ));
    }
    let slot_id = AuthSlotId::next()?;
    let identity = crate::auth::CredentialMaterial::safe_identity(material);
    request.push_pending_slot(PendingAuthSlot {
        id: slot_id,
        credential: requirement.credential.clone(),
        usage_id: requirement.usage_id.clone(),
        step_id: requirement.step_id,
        generation: None,
        identity: identity.clone(),
        provenance: requirement.provenance.clone(),
        placement: PendingAuthPlacement::Certificate,
    });
    Ok(AuthApplication {
        identity,
        material: AuthTransportMaterial::Certificate {
            slot_id,
            identity_id: material.identity_id.clone(),
        },
    })
}

pub async fn invalidate_rejected_credential<Cx, P>(
    slot: &crate::auth::CredentialSlot<Cx, P>,
    vars: &Cx::Vars,
    auth: &Cx::AuthVars,
    auth_state: &Cx::AuthState,
    executor: &dyn crate::auth::AuthHttpExecutor,
    applied: &AuthAppliedCredential,
    reason: crate::auth::InvalidateReason,
) -> Result<(), crate::auth::AuthError>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    let credential_ctx = crate::auth::CredentialContext {
        vars,
        auth,
        auth_state,
        executor,
        credential_id: applied.credential_id.clone(),
        reason: crate::auth::CredentialRefreshReason::Rejected,
    };
    slot.invalidate_generation(credential_ctx, applied.generation, reason)
        .await
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
            identity: AuthIdentity::Anonymous,
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
}
