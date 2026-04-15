use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Expr, Ident, LitBool, LitInt, LitStr, Path, Type};

#[derive(Debug)]
pub struct ApiFile {
    pub client: ClientDef,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct ClientDef {
    pub name: Ident,
    pub scheme: SchemeLit,
    pub host: LitStr,
    pub policy: PolicyBlocks,
    pub vars: Option<VarsBlock>,
    pub auth_vars: Option<VarsBlock>,
    pub auth: Option<AuthBlock>,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache_profiles: Option<CacheProfilesBlock>,
    pub cache: Option<CacheSpec>,
    pub retry_profiles: Option<RetryProfilesBlock>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitProfilesBlock>,
}

#[derive(Debug)]
pub struct VarsBlock {
    pub decls: Vec<VarDeclNoWire>,
}

#[derive(Debug)]
pub struct AuthBlock {
    pub credentials: Vec<AuthCredentialDecl>,
}

#[derive(Debug, Clone)]
pub struct AuthCredentialDecl {
    pub name: Ident,
    pub kind: AuthCredentialKind,
}

#[derive(Debug, Clone)]
pub enum AuthCredentialKind {
    ApiKey {
        secret: SecretRef,
    },
    StaticBearer {
        secret: SecretRef,
    },
    Basic {
        username: SecretRef,
        password: SecretRef,
    },
    OAuth2ClientCredentials {
        token_url: LitStr,
        client_id: SecretRef,
        client_secret: SecretRef,
        scope: Option<LitStr>,
    },
    Custom {
        provider_ty: Type,
        provider: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct SecretRef {
    pub ident: Ident,
}

#[derive(Debug, Clone)]
pub enum AuthUseDecl {
    Single(AuthUseKind),
    AllOf(Vec<AuthUseKind>),
    OneOf(Vec<AuthUseKind>),
}

#[derive(Debug, Clone)]
pub enum AuthUseKind {
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
    Custom {
        usage_ty: Type,
        usage: Expr,
        credential: Ident,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum SchemeLit {
    Http,
    Https,
}

#[derive(Debug)]
pub enum Item {
    Layer(LayerDef),
    Endpoint(EndpointDef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Prefix,
    Path,
}

#[derive(Debug, Clone)]
pub struct RouteExpr {
    pub atoms: Vec<RouteAtom>,
}

#[derive(Debug, Clone)]
pub enum RouteAtom {
    Static(LitStr),
    Var(TemplateVarDecl),
    Ref(ScopedRef),
    Fmt(FmtSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefScope {
    Cx,
    Ep,
    Auth,
}
#[derive(Debug, Clone)]
pub struct ScopedRef {
    pub scope: RefScope,
    pub ident: Ident,
}

#[derive(Debug)]
pub struct LayerDef {
    pub kind: LayerKind,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct EndpointDef {
    pub method: Ident, // "GET", "POST", ...
    pub name: Ident,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,

    pub policy: PolicyBlocks,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,

    pub paginate: Option<PaginateSpec>,
    pub body: Option<CodecSpec>,

    pub response: CodecSpec,
    pub map: Option<MapSpec>,
}

#[derive(Debug)]
pub struct CacheProfilesBlock {
    pub profiles: Vec<CacheProfileDef>,
    pub default: Option<Ident>,
}

#[derive(Debug, Clone)]
pub struct CacheProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: CachePatch,
}

#[derive(Debug, Clone)]
pub enum CacheSpec {
    Profile { only: bool, profile: Ident },
    Patch { only: bool, patch: CachePatch },
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct CachePatch {
    pub http: Option<Span>,
    pub ttl: Option<CacheDurationSpec>,
    pub capacity: Option<CacheCapacitySpec>,
    pub max_body: Option<CacheSizeSpec>,
    pub revalidate: Option<Span>,
    pub shared: Option<LitBool>,
}

#[derive(Debug, Clone)]
pub struct CacheDurationSpec {
    pub amount: LitInt,
    pub unit: RateLimitDurationUnit,
}

#[derive(Debug, Clone)]
pub enum CacheCapacitySpec {
    Entries { amount: LitInt },
    Bytes(CacheSizeSpec),
}

#[derive(Debug, Clone)]
pub struct CacheSizeSpec {
    pub amount: LitInt,
    pub unit: CacheSizeUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheSizeUnit {
    Bytes,
    KiB,
    MiB,
    GiB,
}

#[derive(Debug)]
pub struct RetryProfilesBlock {
    pub profiles: Vec<RetryProfileDef>,
    pub default: Option<Ident>,
}

#[derive(Debug)]
pub struct RetryProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: RetryPatch,
}

#[derive(Debug, Clone)]
pub enum RetrySpec {
    Profile(Ident),
    Patch(RetryPatch),
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct RetryPatch {
    pub attempts: Option<LitInt>,
    pub methods: Option<Vec<Ident>>,
    pub statuses: Option<Vec<LitInt>>,
    pub transport_errors: Option<Vec<Ident>>,
    pub backoff: Option<RetryBackoffSpec>,
    pub respect_retry_after: Option<bool>,
    pub idempotency: Option<RetryIdempotencySpec>,
}

#[derive(Debug, Clone)]
pub enum RetryBackoffSpec {
    None,
}

#[derive(Debug, Clone)]
pub enum RetryIdempotencySpec {
    Header(LitStr),
}

#[derive(Debug)]
pub struct RateLimitProfilesBlock {
    pub profiles: Vec<RateLimitProfileDef>,
    pub default: Vec<Ident>,
    pub response_policy: Option<Path>,
}

#[derive(Debug, Clone)]
pub struct RateLimitProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub plan: RateLimitPlanSpec,
}

#[derive(Debug, Clone)]
pub enum RateLimitSpec {
    Profiles { only: bool, profiles: Vec<Ident> },
    Inline { only: bool, plan: RateLimitPlanSpec },
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimitPlanSpec {
    pub buckets: Vec<RateLimitBucketSpec>,
}

#[derive(Debug, Clone)]
pub struct RateLimitBucketSpec {
    pub kind: Ident,
    pub key: Vec<RateLimitKeySpec>,
    pub windows: Vec<RateLimitWindowSpec>,
}

#[derive(Debug, Clone)]
pub enum RateLimitKeySpec {
    RouteHost,
    Endpoint,
    Method,
    Named(Ident),
    Static(LitStr),
}

#[derive(Debug, Clone)]
pub struct RateLimitWindowSpec {
    pub max: LitInt,
    pub every: LitInt,
    pub unit: RateLimitDurationUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum RateLimitDurationUnit {
    Seconds,
    Minutes,
}

#[derive(Debug, Clone)]
pub struct RateLimitKeyBindingSpec {
    pub name: Ident,
    pub value: Ident,
}

#[derive(Debug)]
pub struct PaginateSpec {
    pub ctrl_ty: Path,
    pub assigns: Vec<PaginateAssign>,
}

#[derive(Debug)]
pub struct PaginateAssign {
    pub key: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct MapSpec {
    pub out_ty: Type,
    pub body: Expr, // expression utilisant `r`
}

#[derive(Debug, Default)]
pub struct PolicyBlocks {
    pub headers: Option<PolicyBlock>,
    pub query: Option<PolicyBlock>,
    pub timeout: Option<Expr>,
}

#[derive(Debug)]
pub struct PolicyBlock {
    pub stmts: Vec<PolicyStmt>,
}

#[derive(Debug)]
pub enum PolicyStmt {
    Remove {
        key: KeySpec,
    },
    Set {
        key: KeySpec,
        value: PolicyValue,
        op: SetOp,
    },
    Bind {
        key: KeySpec,
        decl: VarDeclNoWire,
    },
    BindShort {
        ident_key: Ident,
        decl: VarDeclShort,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOp {
    Set,
    Push, // query only
}

#[derive(Debug)]
pub enum KeySpec {
    Ident(Ident),
    Str(LitStr),
}

/// `as x_debug?: bool = true`
#[derive(Debug, Clone)]
pub struct VarDeclNoWire {
    pub rust: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `page_cursor?: String = "x".into()`
#[derive(Debug, Clone)]
pub struct VarDeclShort {
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `{wire as rust?: Ty = default}`
#[derive(Debug, Clone)]
pub struct TemplateVarDecl {
    pub wire: Ident,
    pub rust: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `Json<T>` (encoding type = `Json`, decoded/body type = `T`)
#[derive(Debug, Clone)]
pub struct CodecSpec {
    pub enc: Path,
    pub ty: Type,
}

#[derive(Debug)]
pub enum PolicyValue {
    Expr(Expr),
    Fmt(FmtSpec),
}

impl PolicyValue {
    #[inline]
    pub fn span(&self) -> Span {
        match self {
            PolicyValue::Expr(e) => e.span(),
            PolicyValue::Fmt(f) => f.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FmtSpec {
    pub span: Span,
    pub require_all: bool,     // fmt? => true
    pub pieces: Vec<FmtPiece>, // ["...", {x:u32}, ...]
}

#[derive(Debug, Clone)]
pub enum FmtPiece {
    Lit(LitStr),
    Var(TemplateVarDecl), // réutilise déjà votre parser de `{wire as rust?: Ty = default}`
    Ref(ScopedRef),
}
