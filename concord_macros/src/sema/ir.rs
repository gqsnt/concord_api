use super::*;

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
    /// Resolved cardinality for client fields that actually participate in
    /// query policy. Other client fields deliberately do not appear here.
    pub client_query_cardinalities: std::collections::BTreeMap<String, QueryValueCardinality>,
    pub rate_limit_response_policy: Option<syn::Path>,

    /// Descriptor facts resolved before code generation.
    pub descriptor: ApiDescriptorIr,

    pub endpoints: Vec<ResolvedEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDescriptorIr {
    pub origin: ApiOriginIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiOriginIr {
    FixedSingle(FixedOriginIr),
    Dynamic,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedOriginIr {
    pub scheme: OriginSchemeIr,
    pub authority: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginSchemeIr {
    Http,
    Https,
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
    pub profile_names: Vec<String>,
    pub decls: Vec<VarInfo>, // endpoint vars declared by this layer
}

#[derive(Debug, Clone, Default)]
pub struct ProfileDocMeta {
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
    /// Resolved query cardinality by endpoint-visible field name. This is
    /// semantic metadata consumed by facade documentation construction.
    pub query_cardinalities: std::collections::BTreeMap<String, QueryValueCardinality>,
    pub io: ResolvedHttpEndpointIo,

    pub policy: ResolvedPolicySpec,
    pub profile_doc: ProfileDocMeta,

    /// Static descriptor facts derived from the same semantic inputs as the
    /// runtime plan. Codegen emits these facts; it does not reclassify them.
    pub descriptor: EndpointDescriptorIr,

    pub paginate: Option<PaginateResolved>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointDescriptorIr {
    pub origin: EndpointOriginIr,
    pub request_body: RequestBodyDescriptorIr,
    pub response_format: ResponseFormatDescriptorIr,
    pub pagination_can_change_origin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointOriginIr {
    Fixed(FixedOriginIr),
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBodyDescriptorIr {
    None,
    Buffered { codec: String },
    Streaming { media: String },
    Multipart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseFormatDescriptorIr {
    Buffered { codec: String },
    Bytes,
    NoContent,
    Streaming { media: String },
}

#[derive(Debug, Clone)]
pub struct ResolvedHttpEndpointIo {
    pub request_entity: RequestEntityPlanIr,
    pub response_entity: ResponseEntityPlanIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoDocIr {
    pub summary: String,
    pub facade_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestIoCapabilities {
    pub has_body: bool,
    pub is_streaming: bool,
    pub is_multipart: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseIoCapabilities {
    pub supports_pagination: bool,
    pub is_streaming: bool,
    pub is_no_content: bool,
}

#[derive(Debug, Clone)]
pub struct RequestEntityPlanIr {
    pub adapter_ty: Type,
    pub public_input_ty: Option<Type>,
    pub body_field_ty: Option<Type>,
    pub doc: IoDocIr,
    pub capabilities: RequestIoCapabilities,
}

#[derive(Debug, Clone)]
pub struct ResponseEntityPlanIr {
    pub adapter_ty: Type,
    pub public_output_ty: Type,
    pub doc: IoDocIr,
    pub capabilities: ResponseIoCapabilities,
}

#[derive(Debug, Clone)]
pub struct BufferedCodecIo {
    pub marker: Type,
    pub codec_path: Path,
    pub value_ty: Type,
}

/// Syntax-level request body classification used while deriving entity metadata.
/// Runtime planning and execution must use [`RequestEntityPlanIr`].
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ResolvedRequestBodyIo {
    None,
    BufferedCodec(BufferedCodecIo),
    RawStream { media_ty: Type },
    Multipart { value_ty: Type },
}

/// Syntax-level response body classification used while deriving entity metadata.
/// Runtime planning and execution must use [`ResponseEntityPlanIr`].
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ResolvedResponseBodyIo {
    BufferedCodec(BufferedCodecIo),
    BufferedBytes,
    NoContent,
    RawStream { media_ty: Type },
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
            scope_modules: self.scope_modules.iter().map(ToString::to_string).collect(),
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
    pub challenge: AuthChallengePolicyIr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthChallengePolicyIr {
    Unauthorized,
    UnauthorizedOrForbidden,
    NeverRecover,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthPlacementIr {
    Bearer,
    Header { name: LitStr },
    Query { key: LitStr },
    Basic,
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
        challenge: AuthChallengePolicyIr,
    },
    Header {
        header: LitStr,
        credential: Ident,
        challenge: AuthChallengePolicyIr,
    },
    Query {
        key: LitStr,
        credential: Ident,
        challenge: AuthChallengePolicyIr,
    },
    Basic {
        credential: Ident,
        challenge: AuthChallengePolicyIr,
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
    pub rate_limit: Option<RateLimitResolved>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryValueCardinality {
    Scalar,
    OptionalScalar,
    Vector,
    OptionalVector,
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
pub enum PolicyOp {
    Remove {
        key: KeyResolved,
    },
    Set {
        key: KeyResolved,
        value: PolicySetValue,
        cardinality: QueryValueCardinality,
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
    pub controller_ty: Type,
    pub assigns: Vec<PaginationAssignmentResolved>,
    pub bindings: Vec<PaginationBindingIr>,
}

#[derive(Debug, Clone)]
pub struct PaginationBindingIr {
    pub controller_field: Ident,
    pub endpoint_rust_field: Ident,
}

#[derive(Debug, Clone)]
pub struct PaginationAssignmentResolved {
    pub field: Ident,
    pub value: PaginationValueKind,
}
