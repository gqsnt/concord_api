use std::time::Duration;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{GenericArgument, Ident, LitBool, LitInt, LitStr, Path, PathArguments, Type, TypePath};

#[derive(Clone)]
pub(crate) struct Clause<T> {
    pub(crate) key_span: Span,
    pub(crate) value: T,
}

#[derive(Clone)]
pub(crate) struct SourceApi {
    pub(crate) client: SourceClient,
    pub(crate) children: Vec<SourceChild>,
}

#[derive(Clone)]
pub(crate) struct SourceClient {
    pub(crate) name: Ident,
    pub(crate) base: Clause<LitStr>,
    pub(crate) values: Option<Clause<Vec<ValueDecl>>>,
    pub(crate) credentials: Option<Clause<CredentialsBlock>>,
    pub(crate) rate_limits: Option<Clause<Vec<RateLimitDefinitionDecl>>>,
    pub(crate) layer: Layer,
}

#[derive(Clone)]
// Parsed syntax is short-lived; keeping the endpoint inline avoids allocation
// throughout resolution.
#[allow(clippy::large_enum_variant)]
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
    pub(crate) parameters: Vec<ValueDecl>,
    pub(crate) body: Option<BodyDecl>,
    pub(crate) layer: Layer,
    pub(crate) response: Type,
    pub(crate) pagination: Option<Clause<PaginationDecl>>,
    pub(crate) contract_id: Option<Clause<LitStr>>,
}

#[derive(Clone)]
pub(crate) struct BodyDecl {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) position: usize,
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
    pub(crate) position: usize,
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
    TraitDefault(Span),
}

#[derive(Clone, Default)]
pub(crate) struct Layer {
    pub(crate) host: Option<Clause<HostOperation>>,
    pub(crate) path: Option<Clause<PathOperation>>,
    pub(crate) query: Option<Clause<QueryOperation>>,
    pub(crate) headers: Option<Clause<HeaderOperation>>,
    pub(crate) auth: Option<AuthOperation>,
    pub(crate) timeout: Option<Clause<TimeoutOperation>>,
    pub(crate) rate_limits: Vec<Clause<RateLimitAttachmentDecl>>,
}

#[derive(Clone)]
pub(crate) enum RateLimitAttachmentDecl {
    Inline(RateLimitDecl),
    Named(RateLimitUseDecl),
}

#[derive(Clone)]
pub(crate) struct RateLimitDefinitionDecl {
    pub(crate) name: Ident,
    pub(crate) id: Option<Clause<LitStr>>,
    pub(crate) dimensions: Vec<RateLimitDefinitionDimensionDecl>,
    pub(crate) cost: u32,
    pub(crate) cost_span: Option<Span>,
    pub(crate) windows: Vec<RateLimitWindowLiteral>,
}

#[derive(Clone)]
pub(crate) enum RateLimitDefinitionDimensionDecl {
    Host { span: Span },
    Endpoint { span: Span },
    Formal { name: Ident },
}

impl RateLimitDefinitionDimensionDecl {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Host { span } | Self::Endpoint { span } => *span,
            Self::Formal { name } => name.span(),
        }
    }

    pub(crate) fn public_name(&self) -> String {
        match self {
            Self::Host { .. } => "host".to_owned(),
            Self::Endpoint { .. } => "endpoint".to_owned(),
            Self::Formal { name } => canonical_dimension_ident(name),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RateLimitUseDecl {
    pub(crate) name: Ident,
    pub(crate) bindings: Vec<RateLimitBindingDecl>,
    pub(crate) cost: Option<Clause<u32>>,
}

#[derive(Clone)]
pub(crate) struct RateLimitBindingDecl {
    pub(crate) name: Ident,
    pub(crate) value: RateLimitBindingValue,
}

#[derive(Clone)]
pub(crate) struct RateLimitBindingValue {
    pub(crate) namespace: Ident,
    pub(crate) name: Ident,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct RateLimitDecl {
    pub(crate) id: Option<Clause<LitStr>>,
    pub(crate) dimensions: Vec<RateLimitDimensionDecl>,
    pub(crate) cost: u32,
    pub(crate) cost_span: Option<Span>,
    pub(crate) windows: Vec<RateLimitWindowLiteral>,
}

#[derive(Clone)]
pub(crate) enum RateLimitDimensionDecl {
    Host {
        span: Span,
    },
    Endpoint {
        span: Span,
    },
    Value {
        namespace: Ident,
        name: Ident,
        span: Span,
    },
}

impl RateLimitDimensionDecl {
    pub(crate) const fn span(&self) -> Span {
        match self {
            Self::Host { span } | Self::Endpoint { span } | Self::Value { span, .. } => *span,
        }
    }

    pub(crate) fn public_name(&self) -> String {
        match self {
            Self::Host { .. } => "host".to_owned(),
            Self::Endpoint { .. } => "endpoint".to_owned(),
            Self::Value {
                namespace, name, ..
            } => format!(
                "{}.{}",
                canonical_dimension_ident(namespace),
                canonical_dimension_ident(name)
            ),
        }
    }
}

fn canonical_dimension_ident(ident: &Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_owned()
}

#[derive(Clone)]
pub(crate) struct RateLimitWindowLiteral {
    pub(crate) capacity: u32,
    pub(crate) capacity_span: Span,
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
        clause_spans: Vec<Span>,
    },
    Off {
        key_span: Span,
    },
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

pub(crate) fn syntactic_option_inner(ty: &Type) -> syn::Result<Option<Type>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    let segments = path.segments.iter().collect::<Vec<_>>();
    let option = match segments.as_slice() {
        [option] if path.leading_colon.is_none() && option.ident == "Option" => *option,
        [std, option_module, option]
            if std.ident == "std"
                && option_module.ident == "option"
                && option.ident == "Option"
                && matches!(std.arguments, PathArguments::None)
                && matches!(option_module.arguments, PathArguments::None) =>
        {
            *option
        }
        _ => return Ok(None),
    };
    let PathArguments::AngleBracketed(arguments) = &option.arguments else {
        return Err(option
            .ident
            .span()
            .error("`Option` requires exactly one type argument"));
    };
    let mut values = arguments.args.iter();
    let Some(GenericArgument::Type(inner)) = values.next() else {
        return Err(arguments
            .span()
            .error("`Option` requires exactly one type argument"));
    };
    if values.next().is_some() {
        return Err(arguments
            .span()
            .error("`Option` requires exactly one type argument"));
    }
    Ok(Some(inner.clone()))
}

trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}
