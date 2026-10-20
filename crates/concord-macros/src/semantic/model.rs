//! Resolved v3 semantic model.
//!
//! The model is the macro's single source of meaning: every generated
//! accessor, patch table, route, auth/rate declaration, documentation card
//! and contract description is emitted from it. Owners (the client root and
//! every scope) and endpoints live in two arenas; value references are
//! resolved to exact `(site, index)` keys, never re-looked-up by name.

use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitStr, Path, Type};

pub(crate) use crate::syntax::ast::{
    AuthChallenge, EffectDecl, PlacementFamily, StableValue, StatementKind,
};

/// Author metadata retained for docs and descriptions.
#[derive(Clone, Default)]
pub(crate) struct Meta {
    pub(crate) docs: Vec<String>,
    pub(crate) deprecated: Option<Deprecation>,
}

#[derive(Clone, Default)]
pub(crate) struct Deprecation {
    pub(crate) note: Option<String>,
    pub(crate) since: Option<String>,
}

/// The site that owns a value: an owner (client root or scope) or an
/// endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Site {
    Owner(usize),
    Endpoint(usize),
}

/// One exact resolved value reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VRef {
    pub(crate) site: Site,
    pub(crate) index: usize,
}

#[derive(Clone)]
pub(crate) enum Initializer {
    None,
    Default(Span),
    Stable(StableValue),
}

impl Initializer {
    pub(crate) const fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A direct query/header placement retained on its value declaration.
#[derive(Clone)]
pub(crate) struct DirectPlacement {
    pub(crate) family: PlacementFamily,
    pub(crate) key: String,
    pub(crate) each: bool,
    pub(crate) span: Span,
}

/// Generated value-control method names (§4.3). `None` means the control
/// does not exist for this value's structural state.
#[derive(Clone, Default)]
pub(crate) struct ValueControls {
    pub(crate) set: Option<Ident>,
    pub(crate) replace: Option<Ident>,
    pub(crate) set_if_some: Option<Ident>,
    pub(crate) omit: Option<Ident>,
    pub(crate) reset: Option<Ident>,
}

impl ValueControls {
    pub(crate) fn names(&self) -> impl Iterator<Item = &Ident> {
        [
            &self.set,
            &self.replace,
            &self.set_if_some,
            &self.omit,
            &self.reset,
        ]
        .into_iter()
        .flatten()
    }
}

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) name: Ident,
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) sensitive: bool,
    pub(crate) optional: bool,
    pub(crate) ty: Type,
    pub(crate) via: Option<Type>,
    pub(crate) init: Initializer,
    pub(crate) placement: Option<DirectPlacement>,
    /// Declaration position among the site's parameters (including a body).
    pub(crate) position: usize,
    /// Storage field of the current selection.
    pub(crate) field: Ident,
    /// Storage field of the retained initial selection (initialized values).
    pub(crate) initial_field: Ident,
    pub(crate) controls: ValueControls,
    /// Whether the value has at least one reachable semantic use (VALUE-06).
    pub(crate) live: bool,
    /// Whether the value is a cursor destination of its endpoint's pagination.
    pub(crate) page_live: bool,
}

impl Value {
    /// Required and uninitialized: a constructor argument (or a builder
    /// requirement at the root).
    pub(crate) const fn is_required_input(&self) -> bool {
        !self.optional && !self.init.is_some()
    }

    pub(crate) const fn is_initialized(&self) -> bool {
        self.init.is_some()
    }

    pub(crate) fn is_deprecated(&self) -> bool {
        self.meta.deprecated.is_some()
    }
}

#[derive(Clone)]
pub(crate) struct Body {
    pub(crate) name: Ident,
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) optional: bool,
    pub(crate) ty: Type,
    pub(crate) via: Option<Type>,
    /// Effective request representation.
    pub(crate) representation: Type,
    /// Display provenance of the representation (`inline` or owner path).
    pub(crate) representation_origin: String,
    pub(crate) position: usize,
    pub(crate) controls: ValueControls,
}

/// Positional constructor arguments of a scope or endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Positional {
    Value(usize),
    Body,
}

// ---- atoms, routes and patch programs ----

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiteralKind {
    String,
    Char,
    Bool,
    Integer,
}

#[derive(Clone)]
pub(crate) enum RAtom {
    Literal {
        text: String,
        kind: LiteralKind,
    },
    /// `span` is the authored reference: generated atom obligations
    /// (`RequestAtom`/`AtomEncoder`) are reported there (DIAG-01).
    Value {
        value: VRef,
        via: Option<Type>,
        span: Span,
    },
    Concat(Vec<RAtom>),
}

impl RAtom {
    /// Values referenced by the atom, in order.
    pub(crate) fn values(&self, out: &mut Vec<VRef>) {
        match self {
            Self::Literal { .. } => {}
            Self::Value { value, .. } => out.push(*value),
            Self::Concat(parts) => parts.iter().for_each(|part| part.values(out)),
        }
    }

    /// Normalizes nested concatenations to one ordered part list.
    pub(crate) fn flatten(self) -> Vec<RAtom> {
        match self {
            Self::Concat(parts) => parts.into_iter().flat_map(Self::flatten).collect(),
            other => vec![other],
        }
    }
}

#[derive(Clone)]
pub(crate) enum Emission {
    Atom(RAtom),
    /// `span` is the authored `each` source (DIAG-01 obligation site).
    Each {
        value: VRef,
        via: Option<Type>,
        span: Span,
    },
}

impl Emission {
    pub(crate) fn values(&self) -> Vec<VRef> {
        let mut out = Vec::new();
        match self {
            Self::Atom(atom) => atom.values(&mut out),
            Self::Each { value, .. } => out.push(*value),
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpSite {
    Direct,
    Block,
}

/// One normalized query/header patch operation. `Clear` is represented by
/// `kind == None` (family fence).
#[derive(Clone)]
pub(crate) struct HeadOp {
    pub(crate) family: PlacementFamily,
    pub(crate) kind: Option<StatementKind>,
    pub(crate) key: String,
    pub(crate) emission: Option<Emission>,
    /// Outer presence source: an optional value whose slot decides presence.
    pub(crate) presence: Option<VRef>,
    pub(crate) site: OpSite,
    pub(crate) sensitive: bool,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FamilyMode {
    Inherit,
    Replace,
    Off,
}

/// One site's normalized local program (§6.1).
#[derive(Clone)]
pub(crate) struct LocalProgram {
    pub(crate) query_mode: FamilyMode,
    pub(crate) header_mode: FamilyMode,
    pub(crate) ops: Vec<HeadOp>,
}

#[derive(Clone)]
pub(crate) enum SegPart {
    Literal(String),
    /// `span` is the template literal that names the hole (DIAG-01).
    Value {
        value: VRef,
        via: Option<Type>,
        span: Span,
    },
}

#[derive(Clone)]
pub(crate) struct Segment {
    pub(crate) parts: Vec<SegPart>,
    pub(crate) sensitive: bool,
}

impl Segment {
    pub(crate) fn static_text(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [SegPart::Literal(text)] => Some(text),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct PathTemplate {
    pub(crate) text: String,
    pub(crate) segments: Vec<Segment>,
}

#[derive(Clone)]
pub(crate) enum LocalPath {
    Append(PathTemplate),
    Replace(PathTemplate),
    Off,
}

#[derive(Clone)]
pub(crate) struct HostLabel {
    pub(crate) atom: RAtom,
    pub(crate) sensitive: bool,
}

#[derive(Clone)]
pub(crate) enum LocalHost {
    Prepend(Vec<HostLabel>),
    Replace(Vec<HostLabel>),
    Off,
}

// ---- credentials and auth ----

#[derive(Clone)]
pub(crate) struct Secret {
    pub(crate) name: Ident,
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) field: Ident,
}

#[derive(Clone)]
pub(crate) enum CredentialSource {
    ApiKey { secret: usize },
    Bearer { secret: usize },
    Basic { username: usize, password: usize },
    Endpoint { endpoint: usize, path_text: String },
    Custom { provider: Type },
}

#[derive(Clone)]
pub(crate) struct Credential {
    pub(crate) name: Ident,
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) source: CredentialSource,
    pub(crate) field: Ident,
    pub(crate) clear: Ident,
    pub(crate) replace: Ident,
    pub(crate) store_terminal: Ident,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum PlacementKind {
    Bearer,
    Basic,
    Authorization(Option<String>),
    Header(String),
    Query(String),
}

impl PlacementKind {
    /// Normalized exclusive target: `("header", lowercase)` or `("query", key)`.
    pub(crate) fn target(&self) -> (PlacementFamily, String) {
        match self {
            Self::Bearer | Self::Basic | Self::Authorization(_) => {
                (PlacementFamily::Header, "authorization".to_owned())
            }
            Self::Header(name) => (PlacementFamily::Header, name.to_ascii_lowercase()),
            Self::Query(name) => (PlacementFamily::Query, name.clone()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Placement {
    pub(crate) kind: PlacementKind,
    pub(crate) credential: usize,
    pub(crate) challenge: AuthChallenge,
    /// Display path of the owner/endpoint that declared the placement.
    pub(crate) origin: String,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum LocalAuth {
    Append(Vec<Placement>),
    Replace(Vec<Placement>),
    Off,
}

// ---- rates ----

#[derive(Clone)]
pub(crate) struct Window {
    pub(crate) capacity: u32,
    pub(crate) duration: Duration,
}

#[derive(Clone)]
pub(crate) struct WindowSet {
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) windows: Vec<Window>,
}

#[derive(Clone)]
pub(crate) enum Windows {
    Set(usize),
    Inline(Vec<Window>),
}

#[derive(Clone)]
pub(crate) enum FormalDim {
    Origin,
    Endpoint,
    Custom {
        label: String,
        formal: usize,
        via: Option<Type>,
    },
}

#[derive(Clone)]
pub(crate) struct Policy {
    pub(crate) logical: String,
    pub(crate) meta: Meta,
    pub(crate) formals: Vec<Ident>,
    pub(crate) id: String,
    pub(crate) dims: Vec<FormalDim>,
    pub(crate) cost: Option<u32>,
    pub(crate) windows: Windows,
}

#[derive(Clone)]
pub(crate) enum BoundDim {
    Origin,
    Endpoint,
    /// `span` is the authored binding/dimension reference where the
    /// `RateKeyPart`/`RateKeyEncoder` obligation is reported (DIAG-01).
    Custom {
        label: String,
        value: VRef,
        via: Option<Type>,
        span: Span,
    },
}

#[derive(Clone)]
pub(crate) enum RateSource {
    Named {
        policy: usize,
        bindings: Vec<VRef>,
    },
    Inline {
        id: String,
        dims: Vec<BoundDim>,
        windows: Windows,
    },
}

/// One resolved rate attachment with its effective key dimensions.
#[derive(Clone)]
pub(crate) struct RateUse {
    pub(crate) source: RateSource,
    pub(crate) id: String,
    pub(crate) dims: Vec<BoundDim>,
    pub(crate) windows: Vec<Window>,
    /// Effective cost (attachment cost, else policy cost, else one).
    pub(crate) cost: u32,
    pub(crate) attachment_cost: Option<u32>,
    pub(crate) origin: String,
    pub(crate) span: Span,
}

// ---- pagination ----

#[derive(Clone)]
pub(crate) enum InitValue {
    Stable(StableValue),
    Value(VRef),
}

#[derive(Clone)]
pub(crate) struct Pagination {
    pub(crate) controller: Type,
    pub(crate) init_type: Path,
    pub(crate) fields: Vec<(Ident, InitValue)>,
    /// Endpoint-local value indexes receiving the cursor.
    pub(crate) cursor: Vec<usize>,
    pub(crate) tuple: bool,
    /// The `paginate` keyword: page-capability obligations are reported here.
    pub(crate) span: Span,
    /// The `cursor -> ...` mapping: cursor type obligations are reported here.
    pub(crate) cursor_span: Span,
}

// ---- owners and endpoints ----

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OwnerKind {
    Client,
    Scope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Child {
    Scope(usize),
    Endpoint(usize),
}

/// Effective representation default for one direction.
#[derive(Clone, Default)]
pub(crate) struct RepresentationDefault {
    pub(crate) ty: Option<Type>,
    /// Display path of the declaring owner.
    pub(crate) origin: Option<String>,
    /// A scope's `explicit` fence blocks inherited defaults.
    pub(crate) fenced: bool,
}

#[derive(Clone)]
pub(crate) enum RepresentationChoice {
    Select(Type),
    Explicit,
}

#[derive(Clone)]
pub(crate) struct OwnerSurface {
    /// Public handle type (the client type or the scope type).
    pub(crate) handle: Ident,
    /// Module segments from the invocation site to this owner's module.
    pub(crate) module_path: Vec<Ident>,
    /// Navigation method on the parent handle (scopes only).
    pub(crate) method: Option<Ident>,
}

#[derive(Clone)]
pub(crate) struct Owner {
    pub(crate) kind: OwnerKind,
    pub(crate) parent: Option<usize>,
    pub(crate) depth: usize,
    pub(crate) logical: String,
    pub(crate) display: String,
    pub(crate) meta: Meta,
    pub(crate) values: Vec<Value>,
    pub(crate) positional: Vec<usize>,
    pub(crate) host: Option<LocalHost>,
    pub(crate) path: Option<LocalPath>,
    pub(crate) program: LocalProgram,
    pub(crate) auth: Option<LocalAuth>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) request_choice: Option<RepresentationChoice>,
    pub(crate) response_choice: Option<RepresentationChoice>,
    pub(crate) rates: Vec<RateUse>,
    pub(crate) effective_timeout: Option<Duration>,
    pub(crate) effective_auth: Vec<Placement>,
    pub(crate) effective_rates: Vec<RateUse>,
    pub(crate) request_default: RepresentationDefault,
    pub(crate) response_default: RepresentationDefault,
    pub(crate) children: Vec<Child>,
    pub(crate) surface: OwnerSurface,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Custom(String),
}

impl Method {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Custom(value) => value,
        }
    }

    pub(crate) const fn default_effect(&self) -> EffectDecl {
        match self {
            Self::Get | Self::Head | Self::Options => EffectDecl::Safe,
            Self::Put | Self::Delete => EffectDecl::Idempotent,
            Self::Post | Self::Patch | Self::Custom(_) => EffectDecl::NonIdempotent,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    Success,
    /// Canonical sorted disjoint inclusive ranges.
    Ranges(Vec<(u16, u16)>),
}

impl Status {
    pub(crate) fn contains(&self, code: u16) -> bool {
        match self {
            Self::Success => (200..=299).contains(&code),
            Self::Ranges(ranges) => ranges
                .iter()
                .any(|(start, end)| (*start..=*end).contains(&code)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct EndpointSurface {
    pub(crate) method: Ident,
    pub(crate) endpoint_type: Ident,
    pub(crate) request_type: Ident,
    pub(crate) spec_type: Ident,
    pub(crate) local_type: Ident,
}

#[derive(Clone)]
pub(crate) struct Endpoint {
    pub(crate) owner: usize,
    pub(crate) logical: String,
    pub(crate) display: String,
    pub(crate) meta: Meta,
    pub(crate) method: Method,
    pub(crate) values: Vec<Value>,
    pub(crate) body: Option<Body>,
    pub(crate) positional: Vec<Positional>,
    pub(crate) output: Type,
    pub(crate) output_via: Option<Type>,
    /// Effective response representation for the declared output (for
    /// `absent on`, the representation of the present payload).
    pub(crate) response: Type,
    pub(crate) response_origin: String,
    pub(crate) id: String,
    pub(crate) id_explicit: bool,
    pub(crate) id_span: Span,
    pub(crate) status: Status,
    pub(crate) status_explicit: bool,
    pub(crate) effect: EffectDecl,
    pub(crate) effect_explicit: bool,
    pub(crate) absence: Option<Vec<u16>>,
    /// The `absent` keyword (the endpoint name when there is no absence):
    /// where head-absence obligations on the generated code are reported.
    pub(crate) absence_span: Span,
    pub(crate) pagination: Option<Pagination>,
    pub(crate) expose_accepted_bytes: bool,
    pub(crate) path: Option<LocalPath>,
    pub(crate) program: LocalProgram,
    pub(crate) auth: Option<LocalAuth>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) rates: Vec<RateUse>,
    pub(crate) effective_timeout: Option<Duration>,
    pub(crate) effective_auth: Vec<Placement>,
    pub(crate) effective_rates: Vec<RateUse>,
    /// Credentials whose source is this endpoint (`store_as_*` terminals).
    pub(crate) storage: Vec<usize>,
    pub(crate) surface: EndpointSurface,
    pub(crate) span: Span,
}

impl Endpoint {
    pub(crate) fn is_clone(&self) -> bool {
        self.body.is_none()
    }
}

#[derive(Clone)]
pub(crate) struct ClientInfo {
    pub(crate) name: Ident,
    pub(crate) logical: String,
    pub(crate) base: LitStr,
    pub(crate) builder: Ident,
    pub(crate) module: Ident,
    pub(crate) secrets: Vec<Secret>,
    pub(crate) credentials: Vec<Credential>,
    pub(crate) window_sets: Vec<WindowSet>,
    pub(crate) policies: Vec<Policy>,
    pub(crate) requires_limiter: bool,
}

impl ClientInfo {
    pub(crate) fn has_required_construction_inputs(&self, root: &Owner) -> bool {
        root.values.iter().any(Value::is_required_input)
            || !self.secrets.is_empty()
            || self.requires_limiter
            || self
                .credentials
                .iter()
                .any(|credential| matches!(credential.source, CredentialSource::Custom { .. }))
    }
}

#[derive(Clone)]
pub(crate) struct Model {
    pub(crate) client: ClientInfo,
    pub(crate) owners: Vec<Owner>,
    pub(crate) endpoints: Vec<Endpoint>,
}

impl Model {
    pub(crate) fn value(&self, key: VRef) -> &Value {
        match key.site {
            Site::Owner(owner) => &self.owners[owner].values[key.index],
            Site::Endpoint(endpoint) => &self.endpoints[endpoint].values[key.index],
        }
    }

    pub(crate) fn root(&self) -> &Owner {
        &self.owners[0]
    }

    /// Owner chain from the root to `owner`, inclusive.
    pub(crate) fn chain(&self, owner: usize) -> Vec<usize> {
        let mut chain = vec![owner];
        let mut current = owner;
        while let Some(parent) = self.owners[current].parent {
            chain.push(parent);
            current = parent;
        }
        chain.reverse();
        chain
    }

    /// Diagnostic owner label of an owner: ``client `Api` `` or
    /// ``scope `Api::regional` `` (DIAG-01).
    pub(crate) fn owner_label(&self, owner: usize) -> String {
        let entry = &self.owners[owner];
        match entry.kind {
            OwnerKind::Client => crate::syntax::diagnostics::client_label(&entry.display),
            OwnerKind::Scope => crate::syntax::diagnostics::scope_label(&entry.display),
        }
    }

    /// Diagnostic owner label of a site, including
    /// ``endpoint `Api::regional::item` `` (DIAG-01).
    pub(crate) fn site_label(&self, site: Site) -> String {
        match site {
            Site::Owner(owner) => self.owner_label(owner),
            Site::Endpoint(endpoint) => {
                crate::syntax::diagnostics::endpoint_label(&self.endpoints[endpoint].display)
            }
        }
    }

    /// Display of a value reference relative to a site (`name`,
    /// `client.name`, `scope.name`).
    pub(crate) fn display_ref(&self, key: VRef, from: Site) -> String {
        let value = &self.value(key).logical;
        if key.site == from {
            return value.clone();
        }
        match key.site {
            Site::Owner(0) => format!("client.{value}"),
            Site::Owner(owner) => format!("{}.{value}", self.owners[owner].logical),
            Site::Endpoint(_) => value.clone(),
        }
    }
}
