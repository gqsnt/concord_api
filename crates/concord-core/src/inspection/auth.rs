//! Credential sources, auth placements and effective authentication (§11).

use crate::auth::ChallengeRecovery;

/// The authority used to produce one declared credential slot.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CredentialSourceKind {
    /// A static API-key value derived from one protected builder input.
    ApiKey,
    /// A static bearer value derived from one protected builder input.
    Bearer,
    /// A static Basic credential derived from two protected builder inputs.
    Basic,
    /// A credential acquired through one generated endpoint.
    Endpoint,
    /// A trusted application credential provider.
    CustomProvider,
}

impl CredentialSourceKind {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Bearer => "bearer",
            Self::Basic => "basic",
            Self::Endpoint => "endpoint",
            Self::CustomProvider => "custom",
        }
    }
}

/// One protected builder input declaration, never its runtime material.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SecretDescription {
    name: &'static str,
    docs: &'static [&'static str],
}

impl SecretDescription {
    /// `secret name;`.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self { name, docs: &[] }
    }

    /// Records author documentation lines.
    #[must_use]
    pub const fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Returns the secret name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn doc_lines(&self) -> &'static [&'static str] {
        self.docs
    }
}

/// One credential source declaration without material.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CredentialDescription {
    pub(crate) name: &'static str,
    pub(crate) kind: CredentialSourceKind,
    pub(crate) secrets: Box<[&'static str]>,
    pub(crate) provider_type: Option<&'static str>,
    pub(crate) source_endpoint: Option<&'static str>,
    pub(crate) resolved_source_endpoint: Option<&'static str>,
    pub(crate) docs: &'static [&'static str],
}

impl CredentialDescription {
    /// `credential name = api_key(secret.secret);`.
    #[must_use]
    pub fn api_key(name: &'static str, secret: &'static str) -> Self {
        Self::with(name, CredentialSourceKind::ApiKey, vec![secret])
    }

    /// `credential name = bearer(secret.secret);`.
    #[must_use]
    pub fn bearer(name: &'static str, secret: &'static str) -> Self {
        Self::with(name, CredentialSourceKind::Bearer, vec![secret])
    }

    /// `credential name = basic(secret.user, secret.password);`.
    #[must_use]
    pub fn basic(name: &'static str, user: &'static str, password: &'static str) -> Self {
        Self::with(name, CredentialSourceKind::Basic, vec![user, password])
    }

    /// `credential name = custom ProviderType;`.
    #[must_use]
    pub fn custom(name: &'static str, provider_type: &'static str) -> Self {
        let mut credential = Self::with(name, CredentialSourceKind::CustomProvider, Vec::new());
        credential.provider_type = Some(provider_type);
        credential
    }

    /// `credential name = endpoint path::to::endpoint;` with the root-relative
    /// endpoint path as written (for example `authenticate` or
    /// `regional::login`).
    #[must_use]
    pub fn endpoint(name: &'static str, root_relative_path: &'static str) -> Self {
        let mut credential = Self::with(name, CredentialSourceKind::Endpoint, Vec::new());
        credential.source_endpoint = Some(root_relative_path);
        credential
    }

    fn with(name: &'static str, kind: CredentialSourceKind, secrets: Vec<&'static str>) -> Self {
        Self {
            name,
            kind,
            secrets: secrets.into_boxed_slice(),
            provider_type: None,
            source_endpoint: None,
            resolved_source_endpoint: None,
            docs: &[],
        }
    }

    /// Records author documentation lines.
    #[must_use]
    pub fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Returns the credential name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the source kind.
    #[must_use]
    pub const fn kind(&self) -> CredentialSourceKind {
        self.kind
    }

    /// Returns referenced secret names in argument order.
    #[must_use]
    pub fn secrets(&self) -> &[&'static str] {
        &self.secrets
    }

    /// Returns the custom provider Rust type spelling.
    #[must_use]
    pub const fn provider_type(&self) -> Option<&'static str> {
        self.provider_type
    }

    /// Returns the root-relative source endpoint path as written.
    #[must_use]
    pub const fn source_endpoint(&self) -> Option<&'static str> {
        self.source_endpoint
    }

    /// Returns the source endpoint's exact owner source path once built.
    #[must_use]
    pub const fn resolved_source_endpoint(&self) -> Option<&'static str> {
        self.resolved_source_endpoint
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn doc_lines(&self) -> &'static [&'static str] {
        self.docs
    }

    /// Returns the static challenge capability of this source.
    #[must_use]
    pub const fn challenge_recovery(&self) -> ChallengeRecovery {
        self.kind.challenge_recovery()
    }
}

/// The private target of one authentication placement.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlacementTarget {
    /// `bearer credential`: the `Authorization` field with `Bearer`.
    Bearer,
    /// `basic credential`: the `Authorization` field with `Basic`.
    Basic,
    /// `authorization = credential` or `authorization "Scheme" = credential`.
    Authorization {
        /// The literal authorization scheme, when written.
        scheme: Option<&'static str>,
    },
    /// `header "Name" = credential`.
    Header(&'static str),
    /// `query "name" = credential`.
    Query(&'static str),
}

impl PlacementTarget {
    /// Returns the stable lowercase target-kind label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bearer => "bearer",
            Self::Basic => "basic",
            Self::Authorization { .. } => "authorization",
            Self::Header(_) => "header",
            Self::Query(_) => "query",
        }
    }

    /// Returns the head family and normalized field this target owns: the
    /// `Authorization` header for the authorization family, the lowercase
    /// header name, or the exact query key.
    #[must_use]
    pub fn owned_field(self) -> (super::HeadFamily, String) {
        match self {
            Self::Bearer | Self::Basic | Self::Authorization { .. } => {
                (super::HeadFamily::Header, "authorization".to_owned())
            }
            Self::Header(name) => (super::HeadFamily::Header, name.to_ascii_lowercase()),
            Self::Query(name) => (super::HeadFamily::Query, name.to_owned()),
        }
    }
}

/// Which final statuses a placement attributes as a credential challenge.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChallengeRule {
    /// No challenge attribution (the default when omitted).
    #[default]
    Never,
    /// `challenge unauthorized`: 401.
    Unauthorized,
    /// `challenge unauthorized_or_forbidden`: 401 and 403.
    UnauthorizedOrForbidden,
}

impl ChallengeRule {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Unauthorized => "unauthorized",
            Self::UnauthorizedOrForbidden => "unauthorized_or_forbidden",
        }
    }

    /// Returns the attributed final statuses.
    #[must_use]
    pub const fn statuses(self) -> &'static [u16] {
        match self {
            Self::Never => &[],
            Self::Unauthorized => &[401],
            Self::UnauthorizedOrForbidden => &[401, 403],
        }
    }
}

/// One authentication placement as written.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthPlacement {
    target: PlacementTarget,
    credential: &'static str,
    challenge: ChallengeRule,
}

impl AuthPlacement {
    /// A placement of `credential` at `target` with no challenge attribution.
    #[must_use]
    pub const fn new(target: PlacementTarget, credential: &'static str) -> Self {
        Self {
            target,
            credential,
            challenge: ChallengeRule::Never,
        }
    }

    /// `bearer credential`.
    #[must_use]
    pub const fn bearer(credential: &'static str) -> Self {
        Self::new(PlacementTarget::Bearer, credential)
    }

    /// `basic credential`.
    #[must_use]
    pub const fn basic(credential: &'static str) -> Self {
        Self::new(PlacementTarget::Basic, credential)
    }

    /// `header "name" = credential`.
    #[must_use]
    pub const fn header(name: &'static str, credential: &'static str) -> Self {
        Self::new(PlacementTarget::Header(name), credential)
    }

    /// `query "name" = credential`.
    #[must_use]
    pub const fn query(name: &'static str, credential: &'static str) -> Self {
        Self::new(PlacementTarget::Query(name), credential)
    }

    /// Records the placement's `challenge` rule.
    #[must_use]
    pub const fn challenge(mut self, challenge: ChallengeRule) -> Self {
        self.challenge = challenge;
        self
    }

    /// Returns the target.
    #[must_use]
    pub const fn target(&self) -> PlacementTarget {
        self.target
    }

    /// Returns the credential source name.
    #[must_use]
    pub const fn credential(&self) -> &'static str {
        self.credential
    }

    /// Returns the challenge rule.
    #[must_use]
    pub const fn challenge_rule(&self) -> ChallengeRule {
        self.challenge
    }
}

/// One owner's local `auth` clause (AUTH-01).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AuthOperation {
    /// `auth { ... }`: append ordered placements.
    Append(Box<[AuthPlacement]>),
    /// `auth replace { ... }`: replace inherited placements.
    Replace(Box<[AuthPlacement]>),
    /// `auth off;`: remove inherited placements.
    Off,
}

impl AuthOperation {
    /// `auth { placements... }`.
    #[must_use]
    pub fn append(placements: Vec<AuthPlacement>) -> Self {
        Self::Append(placements.into_boxed_slice())
    }

    /// `auth replace { placements... }`.
    #[must_use]
    pub fn replace(placements: Vec<AuthPlacement>) -> Self {
        Self::Replace(placements.into_boxed_slice())
    }

    pub(crate) fn placements(&self) -> &[AuthPlacement] {
        match self {
            Self::Append(placements) | Self::Replace(placements) => placements,
            Self::Off => &[],
        }
    }
}

/// How an owner removed inherited placements.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthResetMode {
    /// `auth replace { ... }`.
    Replace,
    /// `auth off;`.
    Off,
}

impl AuthResetMode {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Off => "off",
        }
    }
}

/// One effective placement of one endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectiveAuthPlacement {
    pub(crate) placement: AuthPlacement,
    pub(crate) origin: &'static str,
    pub(crate) source_kind: CredentialSourceKind,
    pub(crate) automatic_recovery: bool,
}

impl EffectiveAuthPlacement {
    /// Returns the placement.
    #[must_use]
    pub const fn placement(&self) -> AuthPlacement {
        self.placement
    }

    /// Returns the owner source path that introduced the placement.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns the selected credential source kind.
    #[must_use]
    pub const fn source_kind(&self) -> CredentialSourceKind {
        self.source_kind
    }

    /// Returns the static challenge capability of the source.
    #[must_use]
    pub const fn recovery(&self) -> ChallengeRecovery {
        self.source_kind.challenge_recovery()
    }

    /// Returns whether an attributed challenge on this endpoint can ever lead
    /// to an automatic authentication recovery replay: only a provider source
    /// with a non-never challenge on a safe or idempotent endpoint. It remains
    /// conditional on runtime gates (§13.5).
    #[must_use]
    pub const fn automatic_recovery_available(&self) -> bool {
        self.automatic_recovery
    }
}

/// An inherited placement removed by a descendant `auth replace`/`auth off`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DisplacedAuthPlacement {
    pub(crate) placement: AuthPlacement,
    pub(crate) origin: &'static str,
    pub(crate) displaced_by: &'static str,
    pub(crate) mode: AuthResetMode,
}

impl DisplacedAuthPlacement {
    /// Returns the removed placement.
    #[must_use]
    pub const fn placement(&self) -> AuthPlacement {
        self.placement
    }

    /// Returns the owner that introduced the removed placement.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns the owner whose clause removed it.
    #[must_use]
    pub const fn displaced_by(&self) -> &'static str {
        self.displaced_by
    }

    /// Returns how it was removed.
    #[must_use]
    pub const fn mode(&self) -> AuthResetMode {
        self.mode
    }
}

/// The effective authentication of one endpoint.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EffectiveAuth {
    pub(crate) placements: Box<[EffectiveAuthPlacement]>,
    pub(crate) displaced: Box<[DisplacedAuthPlacement]>,
}

impl EffectiveAuth {
    /// Returns effective placements in application order.
    #[must_use]
    pub fn placements(&self) -> &[EffectiveAuthPlacement] {
        &self.placements
    }

    /// Returns inherited placements removed along the owner chain.
    #[must_use]
    pub fn displaced(&self) -> &[DisplacedAuthPlacement] {
        &self.displaced
    }

    /// Returns whether any effective placement can recover automatically.
    #[must_use]
    pub fn automatic_recovery_available(&self) -> bool {
        self.placements
            .iter()
            .any(EffectiveAuthPlacement::automatic_recovery_available)
    }
}
