use super::{AuthIdentity, AuthProvenance, AuthUsageId, CredentialId};
use std::fmt;

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
    ChallengeRejected,
}

pub fn apply_secret_credential<M: crate::auth::SecretCredential>(
    request: &mut crate::transport::BuiltRequest,
    requirement: &AuthRequirement,
    material: &M,
) -> Result<AuthIdentity, crate::auth::AuthError> {
    use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
    match requirement.placement {
        AuthPlacement::Bearer => {
            let value = format!("Bearer {}", material.secret_value());
            let value = HeaderValue::from_str(&value).map_err(|_| {
                crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::UnsupportedScheme,
                    "invalid bearer header value",
                )
            })?;
            request.headers.insert(AUTHORIZATION, value);
        }
        AuthPlacement::Header(name) => {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::UnsupportedScheme,
                    "invalid auth header name",
                )
            })?;
            let value = HeaderValue::from_str(material.secret_value()).map_err(|_| {
                crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::UnsupportedScheme,
                    "invalid auth header value",
                )
            })?;
            request.headers.insert(name, value);
        }
        AuthPlacement::Query(name) => {
            request
                .url
                .query_pairs_mut()
                .append_pair(name, material.secret_value());
        }
        _ => {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::UnsupportedScheme,
                "credential material does not support requested auth placement",
            ));
        }
    }
    Ok(crate::auth::CredentialMaterial::safe_identity(material))
}

pub fn apply_basic_credential(
    request: &mut crate::transport::BuiltRequest,
    requirement: &AuthRequirement,
    material: &crate::auth::BasicCredential,
) -> Result<AuthIdentity, crate::auth::AuthError> {
    use base64::Engine;
    use http::header::{AUTHORIZATION, HeaderValue};
    if !matches!(requirement.placement, AuthPlacement::Basic) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "basic credential requires basic auth placement",
        ));
    }
    let raw = format!("{}:{}", material.username, material.password.expose());
    let value = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    );
    let value = HeaderValue::from_str(&value).map_err(|_| {
        crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "invalid basic header value",
        )
    })?;
    request.headers.insert(AUTHORIZATION, value);
    Ok(crate::auth::CredentialMaterial::safe_identity(material))
}

pub fn apply_certificate_credential(
    request: &mut crate::transport::BuiltRequest,
    requirement: &AuthRequirement,
    material: &crate::auth::ClientCertificate,
) -> Result<AuthIdentity, crate::auth::AuthError> {
    if !matches!(requirement.placement, AuthPlacement::Certificate) {
        return Err(crate::auth::AuthError::new(
            crate::auth::AuthErrorKind::UnsupportedScheme,
            "certificate credential requires certificate auth placement",
        ));
    }
    request.extensions.transport_auth = Some(crate::auth::TransportAuth::ClientCertificate {
        identity_id: material.identity_id.clone(),
    });
    Ok(crate::auth::CredentialMaterial::safe_identity(material))
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
