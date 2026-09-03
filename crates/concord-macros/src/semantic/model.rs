use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitBool, LitChar, LitFloat, LitInt, LitStr, Path, Type};

#[derive(Clone)]
pub(crate) struct Api {
    pub(crate) client: Client,
}

#[derive(Clone)]
pub(crate) struct Client {
    pub(crate) attributes: DeclarationMetadata,
    pub(crate) name: Ident,
    pub(crate) surface: ClientSurface,
    pub(crate) base: LitStr,
    pub(crate) values: Vec<Value>,
    pub(crate) secrets: Vec<Secret>,
    pub(crate) credentials: Vec<Credential>,
    pub(crate) rate_limit_profiles: Vec<RateLimitProfile>,
    pub(crate) requires_limiter: bool,
    #[allow(
        dead_code,
        reason = "retained as the normalized client attachment-site IR root"
    )]
    pub(crate) config: NormalizedConfig,
    pub(crate) local_attachments: Vec<AttachmentIr>,
    pub(crate) children: Vec<Child>,
}

#[derive(Clone)]
pub(crate) struct ClientSurface {
    pub(crate) builder_name: Ident,
    pub(crate) support_name: Ident,
    pub(crate) values_type_name: Ident,
}

impl Client {
    pub(crate) fn has_required_construction_inputs(&self) -> bool {
        self.values.iter().any(Value::is_required)
            || !self.secrets.is_empty()
            || self.requires_limiter
            || self
                .credentials
                .iter()
                .any(|credential| matches!(&credential.source, CredentialSource::Custom { .. }))
    }
}

#[derive(Clone)]
// Resolved syntax is short-lived; keeping the endpoint inline avoids allocation
// throughout code generation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Child {
    Scope(Box<Scope>),
    Endpoint(Endpoint),
}

#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) id: usize,
    pub(crate) name: Ident,
    pub(crate) attributes: DeclarationMetadata,
    pub(crate) surface: ScopeSurface,
    pub(crate) parameters: Vec<Value>,
    #[allow(
        dead_code,
        reason = "retained for direct normalized-IR conformance tests"
    )]
    pub(crate) config: NormalizedConfig,
    pub(crate) local_attachments: Vec<AttachmentIr>,
    pub(crate) children: Vec<Child>,
}

#[derive(Clone)]
pub(crate) struct ScopeSurface {
    pub(crate) method_name: Ident,
    pub(crate) module_name: Ident,
    pub(crate) facade_name: Ident,
}

#[derive(Clone)]
pub(crate) struct Endpoint {
    #[allow(
        dead_code,
        reason = "stable semantic identity retained in normalized IR"
    )]
    pub(crate) id: usize,
    pub(crate) method: ResolvedMethod,
    pub(crate) name: Ident,
    pub(crate) attributes: DeclarationMetadata,
    pub(crate) surface: EndpointSurface,
    pub(crate) parameters: Vec<Value>,
    pub(crate) body: Option<RequestBody>,
    pub(crate) contract_id: ContractId,
    pub(crate) config: NormalizedConfig,
    pub(crate) local_attachments: Vec<AttachmentIr>,
    pub(crate) output: Type,
    pub(crate) response_spec: Type,
    pub(crate) pagination: Option<Pagination>,
    pub(crate) dsl_path: Vec<Ident>,
    pub(crate) repeatability: Repeatability,
    pub(crate) repeatability_explicit: bool,
    pub(crate) accept: Option<ResolvedAccept>,
    pub(crate) expose_accepted_bytes: bool,
}

#[derive(Clone)]
pub(crate) enum ResolvedAccept {
    Value(ResolvedExpr),
    Off,
}

#[derive(Clone, Default)]
pub(crate) struct DeclarationMetadata {
    pub(crate) docs: Vec<String>,
    pub(crate) deprecated: Option<String>,
}

#[derive(Clone)]
pub(crate) struct RequestBody {
    pub(crate) name: Ident,
    pub(crate) input_type: Type,
    pub(crate) spec_type: Type,
    pub(crate) docs: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ContractId {
    pub(crate) value: String,
    pub(crate) explicit: bool,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct EndpointSurface {
    pub(crate) method_name: Ident,
    pub(crate) endpoint_type_name: Ident,
    pub(crate) request_type_name: Ident,
    pub(crate) module_path: Vec<Ident>,
    pub(crate) required_arguments: Vec<RequiredArgument>,
    pub(crate) setters: Vec<EndpointSetter>,
    pub(crate) terminals: RequestTerminals,
    pub(crate) clone_eligibility: CloneEligibility,
    pub(crate) docs_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequiredArgument {
    Parameter(usize),
    Body,
}

#[derive(Clone)]
pub(crate) struct EndpointSetter {
    pub(crate) parameter: usize,
    pub(crate) setter: Ident,
    pub(crate) with: Option<Ident>,
    pub(crate) clear: Option<Ident>,
    pub(crate) reset: Option<Ident>,
}

#[derive(Clone)]
pub(crate) struct RequestTerminals {
    pub(crate) response: bool,
    pub(crate) accepted_bytes: bool,
    pub(crate) pagination: bool,
    pub(crate) credential_storage: Vec<CredentialStorageTerminal>,
}

#[derive(Clone)]
pub(crate) struct CredentialStorageTerminal {
    pub(crate) credential: usize,
    pub(crate) method: Ident,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloneEligibility {
    Always,
    BodyInputBound,
}

#[derive(Clone)]
pub(crate) struct Pagination {
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
    Bound { value: ResolvedValue, span: Span },
    Value(PaginationValue),
}

#[derive(Clone)]
// These values are short-lived, and keeping Syn nodes inline avoids incidental
// allocation throughout code generation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum PaginationValue {
    Reference(ResolvedValue),
    Stable(ResolvedDefault),
}

#[derive(Clone)]
pub(crate) struct Secret {
    pub(crate) name: Ident,
}

#[derive(Clone)]
pub(crate) struct Credential {
    pub(crate) name: Ident,
    pub(crate) source: CredentialSource,
}

#[derive(Clone)]
pub(crate) struct RateLimitProfile {
    pub(crate) name: String,
    pub(crate) id: String,
    pub(crate) explicit_id: bool,
    pub(crate) formals: Vec<String>,
    pub(crate) dimensions: Vec<String>,
    pub(crate) cost: u32,
    pub(crate) windows: Vec<NormalizedRateLimitWindow>,
}

#[derive(Clone)]
pub(crate) enum CredentialSource {
    ApiKey {
        secret: usize,
    },
    Bearer {
        secret: usize,
    },
    Basic {
        username: usize,
        password: usize,
    },
    Endpoint {
        path: Path,
        endpoint_id: Option<usize>,
    },
    Custom {
        provider: Type,
    },
}

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) name: Ident,
    pub(crate) payload_type: Type,
    pub(crate) effective_type: Type,
    pub(crate) presence: ValuePresence,
    pub(crate) initializer: Option<ResolvedDefault>,
    pub(crate) observation: ObservationClass,
    pub(crate) docs: Vec<String>,
    pub(crate) source_position: usize,
}

impl Value {
    pub(crate) fn accepts_absence(&self) -> bool {
        matches!(self.presence, ValuePresence::Optional)
    }

    pub(crate) fn has_default(&self) -> bool {
        self.initializer.is_some()
    }

    pub(crate) fn setter_argument_type(&self) -> &Type {
        &self.effective_type
    }

    pub(crate) fn with_argument_type(&self) -> &Type {
        &self.payload_type
    }

    pub(crate) fn client_builder_storage_type(&self) -> &Type {
        &self.effective_type
    }

    pub(crate) fn is_required(&self) -> bool {
        matches!(self.presence, ValuePresence::Required) && self.initializer.is_none()
    }

    pub(crate) fn is_optional(&self) -> bool {
        matches!(self.presence, ValuePresence::Optional)
    }

    pub(crate) fn default(&self) -> Option<&ResolvedDefault> {
        self.initializer.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValuePresence {
    Required,
    Optional,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObservationClass {
    OrdinaryPublic,
    RedactedPublic,
}

#[derive(Clone)]
pub(crate) enum ResolvedDefault {
    String(LitStr),
    Character(LitChar),
    Integer { negative: bool, literal: LitInt },
    Float { negative: bool, literal: LitFloat },
    Boolean(LitBool),
    Path(Path),
    TraitDefault(Span),
}

#[derive(Clone)]
pub(crate) enum ResolvedMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Custom(LitStr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Repeatability {
    Idempotent,
    Never,
}

#[derive(Clone, Default)]
pub(crate) struct NormalizedConfig {
    pub(crate) host: Vec<ResolvedExpr>,
    pub(crate) host_origins: Vec<String>,
    pub(crate) host_span: Option<Span>,
    pub(crate) path: Vec<ResolvedExpr>,
    pub(crate) path_origins: Vec<String>,
    pub(crate) query: Vec<NormalizedQuery>,
    pub(crate) headers: Vec<NormalizedHeader>,
    pub(crate) auth: Vec<NormalizedAuth>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) timeout_origin: Option<String>,
    pub(crate) rate_limits: Vec<NormalizedRateLimitPolicy>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttachmentFamily {
    Subdomain,
    Path,
    Query,
    Headers,
    Authentication,
    Timeout,
    RateLimit,
    Accept,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttachmentOperation {
    Prepend,
    Append,
    Replace,
    Remove,
    Off,
    Set,
}

#[derive(Clone)]
pub(crate) struct AttachmentIr {
    pub(crate) family: AttachmentFamily,
    pub(crate) operation: AttachmentOperation,
    pub(crate) origin: String,
    pub(crate) target: Option<String>,
    pub(crate) item_count: usize,
}

#[derive(Clone)]
pub(crate) struct NormalizedRateLimitPolicy {
    pub(crate) id: String,
    pub(crate) explicit_id: bool,
    pub(crate) id_span: Span,
    pub(crate) origin: String,
    pub(crate) dimensions: Vec<NormalizedRateLimitDimension>,
    pub(crate) cost: u32,
    pub(crate) windows: Vec<NormalizedRateLimitWindow>,
}

#[derive(Clone)]
pub(crate) enum NormalizedRateLimitDimension {
    Host {
        span: Span,
    },
    Endpoint {
        span: Span,
    },
    Value {
        public_name: String,
        reference: ValueReference,
        span: Span,
    },
}

#[derive(Clone)]
pub(crate) struct NormalizedRateLimitWindow {
    pub(crate) capacity: u32,
    pub(crate) duration: Duration,
}

#[derive(Clone)]
pub(crate) struct NormalizedAuth {
    pub(crate) kind: ResolvedAuthPlacement,
    pub(crate) credential: usize,
    pub(crate) challenge: ResolvedChallengePolicy,
    pub(crate) span: Span,
    pub(crate) origin: String,
}

#[derive(Clone)]
pub(crate) enum ResolvedAuthPlacement {
    Bearer,
    Basic,
    Authorization(Option<LitStr>),
    Header(LitStr),
    Query(LitStr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedChallengePolicy {
    Unauthorized,
    UnauthorizedOrForbidden,
    Never,
}

#[derive(Clone)]
pub(crate) struct NormalizedQuery {
    pub(crate) key: LitStr,
    pub(crate) value: ResolvedExpr,
    pub(crate) cardinality: QueryCardinality,
    pub(crate) operation: AttachmentOperation,
    pub(crate) origin: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryCardinality {
    One,
    Each,
}

#[derive(Clone)]
pub(crate) struct NormalizedHeader {
    pub(crate) name: LitStr,
    pub(crate) canonical_name: String,
    pub(crate) value: ResolvedExpr,
    pub(crate) origin: String,
}

#[derive(Clone)]
// These values are short-lived, and keeping Syn nodes inline avoids incidental
// allocation throughout code generation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ResolvedExpr {
    String(LitStr),
    Integer(LitInt),
    Value(ResolvedValue),
    Concat(Vec<ResolvedExpr>),
}

#[derive(Clone)]
pub(crate) struct ResolvedValue {
    pub(crate) reference: ValueReference,
    pub(crate) presence: ValuePresence,
    pub(crate) observation: ObservationClass,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ValueReference {
    Client(usize),
    Scope { scope_id: usize, parameter: usize },
    Endpoint(usize),
}
