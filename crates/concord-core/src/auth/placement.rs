use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use http::header::AUTHORIZATION;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use url::Url;

use crate::endpoint::EndpointId;
use crate::error::{ApiError, AuthErrorKind};
use crate::hooks::BoxFuture;
use crate::http::application::ApplicationRequest;
use crate::http::provider::ProviderHttp;
use crate::limits::MAX_AUTH_PLACEMENTS;
use crate::request::reserved_public_header;

use super::material::{BasicCredential, SecretCredential};
use super::slot::{CredentialContext, CredentialSource, PreparedCredential, SlotSelector};
use super::{
    AuthError, CredentialMaterial, CredentialRef, MAX_BASIC_PASSWORD_BYTES,
    MAX_BASIC_USERNAME_BYTES, MAX_CREDENTIAL_SECRET_BYTES, RecoveryMode,
};

/// Maximum retained byte length of one named authentication placement.
pub const MAX_AUTH_PLACEMENT_NAME_BYTES: usize = 256;
/// Maximum byte length of one final native authentication header value.
pub const MAX_AUTHORIZATION_VALUE_BYTES: usize = 65_536;
/// Maximum byte length of one static custom Authorization scheme.
pub const MAX_AUTHORIZATION_SCHEME_BYTES: usize = 256;
/// Maximum encoded byte length of one private authentication query value.
pub const MAX_AUTH_QUERY_VALUE_BYTES: usize = 65_536;

const BEARER_PREFIX: &[u8] = b"Bearer ";
const BASIC_PREFIX: &[u8] = b"Basic ";

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

    /// Places a single-string credential as the complete Authorization value.
    #[must_use]
    pub fn authorization<M>(credential: CredentialRef<M>) -> Self
    where
        M: SecretCredential,
    {
        Self::secret(PlacementKind::Authorization, credential)
    }

    /// Places a single-string credential after one static Authorization scheme.
    ///
    /// The scheme is a non-empty HTTP token of at most
    /// [`MAX_AUTHORIZATION_SCHEME_BYTES`] ASCII bytes. Its declaration case is
    /// preserved, and the final value contains exactly one ASCII space between
    /// the scheme and credential.
    pub fn authorization_with_scheme<M>(
        scheme: impl AsRef<str>,
        credential: CredentialRef<M>,
    ) -> Result<Self, AuthError>
    where
        M: SecretCredential,
    {
        let scheme = scheme.as_ref();
        validate_authorization_scheme(scheme)?;
        Ok(Self::secret(
            PlacementKind::AuthorizationWithScheme(scheme.to_owned()),
            credential,
        ))
    }

    /// Places a single-string credential in one ordinary custom private header.
    ///
    /// Concord rejects transport- and request-owned names, including generic
    /// `Authorization`, media headers, cookies, proxy authorization, and
    /// connection/framing headers. Dedicated Bearer, Basic, exact
    /// Authorization, and static-scheme Authorization placements own
    /// `Authorization`; ordinary custom private headers such as `X-API-Key`
    /// and private authentication query values remain supported.
    pub fn header<M>(name: HeaderName, credential: CredentialRef<M>) -> Result<Self, AuthError>
    where
        M: SecretCredential,
    {
        validate_auth_header_name(&name)?;
        Ok(Self::secret(PlacementKind::Header(name), credential))
    }

    /// Places a single-string credential in one private native query pair.
    pub fn query<M>(name: impl AsRef<str>, credential: CredentialRef<M>) -> Result<Self, AuthError>
    where
        M: SecretCredential,
    {
        let name = name.as_ref();
        validate_placement_name(name)?;
        Ok(Self::secret(
            PlacementKind::Query(name.to_owned()),
            credential,
        ))
    }

    /// Replaces the challenge policy for this placement.
    #[must_use]
    pub fn challenge(mut self, policy: ChallengePolicy) -> Self {
        self.challenge = policy;
        self
    }

    pub(crate) const fn placement_count(&self) -> usize {
        1
    }

    pub(crate) fn public_name_bytes(&self) -> usize {
        match &self.kind {
            PlacementKind::Bearer | PlacementKind::Basic | PlacementKind::Authorization => {
                AUTHORIZATION.as_str().len()
            }
            PlacementKind::AuthorizationWithScheme(scheme) => {
                AUTHORIZATION.as_str().len() + scheme.len()
            }
            PlacementKind::Header(name) => name.as_str().len(),
            PlacementKind::Query(name) => name.len(),
        }
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
    Authorization,
    AuthorizationWithScheme(String),
    Header(HeaderName),
    Query(String),
}

enum NativeMaterial<'a> {
    Secret(&'a str),
    Basic {
        username: &'a str,
        password: &'a str,
    },
}

#[derive(Clone)]
struct ErasedPrepared {
    token: Arc<dyn Any + Send + Sync>,
}

trait ErasedCredential: Send + Sync {
    fn context(&self) -> &CredentialContext;
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
    fn materialize<'a>(
        &self,
        prepared: &'a ErasedPrepared,
    ) -> Result<NativeMaterial<'a>, AuthError>;
}

struct TypedCredential<M> {
    selector: Arc<SlotSelector<M>>,
    materialize: for<'a> fn(&'a M) -> Result<NativeMaterial<'a>, AuthError>,
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
    fn context(&self) -> &CredentialContext {
        self.selector.context()
    }

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

    fn materialize<'a>(
        &self,
        prepared: &'a ErasedPrepared,
    ) -> Result<NativeMaterial<'a>, AuthError> {
        (self.materialize)(Self::typed(prepared)?.value())
    }
}

fn secret_material<M>(value: &M) -> Result<NativeMaterial<'_>, AuthError>
where
    M: SecretCredential,
{
    let secret = value.expose_secret();
    validate_material_len(secret.len(), MAX_CREDENTIAL_SECRET_BYTES)?;
    Ok(NativeMaterial::Secret(secret))
}

fn basic_material(value: &BasicCredential) -> Result<NativeMaterial<'_>, AuthError> {
    validate_material_len(value.username().len(), MAX_BASIC_USERNAME_BYTES)?;
    validate_material_len(value.password().len(), MAX_BASIC_PASSWORD_BYTES)?;
    if value.username().as_bytes().contains(&b':') {
        return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
    }
    Ok(NativeMaterial::Basic {
        username: value.username(),
        password: value.password(),
    })
}

#[derive(Default)]
pub(crate) struct AuthPlan {
    placements: Vec<Auth>,
}

impl AuthPlan {
    pub(crate) fn push(
        &mut self,
        auth: Auth,
        logical_url: &Url,
        public_headers: &HeaderMap,
    ) -> Result<(), AuthError> {
        validate_placement_kind(&auth.kind)?;
        if self.placements.first().is_some_and(|existing| {
            !existing
                .credential
                .context()
                .same_as(auth.credential.context())
        }) {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        if self.placements.len() >= MAX_AUTH_PLACEMENTS {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
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

    pub(crate) fn placement_count(&self) -> usize {
        self.placements.len()
    }

    pub(crate) fn validate_public(
        &self,
        logical_url: &Url,
        public_headers: &HeaderMap,
    ) -> Result<(), AuthError> {
        if self.placements.len() > MAX_AUTH_PLACEMENTS {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        for (index, placement) in self.placements.iter().enumerate() {
            validate_placement_kind(&placement.kind)?;
            if self.placements[..index].iter().any(|existing| {
                !existing
                    .credential
                    .context()
                    .same_as(placement.credential.context())
            }) {
                return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
            }
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

    pub(crate) fn validate_context(&self, expected: &CredentialContext) -> Result<(), AuthError> {
        if self
            .placements
            .iter()
            .all(|placement| placement.credential.context().same_as(expected))
        {
            Ok(())
        } else {
            Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
        }
    }

    pub(crate) async fn preflight(
        &self,
        http: ProviderHttp<'_>,
    ) -> Result<PreparedAuthPlan, AuthError> {
        for placement in &self.placements {
            validate_placement_kind(&placement.kind)?;
        }
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
        PlacementKind::Bearer
        | PlacementKind::Basic
        | PlacementKind::Authorization
        | PlacementKind::AuthorizationWithScheme(_) => public_headers.contains_key(AUTHORIZATION),
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
        (left, right) if owns_authorization(left) && owns_authorization(right) => true,
        (left, PlacementKind::Header(right)) if owns_authorization(left) => right == AUTHORIZATION,
        (PlacementKind::Header(left), right) if owns_authorization(right) => left == AUTHORIZATION,
        (PlacementKind::Header(left), PlacementKind::Header(right)) => left == right,
        (PlacementKind::Query(left), PlacementKind::Query(right)) => left == right,
        _ => false,
    }
}

fn owns_authorization(kind: &PlacementKind) -> bool {
    matches!(
        kind,
        PlacementKind::Bearer
            | PlacementKind::Basic
            | PlacementKind::Authorization
            | PlacementKind::AuthorizationWithScheme(_)
    )
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
        for placement in &self.placements {
            validate_placement_kind(&placement.kind)?;
        }
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
    pub(crate) fn materialize(
        self,
        request: &mut ApplicationRequest,
        endpoint: &EndpointId,
        method: &Method,
    ) -> Result<(), ApiError> {
        for placement in &self.placements {
            validate_placement_kind(&placement.kind)
                .map_err(|error| auth_error(endpoint, method, error))?;
        }
        for placement in self.placements {
            let material = placement
                .credential
                .materialize(&placement.prepared)
                .map_err(|error| {
                    ApiError::authentication(endpoint.clone(), method.clone(), error)
                })?;
            match (&placement.kind, material) {
                (PlacementKind::Bearer, NativeMaterial::Secret(secret)) => {
                    let value = prefixed_header_value(BEARER_PREFIX, secret)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (PlacementKind::Basic, NativeMaterial::Basic { username, password }) => {
                    let value = basic_header_value(username, password)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (PlacementKind::Authorization, NativeMaterial::Secret(secret)) => {
                    let value = plain_header_value(secret)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (
                    PlacementKind::AuthorizationWithScheme(scheme),
                    NativeMaterial::Secret(secret),
                ) => {
                    let value = scheme_header_value(scheme, secret)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    request.insert_auth_header(AUTHORIZATION, value);
                }
                (PlacementKind::Header(name), NativeMaterial::Secret(secret)) => {
                    let value = plain_header_value(secret)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    request.insert_auth_header(name.clone(), value);
                }
                (PlacementKind::Query(name), NativeMaterial::Secret(secret)) => {
                    let encoded_name = encoded_query_component_len(name)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    let encoded_value = encoded_query_component_len(secret)
                        .map_err(|error| auth_error(endpoint, method, error))?;
                    if encoded_value > MAX_AUTH_QUERY_VALUE_BYTES {
                        return Err(auth_error(
                            endpoint,
                            method,
                            AuthError::new(AuthErrorKind::MaterialTooLarge),
                        ));
                    }
                    let encoded_pair = encoded_name
                        .checked_add(1)
                        .and_then(|length| length.checked_add(encoded_value))
                        .ok_or_else(|| {
                            auth_error(
                                endpoint,
                                method,
                                AuthError::new(AuthErrorKind::MaterialTooLarge),
                            )
                        })?;
                    request
                        .append_auth_query(name, secret, encoded_pair)
                        .map_err(|()| {
                            auth_error(
                                endpoint,
                                method,
                                AuthError::new(AuthErrorKind::MaterialTooLarge),
                            )
                        })?;
                }
                _ => {
                    return Err(auth_error(
                        endpoint,
                        method,
                        AuthError::new(AuthErrorKind::InvalidConfiguration),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn auth_error(endpoint: &EndpointId, method: &Method, error: AuthError) -> ApiError {
    if error.kind() == AuthErrorKind::InvalidConfiguration {
        drop(error);
        ApiError::configuration(
            endpoint.clone(),
            method.clone(),
            crate::error::ConfigurationErrorKind::InvalidAuthentication,
        )
    } else {
        ApiError::authentication(endpoint.clone(), method.clone(), error)
    }
}

fn validate_placement_name(name: &str) -> Result<(), AuthError> {
    if name.is_empty() || name.len() > MAX_AUTH_PLACEMENT_NAME_BYTES {
        Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
    } else {
        Ok(())
    }
}

fn validate_placement_kind(kind: &PlacementKind) -> Result<(), AuthError> {
    match kind {
        PlacementKind::Bearer | PlacementKind::Basic | PlacementKind::Authorization => Ok(()),
        PlacementKind::AuthorizationWithScheme(scheme) => validate_authorization_scheme(scheme),
        PlacementKind::Header(name) => validate_auth_header_name(name),
        PlacementKind::Query(name) => validate_placement_name(name),
    }
}

fn validate_authorization_scheme(scheme: &str) -> Result<(), AuthError> {
    if scheme.is_empty()
        || scheme.len() > MAX_AUTHORIZATION_SCHEME_BYTES
        || !scheme.bytes().all(is_http_token_byte)
    {
        Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
    } else {
        Ok(())
    }
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn validate_auth_header_name(name: &HeaderName) -> Result<(), AuthError> {
    if name.as_str().len() > MAX_AUTH_PLACEMENT_NAME_BYTES
        || reserved_public_header(name)
        || matches!(name.as_str(), "accept" | "content-type" | "user-agent")
    {
        Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
    } else {
        Ok(())
    }
}

fn validate_material_len(length: usize, limit: usize) -> Result<(), AuthError> {
    if length > limit {
        Err(AuthError::new(AuthErrorKind::MaterialTooLarge))
    } else {
        Ok(())
    }
}

fn checked_base64_len(input_len: usize) -> Option<usize> {
    input_len.checked_add(2)?.checked_div(3)?.checked_mul(4)
}

fn prefixed_header_value(prefix: &[u8], secret: &str) -> Result<HeaderValue, AuthError> {
    let output_len = prefix
        .len()
        .checked_add(secret.len())
        .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))?;
    validate_material_len(output_len, MAX_AUTHORIZATION_VALUE_BYTES)?;
    validate_native_header_value(secret.as_bytes())?;
    let mut output = BytesMut::with_capacity(output_len);
    output.extend_from_slice(prefix);
    output.extend_from_slice(secret.as_bytes());
    sensitive_header_value(output.freeze())
}

fn scheme_header_value(scheme: &str, secret: &str) -> Result<HeaderValue, AuthError> {
    validate_authorization_scheme(scheme)?;
    let output_len = scheme
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(secret.len()))
        .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))?;
    validate_material_len(output_len, MAX_AUTHORIZATION_VALUE_BYTES)?;
    validate_native_header_value(secret.as_bytes())?;
    let mut output = BytesMut::with_capacity(output_len);
    output.extend_from_slice(scheme.as_bytes());
    output.extend_from_slice(b" ");
    output.extend_from_slice(secret.as_bytes());
    sensitive_header_value(output.freeze())
}

fn plain_header_value(secret: &str) -> Result<HeaderValue, AuthError> {
    validate_material_len(secret.len(), MAX_AUTHORIZATION_VALUE_BYTES)?;
    let mut value = HeaderValue::from_bytes(secret.as_bytes())
        .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?;
    value.set_sensitive(true);
    Ok(value)
}

fn basic_header_value(username: &str, password: &str) -> Result<HeaderValue, AuthError> {
    validate_material_len(username.len(), MAX_BASIC_USERNAME_BYTES)?;
    validate_material_len(password.len(), MAX_BASIC_PASSWORD_BYTES)?;
    if username.as_bytes().contains(&b':') {
        return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
    }
    let input_len = username
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(password.len()))
        .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))?;
    let encoded_len = checked_base64_len(input_len)
        .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))?;
    let output_len = BASIC_PREFIX
        .len()
        .checked_add(encoded_len)
        .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))?;
    validate_material_len(output_len, MAX_AUTHORIZATION_VALUE_BYTES)?;

    let mut output = BytesMut::with_capacity(output_len);
    output.extend_from_slice(BASIC_PREFIX);
    encode_base64_into(
        username
            .bytes()
            .chain(std::iter::once(b':'))
            .chain(password.bytes()),
        &mut output,
    );
    debug_assert_eq!(output.len(), output_len);
    sensitive_header_value(output.freeze())
}

fn sensitive_header_value(value: Bytes) -> Result<HeaderValue, AuthError> {
    let mut value = HeaderValue::from_maybe_shared(value)
        .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?;
    value.set_sensitive(true);
    Ok(value)
}

fn validate_native_header_value(value: &[u8]) -> Result<(), AuthError> {
    if value
        .iter()
        .copied()
        .all(|byte| byte >= 32 && byte != 127 || byte == b'\t')
    {
        Ok(())
    } else {
        Err(AuthError::new(AuthErrorKind::InvalidConfiguration))
    }
}

fn encoded_query_component_len(value: &str) -> Result<usize, AuthError> {
    url::form_urlencoded::byte_serialize(value.as_bytes()).try_fold(0_usize, |length, fragment| {
        length
            .checked_add(fragment.len())
            .ok_or_else(|| AuthError::new(AuthErrorKind::MaterialTooLarge))
    })
}

fn encode_base64_into(input: impl Iterator<Item = u8>, output: &mut BytesMut) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut input = input;
    while let Some(first) = input.next() {
        let second = input.next();
        let third = input.next();
        let second_value = second.unwrap_or(0);
        let third_value = third.unwrap_or(0);
        output.extend_from_slice(&[
            TABLE[usize::from(first >> 2)],
            TABLE[usize::from(((first & 0x03) << 4) | (second_value >> 4))],
            second.map_or(b'=', |_| {
                TABLE[usize::from(((second_value & 0x0f) << 2) | (third_value >> 6))]
            }),
            third.map_or(b'=', |_| TABLE[usize::from(third_value & 0x3f)]),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::auth::{ApiKey, CredentialMaterial};

    #[derive(Clone)]
    struct CustomSecret(String);

    impl CredentialMaterial for CustomSecret {}

    impl SecretCredential for CustomSecret {
        fn expose_secret(&self) -> &str {
            &self.0
        }
    }

    fn credential() -> CredentialRef<ApiKey> {
        CredentialRef::new(SlotSelector::static_value(
            ApiKey::try_new("test-key").expect("bounded test API key"),
            Duration::from_secs(1),
        ))
    }

    fn credential_in(context: &CredentialContext) -> CredentialRef<ApiKey> {
        CredentialRef::new(SlotSelector::static_value_in_context(
            ApiKey::try_new("context-bound-test-key").expect("bounded test API key"),
            Duration::from_secs(1),
            context.clone(),
        ))
    }

    fn basic_credential_in(context: &CredentialContext) -> CredentialRef<BasicCredential> {
        CredentialRef::new(SlotSelector::static_value_in_context(
            BasicCredential::try_new("context-user", "context-password")
                .expect("bounded Basic credential"),
            Duration::from_secs(1),
            context.clone(),
        ))
    }

    #[test]
    fn every_auth_placement_retains_and_validates_its_client_context() {
        let expected = CredentialContext::new();
        let foreign = CredentialContext::new();
        let logical_url = Url::parse("https://example.test/").expect("validation URL");
        let public_headers = HeaderMap::new();
        let placements = [
            Auth::bearer(credential_in(&expected)),
            Auth::basic(basic_credential_in(&expected)),
            Auth::authorization(credential_in(&expected)),
            Auth::authorization_with_scheme("Token", credential_in(&expected))
                .expect("valid scheme placement"),
            Auth::header(
                HeaderName::from_static("x-context-key"),
                credential_in(&expected),
            )
            .expect("valid header placement"),
            Auth::query("context_key", credential_in(&expected)).expect("valid query placement"),
        ];

        for placement in placements {
            let mut plan = AuthPlan::default();
            plan.push(placement, &logical_url, &public_headers)
                .expect("context-bound placement");
            plan.validate_context(&expected)
                .expect("originating context");
            let error = plan
                .validate_context(&foreign)
                .expect_err("foreign context must fail");
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        }
    }

    #[test]
    fn one_auth_plan_cannot_mix_client_contexts() {
        let first = CredentialContext::new();
        let second = CredentialContext::new();
        let logical_url = Url::parse("https://example.test/").expect("validation URL");
        let public_headers = HeaderMap::new();
        let mut plan = AuthPlan::default();
        plan.push(
            Auth::bearer(credential_in(&first)),
            &logical_url,
            &public_headers,
        )
        .expect("first context");
        let error = plan
            .push(
                Auth::query("key", credential_in(&second)).expect("valid query placement"),
                &logical_url,
                &public_headers,
            )
            .expect_err("mixed contexts must fail");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert_eq!(plan.placement_count(), 1);
    }

    #[test]
    fn basic_encoding_matches_rfc4648_vectors() {
        let value = basic_header_value("Aladdin", "open sesame").expect("valid Basic value");
        assert_eq!(
            value.to_str().expect("ASCII Basic value"),
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert!(value.is_sensitive());
        assert_eq!(
            basic_header_value("a", "b")
                .expect("valid Basic value")
                .to_str()
                .expect("ASCII Basic value"),
            "Basic YTpi"
        );
        assert_eq!(
            basic_header_value("a", "prefix:password")
                .expect("Basic password colon")
                .to_str()
                .expect("ASCII Basic value"),
            "Basic YTpwcmVmaXg6cGFzc3dvcmQ="
        );
        let error = basic_header_value("a:b", "password")
            .expect_err("Basic username colon must fail defensively");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
    }

    #[test]
    fn auth_placement_name_exact_limit_and_first_excess_are_redacted() {
        let exact = "n".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES);
        let query = Auth::query(exact.as_str(), credential()).expect("exact query-name limit");
        assert_eq!(query.public_name_bytes(), MAX_AUTH_PLACEMENT_NAME_BYTES);

        let header = HeaderName::from_bytes(exact.as_bytes()).expect("valid exact header name");
        Auth::header(header, credential()).expect("exact header-name limit");

        let sentinel = format!("{}sentinel", "n".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES));
        let error = Auth::query(sentinel.as_str(), credential())
            .err()
            .expect("query-name first excess");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert!(!error.to_string().contains("sentinel"));
        assert!(!format!("{error:?}").contains("sentinel"));

        let header =
            HeaderName::from_bytes(sentinel.as_bytes()).expect("HTTP accepts oversized name");
        let error = Auth::header(header, credential())
            .err()
            .expect("header-name first excess");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert!(Auth::query("", credential()).is_err());
    }

    #[test]
    fn runtime_auth_headers_reject_every_request_owned_name_and_accept_custom_names() {
        const PROTECTED_CREDENTIAL: &str = "protected-auth-value-sentinel-47ad";
        const RESERVED: &[&str] = &[
            "host",
            "content-length",
            "transfer-encoding",
            "connection",
            "keep-alive",
            "te",
            "trailer",
            "upgrade",
            "proxy-authorization",
            "authorization",
            "cookie",
            "set-cookie",
            "accept",
            "content-type",
            "user-agent",
        ];

        let credential = CredentialRef::new(SlotSelector::static_value(
            ApiKey::try_new(PROTECTED_CREDENTIAL).expect("bounded protected test API key"),
            Duration::from_secs(1),
        ));
        for reserved in RESERVED {
            let name = HeaderName::from_bytes(reserved.as_bytes()).expect("valid reserved name");
            let error = Auth::header(name, credential.clone())
                .err()
                .expect("request-owned authentication header must fail");
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
            assert_eq!(
                error.to_string(),
                "authentication failed (InvalidConfiguration)"
            );
            assert_eq!(
                format!("{error:?}"),
                "AuthError { kind: InvalidConfiguration, safe_message: None, retry_after: None }"
            );
            assert!(!error.to_string().contains(PROTECTED_CREDENTIAL));
        }

        Auth::header(HeaderName::from_static("x-api-key"), credential.clone())
            .expect("ordinary custom authentication header");
        Auth::header(HeaderName::from_static("x-session-token"), credential)
            .expect("another ordinary custom authentication header");
    }

    #[test]
    fn defensive_auth_plan_validation_rejects_reserved_names_without_retention() {
        let logical_url = Url::parse("https://example.test/").expect("validation URL");
        let public_headers = HeaderMap::new();
        let mut forged = Auth::header(HeaderName::from_static("x-api-key"), credential())
            .expect("initially valid placement");
        forged.kind = PlacementKind::Header(HeaderName::from_static("content-type"));

        let mut plan = AuthPlan::default();
        let error = plan
            .push(forged, &logical_url, &public_headers)
            .expect_err("defensive push validation");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert!(plan.placements.is_empty());

        let mut forged = Auth::header(HeaderName::from_static("x-api-key"), credential())
            .expect("initially valid placement");
        forged.kind = PlacementKind::Header(HeaderName::from_static("host"));
        let plan = AuthPlan {
            placements: vec![forged],
        };
        let error = plan
            .validate_public(&logical_url, &public_headers)
            .expect_err("defensive final plan validation");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert!(!error.to_string().contains("host"));
        assert!(!format!("{error:?}").contains("host"));
    }

    #[test]
    fn bearer_and_raw_secret_limits_reject_first_excess_before_copy() {
        let exact = "a".repeat(MAX_AUTHORIZATION_VALUE_BYTES - BEARER_PREFIX.len());
        let value = prefixed_header_value(BEARER_PREFIX, exact.as_str())
            .expect("exact Bearer authorization limit");
        assert_eq!(value.as_bytes().len(), MAX_AUTHORIZATION_VALUE_BYTES);
        assert!(value.is_sensitive());

        let first_excess = format!("{exact}x");
        let error = prefixed_header_value(BEARER_PREFIX, first_excess.as_str())
            .expect_err("first Bearer output excess");
        assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);

        let sentinel = CustomSecret(format!(
            "{}credential-sentinel",
            "s".repeat(MAX_CREDENTIAL_SECRET_BYTES)
        ));
        let error = secret_material(&sentinel)
            .err()
            .expect("first raw credential excess");
        assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);
        assert!(!error.to_string().contains("credential-sentinel"));
        assert!(!format!("{error:?}").contains("credential-sentinel"));
    }

    #[test]
    fn custom_authorization_scheme_validation_is_exact_and_redacted() {
        let exact = "T".repeat(MAX_AUTHORIZATION_SCHEME_BYTES);
        let placement = Auth::authorization_with_scheme(exact.as_str(), credential())
            .expect("exact Authorization scheme limit");
        assert_eq!(
            placement.public_name_bytes(),
            AUTHORIZATION.as_str().len() + MAX_AUTHORIZATION_SCHEME_BYTES
        );

        for invalid in ["", "Token value", "Token:", "Tökén"] {
            let error = Auth::authorization_with_scheme(invalid, credential())
                .err()
                .expect("invalid Authorization scheme");
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
            if !invalid.is_empty() {
                assert!(!error.to_string().contains(invalid));
                assert!(!format!("{error:?}").contains(invalid));
            }
        }

        let sentinel = format!(
            "{}scheme-first-excess-sentinel",
            "T".repeat(MAX_AUTHORIZATION_SCHEME_BYTES)
        );
        let error = Auth::authorization_with_scheme(sentinel.as_str(), credential())
            .err()
            .expect("first excess Authorization scheme");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert!(!error.to_string().contains("scheme-first-excess-sentinel"));
        assert!(!format!("{error:?}").contains("scheme-first-excess-sentinel"));

        for valid in ["Token", "tOkEn", "!#$%&'*+-.^_`|~", "a0"] {
            Auth::authorization_with_scheme(valid, credential())
                .expect("valid HTTP token Authorization scheme");
        }
    }

    #[test]
    fn exact_and_scheme_authorization_values_are_sensitive_and_bounded() {
        let exact =
            plain_header_value("Complete credential").expect("valid exact Authorization value");
        assert_eq!(exact.as_bytes(), b"Complete credential");
        assert!(exact.is_sensitive());

        let scheme =
            scheme_header_value("tOkEn", "credential").expect("valid scheme Authorization value");
        assert_eq!(scheme.as_bytes(), b"tOkEn credential");
        assert!(scheme.is_sensitive());

        let exact_limit = "x".repeat(MAX_AUTHORIZATION_VALUE_BYTES - "Token ".len());
        let value = scheme_header_value("Token", exact_limit.as_str())
            .expect("exact final Authorization value limit");
        assert_eq!(value.as_bytes().len(), MAX_AUTHORIZATION_VALUE_BYTES);
        let first_excess = format!("{exact_limit}x");
        let error = scheme_header_value("Token", first_excess.as_str())
            .expect_err("first excess final Authorization value");
        assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);

        for invalid in ["line\r\nbreak", "delete\u{7f}"] {
            let error = plain_header_value(invalid)
                .expect_err("invalid exact native Authorization header value");
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
            assert!(!error.to_string().contains(invalid));
        }
    }

    #[test]
    fn every_authorization_owner_collides_and_public_authorization_is_rejected() {
        let logical_url = Url::parse("https://example.test/").expect("validation URL");
        let constructors: [fn() -> Auth; 4] = [
            || Auth::bearer(credential()),
            || {
                let basic = CredentialRef::new(SlotSelector::static_value(
                    BasicCredential::try_new("user", "password").expect("bounded Basic credential"),
                    Duration::from_secs(1),
                ));
                Auth::basic(basic)
            },
            || Auth::authorization(credential()),
            || {
                Auth::authorization_with_scheme("Token", credential())
                    .expect("valid Authorization scheme")
            },
        ];
        for (left_index, left) in constructors.iter().enumerate() {
            for (right_index, right) in constructors.iter().enumerate() {
                let mut plan = AuthPlan::default();
                plan.push(left(), &logical_url, &HeaderMap::new())
                    .expect("first Authorization owner");
                let error = plan
                    .push(right(), &logical_url, &HeaderMap::new())
                    .expect_err("second Authorization owner must collide");
                assert_eq!(
                    error.kind(),
                    AuthErrorKind::InvalidConfiguration,
                    "owner pair {left_index}/{right_index}"
                );
            }
        }

        let mut public_headers = HeaderMap::new();
        public_headers.insert(AUTHORIZATION, HeaderValue::from_static("public"));
        for constructor in constructors {
            let mut plan = AuthPlan::default();
            let error = plan
                .push(constructor(), &logical_url, &public_headers)
                .expect_err("public Authorization collision");
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        }
    }

    #[test]
    fn basic_limits_and_checked_base64_arithmetic_are_finite() {
        let username = "u".repeat(MAX_BASIC_USERNAME_BYTES);
        let password = "p".repeat(MAX_BASIC_PASSWORD_BYTES);
        let credential = BasicCredential::try_new(username.as_str(), password.as_str())
            .expect("exact Basic component limits");
        let NativeMaterial::Basic { username, password } =
            basic_material(&credential).expect("exact Basic component limits")
        else {
            panic!("Basic material expected");
        };
        let value = basic_header_value(username, password).expect("bounded Basic output");
        assert!(value.as_bytes().len() <= MAX_AUTHORIZATION_VALUE_BYTES);
        assert!(value.is_sensitive());

        assert_eq!(checked_base64_len(0), Some(0));
        assert_eq!(checked_base64_len(3), Some(4));
        assert_eq!(checked_base64_len(4), Some(8));
        assert_eq!(checked_base64_len(usize::MAX), None);
    }

    #[test]
    fn auth_query_encoded_value_exact_limit_and_first_excess_are_checked() {
        let exact = format!("{}a", "%".repeat((MAX_AUTH_QUERY_VALUE_BYTES - 1) / 3));
        assert_eq!(
            encoded_query_component_len(&exact).expect("encoded query length"),
            MAX_AUTH_QUERY_VALUE_BYTES
        );

        let first_excess = format!("{exact}%");
        let encoded =
            encoded_query_component_len(&first_excess).expect("finite encoded query length");
        assert!(encoded > MAX_AUTH_QUERY_VALUE_BYTES);

        let raw_exact = "q".repeat(MAX_CREDENTIAL_SECRET_BYTES);
        assert_eq!(
            encoded_query_component_len(&raw_exact).expect("unreserved exact query value"),
            MAX_AUTH_QUERY_VALUE_BYTES
        );
    }

    #[test]
    fn custom_material_limit_error_contains_no_value() {
        const SENTINEL: &str = "custom-provider-material-sentinel-52cc";
        let material = CustomSecret(format!(
            "{}{SENTINEL}",
            "x".repeat(MAX_CREDENTIAL_SECRET_BYTES)
        ));
        let error = secret_material(&material)
            .err()
            .expect("custom material must be revalidated by core");
        assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);
        assert!(!error.to_string().contains(SENTINEL));
        assert!(!format!("{error:?}").contains(SENTINEL));
    }

    #[test]
    fn auth_plan_defensive_first_excess_is_rejected_before_retention() {
        const FIRST_EXCESS_NAME: &str = "private-auth-placement-first-excess-sentinel";

        let logical_url = Url::parse("https://example.test/").expect("validation URL");
        let public_headers = HeaderMap::new();
        let credential = credential();
        let mut plan = AuthPlan::default();
        for index in 0..MAX_AUTH_PLACEMENTS {
            plan.push(
                Auth::query(format!("private-auth-{index}"), credential.clone())
                    .expect("bounded unique authentication placement"),
                &logical_url,
                &public_headers,
            )
            .expect("exact authentication-plan limit");
        }
        assert_eq!(plan.placements.len(), MAX_AUTH_PLACEMENTS);

        let error = plan
            .push(
                Auth::query(FIRST_EXCESS_NAME, credential)
                    .expect("bounded first-excess authentication placement"),
                &logical_url,
                &public_headers,
            )
            .expect_err("first excess authentication placement");
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        assert_eq!(plan.placements.len(), MAX_AUTH_PLACEMENTS);
        assert!(!error.to_string().contains(FIRST_EXCESS_NAME));
        assert!(!format!("{error:?}").contains(FIRST_EXCESS_NAME));
    }
}
