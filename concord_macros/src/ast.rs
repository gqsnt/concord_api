use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitBool, LitInt, LitStr, Path, Type};

#[derive(Clone)]
pub(crate) struct SourceApi {
    pub(crate) client: SourceClient,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) struct SourceClient {
    pub(crate) name: Ident,
    pub(crate) base: LitStr,
    pub(crate) values: Vec<ValueDecl>,
    pub(crate) credentials: Option<CredentialsBlock>,
    pub(crate) retry: RetrySelection,
    pub(crate) layer: Layer,
    pub(crate) rate_limit_observer: Option<Path>,
}

#[derive(Clone)]
pub(crate) enum SourceChild {
    Scope(Box<SourceScope>),
    Endpoint(SourceEndpoint),
}

#[derive(Clone)]
pub(crate) struct SourceScope {
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) layer: Layer,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) struct SourceEndpoint {
    pub(crate) method: MethodKind,
    pub(crate) name: Ident,
    pub(crate) alias: Option<Ident>,
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) body: Option<Type>,
    pub(crate) layer: Layer,
    pub(crate) response: Type,
    pub(crate) pagination: Option<PaginationDecl>,
}

#[derive(Clone)]
pub(crate) struct PaginationDecl {
    pub(crate) controller: Type,
    pub(crate) fields: Vec<PaginationField>,
}

#[derive(Clone)]
pub(crate) struct PaginationField {
    pub(crate) name: Ident,
    pub(crate) initializer: PaginationInitializer,
}

#[derive(Clone)]
pub(crate) enum PaginationInitializer {
    Bind {
        namespace: Ident,
        destination: Ident,
    },
    Value(PaginationValue),
}

#[derive(Clone)]
pub(crate) enum PaginationValue {
    Reference { namespace: Ident, name: Ident },
    Stable(DefaultExpr),
}

#[derive(Clone)]
pub(crate) struct ValueDecl {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) kind: ValueKind,
}

#[derive(Clone)]
pub(crate) enum ValueKind {
    Required,
    Optional,
    Defaulted(DefaultExpr),
}

#[derive(Clone)]
pub(crate) enum DefaultExpr {
    String(LitStr),
    Integer { negative: bool, literal: LitInt },
    Boolean(LitBool),
    Path(Path),
}

#[derive(Clone)]
pub(crate) enum RetrySelection {
    Disabled,
    ReqwestDefault,
    Custom(Path),
}

#[derive(Clone, Default)]
pub(crate) struct Layer {
    pub(crate) host: Option<HostOperation>,
    pub(crate) path: Option<PathOperation>,
    pub(crate) query: Option<QueryOperation>,
    pub(crate) headers: Option<HeaderOperation>,
    pub(crate) auth: Option<AuthOperation>,
    pub(crate) timeout: Option<TimeoutOperation>,
    pub(crate) rate_limit: Option<RateLimitDecl>,
}

#[derive(Clone)]
pub(crate) struct RateLimitDecl {
    pub(crate) cost: u32,
    pub(crate) windows: Vec<RateLimitWindowLiteral>,
}

#[derive(Clone)]
pub(crate) struct RateLimitWindowLiteral {
    pub(crate) capacity: u32,
    pub(crate) duration: DurationLiteral,
}

#[derive(Clone)]
pub(crate) struct CredentialsBlock {
    pub(crate) secrets: Vec<SecretDecl>,
    pub(crate) credentials: Vec<CredentialDecl>,
}

#[derive(Clone)]
pub(crate) struct SecretDecl {
    pub(crate) name: Ident,
}

#[derive(Clone)]
pub(crate) struct CredentialDecl {
    pub(crate) name: Ident,
    pub(crate) source: CredentialSource,
}

#[derive(Clone)]
pub(crate) enum CredentialSource {
    ApiKey {
        secret: Ident,
    },
    Bearer {
        secret: Ident,
    },
    Basic {
        username: Ident,
        password: Ident,
    },
    OAuth2Client {
        token_url: LitStr,
        client_id: Ident,
        client_secret: Ident,
        scope: Option<LitStr>,
    },
    Endpoint {
        path: Path,
    },
    Custom {
        material: Type,
    },
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
}

#[derive(Clone)]
pub(crate) enum AuthPlacementKind {
    Bearer,
    Basic,
    Header(LitStr),
    Query(LitStr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthChallenge {
    Unauthorized,
    UnauthorizedOrForbidden,
    Never,
}

#[derive(Clone)]
pub(crate) enum HostOperation {
    Replace(Vec<PublicExpr>),
    Off,
}

#[derive(Clone)]
pub(crate) enum PathOperation {
    Append(Vec<PublicExpr>),
    Replace(Vec<PublicExpr>),
    Off,
}

#[derive(Clone)]
pub(crate) enum QueryOperation {
    Modify(Vec<QueryEntry>),
    Replace(Vec<QueryEntry>),
    Off,
}

#[derive(Clone)]
pub(crate) enum QueryEntry {
    Assign { key: LitStr, value: PublicExpr },
    Remove(LitStr),
}

#[derive(Clone)]
pub(crate) enum HeaderOperation {
    Modify(Vec<HeaderEntry>),
    Replace(Vec<HeaderEntry>),
    Off,
}

#[derive(Clone)]
pub(crate) enum HeaderEntry {
    Assign { name: LitStr, value: PublicExpr },
    Remove(LitStr),
}

#[derive(Clone)]
pub(crate) enum TimeoutOperation {
    Set(DurationLiteral),
    Off,
}

#[derive(Clone)]
pub(crate) struct DurationLiteral {
    pub(crate) value: Duration,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) enum PublicExpr {
    String(LitStr),
    Integer(LitInt),
    Reference { namespace: Ident, name: Ident },
    Format { span: Span, values: Vec<PublicExpr> },
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
    Trace,
    Connect,
    Custom(LitStr),
}
