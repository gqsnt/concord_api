//! The complete effective contract description, its builder and errors.

use std::fmt;

use super::auth::{CredentialDescription, SecretDescription};
use super::endpoint::{EffectiveTimeout, EndpointContract, EndpointSpec};
use super::owner::{OwnerDescription, OwnerSpec};
use super::rate::{PolicyCoverage, PolicyDefinition, WindowSetDescription};
use super::values::{OwnerKind, ValueRef};

/// The normalized fixed base of a client (ORIGIN-01).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BaseDescription {
    pub(crate) literal: &'static str,
    pub(crate) scheme: Box<str>,
    pub(crate) host: Box<str>,
    pub(crate) port: u16,
    pub(crate) path_prefix: Box<str>,
    pub(crate) ip_literal: bool,
}

impl BaseDescription {
    /// Returns the base literal exactly as written.
    #[must_use]
    pub const fn literal(&self) -> &'static str {
        self.literal
    }

    /// Returns the normalized scheme (`http` or `https`).
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Returns the normalized fixed host suffix.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the effective port, including default ports.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the normalized fixed base path, always ending with `/`.
    #[must_use]
    pub fn path_prefix(&self) -> &str {
        &self.path_prefix
    }

    /// Returns whether the host is an IP literal (no generated labels).
    #[must_use]
    pub const fn is_ip_literal(&self) -> bool {
        self.ip_literal
    }

    /// Returns the normalized origin text `scheme://host:port` used as the
    /// fixed suffix identity (generated labels prepend to the host).
    #[must_use]
    pub fn origin(&self) -> String {
        format!("{}://{}:{}", self.scheme, self.host, self.port)
    }
}

/// The category of one opaque dependency (§18.2).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyKind {
    /// An ordinary value payload type (its atom/key encoding is user code).
    PayloadType,
    /// An endpoint `.await` output type.
    OutputType,
    /// A body input type.
    BodyInputType,
    /// A request or response representation type.
    Representation,
    /// An `AtomEncoder` adapter type.
    AtomAdapter,
    /// A `RateKeyEncoder` type.
    RateKeyEncoder,
    /// A pagination controller type.
    PaginationController,
    /// A pagination `Init` type.
    PaginationInit,
    /// A custom credential provider type.
    CredentialProvider,
    /// A Rust constant path used as an initializer.
    Constant,
    /// A redacted sensitive literal: its text is withheld, so comparisons
    /// involving it are unresolved.
    RedactedLiteral,
}

impl DependencyKind {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PayloadType => "payload_type",
            Self::OutputType => "output_type",
            Self::BodyInputType => "body_input_type",
            Self::Representation => "representation",
            Self::AtomAdapter => "atom_adapter",
            Self::RateKeyEncoder => "rate_key_encoder",
            Self::PaginationController => "pagination_controller",
            Self::PaginationInit => "pagination_init",
            Self::CredentialProvider => "credential_provider",
            Self::Constant => "constant",
            Self::RedactedLiteral => "redacted_literal",
        }
    }
}

/// One Rust type/constant/adapter reference whose semantics cannot be proved
/// from declaration text, or one redacted literal.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpaqueDependency {
    pub(crate) kind: DependencyKind,
    pub(crate) reference: Box<str>,
    pub(crate) sites: Box<[Box<str>]>,
}

impl OpaqueDependency {
    /// Returns the dependency category.
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        self.kind
    }

    /// Returns the Rust spelling, or the structural location for a redacted
    /// literal.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Returns sorted structural sites that use the dependency.
    #[must_use]
    pub fn sites(&self) -> &[Box<str>] {
        &self.sites
    }
}

/// Severity of one contract diagnostic.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSeverity {
    /// Informational.
    Note,
    /// A consequential but accepted declaration fact.
    Warning,
}

impl DiagnosticSeverity {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Warning => "warning",
        }
    }
}

/// One client-level diagnostic attached to the description (§19.1 shape).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContractDiagnostic {
    code: &'static str,
    severity: DiagnosticSeverity,
    owner: &'static str,
    subject: &'static str,
    message: &'static str,
    correction: Option<&'static str>,
}

impl ContractDiagnostic {
    /// A diagnostic with a stable Concord code family (for example
    /// `C3-RATE`), owning source path, subject and concise cause.
    #[must_use]
    pub const fn new(
        code: &'static str,
        severity: DiagnosticSeverity,
        owner: &'static str,
        subject: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            code,
            severity,
            owner,
            subject,
            message,
            correction: None,
        }
    }

    /// Records one correction valid in the v3 grammar.
    #[must_use]
    pub const fn correction(mut self, correction: &'static str) -> Self {
        self.correction = Some(correction);
        self
    }

    /// Returns the stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns the owning source path.
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// Returns the structural subject.
    #[must_use]
    pub const fn subject(&self) -> &'static str {
        self.subject
    }

    /// Returns the concise cause.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }

    /// Returns the correction, when recorded.
    #[must_use]
    pub const fn correction_text(&self) -> Option<&'static str> {
        self.correction
    }
}

/// The complete immutable, value-free effective contract of one client.
///
/// # Ownership, concurrency and cost
///
/// A description is plain immutable data (`Send + Sync`). Generated code
/// builds it once inside a `LazyLock` and hands out `&'static` references
/// from `Client::contract()`; reading it requires no executable client,
/// secret, limiter, transport or network, and constructing it never evaluates
/// payload defaults, invokes converters or encoders, or clones values.
///
/// # Sensitivity
///
/// The description holds declaration text only. Sensitive literals are
/// retained as [`InitializerExpression::Redacted`](super::InitializerExpression)
/// / [`LiteralText::Redacted`](super::LiteralText) markers; credential
/// material and secret values never exist here.
///
/// # Errors and negative capabilities
///
/// Construction through [`ContractBuilder::build`] validates structure and
/// returns a value-free [`ContractBuildError`]. A description grants no
/// execution authority: it cannot bind, send, authenticate or admit quota.
#[derive(Clone, Debug)]
pub struct ContractDescription {
    pub(crate) base: BaseDescription,
    pub(crate) root_timeout: EffectiveTimeout,
    pub(crate) owners: Box<[OwnerDescription]>,
    pub(crate) endpoints: Box<[EndpointContract]>,
    pub(crate) secrets: Box<[SecretDescription]>,
    pub(crate) credentials: Box<[CredentialDescription]>,
    pub(crate) window_sets: Box<[WindowSetDescription]>,
    pub(crate) policy_definitions: Box<[PolicyDefinition]>,
    pub(crate) policies: Box<[PolicyCoverage]>,
    pub(crate) dependencies: Box<[super::contract::OpaqueDependency]>,
    pub(crate) diagnostics: Box<[ContractDiagnostic]>,
    pub(crate) endpoint_index: Box<[(Box<str>, usize)]>,
}

impl ContractDescription {
    /// Starts a generated-code builder for client `client` with base literal
    /// `base`.
    #[must_use]
    pub fn builder(client: OwnerSpec, base: &'static str) -> ContractBuilder {
        ContractBuilder::new(client, base)
    }

    /// Returns the client owner record.
    #[must_use]
    pub fn client(&self) -> &OwnerDescription {
        &self.owners[0]
    }

    /// Returns the client name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.owners[0].name
    }

    /// Returns the normalized fixed base.
    #[must_use]
    pub const fn base(&self) -> &BaseDescription {
        &self.base
    }

    /// Returns the client's effective root timeout and its source.
    #[must_use]
    pub const fn root_timeout(&self) -> EffectiveTimeout {
        self.root_timeout
    }

    /// Returns every owner in source pre-order (client first).
    #[must_use]
    pub fn owners(&self) -> &[OwnerDescription] {
        &self.owners
    }

    /// Returns one owner by source path.
    #[must_use]
    pub fn owner(&self, path: &str) -> Option<&OwnerDescription> {
        self.owners.iter().find(|owner| owner.path == path)
    }

    /// Returns every endpoint in source pre-order.
    #[must_use]
    pub fn endpoints(&self) -> &[EndpointContract] {
        &self.endpoints
    }

    /// Returns one endpoint by operational ID.
    #[must_use]
    pub fn endpoint(&self, id: &str) -> Option<&EndpointContract> {
        self.endpoint_index
            .binary_search_by(|(candidate, _)| (**candidate).cmp(id))
            .ok()
            .map(|index| &self.endpoints[self.endpoint_index[index].1])
    }

    /// Returns one endpoint by source path.
    #[must_use]
    pub fn endpoint_by_path(&self, path: &str) -> Option<&EndpointContract> {
        self.endpoints.iter().find(|endpoint| endpoint.path == path)
    }

    /// Returns protected builder-input declarations.
    #[must_use]
    pub fn secrets(&self) -> &[SecretDescription] {
        &self.secrets
    }

    /// Returns credential source declarations.
    #[must_use]
    pub fn credentials(&self) -> &[CredentialDescription] {
        &self.credentials
    }

    /// Returns one credential source by name.
    #[must_use]
    pub fn credential(&self, name: &str) -> Option<&CredentialDescription> {
        self.credentials
            .iter()
            .find(|credential| credential.name == name)
    }

    /// Returns reusable window sets in source order.
    #[must_use]
    pub fn window_sets(&self) -> &[WindowSetDescription] {
        &self.window_sets
    }

    /// Returns named registry policies in source order.
    #[must_use]
    pub fn policy_definitions(&self) -> &[PolicyDefinition] {
        &self.policy_definitions
    }

    /// Returns policy identities with coverage and sharing, sorted by ID.
    #[must_use]
    pub fn policies(&self) -> &[PolicyCoverage] {
        &self.policies
    }

    /// Returns one policy identity by ID.
    #[must_use]
    pub fn policy(&self, id: &str) -> Option<&PolicyCoverage> {
        self.policies
            .binary_search_by(|policy| policy.id.cmp(id))
            .ok()
            .map(|index| &self.policies[index])
    }

    /// Returns opaque dependencies sorted by kind and reference.
    #[must_use]
    pub fn dependencies(&self) -> &[OpaqueDependency] {
        &self.dependencies
    }

    /// Returns client-level diagnostics (generated and computed).
    #[must_use]
    pub fn diagnostics(&self) -> &[ContractDiagnostic] {
        &self.diagnostics
    }

    /// Renders a value reference in the DSL display form: `client.name`,
    /// `scope.name` or `endpoint.name`.
    #[must_use]
    pub fn display_ref(&self, value: &ValueRef) -> String {
        let Some(path) = value.owner_path() else {
            return value.name().to_owned();
        };
        let owner = self.owner(path).map_or("?", |owner| match owner.kind {
            OwnerKind::Client => "client",
            OwnerKind::Endpoint => "endpoint",
            OwnerKind::Scope => owner.name,
        });
        format!("{owner}.{}", value.name())
    }

    /// Renders the complete logical URL template of an endpoint, for example
    /// `https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items`.
    /// Default ports are omitted from the rendering only; the effective port
    /// is recorded in [`BaseDescription::port`].
    #[must_use]
    pub fn url_template(&self, endpoint: &EndpointContract) -> String {
        self.route_template(&endpoint.route)
    }

    /// Renders the logical URL template of any effective route (an
    /// endpoint's or an owner's [`EffectiveOwnerView`](super::EffectiveOwnerView)).
    #[must_use]
    pub fn route_template(&self, route: &super::EffectiveRoute) -> String {
        let display = |value: &ValueRef| self.display_ref(value);
        let mut url = format!("{}://", self.base.scheme);
        for label in route.labels.iter() {
            url.push_str(&super::render_atom(&label.atom, &display));
            url.push('.');
        }
        url.push_str(&self.base.host);
        let default_port = match &*self.base.scheme {
            "https" => 443,
            _ => 80,
        };
        if self.base.port != default_port {
            url.push(':');
            url.push_str(&self.base.port.to_string());
        }
        url.push_str(&self.base.path_prefix);
        let segments = route
            .segments
            .iter()
            .map(|segment| super::render_segment(&segment.segment, &display))
            .collect::<Vec<_>>();
        url.push_str(&segments.join("/"));
        url
    }
}

impl fmt::Display for ContractDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({} endpoints, {})",
            self.name(),
            self.endpoints.len(),
            super::CONTRACT_FORMAT
        )
    }
}

/// Generated-code bridge that assembles one [`ContractDescription`].
///
/// Add owners in source pre-order: the client (given to [`Self::new`]), then
/// every scope and endpoint after its parent. Registry items (secrets,
/// credentials, window sets, policies) may be added in any order. The
/// builder only stores static declaration text; [`Self::build`] validates it
/// and derives every effective view.
#[derive(Clone, Debug)]
pub struct ContractBuilder {
    pub(crate) client: OwnerSpec,
    pub(crate) base: &'static str,
    pub(crate) owners: Vec<OwnerOrEndpoint>,
    pub(crate) secrets: Vec<SecretDescription>,
    pub(crate) credentials: Vec<CredentialDescription>,
    pub(crate) window_sets: Vec<WindowSetDescription>,
    pub(crate) policies: Vec<PolicyDefinition>,
    pub(crate) diagnostics: Vec<ContractDiagnostic>,
}

#[derive(Clone, Debug)]
pub(crate) enum OwnerOrEndpoint {
    Scope(Box<OwnerSpec>),
    Endpoint(Box<EndpointSpec>),
}

impl ContractBuilder {
    /// Starts a builder for `client` (from [`OwnerSpec::client`]) with the
    /// base literal exactly as written.
    #[must_use]
    pub fn new(client: OwnerSpec, base: &'static str) -> Self {
        Self {
            client,
            base,
            owners: Vec::new(),
            secrets: Vec::new(),
            credentials: Vec::new(),
            window_sets: Vec::new(),
            policies: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Adds one `secret` declaration.
    #[must_use]
    pub fn secret(mut self, secret: SecretDescription) -> Self {
        self.secrets.push(secret);
        self
    }

    /// Adds one `credential` declaration.
    #[must_use]
    pub fn credential(mut self, credential: CredentialDescription) -> Self {
        self.credentials.push(credential);
        self
    }

    /// Adds one `windows` set.
    #[must_use]
    pub fn window_set(mut self, window_set: WindowSetDescription) -> Self {
        self.window_sets.push(window_set);
        self
    }

    /// Adds one named registry `policy`.
    #[must_use]
    pub fn policy(mut self, policy: PolicyDefinition) -> Self {
        self.policies.push(policy);
        self
    }

    /// Adds one scope after its parent.
    #[must_use]
    pub fn scope(mut self, scope: OwnerSpec) -> Self {
        self.owners.push(OwnerOrEndpoint::Scope(Box::new(scope)));
        self
    }

    /// Adds one endpoint after its parent.
    #[must_use]
    pub fn endpoint(mut self, endpoint: EndpointSpec) -> Self {
        self.owners
            .push(OwnerOrEndpoint::Endpoint(Box::new(endpoint)));
        self
    }

    /// Adds one generated diagnostic.
    #[must_use]
    pub fn diagnostic(mut self, diagnostic: ContractDiagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Validates the declaration and derives the complete description.
    ///
    /// # Errors
    ///
    /// Returns a value-free [`ContractBuildError`] naming the first
    /// structural violation, its owner source path and a structural subject
    /// (key, value, policy or credential name). No literal text is included.
    pub fn build(self) -> Result<ContractDescription, ContractBuildError> {
        super::resolve::build(self)
    }
}

/// The category of a contract-description validation failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContractBuildErrorKind {
    /// An owner source path is empty, malformed or not under the client.
    InvalidOwnerPath,
    /// Two owners share one source path.
    DuplicateOwner,
    /// The parent path was not added before the owner, or is an endpoint.
    InvalidParent,
    /// A scope uses a reserved owner name or repeats a visible scope name.
    InvalidScopeName,
    /// A singleton clause or body was written twice (LANG-02).
    DuplicateClause,
    /// An owner declares two values with one name.
    DuplicateValue,
    /// A bare or qualified reference names no visible declared value.
    UnboundReference,
    /// A reference redundantly qualifies the current owner.
    RedundantQualification,
    /// A route or concatenation atom references an outer-optional value.
    NonTotalReference,
    /// The base literal is not an absolute http(s) URL without userinfo,
    /// query or fragment.
    InvalidBase,
    /// A literal DNS label is invalid, or labels are generated on an IP base.
    InvalidDnsLabel,
    /// A path segment is empty, `.`/`..`, or contains `/`, `\`, `?` or `#`.
    InvalidPathSegment,
    /// A query/header key is empty, overlong or not a valid field name.
    InvalidKey,
    /// A public header targets a field owned by transport, media, auth or
    /// negotiation, or a public key collides with an auth placement target.
    ForbiddenHeader,
    /// A singleton request header is repeated through `each`/`append`.
    SingletonHeaderRepetition,
    /// Direct placements do not precede block statements, or a direct
    /// placement is not an introduction.
    InvalidPlacementOrder,
    /// A family is `off` while the owner writes operations for it.
    FamilyOffWithOperations,
    /// A plain introduction targets a key with a prior potential owner.
    IntroduceConflict,
    /// `replace`/`when_present`/`remove` targets a key with no prior owner.
    MissingOverrideTarget,
    /// `when_present` needs an outer-optional top-level source.
    WhenPresentRequiresOptional,
    /// A `remove` carries an emission, or an introduction lacks one.
    InvalidEmission,
    /// A secret name is repeated.
    DuplicateSecret,
    /// A credential name is repeated.
    DuplicateCredential,
    /// A placement or credential references an unknown credential/secret.
    UnknownCredential,
    /// A credential source has the wrong arity, provider or endpoint path.
    InvalidCredentialSource,
    /// The credential dependency graph has a cycle.
    CredentialCycle,
    /// Two effective placements claim one target, too many placements are
    /// effective, or more than one placement attributes a challenge.
    AuthPlacementConflict,
    /// A challenge status overlaps accepted or absent statuses.
    ChallengeStatusOverlap,
    /// The method token is invalid or forbidden.
    InvalidMethod,
    /// An explicit endpoint ID is invalid.
    InvalidEndpointId,
    /// A navigation-derived endpoint ID is invalid; an explicit ID is
    /// required.
    InvalidDerivedEndpointId,
    /// Two endpoints share one operational ID.
    DuplicateEndpointId,
    /// A status set is empty, informational, unordered or overlapping.
    InvalidStatus,
    /// No response or body representation is selected (fence named).
    MissingRepresentation,
    /// The endpoint result was not recorded.
    MissingResult,
    /// A timeout or window duration is zero.
    InvalidDuration,
    /// A window set name is repeated or unknown.
    InvalidWindowSet,
    /// A window is invalid, repeated by duration, too numerous, mixed with a
    /// named set, or missing.
    InvalidWindow,
    /// A named policy is repeated or unknown.
    InvalidPolicy,
    /// A policy ID is empty or overlong.
    InvalidPolicyId,
    /// A key label is invalid, reserved or repeated, or the dimension count
    /// is out of range.
    InvalidRateKey,
    /// A named policy declares a formal no dimension uses.
    UnusedFormal,
    /// Attachment bindings are not exact and total.
    InvalidRateBinding,
    /// A custom dimension binds an optional or page-live value.
    InvalidRateValue,
    /// The `endpoint` dimension covers an endpoint without an explicit ID.
    EndpointDimensionRequiresExplicitId,
    /// A cost is zero or exceeds a window capacity.
    InvalidCost,
    /// One endpoint has conflicting attachments of one policy ID, or too
    /// many policies.
    ConflictingPolicy,
    /// One policy ID has two schemas within the client.
    InconsistentPolicySchema,
    /// A pagination declaration violates §14 obligations.
    InvalidPagination,
    /// `absent on` violates §10.2 obligations.
    InvalidAbsence,
    /// `expose accepted_bytes` requires a buffered or discard driver.
    InvalidAcceptedBytes,
    /// A live value has no semantic use, or a no-op value is used.
    DeadValue,
    /// A deprecated no-op value is not deprecated or is a required input.
    InvalidNoOpValue,
    /// An encoding ID is not a bounded graphic ASCII contract name.
    InvalidEncodingId,
    /// A lookup named no endpoint of the contract.
    UnknownEndpoint,
}

impl ContractBuildErrorKind {
    const fn label(self) -> &'static str {
        match self {
            Self::InvalidOwnerPath => "invalid owner path",
            Self::DuplicateOwner => "duplicate owner",
            Self::InvalidParent => "invalid parent owner",
            Self::InvalidScopeName => "invalid scope name",
            Self::DuplicateClause => "duplicate singleton clause",
            Self::DuplicateValue => "duplicate value",
            Self::UnboundReference => "unbound value reference",
            Self::RedundantQualification => "redundant owner qualification",
            Self::NonTotalReference => "outer-optional value used where a total value is required",
            Self::InvalidBase => "invalid base URL",
            Self::InvalidDnsLabel => "invalid DNS label",
            Self::InvalidPathSegment => "invalid path segment",
            Self::InvalidKey => "invalid query/header key",
            Self::ForbiddenHeader => "forbidden or conflicting header target",
            Self::SingletonHeaderRepetition => "repeated singleton header",
            Self::InvalidPlacementOrder => "invalid direct placement order",
            Self::FamilyOffWithOperations => "family off with local operations",
            Self::IntroduceConflict => "introduction conflicts with an earlier owner",
            Self::MissingOverrideTarget => "override has no earlier owner",
            Self::WhenPresentRequiresOptional => "when_present requires an optional source",
            Self::InvalidEmission => "invalid emission",
            Self::DuplicateSecret => "duplicate secret",
            Self::DuplicateCredential => "duplicate credential",
            Self::UnknownCredential => "unknown credential or secret",
            Self::InvalidCredentialSource => "invalid credential source",
            Self::CredentialCycle => "credential source cycle",
            Self::AuthPlacementConflict => "conflicting auth placements",
            Self::ChallengeStatusOverlap => "challenge status overlaps an outcome status",
            Self::InvalidMethod => "invalid method",
            Self::InvalidEndpointId => "invalid endpoint ID",
            Self::InvalidDerivedEndpointId => {
                "invalid derived endpoint ID; an explicit ID is required"
            }
            Self::DuplicateEndpointId => "duplicate endpoint ID",
            Self::InvalidStatus => "invalid status set",
            Self::MissingRepresentation => "missing representation",
            Self::MissingResult => "missing endpoint result",
            Self::InvalidDuration => "invalid duration",
            Self::InvalidWindowSet => "invalid window set",
            Self::InvalidWindow => "invalid rate window",
            Self::InvalidPolicy => "invalid policy",
            Self::InvalidPolicyId => "invalid policy ID",
            Self::InvalidRateKey => "invalid rate key",
            Self::UnusedFormal => "unused policy formal",
            Self::InvalidRateBinding => "invalid policy bindings",
            Self::InvalidRateValue => "invalid rate key value",
            Self::EndpointDimensionRequiresExplicitId => {
                "endpoint dimension requires an explicit endpoint ID"
            }
            Self::InvalidCost => "invalid rate cost",
            Self::ConflictingPolicy => "conflicting effective policy",
            Self::InconsistentPolicySchema => "inconsistent policy schema",
            Self::InvalidPagination => "invalid pagination",
            Self::InvalidAbsence => "invalid expected absence",
            Self::InvalidAcceptedBytes => "invalid accepted-bytes exposure",
            Self::DeadValue => "value liveness violation",
            Self::InvalidNoOpValue => "invalid deprecated no-op value",
            Self::InvalidEncodingId => "invalid encoding ID",
            Self::UnknownEndpoint => "unknown endpoint",
        }
    }
}

/// A value-free contract-description validation failure.
///
/// The subject is structural declaration text (a key, value, policy or
/// credential name, or an endpoint ID); literal values are never included.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractBuildError {
    kind: ContractBuildErrorKind,
    owner: Option<&'static str>,
    subject: Option<Box<str>>,
}

impl ContractBuildError {
    pub(crate) fn new(
        kind: ContractBuildErrorKind,
        owner: Option<&'static str>,
        subject: Option<&str>,
    ) -> Self {
        Self {
            kind,
            owner,
            subject: subject.map(Box::from),
        }
    }

    /// Returns the failure category.
    #[must_use]
    pub const fn kind(&self) -> ContractBuildErrorKind {
        self.kind
    }

    /// Returns the owning source path, when known.
    #[must_use]
    pub const fn owner(&self) -> Option<&'static str> {
        self.owner
    }

    /// Returns the structural subject, when known.
    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }
}

impl fmt::Display for ContractBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Concord contract description: ")?;
        formatter.write_str(self.kind.label())?;
        if let Some(owner) = self.owner {
            write!(formatter, " at `{owner}`")?;
        }
        if let Some(subject) = &self.subject {
            write!(formatter, " ({subject})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ContractBuildError {}
