use std::time::Duration;

use proc_macro2::Span;
use syn::{Ident, LitBool, LitInt, LitStr, Path, Type};

#[derive(Clone)]
pub(crate) struct Api {
    pub(crate) client: Client,
}

#[derive(Clone)]
pub(crate) struct Client {
    pub(crate) name: Ident,
    pub(crate) surface: ClientSurface,
    pub(crate) base: LitStr,
    pub(crate) values: Vec<Value>,
    pub(crate) secrets: Vec<Secret>,
    pub(crate) credentials: Vec<Credential>,
    pub(crate) requires_limiter: bool,
    #[allow(
        dead_code,
        reason = "retained as the normalized client attachment-site IR root"
    )]
    pub(crate) config: NormalizedConfig,
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
    pub(crate) surface: ScopeSurface,
    pub(crate) parameters: Vec<Value>,
    #[allow(
        dead_code,
        reason = "retained for direct normalized-IR conformance tests"
    )]
    pub(crate) config: NormalizedConfig,
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
    pub(crate) surface: EndpointSurface,
    pub(crate) docs: EndpointDocModel,
    pub(crate) parameters: Vec<Value>,
    pub(crate) body: Option<RequestBody>,
    pub(crate) contract_id: ContractId,
    pub(crate) config: NormalizedConfig,
    pub(crate) response: Type,
    pub(crate) pagination: Option<Pagination>,
    pub(crate) dsl_path: Vec<Ident>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocModel {
    pub(crate) client_name: String,
    pub(crate) scope_chain: Vec<String>,
    pub(crate) direct_method_name: String,
    pub(crate) endpoint_type_path: String,
    pub(crate) request_type_path: String,
    pub(crate) http_method: String,
    pub(crate) base_url: String,
    pub(crate) host: Option<EndpointDocHost>,
    pub(crate) route_atoms: Vec<EndpointDocRouteAtom>,
    pub(crate) scope_inputs: Vec<EndpointDocInput>,
    pub(crate) required_endpoint_inputs: Vec<EndpointDocInput>,
    pub(crate) endpoint_setters: Vec<EndpointDocSetter>,
    pub(crate) safe_query_names: Vec<String>,
    pub(crate) safe_header_names: Vec<String>,
    pub(crate) body: Option<EndpointDocBody>,
    pub(crate) response: EndpointDocResponse,
    pub(crate) authentication: Vec<EndpointDocAuthPlacement>,
    pub(crate) rate_limits: Vec<EndpointDocRateLimit>,
    pub(crate) timeout: EndpointDocTimeout,
    pub(crate) terminals: EndpointDocTerminals,
    pub(crate) contract_id: String,
    pub(crate) explicit_contract_id: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocHost {
    pub(crate) label_atoms: Vec<EndpointDocRouteAtom>,
    pub(crate) base_host: String,
    pub(crate) port: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EndpointDocRouteAtom {
    Literal(String),
    Value {
        namespace: EndpointDocNamespace,
        name: String,
    },
    Format(Vec<EndpointDocRouteAtom>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EndpointDocNamespace {
    Vars,
    Scope,
    Endpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocInput {
    pub(crate) owner: Option<String>,
    pub(crate) name: String,
    pub(crate) type_name: String,
    pub(crate) kind: EndpointDocInputKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EndpointDocInputKind {
    Required,
    Optional,
    Defaulted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocSetter {
    pub(crate) name: String,
    pub(crate) setter: String,
    pub(crate) clear: Option<String>,
    pub(crate) reset: Option<String>,
    pub(crate) type_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocBody {
    pub(crate) argument: String,
    pub(crate) specification: String,
    pub(crate) input_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocResponse {
    pub(crate) specification: String,
    pub(crate) output_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocAuthPlacement {
    pub(crate) placement: String,
    pub(crate) challenge: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocRateLimit {
    pub(crate) id: String,
    pub(crate) explicit_id: bool,
    pub(crate) origin: String,
    pub(crate) dimensions: Vec<String>,
    pub(crate) cost: u32,
    pub(crate) windows: Vec<EndpointDocRateLimitWindow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocRateLimitWindow {
    pub(crate) capacity: u32,
    pub(crate) duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EndpointDocTimeout {
    Inherited(Duration),
    Disabled,
    Overridden(Duration),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EndpointDocTerminals {
    pub(crate) direct_await: bool,
    pub(crate) response: bool,
    pub(crate) pagination: bool,
    pub(crate) credential_storage: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct RequestBody {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
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
    pub(crate) clear: Option<Ident>,
    pub(crate) reset: Option<Ident>,
}

#[derive(Clone)]
pub(crate) struct RequestTerminals {
    pub(crate) response: bool,
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
    pub(crate) ty: Type,
    pub(crate) kind: ResolvedValueKind,
    pub(crate) shape: ValueShape,
    pub(crate) source_position: usize,
}

impl Value {
    pub(crate) fn accepts_absence(&self) -> bool {
        matches!(
            self.shape,
            ValueShape::Option { .. } | ValueShape::OptionVec { .. }
        )
    }

    pub(crate) fn has_default(&self) -> bool {
        matches!(self.kind, ResolvedValueKind::Defaulted(_))
    }

    pub(crate) fn setter_argument_type(&self) -> &Type {
        match &self.shape {
            ValueShape::Option { inner } | ValueShape::OptionVec { inner, .. } => inner,
            ValueShape::Scalar | ValueShape::Vec { .. } => &self.ty,
        }
    }

    pub(crate) fn client_builder_storage_type(&self) -> &Type {
        if self.is_optional() {
            self.setter_argument_type()
        } else {
            &self.ty
        }
    }

    pub(crate) fn is_required(&self) -> bool {
        matches!(self.kind, ResolvedValueKind::Required)
    }

    pub(crate) fn is_optional(&self) -> bool {
        matches!(self.kind, ResolvedValueKind::Optional)
    }

    pub(crate) fn default(&self) -> Option<&ResolvedDefault> {
        match &self.kind {
            ResolvedValueKind::Defaulted(value) => Some(value),
            ResolvedValueKind::Required | ResolvedValueKind::Optional => None,
        }
    }
}

#[derive(Clone)]
pub(crate) enum ResolvedValueKind {
    Required,
    Optional,
    Defaulted(ResolvedDefault),
}

#[derive(Clone)]
pub(crate) enum ResolvedDefault {
    String(LitStr),
    Integer { negative: bool, literal: LitInt },
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
    Trace,
    Connect,
    Custom(LitStr),
}

#[derive(Clone)]
// These values are short-lived, and keeping Syn nodes inline avoids incidental
// allocation throughout code generation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ValueShape {
    Scalar,
    Option { inner: Type },
    Vec { element: Type },
    OptionVec { inner: Type, element: Type },
}

#[derive(Clone, Default)]
pub(crate) struct NormalizedConfig {
    pub(crate) host: Vec<ResolvedExpr>,
    pub(crate) host_span: Option<Span>,
    pub(crate) path: Vec<ResolvedExpr>,
    pub(crate) query: Vec<NormalizedQuery>,
    pub(crate) headers: Vec<NormalizedHeader>,
    pub(crate) auth: Vec<NormalizedAuth>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) rate_limits: Vec<NormalizedRateLimitPolicy>,
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
}

#[derive(Clone)]
pub(crate) struct NormalizedHeader {
    pub(crate) name: LitStr,
    pub(crate) canonical_name: String,
    pub(crate) value: ResolvedExpr,
}

#[derive(Clone)]
// These values are short-lived, and keeping Syn nodes inline avoids incidental
// allocation throughout code generation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ResolvedExpr {
    String(LitStr),
    Integer(LitInt),
    Value(ResolvedValue),
    Format(Vec<ResolvedExpr>),
}

#[derive(Clone)]
pub(crate) struct ResolvedValue {
    pub(crate) reference: ValueReference,
    pub(crate) shape: ValueShape,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ValueReference {
    Client(usize),
    Scope { scope_id: usize, parameter: usize },
    Endpoint(usize),
}
