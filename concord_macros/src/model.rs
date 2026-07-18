use std::time::Duration;

use syn::{Ident, LitInt, LitStr, Path, Type};

pub(crate) use crate::ast::{
    AuthChallenge, AuthPlacementKind, DefaultExpr, MethodKind, RequestBodyKind, ResponseKind,
    RetrySelection, ValueKind,
};

#[derive(Clone)]
pub(crate) struct Api {
    pub(crate) clients: Vec<Client>,
}

#[derive(Clone)]
pub(crate) struct Client {
    pub(crate) name: Ident,
    pub(crate) builder_name: Ident,
    pub(crate) support_name: Ident,
    pub(crate) endpoint_capability_name: Ident,
    pub(crate) values_type_name: Ident,
    pub(crate) base: LitStr,
    pub(crate) values: Vec<Value>,
    pub(crate) secrets: Vec<Secret>,
    pub(crate) credentials: Vec<Credential>,
    pub(crate) retry: RetrySelection,
    pub(crate) rate_observer: Option<Path>,
    pub(crate) requires_limiter: bool,
    #[allow(dead_code, reason = "retained as the normalized client-layer IR root")]
    pub(crate) config: NormalizedConfig,
    pub(crate) children: Vec<Child>,
}

#[derive(Clone)]
pub(crate) enum Child {
    Scope(Box<Scope>),
    Endpoint(Endpoint),
}

#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) id: usize,
    pub(crate) name: Ident,
    pub(crate) method_name: Ident,
    pub(crate) module_name: Ident,
    pub(crate) facade_name: Ident,
    pub(crate) parameters: Vec<Value>,
    #[allow(
        dead_code,
        reason = "retained for direct normalized-IR conformance tests"
    )]
    pub(crate) config: NormalizedConfig,
    pub(crate) children: Vec<Child>,
}

#[derive(Clone)]
pub(crate) struct Endpoint {
    #[allow(
        dead_code,
        reason = "stable semantic identity retained in normalized IR"
    )]
    pub(crate) id: usize,
    pub(crate) method: MethodKind,
    pub(crate) name: Ident,
    #[allow(dead_code, reason = "retained for deterministic public-name audit")]
    pub(crate) alias: Option<Ident>,
    pub(crate) public_method_name: Ident,
    pub(crate) request_ext_name: Ident,
    pub(crate) prelude_alias: Ident,
    pub(crate) parameters: Vec<Value>,
    pub(crate) body: Option<RequestBodyKind>,
    pub(crate) config: NormalizedConfig,
    pub(crate) response: ResponseKind,
    pub(crate) dsl_path: Vec<Ident>,
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
    OAuth2Client {
        token_url: LitStr,
        client_id: usize,
        client_secret: usize,
        scope: Option<LitStr>,
    },
    Endpoint {
        path: Path,
        endpoint_id: Option<usize>,
    },
    Custom {
        material: Type,
    },
}

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) kind: ValueKind,
    pub(crate) shape: ValueShape,
}

impl Value {
    pub(crate) fn setter_type(&self) -> &Type {
        if self.is_optional() {
            match &self.shape {
                ValueShape::Option { inner } | ValueShape::OptionVec { inner, .. } => inner,
                ValueShape::Scalar | ValueShape::Vec { .. } => &self.ty,
            }
        } else {
            &self.ty
        }
    }

    pub(crate) fn is_required(&self) -> bool {
        matches!(self.kind, ValueKind::Required)
    }

    pub(crate) fn is_optional(&self) -> bool {
        matches!(self.kind, ValueKind::Optional)
    }

    pub(crate) fn default(&self) -> Option<&DefaultExpr> {
        match &self.kind {
            ValueKind::Defaulted(value) => Some(value),
            ValueKind::Required | ValueKind::Optional => None,
        }
    }
}

#[derive(Clone)]
pub(crate) enum ValueShape {
    Scalar,
    Option { inner: Type },
    Vec { element: Type },
    OptionVec { inner: Type, element: Type },
}

#[derive(Clone, Default)]
pub(crate) struct NormalizedConfig {
    pub(crate) host: Vec<ResolvedExpr>,
    pub(crate) path: Vec<ResolvedExpr>,
    pub(crate) query: Vec<NormalizedQuery>,
    pub(crate) headers: Vec<NormalizedHeader>,
    pub(crate) auth: Vec<NormalizedAuth>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) rate_limits: Vec<NormalizedRateLimitRule>,
}

#[derive(Clone)]
pub(crate) struct NormalizedRateLimitRule {
    pub(crate) id: String,
    pub(crate) declared_name: Ident,
    pub(crate) dimensions: Vec<ResolvedRateLimitDimension>,
    pub(crate) cost: u32,
    pub(crate) windows: Vec<NormalizedRateLimitWindow>,
}

#[derive(Clone)]
pub(crate) enum ResolvedRateLimitDimension {
    Host,
    Endpoint,
    Method,
    Value { name: String, value: ResolvedValue },
}

#[derive(Clone)]
pub(crate) struct NormalizedRateLimitWindow {
    pub(crate) capacity: u32,
    pub(crate) duration: Duration,
}

#[derive(Clone)]
pub(crate) struct NormalizedAuth {
    pub(crate) kind: AuthPlacementKind,
    pub(crate) credential: usize,
    pub(crate) challenge: AuthChallenge,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueReference {
    Client(usize),
    Scope { scope_id: usize, parameter: usize },
    Endpoint(usize),
}
