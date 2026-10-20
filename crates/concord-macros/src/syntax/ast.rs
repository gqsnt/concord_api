//! Parsed v3 declaration syntax. Short-lived; resolved by `semantic`.
//!
//! The parser retains every source span it reads so diagnostics can point at
//! the exact clause; not every retained span is consumed by the current
//! diagnostic set.
#![allow(dead_code)]

use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitBool, LitChar, LitFloat, LitInt, LitStr, Path, Type};

#[derive(Clone)]
pub(crate) struct Clause<T> {
    pub(crate) key_span: Span,
    pub(crate) value: T,
}

#[derive(Clone)]
pub(crate) struct SourceApi {
    pub(crate) client: SourceClient,
}

/// Supported declaration attributes (§3.5).
#[derive(Clone, Default)]
pub(crate) struct Attrs {
    pub(crate) docs: Vec<LitStr>,
    pub(crate) deprecated: Option<Deprecation>,
    pub(crate) sensitive: Option<Span>,
    pub(crate) default: Option<Span>,
}

#[derive(Clone)]
pub(crate) struct Deprecation {
    pub(crate) note: Option<LitStr>,
    pub(crate) since: Option<LitStr>,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct SourceClient {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) base: Clause<LitStr>,
    pub(crate) values: Option<Clause<Vec<ValueDecl>>>,
    pub(crate) credentials: Option<Clause<CredentialsBlock>>,
    pub(crate) rate_registry: Option<Clause<RateRegistry>>,
    pub(crate) site: AttachmentSite,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum SourceChild {
    Scope(Box<SourceScope>),
    Endpoint(Box<SourceEndpoint>),
}

#[derive(Clone)]
pub(crate) struct SourceScope {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) site: AttachmentSite,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) struct SourceEndpoint {
    pub(crate) attrs: Attrs,
    pub(crate) method: MethodKind,
    pub(crate) method_span: Span,
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) body: Option<BodyDecl>,
    pub(crate) output: Type,
    pub(crate) output_via: Option<Type>,
    pub(crate) site: AttachmentSite,
    pub(crate) id: Option<Clause<LitStr>>,
    pub(crate) status: Option<Clause<StatusDecl>>,
    pub(crate) effect: Option<Clause<EffectDecl>>,
    pub(crate) absence: Option<Clause<Vec<u16>>>,
    pub(crate) pagination: Option<Clause<PaginationDecl>>,
    pub(crate) expose_accepted_bytes: Option<Span>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlacementFamily {
    Query,
    Header,
}

/// Direct query/header placement on a value declaration (§6.1 step 2).
#[derive(Clone)]
pub(crate) struct Placement {
    pub(crate) family: PlacementFamily,
    pub(crate) key: Option<LitStr>,
    pub(crate) each: bool,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct ValueDecl {
    pub(crate) attrs: Attrs,
    pub(crate) placement: Option<Placement>,
    pub(crate) name: Ident,
    pub(crate) optional: bool,
    pub(crate) ty: Type,
    pub(crate) via: Option<Type>,
    pub(crate) initializer: Option<StableValue>,
    pub(crate) position: usize,
}

#[derive(Clone)]
pub(crate) struct BodyDecl {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) optional: bool,
    pub(crate) ty: Type,
    pub(crate) via: Option<Type>,
    pub(crate) position: usize,
}

#[derive(Clone)]
pub(crate) enum StableValue {
    Str(LitStr),
    Char(LitChar),
    Bool(LitBool),
    Int { negative: bool, literal: LitInt },
    Float { negative: bool, literal: LitFloat },
    Path(Path),
}

impl StableValue {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Str(value) => value.span(),
            Self::Char(value) => value.span(),
            Self::Bool(value) => value.span,
            Self::Int { literal, .. } => literal.span(),
            Self::Float { literal, .. } => literal.span(),
            Self::Path(path) => syn::spanned::Spanned::span(path),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct AttachmentSite {
    pub(crate) request: Option<Clause<RepresentationSelection>>,
    pub(crate) response: Option<Clause<RepresentationSelection>>,
    pub(crate) subdomain: Option<Clause<SubdomainOperation>>,
    pub(crate) path: Option<Clause<PathOperation>>,
    pub(crate) query: Option<Clause<KeyBlock>>,
    pub(crate) headers: Option<Clause<KeyBlock>>,
    pub(crate) auth: Option<Clause<AuthOperation>>,
    pub(crate) timeout: Option<Clause<DurationLiteral>>,
    pub(crate) rate_limits: Vec<Clause<RateAttachment>>,
}

#[derive(Clone)]
pub(crate) enum RepresentationSelection {
    Select(Box<Type>),
    Explicit,
}

#[derive(Clone)]
pub(crate) enum SubdomainOperation {
    Prepend(Vec<Atom>),
    Replace(Vec<Atom>),
    Off,
}

#[derive(Clone)]
pub(crate) enum PathOperation {
    Append(Template),
    Replace(Template),
    Off,
}

/// A decoded relative path template (§5.3).
#[derive(Clone)]
pub(crate) struct Template {
    pub(crate) literal: LitStr,
    pub(crate) segments: Vec<TemplateSegment>,
}

#[derive(Clone)]
pub(crate) struct TemplateSegment {
    pub(crate) parts: Vec<TemplatePart>,
    /// Byte range of the segment inside the decoded template string.
    pub(crate) range: (usize, usize),
}

#[derive(Clone)]
pub(crate) enum TemplatePart {
    Literal(String),
    Hole {
        reference: Reference,
        via: Option<Type>,
        range: (usize, usize),
    },
}

#[derive(Clone)]
pub(crate) enum KeyBlock {
    Modify(Vec<KeyStatement>),
    Replace(Vec<KeyStatement>),
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatementKind {
    Introduce,
    Replace,
    WhenPresent,
    Append,
    Remove,
}

#[derive(Clone)]
pub(crate) struct KeyStatement {
    pub(crate) kind: StatementKind,
    pub(crate) key: LitStr,
    pub(crate) emission: Option<Emission>,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum Emission {
    Atom(Atom),
    Each {
        reference: Reference,
        via: Option<Type>,
        span: Span,
    },
}

#[derive(Clone)]
pub(crate) enum Atom {
    Str(LitStr),
    Char(LitChar),
    Bool(LitBool),
    Int {
        negative: bool,
        literal: LitInt,
    },
    Reference {
        reference: Reference,
        via: Option<Box<Type>>,
    },
    Concat {
        parts: Vec<Atom>,
        span: Span,
    },
}

impl Atom {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Str(value) => value.span(),
            Self::Char(value) => value.span(),
            Self::Bool(value) => value.span,
            Self::Int { literal, .. } => literal.span(),
            Self::Reference { reference, .. } => reference.span,
            Self::Concat { span, .. } => *span,
        }
    }
}

/// A value reference. `owner == None` binds the current owner only (LANG-03).
#[derive(Clone)]
pub(crate) struct Reference {
    pub(crate) owner: Option<Ident>,
    pub(crate) name: Ident,
    pub(crate) span: Span,
}

impl Reference {
    pub(crate) fn display(&self) -> String {
        let name = crate::semantic::surface::canonical_ident(&self.name);
        match &self.owner {
            Some(owner) => format!(
                "{}.{name}",
                crate::semantic::surface::canonical_ident(owner)
            ),
            None => name,
        }
    }
}

#[derive(Clone)]
pub(crate) struct DurationLiteral {
    pub(crate) value: Duration,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StatusDecl {
    Success,
    Codes(Vec<u16>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectDecl {
    Safe,
    Idempotent,
    NonIdempotent,
}

#[derive(Clone)]
pub(crate) enum MethodKind {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Custom(LitStr),
}

// ---- credentials and auth (§11) ----

#[derive(Clone)]
pub(crate) struct CredentialsBlock {
    pub(crate) secrets: Vec<SecretDecl>,
    pub(crate) credentials: Vec<CredentialDecl>,
}

#[derive(Clone)]
pub(crate) struct SecretDecl {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
}

#[derive(Clone)]
pub(crate) struct CredentialDecl {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) source: CredentialSource,
}

#[derive(Clone)]
pub(crate) enum CredentialSource {
    ApiKey { secret: Ident },
    Bearer { secret: Ident },
    Basic { username: Ident, password: Ident },
    Endpoint { path: Path },
    Custom { provider: Type },
}

#[derive(Clone)]
pub(crate) enum AuthOperation {
    Modify {
        replace: bool,
        placements: Vec<AuthPlacement>,
    },
    Off,
}

#[derive(Clone)]
pub(crate) struct AuthPlacement {
    pub(crate) kind: AuthPlacementKind,
    pub(crate) credential: Ident,
    pub(crate) challenge: Option<AuthChallenge>,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum AuthPlacementKind {
    Bearer,
    Basic,
    Authorization(Option<LitStr>),
    Header(LitStr),
    Query(LitStr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthChallenge {
    Unauthorized,
    UnauthorizedOrForbidden,
    Never,
}

// ---- rates (§12) ----

#[derive(Clone, Default)]
pub(crate) struct RateRegistry {
    pub(crate) window_sets: Vec<WindowSetDecl>,
    pub(crate) policies: Vec<PolicyDecl>,
}

#[derive(Clone)]
pub(crate) struct WindowSetDecl {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) windows: Vec<WindowLiteral>,
}

#[derive(Clone)]
pub(crate) struct WindowLiteral {
    pub(crate) capacity: u32,
    pub(crate) capacity_span: Span,
    pub(crate) duration: DurationLiteral,
}

#[derive(Clone)]
pub(crate) enum WindowSource {
    Using(Ident),
    Inline(Vec<WindowLiteral>),
}

#[derive(Clone)]
pub(crate) struct PolicyDecl {
    pub(crate) attrs: Attrs,
    pub(crate) name: Ident,
    pub(crate) formals: Vec<Ident>,
    pub(crate) id: LitStr,
    pub(crate) key: Vec<FormalDimension>,
    pub(crate) cost: Option<(u32, Span)>,
    pub(crate) windows: WindowSource,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum FormalDimension {
    Origin(Span),
    Endpoint(Span),
    Custom {
        label: LitStr,
        formal: Ident,
        via: Option<Type>,
    },
}

#[derive(Clone)]
pub(crate) enum BoundDimension {
    Origin(Span),
    Endpoint(Span),
    Custom {
        label: LitStr,
        reference: Reference,
        via: Option<Type>,
    },
}

#[derive(Clone)]
pub(crate) enum RateAttachment {
    Named {
        name: Ident,
        bindings: Vec<(Ident, Reference)>,
        cost: Option<(u32, Span)>,
    },
    Inline {
        id: LitStr,
        key: Vec<BoundDimension>,
        cost: Option<(u32, Span)>,
        windows: WindowSource,
    },
}

// ---- pagination (§14) ----

#[derive(Clone)]
pub(crate) struct PaginationDecl {
    pub(crate) controller: Type,
    pub(crate) init_type: Path,
    pub(crate) init_fields: Vec<(Ident, InitValue)>,
    pub(crate) cursor: Vec<Ident>,
    pub(crate) tuple: bool,
    pub(crate) cursor_span: Span,
}

#[derive(Clone)]
pub(crate) enum InitValue {
    Stable(StableValue),
    Reference(Reference),
}
