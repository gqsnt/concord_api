#[derive(Debug)]
pub struct ResolvedApi {
    pub mod_name: Ident,
    pub client_name: Ident,
    pub scheme: Scheme,
    pub domain: LitStr,

    pub client_vars: Vec<VarInfo>,      // stable order
    pub client_auth_vars: Vec<VarInfo>, // stable order
    pub client_auth_credentials: Vec<AuthCredentialIr>,
    pub client_policy: PolicyBlocksResolved,
    pub rate_limit_response_policy: Option<syn::Path>,

    pub endpoints: Vec<ResolvedEndpoint>,
}

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub rust: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub struct LayerIr {
    pub scope_name: Option<Ident>,
    pub kind: RouteLayerKind,
    pub prefix_pieces: Vec<PrefixPiece>, // if Prefix
    pub path_pieces: Vec<PathPiece>,     // if Path
    pub policy: PolicyBlocksResolved,
    pub auth: Vec<AuthUsePlanIr>,
    pub rate_limit_key_bindings: Vec<RateLimitKeyBindingResolved>,
    pub behavior_names: Vec<String>,
    pub decls: Vec<VarInfo>, // endpoint vars declared by this layer
}

#[derive(Debug, Clone, Default)]
pub struct BehaviorDocMeta {
    pub names: Vec<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ResolvedEndpoint {
    pub name: Ident,
    pub alias: Option<Ident>,
    // Resolved facade module path and stable ordered facade parameter groups.
    // These are sema outputs, not raw AST ancestry.
    pub scope_modules: Vec<Ident>,
    pub facade_param_groups: Vec<Vec<VarInfo>>,
    pub method: Ident,
    // Fully resolved route fragments split by emission concern. Codegen may
    // emit them directly; it must not walk raw scope ancestry to rediscover
    // these pieces.
    pub prefix_pieces: Vec<PrefixPiece>,
    pub scope_path_pieces: Vec<PathPiece>,
    pub route_pieces: Vec<PathPiece>,

    pub vars: Vec<VarInfo>, // endpoint vars (union, stable)
    pub io: ResolvedHttpEndpointIo,

    pub policy: ResolvedPolicySpec,
    pub behavior_doc: BehaviorDocMeta,

    pub paginate: Option<PaginateResolved>,
    pub map: Option<MapResolved>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedHttpEndpointIo {
    pub request: ResolvedRequestBodyIo,
    pub response: ResolvedResponseBodyIo,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BufferedCodecIo {
    pub marker: Type,
    pub codec_path: Path,
    pub value_ty: Type,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ResolvedRequestBodyIo {
    None,
    BufferedCodec(BufferedCodecIo),
    RawStream { media_ty: Type },
    Records { item_ty: Type, format_ty: Type },
    Multipart { value_ty: Type, format_ty: Type },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ResolvedResponseBodyIo {
    BufferedCodec(BufferedCodecIo),
    BufferedBytes,
    NoContent,
    RawStream { media_ty: Type },
    Records { item_ty: Type, format_ty: Type },
    Multipart { part_ty: Type, format_ty: Type },
    Sse { event_ty: Type, codec_ty: Type },
}

#[allow(dead_code)]
impl ResolvedRequestBodyIo {
    pub fn is_none(&self) -> bool {
        matches!(self, ResolvedRequestBodyIo::None)
    }

    pub fn as_buffered_codec(&self) -> Option<&BufferedCodecIo> {
        match self {
            ResolvedRequestBodyIo::BufferedCodec(io) => Some(io),
            _ => None,
        }
    }
}

#[allow(dead_code)]
impl ResolvedResponseBodyIo {
    pub fn buffered_codec(&self) -> Option<&BufferedCodecIo> {
        match self {
            ResolvedResponseBodyIo::BufferedCodec(io) => Some(io),
            _ => None,
        }
    }
}

#[allow(dead_code)]
impl ResolvedEndpoint {
    pub fn http_io(&self) -> &ResolvedHttpEndpointIo {
        &self.io
    }

    pub fn request_io(&self) -> &ResolvedRequestBodyIo {
        &self.io.request
    }

    pub fn response_io(&self) -> &ResolvedResponseBodyIo {
        &self.io.response
    }
}

#[derive(Debug)]
pub struct ResolvedPolicySpec {
    // Stable ordered inherited scope policy snapshots plus endpoint-local
    // policy. These are normalized sema results, not raw policy AST nodes.
    pub scopes: Vec<PolicyBlocksResolved>,
    pub endpoint: PolicyBlocksResolved,
    pub auth: Vec<AuthRequirementIr>,
}

#[derive(Debug, Clone)]
pub struct AuthCredentialIr {
    pub name: Ident,
    pub kind: AuthCredentialKindIr,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AuthMaterialShapeIr {
    AccessToken,
    SecretValue,
    Basic,
    Certificate,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointTargetIr {
    pub scope_modules: Vec<Ident>,
    pub endpoint: Ident,
}

impl EndpointTargetIr {
    pub(crate) fn key(&self) -> EndpointTargetKey {
        EndpointTargetKey {
            scope_modules: self
                .scope_modules
                .iter()
                .map(ToString::to_string)
                .collect(),
            endpoint: self.endpoint.to_string(),
        }
    }

    pub fn display_string(&self) -> String {
        if self.scope_modules.is_empty() {
            self.endpoint.to_string()
        } else {
            format!(
                "{}::{}",
                self.scope_modules
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("::"),
                self.endpoint
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EndpointTargetKey {
    pub scope_modules: Vec<String>,
    pub endpoint: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AuthCredentialKindIr {
    ApiKey {
        secret: Ident,
    },
    StaticBearer {
        secret: Ident,
    },
    Basic {
        username: Ident,
        password: Ident,
    },
    OAuth2ClientCredentials {
        token_url: LitStr,
        client_id: Ident,
        client_secret: Ident,
        scope: Option<LitStr>,
    },
    Endpoint {
        target: EndpointTargetIr,
        output_ty: Type,
        material_shape: AuthMaterialShapeIr,
    },
}

#[derive(Debug, Clone)]
pub struct AuthUseIr {
    pub kind: AuthUseKindIr,
    pub provenance: AuthUseProvenanceIr,
}

#[derive(Debug, Clone)]
pub struct AuthRequirementIr {
    pub credential: Ident,
    pub placement: AuthPlacementIr,
    pub usage_id: String,
    pub step_id: String,
    pub provenance: AuthProvenanceIr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthPlacementIr {
    Bearer,
    Header { name: LitStr },
    Query { key: LitStr },
    Basic,
    Certificate,
}

#[derive(Debug, Clone)]
pub struct AuthProvenanceIr {
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum AuthUsePlanIr {
    Use(Box<AuthUseIr>),
}

#[derive(Debug, Clone)]
pub enum AuthUseKindIr {
    Bearer {
        credential: Ident,
    },
    Header {
        header: LitStr,
        credential: Ident,
    },
    Query {
        key: LitStr,
        credential: Ident,
    },
    Basic {
        credential: Ident,
    },
    Certificate {
        credential: Ident,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthUseProvenanceIr {
    Client,
    Scope(usize),
    Endpoint,
}

#[derive(Debug, Clone)]
pub enum PrefixPiece {
    Static(String),
    CxVar { field: Ident, optional: bool },
    EpVar { field: Ident },
    Fmt(FmtResolved),
}

#[derive(Debug, Clone)]
pub enum PathPiece {
    Static(String),
    CxVar { field: Ident, optional: bool },
    EpVar { field: Ident },
    Fmt(FmtResolved),
}

#[derive(Debug, Clone, Default)]
pub struct PolicyBlocksResolved {
    pub headers: Vec<PolicyOp>,
    pub query: Vec<PolicyOp>,
    pub timeout: Option<PublicValueKind>,
    pub retry: Option<RetryResolved>,
    pub rate_limit: Option<RateLimitResolved>,
}

#[derive(Debug, Clone)]
pub enum RetryResolved {
    Set(RetryConfigResolved),
    Clear,
}

#[derive(Debug, Clone)]
pub(crate) enum RetryDirectiveResolved {
    Set(RetryConfigResolved),
    Patch(RetryPatchResolved),
    Clear,
}

#[derive(Debug, Clone)]
pub enum RateLimitResolved {
    Add(RateLimitPlanResolved),
    Replace(RateLimitPlanResolved),
    Clear,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimitPlanResolved {
    pub buckets: Vec<RateLimitBucketResolved>,
}

#[derive(Debug, Clone)]
pub struct RateLimitBucketResolved {
    pub kind: String,
    pub name: String,
    pub key: Vec<RateLimitKeyResolved>,
    pub cost: u32,
    pub windows: Vec<RateLimitWindowResolved>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateLimitAttachmentContext {
    ClientBase,
    Layer,
    Endpoint,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RateLimitPlanTemplate {
    pub buckets: Vec<RateLimitBucketTemplate>,
}

#[derive(Debug, Clone)]
pub(crate) struct RateLimitBucketTemplate {
    pub kind: String,
    pub name: String,
    pub key: Vec<RateLimitKeyTemplate>,
    pub cost: u32,
    pub windows: Vec<RateLimitWindowResolved>,
}

#[derive(Debug, Clone)]
pub(crate) enum RateLimitKeyTemplate {
    RouteHost,
    Endpoint,
    Method,
    Named { name: String, span: Span },
    Static { name: String, value: String },
}

#[derive(Debug, Clone)]
pub enum RateLimitKeyResolved {
    RouteHost,
    Endpoint,
    Method,
    EpField { name: String, field: Ident },
    Static { name: String, value: String },
}

#[derive(Debug, Clone)]
pub struct RateLimitWindowResolved {
    pub max: u32,
    pub per_secs: u64,
}

#[derive(Debug, Clone)]
pub struct RateLimitKeyBindingResolved {
    pub name: String,
    pub field: Ident,
}

#[derive(Debug, Clone)]
pub struct RetryConfigResolved {
    pub max_attempts: u32,
    pub methods: Vec<Ident>,
    pub statuses: Vec<u16>,
    pub transport_errors: Vec<Ident>,
    pub backoff: RetryBackoffResolved,
    pub respect_retry_after: bool,
    pub idempotency: RetryIdempotencyResolved,
}

impl Default for RetryConfigResolved {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: Vec::new(),
            backoff: RetryBackoffResolved::None,
            respect_retry_after: false,
            idempotency: RetryIdempotencyResolved::SafeMethodsOnly,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RetryPatchResolved {
    pub max_attempts: Option<u32>,
    pub methods: Option<Vec<Ident>>,
    pub statuses: Option<Vec<u16>>,
    pub transport_errors: Option<Vec<Ident>>,
    pub respect_retry_after: Option<bool>,
    pub idempotency: Option<RetryIdempotencyResolved>,
}

#[derive(Debug, Clone)]
pub enum RetryBackoffResolved {
    None,
}

#[derive(Debug, Clone)]
pub enum RetryIdempotencyResolved {
    SafeMethodsOnly,
    Header(LitStr),
}

#[derive(Debug, Clone)]
pub enum PolicyOp {
    Remove {
        key: KeyResolved,
    },
    Set {
        key: KeyResolved,
        value: PolicySetValue,
        op: SetOp,
    },
}

#[derive(Debug, Clone)]
pub enum PolicySetValue {
    Value(PublicValueKind),
    OptionalCxField(Ident),
    OptionalEpField(Ident),
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    LitStr(LitStr),
    CxField(Ident),
    EpField(Ident),
    OtherExpr(Expr),
    Fmt(FmtResolved),
}

#[derive(Debug, Clone)]
pub enum PublicValueKind {
    LitStr(LitStr),
    CxField(Ident),
    EpField(Ident),
    OtherExpr(Expr),
    Fmt(FmtResolved),
}

#[derive(Debug, Clone)]
pub enum PaginationValueKind {
    LitStr(LitStr),
    EpField(Ident),
    OtherExpr(Expr),
    Fmt(FmtResolved),
}

#[derive(Debug, Clone)]
pub enum KeyResolved {
    Static(LitStr), // literal key as-is (string literal)
    Ident(Ident),   // ident key (headers: kebab, query: ident)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyKeyKind {
    Header,
    Query,
}

#[derive(Debug, Clone)]
pub struct PaginateResolved {
    pub controller: PaginationControllerResolved,
    pub assigns: Vec<PaginationAssignmentResolved>,
    pub bindings: Vec<PaginationBindingIr>,
}

#[derive(Debug, Clone)]
pub struct PaginationBindingIr {
    pub controller_field: Ident,
    pub endpoint_field: Ident,
    pub endpoint_rust_field: Ident,
    pub endpoint_field_ty: Type,
    pub assignment_span: Span,
}

#[derive(Debug, Clone)]
pub enum PaginationControllerResolved {
    OffsetLimit(OffsetLimitPaginationResolved),
    Cursor(CursorPaginationResolved),
    Paged(PagedPaginationResolved),
    Custom { ctrl_ty: Type },
}

impl PaginationControllerResolved {
    pub fn display_name(&self) -> String {
        match self {
            PaginationControllerResolved::OffsetLimit(_) => "OffsetLimitPagination".to_string(),
            PaginationControllerResolved::Cursor(_) => "CursorPagination".to_string(),
            PaginationControllerResolved::Paged(_) => "PagedPagination".to_string(),
            PaginationControllerResolved::Custom { ctrl_ty } => {
                quote::quote!(#ctrl_ty).to_string()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OffsetLimitPaginationResolved {
    pub assigns: Vec<PaginationAssignmentResolved>,
}

#[derive(Debug, Clone)]
pub struct CursorPaginationResolved {
    pub assigns: Vec<PaginationAssignmentResolved>,
    pub send_cursor_on_first: bool,
    pub stop_when_cursor_missing: bool,
}

#[derive(Debug, Clone)]
pub struct PagedPaginationResolved {
    pub assigns: Vec<PaginationAssignmentResolved>,
}

#[derive(Debug, Clone)]
pub struct PaginationAssignmentResolved {
    pub field: Ident,
    pub value: PaginationValueKind,
}

#[derive(Debug)]
pub struct MapResolved {
    pub body: syn::Expr,
    pub out_ty: Type,
}

