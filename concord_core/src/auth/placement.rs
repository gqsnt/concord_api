use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use http::header::AUTHORIZATION;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use url::Url;

use crate::endpoint::EndpointId;
use crate::error::{ApiError, AuthErrorKind, ConfigurationErrorKind};
use crate::hooks::BoxFuture;
use crate::http::application::ApplicationRequest;
use crate::http::provider::ProviderHttp;

use super::material::{BasicCredential, SecretCredential, SecretString};
use super::slot::{CredentialSource, PreparedCredential, SlotSelector};
use super::{AuthError, CredentialMaterial, CredentialRef, RecoveryMode};

/// Statuses that may be attributed to one authentication placement.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChallengePolicy {
    /// Treat only `401 Unauthorized` as an authentication challenge.
    Unauthorized,
    /// Treat `401 Unauthorized` and `403 Forbidden` as authentication challenges.
    UnauthorizedOrForbidden,
    /// Never attribute a response status to this placement.
    Never,
}

/// One opaque typed authentication placement.
pub struct Auth {
    kind: PlacementKind,
    credential: Arc<dyn ErasedCredential>,
    challenge: ChallengePolicy,
}

impl Auth {
    /// Places a single-string credential as bearer authorization.
    #[must_use]
    pub fn bearer<M>(credential: CredentialRef<M>) -> Self
    where
        M: SecretCredential,
    {
        Self::secret(PlacementKind::Bearer, credential)
    }

    /// Places an atomic Basic credential as authorization.
    #[must_use]
    pub fn basic(credential: CredentialRef<BasicCredential>) -> Self {
        Self {
            kind: PlacementKind::Basic,
            credential: Arc::new(TypedCredential {
                selector: credential.selector,
                materialize: basic_material,
            }),
            challenge: ChallengePolicy::Never,
        }
    }

    /// Places a single-string credential in one private native header.
    #[must_use]
    pub fn header<M>(name: HeaderName, credential: CredentialRef<M>) -> Self
    where
        M: SecretCredential,
    {
        Self::secret(PlacementKind::Header(name), credential)
    }

    /// Places a single-string credential in one private native query pair.
    #[must_use]
    pub fn query<M>(name: impl Into<String>, credential: CredentialRef<M>) -> Self
    where
        M: SecretCredential,
    {
        Self::secret(PlacementKind::Query(name.into()), credential)
    }

    /// Replaces the challenge policy for this placement.
    #[must_use]
    pub fn challenge(mut self, policy: ChallengePolicy) -> Self {
        self.challenge = policy;
        self
    }

    fn secret<M>(kind: PlacementKind, credential: CredentialRef<M>) -> Self
    where
        M: SecretCredential,
    {
        Self {
            kind,
            credential: Arc::new(TypedCredential {
                selector: credential.selector,
                materialize: secret_material::<M>,
            }),
            challenge: ChallengePolicy::Never,
        }
    }
}

#[derive(Clone)]
enum PlacementKind {
    Bearer,
    Basic,
    Header(HeaderName),
    Query(String),
}

#[derive(Clone)]
enum NativeMaterial {
    Secret(SecretString),
    Basic {
        username: SecretString,
        password: SecretString,
    },
}

#[derive(Clone)]
struct ErasedPrepared {
    token: Arc<dyn Any + Send + Sync>,
}

trait ErasedCredential: Send + Sync {
    fn key(&self) -> usize;
    fn source(&self) -> Result<CredentialSource, AuthError>;
    fn recovery_mode(&self) -> Result<RecoveryMode, AuthError>;
    fn preflight<'a>(
        &'a self,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<ErasedPrepared, AuthError>>;
    fn finalize<'a>(
        &'a self,
        prepared: &'a ErasedPrepared,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<ErasedPrepared, AuthError>>;
    fn recover<'a>(
        &'a self,
        prepared: &'a ErasedPrepared,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<Option<ErasedPrepared>, AuthError>>;
    fn reject_exact(&self, prepared: &ErasedPrepared) -> Result<(), AuthError>;
    fn clear_exact(&self, prepared: &ErasedPrepared) -> Result<(), AuthError>;
    fn materialize(&self, prepared: &ErasedPrepared) -> Result<NativeMaterial, AuthError>;
}

struct TypedCredential<M> {
    selector: Arc<SlotSelector<M>>,
    materialize: fn(&M) -> NativeMaterial,
}

impl<M> TypedCredential<M>
where
    M: CredentialMaterial,
{
    fn typed(prepared: &ErasedPrepared) -> Result<&PreparedCredential<M>, AuthError> {
        prepared
            .token
            .downcast_ref::<PreparedCredential<M>>()
            .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))
    }

    fn erase(&self, prepared: PreparedCredential<M>) -> ErasedPrepared {
        ErasedPrepared {
            token: Arc::new(prepared),
        }
    }
}

impl<M> ErasedCredential for TypedCredential<M>
where
    M: CredentialMaterial,
{
    fn key(&self) -> usize {
        Arc::as_ptr(&self.selector).cast::<()>() as usize
    }

    fn source(&self) -> Result<CredentialSource, AuthError> {
        self.selector.source()
    }

    fn recovery_mode(&self) -> Result<RecoveryMode, AuthError> {
        self.selector.recovery_mode()
    }

    fn preflight<'a>(
        &'a self,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<ErasedPrepared, AuthError>> {
        Box::pin(async move {
            self.selector
                .preflight(http)
                .await
                .map(|prepared| self.erase(prepared))
        })
    }

    fn finalize<'a>(
        &'a self,
        prepared: &'a ErasedPrepared,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<ErasedPrepared, AuthError>> {
        Box::pin(async move {
            self.selector
                .finalize(Self::typed(prepared)?, http)
                .await
                .map(|prepared| self.erase(prepared))
        })
    }

    fn recover<'a>(
        &'a self,
        prepared: &'a ErasedPrepared,
        http: ProviderHttp<'a>,
    ) -> BoxFuture<'a, Result<Option<ErasedPrepared>, AuthError>> {
        Box::pin(async move {
            self.selector
                .recover(Self::typed(prepared)?, http)
                .await
                .map(|prepared| prepared.map(|prepared| self.erase(prepared)))
        })
    }

    fn reject_exact(&self, prepared: &ErasedPrepared) -> Result<(), AuthError> {
        self.selector.reject_static_exact(Self::typed(prepared)?)
    }

    fn clear_exact(&self, prepared: &ErasedPrepared) -> Result<(), AuthError> {
        self.selector.clear_exact(Self::typed(prepared)?)
    }

    fn materialize(&self, prepared: &ErasedPrepared) -> Result<NativeMaterial, AuthError> {
        Ok((self.materialize)(Self::typed(prepared)?.value()))
    }
}

fn secret_material<M>(value: &M) -> NativeMaterial
where
    M: SecretCredential,
{
    NativeMaterial::Secret(SecretString::new(value.expose_secret()))
}

fn basic_material(value: &BasicCredential) -> NativeMaterial {
    NativeMaterial::Basic {
        username: SecretString::new(value.username()),
        password: SecretString::new(value.password()),
    }
}

#[derive(Default)]
pub(crate) struct AuthPlan {
    placements: Vec<Auth>,
}

impl AuthPlan {
    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) fn len(&self) -> usize {
        self.placements.len()
    }

    pub(crate) fn push(
        &mut self,
        auth: Auth,
        logical_url: &Url,
        public_headers: &HeaderMap,
    ) -> Result<(), AuthError> {
        if auth.challenge != ChallengePolicy::Never
            && self
                .placements
                .iter()
                .any(|placement| placement.challenge != ChallengePolicy::Never)
        {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        validate_public_collision(&auth.kind, logical_url, public_headers)?;
        if self
            .placements
            .iter()
            .any(|existing| placements_collide(&existing.kind, &auth.kind))
        {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        self.placements.push(auth);
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }

    pub(crate) fn validate_public(
        &self,
        logical_url: &Url,
        public_headers: &HeaderMap,
    ) -> Result<(), AuthError> {
        for (index, placement) in self.placements.iter().enumerate() {
            validate_public_collision(&placement.kind, logical_url, public_headers)?;
            if self.placements[..index]
                .iter()
                .any(|existing| placements_collide(&existing.kind, &placement.kind))
            {
                return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
            }
        }
        if self
            .placements
            .iter()
            .filter(|placement| placement.challenge != ChallengePolicy::Never)
            .count()
            > 1
        {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        Ok(())
    }

    pub(crate) async fn preflight(
        &self,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedAuthPlan, AuthError> {
        let mut cache: HashMap<usize, ErasedPrepared> = HashMap::new();
        let mut placements = Vec::with_capacity(self.placements.len());
        for placement in &self.placements {
            let key = placement.credential.key();
            let prepared = match cache.get(&key) {
                Some(prepared) => prepared.clone(),
                None => {
                    let prepared = placement.credential.preflight(http).await?;
                    cache.insert(key, prepared.clone());
                    prepared
                }
            };
            placements.push(PreparedPlacement {
                kind: placement.kind.clone(),
                challenge: placement.challenge,
                credential: Arc::clone(&placement.credential),
                prepared,
            });
        }
        Ok(PreparedAuthPlan { placements })
    }
}

fn validate_public_collision(
    kind: &PlacementKind,
    logical_url: &Url,
    public_headers: &HeaderMap,
) -> Result<(), AuthError> {
    let collision = match kind {
        PlacementKind::Bearer | PlacementKind::Basic => public_headers.contains_key(AUTHORIZATION),
        PlacementKind::Header(name) => *name == AUTHORIZATION || public_headers.contains_key(name),
        PlacementKind::Query(name) => {
            name.is_empty()
                || logical_url
                    .query_pairs()
                    .any(|(key, _)| key == name.as_str())
        }
    };
    if collision {
        Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
    } else {
        Ok(())
    }
}

fn placements_collide(left: &PlacementKind, right: &PlacementKind) -> bool {
    match (left, right) {
        (
            PlacementKind::Bearer | PlacementKind::Basic,
            PlacementKind::Bearer | PlacementKind::Basic,
        )
        | (PlacementKind::Bearer | PlacementKind::Basic, PlacementKind::Header(AUTHORIZATION))
        | (PlacementKind::Header(AUTHORIZATION), PlacementKind::Bearer | PlacementKind::Basic) => {
            true
        }
        (PlacementKind::Header(left), PlacementKind::Header(right)) => left == right,
        (PlacementKind::Query(left), PlacementKind::Query(right)) => left == right,
        _ => false,
    }
}

struct PreparedPlacement {
    kind: PlacementKind,
    challenge: ChallengePolicy,
    credential: Arc<dyn ErasedCredential>,
    prepared: ErasedPrepared,
}

pub(crate) struct PreparedAuthPlan {
    placements: Vec<PreparedPlacement>,
}

impl PreparedAuthPlan {
    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) fn len(&self) -> usize {
        self.placements.len()
    }

    pub(crate) fn challenge_index(&self, status: StatusCode) -> Option<usize> {
        self.placements.iter().position(|placement| {
            matches!(
                (placement.challenge, status),
                (ChallengePolicy::Unauthorized, StatusCode::UNAUTHORIZED)
                    | (
                        ChallengePolicy::UnauthorizedOrForbidden,
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                    )
            )
        })
    }

    pub(crate) fn challenge_can_recover(&self, index: usize) -> Result<bool, AuthError> {
        let placement = self
            .placements
            .get(index)
            .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
        Ok(placement.credential.source()? == CredentialSource::Provider
            && placement.credential.recovery_mode()? == RecoveryMode::Refresh)
    }

    pub(crate) fn apply_terminal_challenge(&self, index: usize) -> Result<(), AuthError> {
        let placement = self
            .placements
            .get(index)
            .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
        match placement.credential.source()? {
            CredentialSource::Static => placement.credential.reject_exact(&placement.prepared),
            CredentialSource::Endpoint => placement.credential.clear_exact(&placement.prepared),
            CredentialSource::Provider => match placement.credential.recovery_mode()? {
                RecoveryMode::None => Ok(()),
                RecoveryMode::InvalidateOnly => {
                    placement.credential.clear_exact(&placement.prepared)
                }
                RecoveryMode::Refresh => placement.credential.reject_exact(&placement.prepared),
            },
        }
    }

    pub(crate) async fn recover_attributed(
        &self,
        index: usize,
        http: ProviderHttp<'_>,
    ) -> Result<bool, AuthError> {
        let placement = self
            .placements
            .get(index)
            .ok_or_else(|| AuthError::new(AuthErrorKind::Internal))?;
        Ok(placement
            .credential
            .recover(&placement.prepared, http)
            .await?
            .is_some())
    }

    pub(crate) async fn preflight_current(
        &self,
        http: ProviderHttp<'_>,
    ) -> Result<Self, AuthError> {
        let mut cache: HashMap<usize, ErasedPrepared> = HashMap::new();
        let mut placements = Vec::with_capacity(self.placements.len());
        for placement in &self.placements {
            let key = placement.credential.key();
            let prepared = match cache.get(&key) {
                Some(prepared) => prepared.clone(),
                None => {
                    let prepared = placement.credential.preflight(http).await?;
                    cache.insert(key, prepared.clone());
                    prepared
                }
            };
            placements.push(PreparedPlacement {
                kind: placement.kind.clone(),
                challenge: placement.challenge,
                credential: Arc::clone(&placement.credential),
                prepared,
            });
        }
        Ok(Self { placements })
    }

    pub(crate) async fn finalize(
        &self,
        http: ProviderHttp<'_>,
    ) -> Result<(Self, FinalizedAuthPlan), AuthError> {
        let mut cache: HashMap<usize, ErasedPrepared> = HashMap::new();
        let mut prepared_placements = Vec::with_capacity(self.placements.len());
        let mut finalized_placements = Vec::with_capacity(self.placements.len());
        for placement in &self.placements {
            let key = placement.credential.key();
            let prepared = match cache.get(&key) {
                Some(prepared) => prepared.clone(),
                None => {
                    let prepared = placement
                        .credential
                        .finalize(&placement.prepared, http)
                        .await?;
                    cache.insert(key, prepared.clone());
                    prepared
                }
            };
            prepared_placements.push(PreparedPlacement {
                kind: placement.kind.clone(),
                challenge: placement.challenge,
                credential: Arc::clone(&placement.credential),
                prepared: prepared.clone(),
            });
            finalized_placements.push(FinalizedPlacement {
                kind: placement.kind.clone(),
                credential: Arc::clone(&placement.credential),
                prepared,
            });
        }
        Ok((
            Self {
                placements: prepared_placements,
            },
            FinalizedAuthPlan {
                placements: finalized_placements,
            },
        ))
    }
}

struct FinalizedPlacement {
    kind: PlacementKind,
    credential: Arc<dyn ErasedCredential>,
    prepared: ErasedPrepared,
}

pub(crate) struct FinalizedAuthPlan {
    placements: Vec<FinalizedPlacement>,
}

impl FinalizedAuthPlan {
    #[cfg(feature = "dangerous-dev-tools")]
    pub(crate) fn len(&self) -> usize {
        self.placements.len()
    }

    pub(crate) fn materialize(
        self,
        request: &mut ApplicationRequest,
        endpoint: &EndpointId,
        method: &Method,
    ) -> Result<(), ApiError> {
        for placement in self.placements {
            let material = placement
                .credential
                .materialize(&placement.prepared)
                .map_err(|error| {
                    ApiError::authentication(endpoint.clone(), method.clone(), error)
                })?;
            match (placement.kind, material) {
                (PlacementKind::Bearer, NativeMaterial::Secret(secret)) => {
                    let value =
                        HeaderValue::from_str(&format!("Bearer {}", secret.expose_secret()))
                            .map_err(|_| invalid_auth(endpoint, method))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (PlacementKind::Basic, NativeMaterial::Basic { username, password }) => {
                    let mut input = String::with_capacity(
                        username.expose_secret().len() + password.expose_secret().len() + 1,
                    );
                    input.push_str(username.expose_secret());
                    input.push(':');
                    input.push_str(password.expose_secret());
                    let encoded = encode_base64(input.as_bytes());
                    let value = HeaderValue::from_str(&format!("Basic {encoded}"))
                        .map_err(|_| invalid_auth(endpoint, method))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (PlacementKind::Header(name), NativeMaterial::Secret(secret)) => {
                    let value = HeaderValue::from_str(secret.expose_secret())
                        .map_err(|_| invalid_auth(endpoint, method))?;
                    request.insert_auth_header(name, value);
                }
                (PlacementKind::Query(name), NativeMaterial::Secret(secret)) => {
                    request.append_auth_query(&name, secret.expose_secret());
                }
                _ => return Err(invalid_auth(endpoint, method)),
            }
        }
        Ok(())
    }
}

fn invalid_auth(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint.clone(),
        method.clone(),
        ConfigurationErrorKind::InvalidAuthentication,
    )
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(TABLE[usize::from(first >> 2)]));
        output.push(char::from(
            TABLE[usize::from(((first & 0x03) << 4) | (second >> 4))],
        ));
        if chunk.len() > 1 {
            output.push(char::from(
                TABLE[usize::from(((second & 0x0f) << 2) | (third >> 6))],
            ));
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(char::from(TABLE[usize::from(third & 0x3f)]));
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_encoding_matches_rfc4648_vectors() {
        assert_eq!(
            encode_base64(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(encode_base64(b"a:b"), "YTpi");
    }
}
