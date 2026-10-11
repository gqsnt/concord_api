//! Owner declarations: local attachments and the owner tree.

use std::time::Duration;

use super::auth::{AuthOperation, EffectiveAuth};
use super::endpoint::{EffectiveKeyOperation, EffectiveRoute, EffectiveTimeout};
use super::rate::{EffectivePolicy, RateAttachment};
use super::shape::{
    FamilyMode, HeadFamily, KeyOperation, PathOperation, RepresentationSelection,
    SubdomainOperation,
};
use super::values::{Deprecation, OwnerKind, ValueDescription, ValueSpec};

/// The attachment clauses written at one owner, in normalized local order.
///
/// Query and header operations list direct placements first (declaration
/// order) followed by explicit block statements (source order), which is the
/// §6.1 normalization. In a built description every value reference is
/// resolved and literals under sensitive keys are redacted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalAttachments {
    pub(crate) subdomain: Option<SubdomainOperation>,
    pub(crate) path: Option<PathOperation>,
    pub(crate) query_mode: FamilyMode,
    pub(crate) query: Vec<KeyOperation>,
    pub(crate) header_mode: FamilyMode,
    pub(crate) headers: Vec<KeyOperation>,
    pub(crate) auth: Option<AuthOperation>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) request_default: Option<RepresentationSelection>,
    pub(crate) response_default: Option<RepresentationSelection>,
    pub(crate) rate_limits: Vec<RateAttachment>,
}

impl LocalAttachments {
    /// Returns the local `subdomain` clause.
    #[must_use]
    pub const fn subdomain(&self) -> Option<&SubdomainOperation> {
        self.subdomain.as_ref()
    }

    /// Returns the local `path` clause.
    #[must_use]
    pub const fn path(&self) -> Option<&PathOperation> {
        self.path.as_ref()
    }

    /// Returns the family mode for `family`.
    #[must_use]
    pub const fn mode(&self, family: HeadFamily) -> FamilyMode {
        match family {
            HeadFamily::Query => self.query_mode,
            HeadFamily::Header => self.header_mode,
        }
    }

    /// Returns the normalized local operations for `family`.
    #[must_use]
    pub fn operations(&self, family: HeadFamily) -> &[KeyOperation] {
        match family {
            HeadFamily::Query => &self.query,
            HeadFamily::Header => &self.headers,
        }
    }

    pub(crate) fn operations_mut(&mut self, family: HeadFamily) -> &mut Vec<KeyOperation> {
        match family {
            HeadFamily::Query => &mut self.query,
            HeadFamily::Header => &mut self.headers,
        }
    }

    /// Returns the local `auth` clause.
    #[must_use]
    pub const fn auth(&self) -> Option<&AuthOperation> {
        self.auth.as_ref()
    }

    /// Returns the local finite `timeout` selection.
    #[must_use]
    pub const fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Returns the local `request` default or fence.
    #[must_use]
    pub const fn request_default(&self) -> Option<RepresentationSelection> {
        self.request_default
    }

    /// Returns the local `response` default or fence.
    #[must_use]
    pub const fn response_default(&self) -> Option<RepresentationSelection> {
        self.response_default
    }

    /// Returns local rate attachments in source order.
    #[must_use]
    pub fn rate_limits(&self) -> &[RateAttachment] {
        &self.rate_limits
    }
}

/// Generated-code bridge input for one client or scope owner.
///
/// Every method records static declaration text only. Calling a singleton
/// clause setter twice records a duplicate-clause violation that
/// [`ContractBuilder::build`](super::ContractBuilder::build) rejects
/// (LANG-02); it is never last-writer-wins.
#[derive(Clone, Debug)]
pub struct OwnerSpec {
    pub(crate) kind: OwnerKind,
    pub(crate) path: &'static str,
    pub(crate) docs: &'static [&'static str],
    pub(crate) deprecation: Option<Deprecation>,
    pub(crate) values: Vec<ValueSpec>,
    pub(crate) local: LocalAttachments,
    pub(crate) duplicate_clause: Option<&'static str>,
}

impl OwnerSpec {
    /// The client root `client Name { ... }`; its source path is `name`.
    #[must_use]
    pub fn client(name: &'static str) -> Self {
        Self::new(OwnerKind::Client, name)
    }

    /// A scope with its complete source path, for example
    /// `CatalogApi::regional`. The parent is the path without its last
    /// `::` segment and must already have been added.
    #[must_use]
    pub fn scope(path: &'static str) -> Self {
        Self::new(OwnerKind::Scope, path)
    }

    pub(crate) fn new(kind: OwnerKind, path: &'static str) -> Self {
        Self {
            kind,
            path,
            docs: &[],
            deprecation: None,
            values: Vec::new(),
            local: LocalAttachments::default(),
            duplicate_clause: None,
        }
    }

    fn duplicate(&mut self, clause: &'static str, already: bool) {
        if already && self.duplicate_clause.is_none() {
            self.duplicate_clause = Some(clause);
        }
    }

    /// Records author documentation lines in source order.
    #[must_use]
    pub fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Records author deprecation.
    #[must_use]
    pub fn deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecation = Some(deprecation);
        self
    }

    /// Appends one value declaration (root `values` entry or scope
    /// parameter) in declaration order.
    #[must_use]
    pub fn value(mut self, value: ValueSpec) -> Self {
        self.values.push(value);
        self
    }

    /// Records the `subdomain` clause.
    #[must_use]
    pub fn subdomain(mut self, operation: SubdomainOperation) -> Self {
        let already = self.local.subdomain.is_some();
        self.duplicate("subdomain", already);
        self.local.subdomain = Some(operation);
        self
    }

    /// Records the `path` clause.
    #[must_use]
    pub fn path(mut self, operation: PathOperation) -> Self {
        let already = self.local.path.is_some();
        self.duplicate("path", already);
        self.local.path = Some(operation);
        self
    }

    /// Records the query family mode (`query replace { }` or `query off;`).
    #[must_use]
    pub fn query_mode(mut self, mode: FamilyMode) -> Self {
        let already = self.local.query_mode != FamilyMode::Inherit;
        self.duplicate("query", already);
        self.local.query_mode = mode;
        self
    }

    /// Appends one query operation in normalized local order.
    #[must_use]
    pub fn query(mut self, operation: KeyOperation) -> Self {
        self.local.query.push(operation);
        self
    }

    /// Records the header family mode (`headers replace { }` or `headers off;`).
    #[must_use]
    pub fn header_mode(mut self, mode: FamilyMode) -> Self {
        let already = self.local.header_mode != FamilyMode::Inherit;
        self.duplicate("headers", already);
        self.local.header_mode = mode;
        self
    }

    /// Appends one header operation in normalized local order.
    #[must_use]
    pub fn header(mut self, operation: KeyOperation) -> Self {
        self.local.headers.push(operation);
        self
    }

    /// Records the `auth` clause.
    #[must_use]
    pub fn auth(mut self, operation: AuthOperation) -> Self {
        let already = self.local.auth.is_some();
        self.duplicate("auth", already);
        self.local.auth = Some(operation);
        self
    }

    /// Records the finite `timeout` selection.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        let already = self.local.timeout.is_some();
        self.duplicate("timeout", already);
        self.local.timeout = Some(timeout);
        self
    }

    /// Records the `request` default or fence.
    #[must_use]
    pub fn request_default(mut self, selection: RepresentationSelection) -> Self {
        let already = self.local.request_default.is_some();
        self.duplicate("request", already);
        self.local.request_default = Some(selection);
        self
    }

    /// Records the `response` default or fence.
    #[must_use]
    pub fn response_default(mut self, selection: RepresentationSelection) -> Self {
        let already = self.local.response_default.is_some();
        self.duplicate("response", already);
        self.local.response_default = Some(selection);
        self
    }

    /// Appends one rate attachment in source order.
    #[must_use]
    pub fn rate_limit(mut self, attachment: RateAttachment) -> Self {
        self.local.rate_limits.push(attachment);
        self
    }
}

/// One resolved owner (client, scope or endpoint) in the owner tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerDescription {
    pub(crate) kind: OwnerKind,
    pub(crate) path: &'static str,
    pub(crate) name: &'static str,
    pub(crate) parent: Option<&'static str>,
    pub(crate) children: Box<[&'static str]>,
    pub(crate) docs: &'static [&'static str],
    pub(crate) deprecation: Option<Deprecation>,
    pub(crate) values: Box<[ValueDescription]>,
    pub(crate) local: LocalAttachments,
    pub(crate) effective: Option<EffectiveOwnerView>,
}

impl OwnerDescription {
    /// Returns the owner kind.
    #[must_use]
    pub const fn kind(&self) -> OwnerKind {
        self.kind
    }

    /// Returns the source path, for example `CatalogApi::regional`.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Returns the logical owner name (the last path segment).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the parent owner path; `None` for the client.
    #[must_use]
    pub const fn parent(&self) -> Option<&'static str> {
        self.parent
    }

    /// Returns child owner paths (scopes and endpoints) in source order.
    #[must_use]
    pub fn children(&self) -> &[&'static str] {
        &self.children
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn docs(&self) -> &'static [&'static str] {
        self.docs
    }

    /// Returns author deprecation.
    #[must_use]
    pub const fn deprecation(&self) -> Option<Deprecation> {
        self.deprecation
    }

    /// Returns owned value declarations in declaration order.
    #[must_use]
    pub fn values(&self) -> &[ValueDescription] {
        &self.values
    }

    /// Returns one owned value by name.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&ValueDescription> {
        self.values.iter().find(|value| value.name() == name)
    }

    /// Returns the attachments written locally at this owner.
    #[must_use]
    pub const fn local(&self) -> &LocalAttachments {
        &self.local
    }

    /// Returns the effective inherited-plus-local attachment state of a
    /// client or scope (the projection runtime binding at this owner uses).
    /// `None` for endpoints, whose effective state is their
    /// [`EndpointContract`](super::EndpointContract).
    #[must_use]
    pub const fn effective(&self) -> Option<&EffectiveOwnerView> {
        self.effective.as_ref()
    }
}

/// The nearest representation default for one direction at an owner.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct EffectiveDefault {
    pub(crate) type_name: Option<&'static str>,
    pub(crate) owner: Option<&'static str>,
    pub(crate) fenced: bool,
}

impl EffectiveDefault {
    /// Returns the inherited default representation type, when one applies.
    #[must_use]
    pub const fn type_name(&self) -> Option<&'static str> {
        self.type_name
    }

    /// Returns the owner that selected the default or wrote the fence.
    #[must_use]
    pub const fn owner(&self) -> Option<&'static str> {
        self.owner
    }

    /// Returns whether the nearest selection is an `explicit` fence.
    #[must_use]
    pub const fn fenced(&self) -> bool {
        self.fenced
    }
}

/// The effective attachment state at one client or scope owner.
///
/// Automatic-recovery facts assume a safe or idempotent endpoint; an
/// endpoint's own record states the exact answer for its effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveOwnerView {
    pub(crate) route: EffectiveRoute,
    pub(crate) query: Box<[EffectiveKeyOperation]>,
    pub(crate) headers: Box<[EffectiveKeyOperation]>,
    pub(crate) auth: EffectiveAuth,
    pub(crate) policies: Box<[EffectivePolicy]>,
    pub(crate) timeout: EffectiveTimeout,
    pub(crate) request_default: EffectiveDefault,
    pub(crate) response_default: EffectiveDefault,
}

impl EffectiveOwnerView {
    /// Returns the effective generated route at this owner.
    #[must_use]
    pub const fn route(&self) -> &EffectiveRoute {
        &self.route
    }

    /// Returns the effective symbolic program of `family`.
    #[must_use]
    pub fn operations(&self, family: HeadFamily) -> &[EffectiveKeyOperation] {
        match family {
            HeadFamily::Query => &self.query,
            HeadFamily::Header => &self.headers,
        }
    }

    /// Returns the effective authentication.
    #[must_use]
    pub const fn auth(&self) -> &EffectiveAuth {
        &self.auth
    }

    /// Returns the effective, deduplicated policies.
    #[must_use]
    pub fn policies(&self) -> &[EffectivePolicy] {
        &self.policies
    }

    /// Returns the effective timeout and its source.
    #[must_use]
    pub const fn timeout(&self) -> EffectiveTimeout {
        self.timeout
    }

    /// Returns the nearest request representation default.
    #[must_use]
    pub const fn request_default(&self) -> EffectiveDefault {
        self.request_default
    }

    /// Returns the nearest response representation default.
    #[must_use]
    pub const fn response_default(&self) -> EffectiveDefault {
        self.response_default
    }
}
