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
    pub cache_store_enabled: bool,
    pub cache_store_config: Option<CacheConfigResolved>,
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
    pub body: Option<CodecSpec>,
    pub response: CodecSpec,

    pub policy: ResolvedPolicySpec,
    pub behavior_doc: BehaviorDocMeta,

    pub paginate: Option<PaginateResolved>,
    pub map: Option<MapResolved>,
}

#[derive(Debug)]
pub struct ResolvedPolicySpec {
    // Stable ordered inherited scope policy snapshots plus endpoint-local
    // policy. These are normalized sema results, not raw policy AST nodes.
    pub scopes: Vec<PolicyBlocksResolved>,
    pub endpoint: PolicyBlocksResolved,
    pub auth: Vec<AuthUsePlanIr>,
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
        endpoint: syn::Path,
        endpoint_key: String,
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

#[derive(Debug, Clone, Copy)]
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
    pub cache: Option<CacheResolved>,
    pub retry: Option<RetryResolved>,
    pub rate_limit: Option<RateLimitResolved>,
}

#[derive(Debug, Clone)]
pub enum RetryResolved {
    Set(RetryConfigResolved),
    Patch(RetryPatchResolved),
    Clear,
}

#[derive(Debug, Clone)]
pub enum CacheResolved {
    Set(CacheConfigResolved),
    Patch(CacheConfigPatchResolved),
    Clear,
}

#[derive(Debug, Clone, Default)]
pub struct CacheConfigResolved {
    pub http: bool,
    pub default_ttl_secs: Option<u64>,
    pub revalidate: Option<bool>,
    pub failure_mode: Option<CacheFailureModeResolved>,
    pub capacity_entries: Option<u64>,
    pub max_body_bytes: Option<usize>,
    pub shared: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheConfigPatchResolved {
    pub http: Option<bool>,
    pub default_ttl_secs: Option<u64>,
    pub revalidate: Option<bool>,
    pub failure_mode: Option<CacheFailureModeResolved>,
    pub capacity_entries: Option<u64>,
    pub max_body_bytes: Option<usize>,
    pub shared: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheFailureModeResolved {
    Ignore,
    ServeStaleOnError,
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

#[derive(Debug, Clone)]
pub enum RateLimitKeyResolved {
    RouteHost,
    Endpoint,
    Method,
    Named { name: String, span: Span },
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
    AuthField(Ident),
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

#[derive(Debug)]
pub struct PaginateResolved {
    pub ctrl_ty: syn::Path,
    pub assigns: Vec<(Ident, PaginationValueKind)>,
}

#[derive(Debug)]
pub struct MapResolved {
    pub body: syn::Expr,
    pub out_ty: Type,
}

