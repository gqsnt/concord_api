// concord_macros/src/sema.rs
use crate::ast::*;
use crate::emit_helpers;
use proc_macro2::Span;
use std::collections::BTreeMap;
use syn::{Expr, Ident, LitStr, Result, Type, spanned::Spanned};

#[derive(Debug)]
pub struct Ir {
    pub mod_name: Ident,
    pub client_name: Ident,
    pub scheme: SchemeLit,
    pub domain: LitStr,

    pub client_vars: Vec<VarInfo>,      // stable order
    pub client_auth_vars: Vec<VarInfo>, // stable order
    pub client_auth_credentials: Vec<AuthCredentialIr>,
    pub client_policy: PolicyBlocksResolved,
    pub cache_store_enabled: bool,
    pub cache_store_config: Option<CacheConfigResolved>,
    pub rate_limit_response_policy: Option<syn::Path>,

    pub layers: Vec<LayerIr>,
    pub endpoints: Vec<EndpointIr>,
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
    pub kind: LayerKind,
    pub prefix_pieces: Vec<PrefixPiece>, // if Prefix
    pub path_pieces: Vec<PathPiece>,     // if Path
    pub policy: PolicyBlocksResolved,
    pub auth_uses: Vec<AuthUsePlanIr>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingResolved>,
    pub decls: Vec<VarInfo>, // endpoint vars declared by this layer
}

#[derive(Debug)]
pub struct EndpointIr {
    pub name: Ident,
    pub scope_modules: Vec<Ident>,
    pub method: Ident,
    pub route_pieces: Vec<PathPiece>,

    pub ancestry: Vec<usize>, // layer ids in nesting order (outer -> inner)

    pub vars: Vec<VarInfo>, // endpoint vars (union, stable)
    pub body: Option<CodecSpec>,
    pub response: CodecSpec,

    pub policy: PolicyBlocksResolved,
    pub auth_uses: Vec<AuthUsePlanIr>,

    pub paginate: Option<PaginateResolved>,
    pub map: Option<MapResolved>,
}

#[derive(Debug, Clone)]
pub struct AuthCredentialIr {
    pub name: Ident,
    pub kind: AuthCredentialKindIr,
}

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
    },
    Custom {
        provider_ty: Type,
        provider: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct AuthUseIr {
    pub kind: AuthUseKindIr,
    pub provenance: AuthUseProvenanceIr,
}

#[derive(Debug, Clone)]
pub enum AuthUsePlanIr {
    Use(AuthUseIr),
    OneOf(Vec<AuthUseIr>),
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
    Custom {
        usage_ty: Type,
        usage: Expr,
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

#[derive(Debug, Default)]
pub struct PolicyBlocksResolved {
    pub headers: Vec<PolicyOp>,
    pub query: Vec<PolicyOp>,
    pub timeout: Option<ValueKind>,
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
    pub capacity: Option<CacheCapacityResolved>,
    pub max_body_bytes: Option<u64>,
    pub revalidate: Option<bool>,
    pub shared: Option<bool>,
    pub failure_mode: Option<CacheFailureModeResolved>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheConfigPatchResolved {
    pub http: Option<bool>,
    pub default_ttl_secs: Option<u64>,
    pub capacity: Option<CacheCapacityResolved>,
    pub max_body_bytes: Option<u64>,
    pub revalidate: Option<bool>,
    pub shared: Option<bool>,
    pub failure_mode: Option<CacheFailureModeResolved>,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheCapacityResolved {
    Entries(u64),
    Bytes(u64),
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
    pub attempts: u32,
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
            attempts: 1,
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
    pub attempts: Option<u32>,
    pub methods: Option<Vec<Ident>>,
    pub statuses: Option<Vec<u16>>,
    pub transport_errors: Option<Vec<Ident>>,
    pub backoff: Option<RetryBackoffResolved>,
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
        value: ValueKind,
        op: SetOp,
        // if value is a pure optional ref, emit conditional set/remove
        conditional_on_optional_ref: Option<OptionalRefKind>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum OptionalRefKind {
    Cx,
    Ep,
    Auth,
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
    pub assigns: Vec<(Ident, ValueKind)>,
}

#[derive(Debug)]
pub struct MapResolved {
    pub body: syn::Expr,
    pub out_ty: Type,
}

pub fn analyze(ast: ApiFile) -> Result<Ir> {
    let client_name = ast.client.name.clone();
    let mod_name_str = emit_helpers::to_snake(&client_name.to_string());
    let mod_name = Ident::new(&mod_name_str, client_name.span());

    // client vars: explicit `vars {}` only.
    let mut client_vars_map: BTreeMap<String, VarInfo> = BTreeMap::new();
    if let Some(vb) = &ast.client.vars {
        for d in &vb.decls {
            upsert_var(
                &mut client_vars_map,
                &d.rust,
                d.optional,
                &d.ty,
                d.default.as_ref(),
            )?;
        }
    }

    // secret vars: only from `secret {}`.
    let mut auth_vars_map: BTreeMap<String, VarInfo> = BTreeMap::new();
    if let Some(vb) = &ast.client.auth_vars {
        for d in &vb.decls {
            upsert_var(
                &mut auth_vars_map,
                &d.rust,
                d.optional,
                &d.ty,
                d.default.as_ref(),
            )?;
        }
    }

    let endpoint_output_map = collect_endpoint_output_types(&ast.items)?;
    let client_auth_credentials = analyze_auth_credentials(
        ast.client.auth.as_ref(),
        &auth_vars_map,
        &endpoint_output_map,
    )?;
    let auth_credential_map: BTreeMap<String, AuthCredentialIr> = client_auth_credentials
        .iter()
        .map(|c| (c.name.to_string(), c.clone()))
        .collect();
    let client_auth_uses = resolve_auth_uses(
        &ast.client.auth_uses,
        &auth_credential_map,
        AuthUseProvenanceIr::Client,
    )?;

    let cache_profiles = resolve_cache_profiles(ast.client.cache_profiles.as_ref())?;
    let retry_profiles = resolve_retry_profiles(ast.client.retry_profiles.as_ref())?;
    let rate_limit_profiles = resolve_rate_limit_profiles(ast.client.rate_limit.as_ref())?;

    // validate client policy + resolve
    let mut client_policy = resolve_policy_blocks(
        &ast.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        &auth_vars_map,
        None,
    )?;
    client_policy.retry = resolve_client_retry(
        ast.client.retry.as_ref(),
        ast.client
            .retry_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &retry_profiles,
    )?;
    client_policy.cache = resolve_client_cache(
        ast.client.cache.as_ref(),
        ast.client
            .cache_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &cache_profiles,
    )?;
    client_policy.rate_limit = resolve_client_rate_limit(
        ast.client.rate_limit.as_ref(),
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;

    let client_vars: Vec<VarInfo> = client_vars_map.values().cloned().collect();
    let client_auth_vars: Vec<VarInfo> = auth_vars_map.values().cloned().collect();

    // walk layers/endpoints
    let mut layers: Vec<LayerIr> = Vec::new();
    let mut endpoints: Vec<EndpointIr> = Vec::new();

    let mut ancestry: Vec<usize> = Vec::new();
    walk_items(
        &ast.items,
        &mut ancestry,
        &client_vars_map,
        &auth_vars_map,
        &auth_credential_map,
        &client_auth_uses,
        &cache_profiles,
        &retry_profiles,
        &rate_limit_profiles,
        &mut layers,
        &mut endpoints,
    )?;

    let cache_store_enabled = policy_uses_cache(&client_policy)
        || layers.iter().any(|layer| policy_uses_cache(&layer.policy))
        || endpoints
            .iter()
            .any(|endpoint| policy_uses_cache(&endpoint.policy));
    let cache_store_config = match &client_policy.cache {
        Some(CacheResolved::Set(config)) => Some(config.clone()),
        Some(CacheResolved::Patch(patch)) => Some(cache_config_from_patch(patch)),
        Some(CacheResolved::Clear) | None => None,
    };

    Ok(Ir {
        mod_name,
        client_name,
        scheme: ast.client.scheme,
        domain: ast.client.host,
        client_vars,
        client_auth_vars,
        client_auth_credentials,
        client_policy,
        cache_store_enabled,
        cache_store_config,
        rate_limit_response_policy: ast
            .client
            .rate_limit
            .as_ref()
            .and_then(|block| block.response_policy.clone()),
        layers,
        endpoints,
    })
}

fn policy_uses_cache(policy: &PolicyBlocksResolved) -> bool {
    policy
        .cache
        .as_ref()
        .is_some_and(|cache| matches!(cache, CacheResolved::Set(_) | CacheResolved::Patch(_)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyOwner {
    Client,
    Endpoint,
    Layer,
}

fn upsert_var(
    out: &mut BTreeMap<String, VarInfo>,
    rust: &Ident,
    optional: bool,
    ty: &Type,
    default: Option<&Expr>,
) -> Result<()> {
    let k = rust.to_string();
    if let Some(prev) = out.get(&k) {
        // strict compatibility
        if prev.optional != optional {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different optionality", k),
            ));
        }
        if prev.ty != *ty {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different type", k),
            ));
        }
        // default compatibility: allow same tokens or missing
        if prev.default.is_some()
            && default.is_some()
            && prev.default.as_ref().unwrap() != default.unwrap()
        {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different default", k),
            ));
        }
        return Ok(());
    }

    out.insert(
        k,
        VarInfo {
            rust: rust.clone(),
            optional,
            ty: ty.clone(),
            default: default.cloned(),
        },
    );
    Ok(())
}

fn analyze_auth_credentials(
    block: Option<&AuthBlock>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_outputs: &BTreeMap<String, Type>,
) -> Result<Vec<AuthCredentialIr>> {
    let Some(block) = block else {
        return Ok(Vec::new());
    };

    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    let mut out = Vec::new();
    for decl in &block.credentials {
        let name_key = decl.name.to_string();
        if seen.contains_key(&name_key) {
            return Err(syn::Error::new(
                decl.name.span(),
                format!("duplicate auth credential `{}`", decl.name),
            ));
        }
        seen.insert(name_key, decl.name.span());

        let kind = match &decl.kind {
            AuthCredentialKind::ApiKey { secret } => {
                validate_required_secret(secret, auth_vars)?;
                AuthCredentialKindIr::ApiKey {
                    secret: secret.ident.clone(),
                }
            }
            AuthCredentialKind::StaticBearer { secret } => {
                validate_required_secret(secret, auth_vars)?;
                AuthCredentialKindIr::StaticBearer {
                    secret: secret.ident.clone(),
                }
            }
            AuthCredentialKind::Basic { username, password } => {
                validate_required_secret(username, auth_vars)?;
                validate_required_secret(password, auth_vars)?;
                AuthCredentialKindIr::Basic {
                    username: username.ident.clone(),
                    password: password.ident.clone(),
                }
            }
            AuthCredentialKind::OAuth2ClientCredentials {
                token_url,
                client_id,
                client_secret,
                scope,
            } => {
                validate_required_secret(client_id, auth_vars)?;
                validate_required_secret(client_secret, auth_vars)?;
                AuthCredentialKindIr::OAuth2ClientCredentials {
                    token_url: token_url.clone(),
                    client_id: client_id.ident.clone(),
                    client_secret: client_secret.ident.clone(),
                    scope: scope.clone(),
                }
            }
            AuthCredentialKind::Endpoint { endpoint } => {
                let endpoint_key = endpoint_ref_key(endpoint)?;
                let output_ty = endpoint_outputs.get(&endpoint_key).ok_or_else(|| {
                    syn::Error::new(
                        endpoint.span(),
                        format!("unknown auth endpoint `{endpoint_key}` in credential source"),
                    )
                })?;
                AuthCredentialKindIr::Endpoint {
                    endpoint: endpoint.clone(),
                    endpoint_key,
                    output_ty: output_ty.clone(),
                }
            }
            AuthCredentialKind::Custom {
                provider_ty,
                provider,
            } => AuthCredentialKindIr::Custom {
                provider_ty: provider_ty.clone(),
                provider: provider.clone(),
            },
        };

        out.push(AuthCredentialIr {
            name: decl.name.clone(),
            kind,
        });
    }

    Ok(out)
}

fn endpoint_ref_key(path: &syn::Path) -> Result<String> {
    if path.segments.is_empty() {
        return Err(syn::Error::new_spanned(
            path,
            "auth endpoint reference must be `Endpoint(Name)` or `Endpoint(scope::Name)`",
        ));
    }
    let mut out = Vec::new();
    for segment in &path.segments {
        if !matches!(segment.arguments, syn::PathArguments::None) {
            return Err(syn::Error::new_spanned(
                segment,
                "auth endpoint reference segments must not contain generic arguments",
            ));
        }
        out.push(segment.ident.to_string());
    }
    Ok(out.join("::"))
}

fn validate_required_secret(
    secret: &SecretRef,
    auth_vars: &BTreeMap<String, VarInfo>,
) -> Result<()> {
    let Some(info) = auth_vars.get(&secret.ident.to_string()) else {
        return Err(syn::Error::new(
            secret.ident.span(),
            format!(
                "unknown secret `secret.{}` in auth credential",
                secret.ident
            ),
        ));
    };
    if info.optional {
        return Err(syn::Error::new(
            secret.ident.span(),
            format!(
                "auth credential secret `secret.{}` must be required; optional secrets are not supported yet",
                secret.ident
            ),
        ));
    }
    Ok(())
}

fn resolve_auth_uses(
    uses: &[AuthUseDecl],
    credentials: &BTreeMap<String, AuthCredentialIr>,
    provenance: AuthUseProvenanceIr,
) -> Result<Vec<AuthUsePlanIr>> {
    let mut out = Vec::new();
    for u in uses {
        match u {
            AuthUseDecl::Single(kind) => {
                out.push(AuthUsePlanIr::Use(resolve_auth_use_kind(
                    kind,
                    credentials,
                    provenance,
                )?));
            }
            AuthUseDecl::AllOf(kinds) => {
                for kind in kinds {
                    out.push(AuthUsePlanIr::Use(resolve_auth_use_kind(
                        kind,
                        credentials,
                        provenance,
                    )?));
                }
            }
            AuthUseDecl::OneOf(kinds) => {
                if kinds.len() < 2 {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        "use_auth one_of[...] requires at least two auth usages",
                    ));
                }
                let mut alts = Vec::new();
                for kind in kinds {
                    alts.push(resolve_auth_use_kind(kind, credentials, provenance)?);
                }
                out.push(AuthUsePlanIr::OneOf(alts));
            }
        }
    }
    Ok(out)
}

fn resolve_auth_use_kind(
    kind: &AuthUseKind,
    credentials: &BTreeMap<String, AuthCredentialIr>,
    provenance: AuthUseProvenanceIr,
) -> Result<AuthUseIr> {
    let credential = auth_use_credential_ident(kind);
    let cred = credentials.get(&credential.to_string()).ok_or_else(|| {
        syn::Error::new(
            credential.span(),
            format!("unknown auth credential `{credential}`"),
        )
    })?;
    validate_auth_usage_compatibility(kind, cred)?;

    let kind = match kind {
        AuthUseKind::Bearer { credential } => AuthUseKindIr::Bearer {
            credential: credential.clone(),
        },
        AuthUseKind::Header { header, credential } => AuthUseKindIr::Header {
            header: header.clone(),
            credential: credential.clone(),
        },
        AuthUseKind::Query { key, credential } => AuthUseKindIr::Query {
            key: key.clone(),
            credential: credential.clone(),
        },
        AuthUseKind::Basic { credential } => AuthUseKindIr::Basic {
            credential: credential.clone(),
        },
        AuthUseKind::Certificate { credential } => AuthUseKindIr::Certificate {
            credential: credential.clone(),
        },
        AuthUseKind::Custom {
            usage_ty,
            usage,
            credential,
        } => AuthUseKindIr::Custom {
            usage_ty: usage_ty.clone(),
            usage: usage.clone(),
            credential: credential.clone(),
        },
    };
    Ok(AuthUseIr { kind, provenance })
}

fn auth_use_credential_ident(u: &AuthUseKind) -> &Ident {
    match u {
        AuthUseKind::Bearer { credential }
        | AuthUseKind::Header { credential, .. }
        | AuthUseKind::Query { credential, .. }
        | AuthUseKind::Basic { credential }
        | AuthUseKind::Certificate { credential }
        | AuthUseKind::Custom { credential, .. } => credential,
    }
}

fn auth_use_credential_ident_ir(u: &AuthUseIr) -> &Ident {
    match &u.kind {
        AuthUseKindIr::Bearer { credential }
        | AuthUseKindIr::Header { credential, .. }
        | AuthUseKindIr::Query { credential, .. }
        | AuthUseKindIr::Basic { credential }
        | AuthUseKindIr::Certificate { credential }
        | AuthUseKindIr::Custom { credential, .. } => credential,
    }
}

fn auth_plan_references_credential(plans: &[AuthUsePlanIr], target: &Ident) -> bool {
    let target = target.to_string();
    plans.iter().any(|plan| match plan {
        AuthUsePlanIr::Use(auth_use) => {
            auth_use_credential_ident_ir(auth_use).to_string() == target
        }
        AuthUsePlanIr::OneOf(uses) => uses
            .iter()
            .any(|auth_use| auth_use_credential_ident_ir(auth_use).to_string() == target),
    })
}

fn validate_auth_usage_compatibility(u: &AuthUseKind, cred: &AuthCredentialIr) -> Result<()> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum MaterialShape {
        AccessToken,
        SecretValue,
        Basic,
        Certificate,
        Unknown,
    }

    fn shape_from_type(ty: &Type) -> MaterialShape {
        let Type::Path(type_path) = ty else {
            return MaterialShape::Unknown;
        };
        let Some(segment) = type_path.path.segments.last() else {
            return MaterialShape::Unknown;
        };
        match segment.ident.to_string().as_str() {
            "AccessToken" => MaterialShape::AccessToken,
            "ApiKey" => MaterialShape::SecretValue,
            "BasicCredential" => MaterialShape::Basic,
            "ClientCertificate" => MaterialShape::Certificate,
            _ => MaterialShape::Unknown,
        }
    }

    let shape = match &cred.kind {
        AuthCredentialKindIr::ApiKey { .. } => MaterialShape::SecretValue,
        AuthCredentialKindIr::StaticBearer { .. }
        | AuthCredentialKindIr::OAuth2ClientCredentials { .. } => MaterialShape::AccessToken,
        AuthCredentialKindIr::Basic { .. } => MaterialShape::Basic,
        AuthCredentialKindIr::Endpoint { output_ty, .. } => shape_from_type(output_ty),
        AuthCredentialKindIr::Custom { .. } => MaterialShape::Unknown,
    };

    let compatible = match u {
        AuthUseKind::Custom { .. } => true,
        AuthUseKind::Bearer { .. } => {
            matches!(shape, MaterialShape::AccessToken | MaterialShape::Unknown)
        }
        AuthUseKind::Header { .. } | AuthUseKind::Query { .. } => {
            matches!(
                shape,
                MaterialShape::SecretValue | MaterialShape::AccessToken | MaterialShape::Unknown
            )
        }
        AuthUseKind::Basic { .. } => matches!(shape, MaterialShape::Basic | MaterialShape::Unknown),
        AuthUseKind::Certificate { .. } => {
            matches!(shape, MaterialShape::Certificate | MaterialShape::Unknown)
        }
    };

    if compatible {
        return Ok(());
    }

    match u {
        AuthUseKind::Bearer { credential } => Err(syn::Error::new(
            credential.span(),
            format!(
                "BearerAuth requires an access-token credential; `{}` is not compatible",
                cred.name
            ),
        )),
        AuthUseKind::Header { credential, .. } | AuthUseKind::Query { credential, .. } => {
            Err(syn::Error::new(
                credential.span(),
                format!(
                    "header/query auth requires a secret credential; `{}` is not compatible",
                    cred.name
                ),
            ))
        }
        AuthUseKind::Basic { credential } => Err(syn::Error::new(
            credential.span(),
            format!(
                "BasicAuth requires a Basic credential; `{}` is not compatible",
                cred.name
            ),
        )),
        AuthUseKind::Certificate { credential } => Err(syn::Error::new(
            credential.span(),
            format!(
                "CertificateAuth requires a client-certificate credential; `{}` is not compatible",
                cred.name
            ),
        )),
        AuthUseKind::Custom { .. } => Ok(()),
    }
}

fn resolve_retry_profiles(
    block: Option<&RetryProfilesBlock>,
) -> Result<BTreeMap<String, RetryConfigResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw: BTreeMap<String, &RetryProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate retry profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut stack = Vec::new();
    for profile in &block.profiles {
        resolve_retry_profile(&profile.name, &raw, &mut resolved, &mut stack)?;
    }
    if let Some(default) = &block.default
        && !resolved.contains_key(&default.to_string())
    {
        return Err(syn::Error::new(
            default.span(),
            format!("unknown default retry profile `{default}`"),
        ));
    }

    Ok(resolved)
}

fn resolve_retry_profile(
    name: &Ident,
    raw: &BTreeMap<String, &RetryProfileDef>,
    resolved: &mut BTreeMap<String, RetryConfigResolved>,
    stack: &mut Vec<String>,
) -> Result<RetryConfigResolved> {
    let key = name.to_string();
    if let Some(config) = resolved.get(&key) {
        return Ok(config.clone());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(syn::Error::new(
            name.span(),
            format!("retry profile inheritance cycle involving `{key}`"),
        ));
    }

    let Some(profile) = raw.get(&key) else {
        return Err(syn::Error::new(
            name.span(),
            format!("unknown retry profile `{key}`"),
        ));
    };

    stack.push(key.clone());
    let mut config = if let Some(base) = &profile.extends {
        resolve_retry_profile(base, raw, resolved, stack)?
    } else {
        RetryConfigResolved::default()
    };
    let patch = resolve_retry_patch(&profile.patch)?;
    apply_retry_patch(&mut config, &patch);
    stack.pop();

    resolved.insert(key, config.clone());
    Ok(config)
}

fn resolve_client_retry(
    spec: Option<&RetrySpec>,
    default_profile: Option<&Ident>,
    profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<Option<RetryResolved>> {
    if let Some(spec) = spec {
        return resolve_retry_spec(Some(spec), profiles);
    }

    let Some(default_profile) = default_profile else {
        return Ok(None);
    };
    let Some(config) = profiles.get(&default_profile.to_string()) else {
        return Err(syn::Error::new(
            default_profile.span(),
            format!("unknown default retry profile `{default_profile}`"),
        ));
    };
    Ok(Some(RetryResolved::Set(config.clone())))
}

fn resolve_retry_spec(
    spec: Option<&RetrySpec>,
    profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<Option<RetryResolved>> {
    match spec {
        None => Ok(None),
        Some(RetrySpec::Off) => Ok(Some(RetryResolved::Clear)),
        Some(RetrySpec::Patch(patch)) => {
            Ok(Some(RetryResolved::Patch(resolve_retry_patch(patch)?)))
        }
        Some(RetrySpec::Profile(name)) => {
            let Some(config) = profiles.get(&name.to_string()) else {
                return Err(syn::Error::new(
                    name.span(),
                    format!("unknown retry profile `{name}`"),
                ));
            };
            Ok(Some(RetryResolved::Set(config.clone())))
        }
    }
}

fn resolve_retry_patch(patch: &RetryPatch) -> Result<RetryPatchResolved> {
    Ok(RetryPatchResolved {
        attempts: patch
            .attempts
            .as_ref()
            .map(resolve_retry_attempts)
            .transpose()?,
        methods: patch
            .methods
            .as_ref()
            .map(|methods| resolve_retry_methods(methods))
            .transpose()?,
        statuses: patch
            .statuses
            .as_ref()
            .map(|statuses| resolve_retry_statuses(statuses))
            .transpose()?,
        transport_errors: patch
            .transport_errors
            .as_ref()
            .map(|kinds| resolve_retry_transport_errors(kinds))
            .transpose()?,
        backoff: patch
            .backoff
            .as_ref()
            .map(resolve_retry_backoff)
            .transpose()?,
        respect_retry_after: patch.respect_retry_after,
        idempotency: patch
            .idempotency
            .as_ref()
            .map(resolve_retry_idempotency)
            .transpose()?,
    })
}

fn apply_retry_patch(config: &mut RetryConfigResolved, patch: &RetryPatchResolved) {
    if let Some(attempts) = patch.attempts {
        config.attempts = attempts;
    }
    if let Some(methods) = &patch.methods {
        config.methods = methods.clone();
    }
    if let Some(statuses) = &patch.statuses {
        config.statuses = statuses.clone();
    }
    if let Some(transport_errors) = &patch.transport_errors {
        config.transport_errors = transport_errors.clone();
    }
    if let Some(backoff) = &patch.backoff {
        config.backoff = backoff.clone();
    }
    if let Some(respect_retry_after) = patch.respect_retry_after {
        config.respect_retry_after = respect_retry_after;
    }
    if let Some(idempotency) = &patch.idempotency {
        config.idempotency = idempotency.clone();
    }
}

fn resolve_retry_attempts(lit: &syn::LitInt) -> Result<u32> {
    let attempts = lit.base10_parse::<u32>()?;
    if attempts == 0 {
        return Err(syn::Error::new(
            lit.span(),
            "retry attempts must be at least 1",
        ));
    }
    Ok(attempts)
}

fn resolve_retry_methods(methods: &[Ident]) -> Result<Vec<Ident>> {
    if methods.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry methods list must not be empty",
        ));
    }

    methods
        .iter()
        .map(|method| {
            let name = match method.to_string().as_str() {
                "GET" | "get" => "GET",
                "HEAD" | "head" => "HEAD",
                "POST" | "post" => "POST",
                "PUT" | "put" => "PUT",
                "PATCH" | "patch" => "PATCH",
                "DELETE" | "delete" => "DELETE",
                "OPTIONS" | "options" => "OPTIONS",
                _ => {
                    return Err(syn::Error::new(
                        method.span(),
                        "unknown retry method; expected GET, HEAD, POST, PUT, PATCH, DELETE, or OPTIONS",
                    ));
                }
            };
            Ok(Ident::new(name, method.span()))
        })
        .collect()
}

fn resolve_retry_statuses(statuses: &[syn::LitInt]) -> Result<Vec<u16>> {
    if statuses.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry status list must not be empty",
        ));
    }

    statuses
        .iter()
        .map(|status| {
            let value = status.base10_parse::<u16>()?;
            if !(100..=599).contains(&value) {
                return Err(syn::Error::new(
                    status.span(),
                    "retry status must be an HTTP status code in 100..=599",
                ));
            }
            Ok(value)
        })
        .collect()
}

fn resolve_retry_transport_errors(kinds: &[Ident]) -> Result<Vec<Ident>> {
    if kinds.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry transport list must not be empty",
        ));
    }

    kinds
        .iter()
        .map(|kind| {
            let variant = match kind.to_string().as_str() {
                "Timeout" | "timeout" => "Timeout",
                "Connect" | "connect" => "Connect",
                "Tls" | "TLS" | "tls" => "Tls",
                "Dns" | "DNS" | "dns" => "Dns",
                "Io" | "IO" | "io" => "Io",
                "Request" | "request" => "Request",
                "Other" | "other" => "Other",
                _ => {
                    return Err(syn::Error::new(
                        kind.span(),
                        "unknown transport retry kind; expected Timeout, Connect, Tls, Dns, Io, Request, or Other",
                    ));
                }
            };
            Ok(Ident::new(variant, kind.span()))
        })
        .collect()
}

fn resolve_retry_backoff(spec: &RetryBackoffSpec) -> Result<RetryBackoffResolved> {
    match spec {
        RetryBackoffSpec::None => Ok(RetryBackoffResolved::None),
    }
}

fn resolve_retry_idempotency(spec: &RetryIdempotencySpec) -> Result<RetryIdempotencyResolved> {
    match spec {
        RetryIdempotencySpec::Header(header) => {
            if header.value().trim().is_empty() {
                return Err(syn::Error::new(
                    header.span(),
                    "retry idempotency header must not be empty",
                ));
            }
            Ok(RetryIdempotencyResolved::Header(header.clone()))
        }
    }
}

fn resolve_cache_profiles(
    block: Option<&CacheProfilesBlock>,
) -> Result<BTreeMap<String, CacheConfigResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw: BTreeMap<String, &CacheProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate cache profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut stack = Vec::new();
    for profile in &block.profiles {
        resolve_cache_profile(&profile.name, &raw, &mut resolved, &mut stack)?;
    }
    if let Some(default) = &block.default
        && !resolved.contains_key(&default.to_string())
    {
        return Err(syn::Error::new(
            default.span(),
            format!("unknown default cache profile `{default}`"),
        ));
    }

    Ok(resolved)
}

fn resolve_cache_profile(
    name: &Ident,
    raw: &BTreeMap<String, &CacheProfileDef>,
    resolved: &mut BTreeMap<String, CacheConfigResolved>,
    stack: &mut Vec<String>,
) -> Result<CacheConfigResolved> {
    let key = name.to_string();
    if let Some(config) = resolved.get(&key) {
        return Ok(config.clone());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(syn::Error::new(
            name.span(),
            format!("cache profile inheritance cycle involving `{key}`"),
        ));
    }
    let Some(profile) = raw.get(&key) else {
        return Err(syn::Error::new(
            name.span(),
            format!("unknown cache profile `{key}`"),
        ));
    };

    stack.push(key.clone());
    let mut config = if let Some(base) = &profile.extends {
        resolve_cache_profile(base, raw, resolved, stack)?
    } else {
        CacheConfigResolved::default()
    };
    apply_cache_patch(&mut config, &profile.patch)?;
    stack.pop();

    resolved.insert(key, config.clone());
    Ok(config)
}

fn resolve_client_cache(
    spec: Option<&CacheSpec>,
    default: Option<&Ident>,
    profiles: &BTreeMap<String, CacheConfigResolved>,
) -> Result<Option<CacheResolved>> {
    if let Some(spec) = spec {
        return resolve_cache_spec(Some(spec), profiles).map(|resolved| {
            resolved.map(|cache| match cache {
                CacheResolved::Patch(patch) => CacheResolved::Set(cache_config_from_patch(&patch)),
                other => other,
            })
        });
    }
    let Some(default) = default else {
        return Ok(None);
    };
    let Some(config) = profiles.get(&default.to_string()) else {
        return Err(syn::Error::new(
            default.span(),
            format!("unknown default cache profile `{default}`"),
        ));
    };
    Ok(Some(CacheResolved::Set(config.clone())))
}

fn resolve_cache_spec(
    spec: Option<&CacheSpec>,
    profiles: &BTreeMap<String, CacheConfigResolved>,
) -> Result<Option<CacheResolved>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    match spec {
        CacheSpec::Off => Ok(Some(CacheResolved::Clear)),
        CacheSpec::Profile { only, profile } => {
            let _ = only;
            let Some(config) = profiles.get(&profile.to_string()) else {
                return Err(syn::Error::new(
                    profile.span(),
                    format!("unknown cache profile `{profile}`"),
                ));
            };
            Ok(Some(CacheResolved::Set(config.clone())))
        }
        CacheSpec::Patch { only, patch } => {
            let patch = resolve_cache_patch(patch)?;
            if *only {
                Ok(Some(CacheResolved::Set(cache_config_from_patch(&patch))))
            } else {
                Ok(Some(CacheResolved::Patch(patch)))
            }
        }
    }
}

fn apply_cache_patch(config: &mut CacheConfigResolved, patch: &CachePatch) -> Result<()> {
    let patch = resolve_cache_patch(patch)?;
    apply_cache_patch_resolved(config, &patch);
    Ok(())
}

fn resolve_cache_patch(patch: &CachePatch) -> Result<CacheConfigPatchResolved> {
    let mut out = CacheConfigPatchResolved::default();
    if patch.http.is_some() {
        out.http = Some(true);
    }
    if let Some(ttl) = &patch.ttl {
        out.default_ttl_secs = Some(resolve_cache_duration_secs(ttl)?);
    }
    if let Some(capacity) = &patch.capacity {
        out.capacity = Some(resolve_cache_capacity(capacity)?);
    }
    if let Some(max_body) = &patch.max_body {
        out.max_body_bytes = Some(resolve_cache_size_bytes(max_body)?);
    }
    if let Some(revalidate) = &patch.revalidate {
        out.revalidate = Some(revalidate.value);
    }
    if let Some(shared) = &patch.shared {
        out.shared = Some(shared.value);
    }
    if let Some(on_error) = patch.on_error {
        out.failure_mode = Some(match on_error {
            CacheOnErrorSpec::Ignore => CacheFailureModeResolved::Ignore,
            CacheOnErrorSpec::ServeStale => CacheFailureModeResolved::ServeStaleOnError,
        });
    }
    Ok(out)
}

fn apply_cache_patch_resolved(config: &mut CacheConfigResolved, patch: &CacheConfigPatchResolved) {
    if let Some(http) = patch.http {
        config.http = http;
    }
    if let Some(ttl) = patch.default_ttl_secs {
        config.default_ttl_secs = Some(ttl);
    }
    if let Some(capacity) = patch.capacity {
        config.capacity = Some(capacity);
    }
    if let Some(max_body_bytes) = patch.max_body_bytes {
        config.max_body_bytes = Some(max_body_bytes);
    }
    if let Some(revalidate) = patch.revalidate {
        config.revalidate = Some(revalidate);
    }
    if let Some(shared) = patch.shared {
        config.shared = Some(shared);
    }
    if let Some(failure_mode) = patch.failure_mode {
        config.failure_mode = Some(failure_mode);
    }
}

fn cache_config_from_patch(patch: &CacheConfigPatchResolved) -> CacheConfigResolved {
    let mut config = CacheConfigResolved::default();
    apply_cache_patch_resolved(&mut config, patch);
    config
}

fn resolve_cache_duration_secs(ttl: &CacheDurationSpec) -> Result<u64> {
    let amount = ttl.amount.base10_parse::<u64>()?;
    if amount == 0 {
        return Err(syn::Error::new(
            ttl.amount.span(),
            "cache ttl must be greater than zero",
        ));
    }
    let multiplier = match ttl.unit {
        RateLimitDurationUnit::Seconds => 1,
        RateLimitDurationUnit::Minutes => 60,
    };
    Ok(amount.saturating_mul(multiplier))
}

fn resolve_cache_capacity(capacity: &CacheCapacitySpec) -> Result<CacheCapacityResolved> {
    match capacity {
        CacheCapacitySpec::Entries { amount } => {
            let entries = amount.base10_parse::<u64>()?;
            if entries == 0 {
                return Err(syn::Error::new(
                    amount.span(),
                    "cache capacity entries must be greater than zero",
                ));
            }
            Ok(CacheCapacityResolved::Entries(entries))
        }
        CacheCapacitySpec::Bytes(size) => Ok(CacheCapacityResolved::Bytes(
            resolve_cache_size_bytes(size)?,
        )),
    }
}

fn resolve_cache_size_bytes(size: &CacheSizeSpec) -> Result<u64> {
    let amount = size.amount.base10_parse::<u64>()?;
    if amount == 0 {
        return Err(syn::Error::new(
            size.amount.span(),
            "cache size must be greater than zero",
        ));
    }
    let multiplier = match size.unit {
        CacheSizeUnit::Bytes => 1,
        CacheSizeUnit::KiB => 1024,
        CacheSizeUnit::MiB => 1024 * 1024,
        CacheSizeUnit::GiB => 1024 * 1024 * 1024,
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(|| syn::Error::new(size.amount.span(), "cache size is too large to represent"))
}

fn resolve_rate_limit_profiles(
    block: Option<&RateLimitProfilesBlock>,
) -> Result<BTreeMap<String, RateLimitPlanResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw: BTreeMap<String, &RateLimitProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate rate_limit profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut stack = Vec::new();
    for profile in &block.profiles {
        resolve_rate_limit_profile(&profile.name, &raw, &mut resolved, &mut stack)?;
    }
    for default in &block.default {
        if !resolved.contains_key(&default.to_string()) {
            return Err(syn::Error::new(
                default.span(),
                format!("unknown default rate_limit profile `{default}`"),
            ));
        }
    }

    Ok(resolved)
}

fn resolve_rate_limit_profile(
    name: &Ident,
    raw: &BTreeMap<String, &RateLimitProfileDef>,
    resolved: &mut BTreeMap<String, RateLimitPlanResolved>,
    stack: &mut Vec<String>,
) -> Result<RateLimitPlanResolved> {
    let key = name.to_string();
    if let Some(plan) = resolved.get(&key) {
        return Ok(plan.clone());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(syn::Error::new(
            name.span(),
            format!("rate_limit profile inheritance cycle involving `{key}`"),
        ));
    }

    let Some(profile) = raw.get(&key) else {
        return Err(syn::Error::new(
            name.span(),
            format!("unknown rate_limit profile `{key}`"),
        ));
    };

    stack.push(key.clone());
    let mut plan = if let Some(base) = &profile.extends {
        resolve_rate_limit_profile(base, raw, resolved, stack)?
    } else {
        RateLimitPlanResolved::default()
    };
    let mut own = resolve_rate_limit_plan_spec(&profile.plan, &key)?;
    plan.buckets.append(&mut own.buckets);
    stack.pop();

    resolved.insert(key, plan.clone());
    Ok(plan)
}

fn resolve_client_rate_limit(
    block: Option<&RateLimitProfilesBlock>,
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Option<RateLimitResolved>> {
    let Some(block) = block else {
        return Ok(None);
    };
    if block.default.is_empty() {
        return Ok(None);
    }
    let plan = combine_rate_limit_profiles(&block.default, profiles)?;
    Ok(Some(RateLimitResolved::Add(materialize_rate_limit_plan(
        plan,
        visible_keys,
        endpoint_vars,
    )?)))
}

fn resolve_rate_limit_spec(
    spec: Option<&RateLimitSpec>,
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Option<RateLimitResolved>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    match spec {
        RateLimitSpec::Off => Ok(Some(RateLimitResolved::Clear)),
        RateLimitSpec::Profiles {
            only,
            profiles: names,
        } => {
            let plan = combine_rate_limit_profiles(names, profiles)?;
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars)?;
            if *only {
                Ok(Some(RateLimitResolved::Replace(plan)))
            } else {
                Ok(Some(RateLimitResolved::Add(plan)))
            }
        }
        RateLimitSpec::Inline { only, plan } => {
            let plan = resolve_rate_limit_plan_spec(plan, "inline")?;
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars)?;
            if *only {
                Ok(Some(RateLimitResolved::Replace(plan)))
            } else {
                Ok(Some(RateLimitResolved::Add(plan)))
            }
        }
    }
}

fn combine_rate_limit_profiles(
    names: &[Ident],
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
) -> Result<RateLimitPlanResolved> {
    let mut out = RateLimitPlanResolved::default();
    for name in names {
        let Some(plan) = profiles.get(&name.to_string()) else {
            return Err(syn::Error::new(
                name.span(),
                format!("unknown rate_limit profile `{name}`"),
            ));
        };
        out.buckets.extend(plan.buckets.clone());
    }
    Ok(out)
}

fn resolve_rate_limit_plan_spec(
    plan: &RateLimitPlanSpec,
    default_bucket_name: &str,
) -> Result<RateLimitPlanResolved> {
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    let mut out = RateLimitPlanResolved::default();
    for (idx, bucket) in plan.buckets.iter().enumerate() {
        if bucket.windows.is_empty() {
            return Err(syn::Error::new(
                bucket.kind.span(),
                "rate_limit bucket must contain at least one `limit`",
            ));
        }
        let cost = if let Some(cost_lit) = &bucket.cost {
            let cost = cost_lit.base10_parse::<u32>()?;
            if cost == 0 {
                return Err(syn::Error::new(
                    cost_lit.span(),
                    "rate_limit bucket cost must be greater than zero",
                ));
            }
            cost
        } else {
            1
        };
        let mut windows = Vec::new();
        for window in &bucket.windows {
            let max = window.max.base10_parse::<u32>()?;
            if max == 0 {
                return Err(syn::Error::new(
                    window.max.span(),
                    "rate_limit max must be greater than zero",
                ));
            }
            let amount = window.every.base10_parse::<u64>()?;
            if amount == 0 {
                return Err(syn::Error::new(
                    window.every.span(),
                    "rate_limit duration must be greater than zero",
                ));
            }
            let multiplier = match window.unit {
                RateLimitDurationUnit::Seconds => 1,
                RateLimitDurationUnit::Minutes => 60,
            };
            let per_secs = amount.checked_mul(multiplier).ok_or_else(|| {
                syn::Error::new(window.every.span(), "rate_limit duration is too large")
            })?;
            let per_nanos = (per_secs as u128)
                .checked_mul(NANOS_PER_SECOND)
                .ok_or_else(|| {
                    syn::Error::new(window.every.span(), "rate_limit duration is too large")
                })?;
            if max as u128 > per_nanos {
                return Err(syn::Error::new(
                    window.max.span(),
                    "rate_limit window is too small for max; reduce `limit` or increase `every`",
                ));
            }
            windows.push(RateLimitWindowResolved { max, per_secs });
        }
        out.buckets.push(RateLimitBucketResolved {
            kind: bucket.kind.to_string(),
            name: format!("{default_bucket_name}_{idx}"),
            key: bucket.key.iter().map(resolve_rate_limit_key_spec).collect(),
            cost,
            windows,
        });
    }
    Ok(out)
}

fn resolve_rate_limit_key_spec(spec: &RateLimitKeySpec) -> RateLimitKeyResolved {
    match spec {
        RateLimitKeySpec::RouteHost => RateLimitKeyResolved::RouteHost,
        RateLimitKeySpec::Endpoint => RateLimitKeyResolved::Endpoint,
        RateLimitKeySpec::Method => RateLimitKeyResolved::Method,
        RateLimitKeySpec::Named(name) => RateLimitKeyResolved::Named {
            name: name.to_string(),
            span: name.span(),
        },
        RateLimitKeySpec::Static(value) => RateLimitKeyResolved::Static {
            name: "static".to_string(),
            value: value.value(),
        },
    }
}

fn materialize_rate_limit_plan(
    mut plan: RateLimitPlanResolved,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<RateLimitPlanResolved> {
    for bucket in &mut plan.buckets {
        for key in &mut bucket.key {
            let RateLimitKeyResolved::Named { name, span } = key else {
                continue;
            };
            if let Some(binding) = visible_keys.get(name) {
                *key = RateLimitKeyResolved::EpField {
                    name: name.clone(),
                    field: binding.field.clone(),
                };
                continue;
            }
            let Some(vars) = endpoint_vars else {
                return Err(syn::Error::new(
                    *span,
                    format!("rate_limit key `{name}` requires endpoint/scope params"),
                ));
            };
            let Some(var) = vars.get(name) else {
                return Err(syn::Error::new(
                    *span,
                    format!("unknown rate_limit key `{name}`"),
                ));
            };
            if var.optional {
                return Err(syn::Error::new(
                    var.rust.span(),
                    format!("rate_limit key `{name}` cannot use optional param"),
                ));
            }
            *key = RateLimitKeyResolved::EpField {
                name: name.clone(),
                field: var.rust.clone(),
            };
        }
    }
    Ok(plan)
}

fn resolve_rate_limit_key_bindings(
    bindings: &[RateLimitKeyBindingSpec],
    decls: &[VarInfo],
) -> Result<Vec<RateLimitKeyBindingResolved>> {
    let decl_map: BTreeMap<String, &VarInfo> = decls
        .iter()
        .map(|decl| (decl.rust.to_string(), decl))
        .collect();
    let mut seen = BTreeMap::new();
    let mut out = Vec::new();
    for binding in bindings {
        let name = binding.name.to_string();
        if seen.insert(name.clone(), binding.name.span()).is_some() {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("duplicate rate_limit key `{name}`"),
            ));
        }
        let Some(target) = decl_map.get(&binding.value.to_string()) else {
            return Err(syn::Error::new(
                binding.value.span(),
                format!(
                    "unknown scope param `{}` in rate_limit key binding",
                    binding.value
                ),
            ));
        };
        if target.optional {
            return Err(syn::Error::new(
                binding.value.span(),
                "rate_limit key binding cannot target an optional param",
            ));
        }
        out.push(RateLimitKeyBindingResolved {
            name,
            field: binding.value.clone(),
        });
    }
    Ok(out)
}

fn rate_limit_key_bindings_for_ancestry(
    ancestry: &[usize],
    layers: &[LayerIr],
) -> BTreeMap<String, RateLimitKeyBindingResolved> {
    let mut out = BTreeMap::new();
    for &lid in ancestry {
        for binding in &layers[lid].rate_limit_keys {
            out.insert(binding.name.clone(), binding.clone());
        }
    }
    out
}

fn walk_items(
    items: &[Item],
    ancestry: &mut Vec<usize>,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    auth_credentials: &BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &[AuthUsePlanIr],
    cache_profiles: &BTreeMap<String, CacheConfigResolved>,
    retry_profiles: &BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanResolved>,
    layers: &mut Vec<LayerIr>,
    endpoints: &mut Vec<EndpointIr>,
) -> Result<()> {
    for it in items {
        match it {
            Item::Layer(ld) => {
                let id = layers.len();
                let (prefix_pieces, path_pieces, decls) = analyze_layer_route_and_decls(ld)?;
                let key_bindings = resolve_rate_limit_key_bindings(&ld.rate_limit_keys, &decls)?;
                let mut policy = resolve_policy_blocks(
                    &ld.policy,
                    PolicyOwner::Layer,
                    client_vars,
                    auth_vars,
                    None, // endpoint vars not known at layer-level alone (validated per endpoint)
                )?;
                policy.retry = resolve_retry_spec(ld.retry.as_ref(), retry_profiles)?;
                policy.cache = resolve_cache_spec(ld.cache.as_ref(), cache_profiles)?;
                let mut visible_keys = rate_limit_key_bindings_for_ancestry(ancestry, layers);
                for binding in &key_bindings {
                    visible_keys.insert(binding.name.clone(), binding.clone());
                }
                policy.rate_limit = resolve_rate_limit_spec(
                    ld.rate_limit.as_ref(),
                    rate_limit_profiles,
                    &visible_keys,
                    None,
                )?;
                let auth_uses = resolve_auth_uses(
                    &ld.auth_uses,
                    auth_credentials,
                    AuthUseProvenanceIr::Scope(id),
                )?;

                layers.push(LayerIr {
                    scope_name: ld.scope_name.clone(),
                    kind: ld.kind,
                    prefix_pieces,
                    path_pieces,
                    policy,
                    auth_uses,
                    rate_limit_keys: key_bindings,
                    decls,
                });

                ancestry.push(id);
                walk_items(
                    &ld.items,
                    ancestry,
                    client_vars,
                    auth_vars,
                    auth_credentials,
                    client_auth_uses,
                    cache_profiles,
                    retry_profiles,
                    rate_limit_profiles,
                    layers,
                    endpoints,
                )?;
                ancestry.pop();
            }
            Item::Endpoint(ed) => {
                let endpoint_ir = analyze_endpoint(
                    ed,
                    ancestry,
                    client_vars,
                    auth_vars,
                    auth_credentials,
                    client_auth_uses,
                    cache_profiles,
                    retry_profiles,
                    rate_limit_profiles,
                    layers,
                )?;
                endpoints.push(endpoint_ir);
            }
        }
    }
    Ok(())
}

fn reject_formatted_lit(lit: &LitStr, ctx: &'static str) -> Result<()> {
    let s = lit.value();
    if s.contains('{') || s.contains('}') {
        return Err(syn::Error::new(
            lit.span(),
            format!(
                "{ctx} string literals must not contain `{{` or `}}`; use separate route atoms such as \"a\", id, \"b\", or part[\"x\", id]"
            ),
        ));
    }
    Ok(())
}

fn collect_endpoint_output_types(items: &[Item]) -> Result<BTreeMap<String, Type>> {
    let mut out = BTreeMap::new();
    let mut scope_stack: Vec<String> = Vec::new();
    collect_endpoint_output_types_into(items, &mut out, &mut scope_stack)?;
    Ok(out)
}

fn collect_endpoint_output_types_into(
    items: &[Item],
    out: &mut BTreeMap<String, Type>,
    scope_stack: &mut Vec<String>,
) -> Result<()> {
    for item in items {
        match item {
            Item::Layer(layer) => {
                if let Some(name) = &layer.scope_name {
                    scope_stack.push(name.to_string());
                    collect_endpoint_output_types_into(&layer.items, out, scope_stack)?;
                    let _ = scope_stack.pop();
                } else {
                    collect_endpoint_output_types_into(&layer.items, out, scope_stack)?;
                }
            }
            Item::Endpoint(endpoint) => {
                let key = if scope_stack.is_empty() {
                    endpoint.name.to_string()
                } else {
                    format!("{}::{}", scope_stack.join("::"), endpoint.name)
                };
                if out.contains_key(&key) {
                    return Err(syn::Error::new(
                        endpoint.name.span(),
                        format!("duplicate endpoint `{key}`"),
                    ));
                }
                let output_ty = endpoint
                    .map
                    .as_ref()
                    .map(|m| m.out_ty.clone())
                    .unwrap_or_else(|| endpoint.response.ty.clone());
                out.insert(key, output_ty);
            }
        }
    }
    Ok(())
}

fn endpoint_scope_key(scope_modules: &[Ident], endpoint: &Ident) -> String {
    if scope_modules.is_empty() {
        endpoint.to_string()
    } else {
        format!(
            "{}::{}",
            scope_modules
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("::"),
            endpoint
        )
    }
}

fn analyze_layer_route_and_decls(
    ld: &LayerDef,
) -> Result<(Vec<PrefixPiece>, Vec<PathPiece>, Vec<VarInfo>)> {
    let decls: Vec<VarInfo> = ld
        .params
        .iter()
        .map(|d| VarInfo {
            rust: d.rust.clone(),
            optional: d.optional,
            ty: d.ty.clone(),
            default: d.default.clone(),
        })
        .collect();
    let mut prefix_pieces: Vec<PrefixPiece> = Vec::new();
    let mut path_pieces: Vec<PathPiece> = Vec::new();

    match ld.kind {
        LayerKind::Prefix => {
            for atom in &ld.route.atoms {
                match atom {
                    RouteAtom::Static(lit) => {
                        reject_formatted_lit(lit, "prefix")?;
                        // Allow "a.b.c" as a shorthand: split into host labels.
                        for label in lit.value().split('.') {
                            let label = label.trim();
                            if label.is_empty() {
                                return Err(syn::Error::new(
                                    lit.span(),
                                    "prefix label must not be empty",
                                ));
                            }
                            prefix_pieces.push(PrefixPiece::Static(label.to_string()));
                        }
                    }
                    RouteAtom::Fmt(spec) => {
                        let resolved = resolve_route_fmt_spec(spec, None, None)?;
                        prefix_pieces.push(PrefixPiece::Fmt(resolved));
                    }
                    RouteAtom::Ref(r) => {
                        match r.scope {
                            RefScope::Cx => {
                                prefix_pieces.push(PrefixPiece::CxVar {
                                    field: r.ident.clone(),
                                    optional: false, /* resolved later */
                                });
                            }
                            RefScope::Ep => {
                                prefix_pieces.push(PrefixPiece::EpVar {
                                    field: r.ident.clone(),
                                });
                            }
                            RefScope::Auth => {
                                return Err(syn::Error::new(
                                    r.ident.span(),
                                    "{secret.*} is not allowed in prefix route (headers/query only)",
                                ));
                            }
                        }
                    }
                }
            }
        }
        LayerKind::Path => {
            for atom in &ld.route.atoms {
                match atom {
                    RouteAtom::Static(lit) => {
                        reject_formatted_lit(lit, "path")?;
                        path_pieces.push(PathPiece::Static(lit.value()));
                    }
                    RouteAtom::Fmt(spec) => {
                        let resolved = resolve_route_fmt_spec(spec, None, None)?;
                        path_pieces.push(PathPiece::Fmt(resolved));
                    }
                    RouteAtom::Ref(r) => {
                        match r.scope {
                            RefScope::Cx => {
                                path_pieces.push(PathPiece::CxVar {
                                    field: r.ident.clone(),
                                    optional: false, /* resolved later */
                                });
                            }
                            RefScope::Ep => {
                                path_pieces.push(PathPiece::EpVar {
                                    field: r.ident.clone(),
                                });
                            }
                            RefScope::Auth => {
                                return Err(syn::Error::new(
                                    r.ident.span(),
                                    "{secret.*} is not allowed in path/prefix (headers/query only)",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((prefix_pieces, path_pieces, decls))
}

fn analyze_endpoint(
    ed: &EndpointDef,
    ancestry: &[usize],
    client_vars: &std::collections::BTreeMap<String, VarInfo>,
    auth_vars: &std::collections::BTreeMap<String, VarInfo>,
    auth_credentials: &std::collections::BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &[AuthUsePlanIr],
    cache_profiles: &BTreeMap<String, CacheConfigResolved>,
    retry_profiles: &BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanResolved>,
    layers: &[LayerIr],
) -> syn::Result<EndpointIr> {
    use std::collections::BTreeMap;

    // 1) Start endpoint var registry from ancestor layers.
    //    This defines what `ep.<field>` will contain (plus endpoint-local vars).
    let mut ep_vars: BTreeMap<String, VarInfo> = BTreeMap::new();
    let mut ep_var_order: Vec<String> = Vec::new();
    let mut upsert_ep = |rust: &Ident, optional: bool, ty: &Type, default: Option<&Expr>| {
        let key = rust.to_string();
        if !ep_vars.contains_key(&key) {
            ep_var_order.push(key.clone());
        }
        upsert_var(&mut ep_vars, rust, optional, ty, default)
    };

    for &lid in ancestry {
        for v in &layers[lid].decls {
            upsert_ep(&v.rust, v.optional, &v.ty, v.default.as_ref())?;
        }
    }
    for d in &ed.params {
        upsert_ep(&d.rust, d.optional, &d.ty, d.default.as_ref())?;
    }

    // 2) Build endpoint route pieces.
    let mut route_pieces: Vec<PathPiece> = Vec::new();

    for atom in &ed.route.atoms {
        match atom {
            RouteAtom::Static(lit) => {
                // Keep existing restriction for route literals.
                reject_formatted_lit(lit, "endpoint route")?;
                route_pieces.push(PathPiece::Static(lit.value()));
            }

            RouteAtom::Fmt(spec) => {
                let resolved = resolve_route_fmt_spec(spec, Some(client_vars), Some(&ep_vars))?;
                route_pieces.push(PathPiece::Fmt(resolved));
            }
            RouteAtom::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let v = client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                        syn::Error::new(
                            r.ident.span(),
                            format!("unknown client var `vars.{}`", r.ident),
                        )
                    })?;
                    route_pieces.push(PathPiece::CxVar {
                        field: r.ident.clone(),
                        optional: v.optional,
                    });
                }
                RefScope::Ep => {
                    let _v = ep_vars.get(&r.ident.to_string()).ok_or_else(|| {
                        syn::Error::new(
                            r.ident.span(),
                            format!("unknown endpoint var `ep.{}`", r.ident),
                        )
                    })?;
                    route_pieces.push(PathPiece::EpVar {
                        field: r.ident.clone(),
                    });
                }
                RefScope::Auth => {
                    return Err(syn::Error::new(
                        r.ident.span(),
                        "{secret.*} is not allowed in path/prefix (headers/query only)",
                    ));
                }
            },
        }
    }

    // 3) Resolve policy blocks now that endpoint vars are known.
    let mut policy = resolve_policy_blocks(
        &ed.policy,
        PolicyOwner::Endpoint,
        client_vars,
        auth_vars,
        Some(&ep_vars),
    )?;
    policy.retry = resolve_retry_spec(ed.retry.as_ref(), retry_profiles)?;
    policy.cache = resolve_cache_spec(ed.cache.as_ref(), cache_profiles)?;
    let endpoint_decls = ep_var_order
        .iter()
        .filter_map(|key| ep_vars.get(key))
        .cloned()
        .collect::<Vec<_>>();
    let endpoint_key_bindings =
        resolve_rate_limit_key_bindings(&ed.rate_limit_keys, &endpoint_decls)?;
    let mut visible_keys = rate_limit_key_bindings_for_ancestry(ancestry, layers);
    for binding in endpoint_key_bindings {
        visible_keys.insert(binding.name.clone(), binding);
    }
    policy.rate_limit = resolve_rate_limit_spec(
        ed.rate_limit.as_ref(),
        rate_limit_profiles,
        &visible_keys,
        Some(&ep_vars),
    )?;
    let mut auth_uses = client_auth_uses.to_vec();
    for &lid in ancestry {
        auth_uses.extend(layers[lid].auth_uses.iter().cloned());
    }
    auth_uses.extend(resolve_auth_uses(
        &ed.auth_uses,
        auth_credentials,
        AuthUseProvenanceIr::Endpoint,
    )?);
    let scope_modules: Vec<Ident> = ancestry
        .iter()
        .filter_map(|&lid| layers[lid].scope_name.clone())
        .collect();
    let current_endpoint_key = endpoint_scope_key(&scope_modules, &ed.name);
    for credential in auth_credentials.values() {
        let AuthCredentialKindIr::Endpoint { endpoint_key, .. } = &credential.kind else {
            continue;
        };
        if endpoint_key != &current_endpoint_key {
            continue;
        }
        if auth_plan_references_credential(&auth_uses, &credential.name) {
            return Err(syn::Error::new(
                ed.name.span(),
                format!(
                    "credential `{}` cannot acquire via endpoint `{}` because the endpoint uses that credential",
                    credential.name, ed.name
                ),
            ));
        }
    }

    // 4) Resolve paginate, if any.
    let paginate = match &ed.paginate {
        None => None,
        Some(p) => Some(resolve_paginate(p, client_vars, auth_vars, &ep_vars)?),
    };

    // 5) Resolve map block, if any.
    let map = ed.map.as_ref().map(|m| MapResolved {
        out_ty: m.out_ty.clone(),
        body: m.body.clone(),
    });

    // 6) Produce final IR.
    Ok(EndpointIr {
        name: ed.name.clone(),
        scope_modules,
        method: ed.method.clone(),
        route_pieces,
        ancestry: ancestry.to_vec(),

        // Stable declaration order.
        vars: endpoint_decls,

        body: ed.body.clone(),
        response: ed.response.clone(),

        policy,
        auth_uses,
        paginate,
        map,
    })
}

fn resolve_paginate(
    p: &PaginateSpec,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    ep_vars: &BTreeMap<String, VarInfo>,
) -> Result<PaginateResolved> {
    let mut assigns = Vec::new();
    for a in &p.assigns {
        let vk = resolve_value_kind(
            &a.value,
            client_vars,
            auth_vars,
            Some(ep_vars),
            a.value.span(),
        )?;
        // rule: forbid `vars.*` and `secret.*` in pagination (controller config must not depend on runtime vars/secrets)
        if matches!(vk, ValueKind::CxField(_) | ValueKind::AuthField(_)) {
            return Err(syn::Error::new(
                a.value.span(),
                "paginate assignments must not reference `vars.*` or `secret.*`; use `ep.*` or constants",
            ));
        }
        assigns.push((a.key.clone(), vk));
    }
    Ok(PaginateResolved {
        ctrl_ty: p.ctrl_ty.clone(),
        assigns,
    })
}

fn resolve_policy_blocks(
    policy: &PolicyBlocks,
    owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<PolicyBlocksResolved> {
    let mut out = PolicyBlocksResolved::default();

    if let Some(h) = &policy.headers {
        out.headers = resolve_policy_block(
            h,
            PolicyKeyKind::Header,
            owner,
            client_vars,
            auth_vars,
            endpoint_vars,
        )?;
    }
    if let Some(q) = &policy.query {
        out.query = resolve_policy_block(
            q,
            PolicyKeyKind::Query,
            owner,
            client_vars,
            auth_vars,
            endpoint_vars,
        )?;
    }
    if let Some(t) = &policy.timeout {
        // timeout expr must not contain nested vars/ep; allow `vars.x` or `ep.y` only as root
        if emit_helpers::contains_cx_or_ep(t)
            && emit_helpers::is_cx_field(t).is_none()
            && emit_helpers::is_ep_field(t).is_none()
        {
            return Err(syn::Error::new(
                t.span(),
                "timeout expression cannot contain nested `vars`/`ep`; use a plain `vars.x`, `ep.y`, or a pure expression without them",
            ));
        }
        out.timeout = Some(resolve_value_kind(
            t,
            client_vars,
            auth_vars,
            endpoint_vars,
            t.span(),
        )?);
    }

    Ok(out)
}

fn resolve_policy_block(
    blk: &PolicyBlock,
    kind: PolicyKeyKind,
    owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Vec<PolicyOp>> {
    let mut ops = Vec::new();

    for stmt in &blk.stmts {
        match stmt {
            PolicyStmt::Remove { key } => {
                ops.push(PolicyOp::Remove {
                    key: resolve_key(key),
                });
            }
            PolicyStmt::Set { key, value, op } => {
                if kind == PolicyKeyKind::Header && *op == SetOp::Push {
                    return Err(syn::Error::new(
                        value.span(),
                        "`+=` is not allowed in headers; only in query",
                    ));
                }
                let vk = resolve_policy_value_kind(
                    value,
                    owner,
                    client_vars,
                    auth_vars,
                    endpoint_vars,
                    value.span(),
                )?;

                // Optional-ref conditional set/remove for pure vars/ep refs
                let cond = match &vk {
                    ValueKind::CxField(id) => {
                        let v = client_vars.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(id.span(), format!("unknown client var `vars.{}`", id))
                        })?;
                        if v.optional {
                            Some(OptionalRefKind::Cx)
                        } else {
                            None
                        }
                    }
                    ValueKind::EpField(id) => {
                        let ep = endpoint_vars.ok_or_else(|| {
                            syn::Error::new(id.span(), "ep.* is not allowed here")
                        })?;
                        let v = ep.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(id.span(), format!("unknown endpoint var `ep.{}`", id))
                        })?;
                        if v.optional {
                            Some(OptionalRefKind::Ep)
                        } else {
                            None
                        }
                    }
                    ValueKind::AuthField(id) => {
                        let v = auth_vars.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(
                                id.span(),
                                format!("unknown secret var `secret.{}`", id),
                            )
                        })?;
                        if v.optional {
                            Some(OptionalRefKind::Auth)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                ops.push(PolicyOp::Set {
                    key: resolve_key(key),
                    value: vk,
                    op: *op,
                    conditional_on_optional_ref: cond,
                });
            }
        }
    }

    // validate references to ep in non-endpoint contexts
    if owner == PolicyOwner::Client {
        for op in &ops {
            if let PolicyOp::Set { value, .. } = op
                && matches!(value, ValueKind::EpField(_))
            {
                let sp = blk
                    .stmts
                    .first()
                    .map(policy_stmt_span)
                    .unwrap_or_else(Span::call_site);
                return Err(syn::Error::new(
                    sp,
                    "`ep.*` is not allowed in client policy",
                ));
            }
        }
    }

    // validate vars/ep existence
    for op in &ops {
        if let PolicyOp::Set { value, .. } = op {
            match value {
                ValueKind::CxField(id) => {
                    if !client_vars.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            format!("unknown client var `vars.{}`", id),
                        ));
                    }
                }
                ValueKind::AuthField(id) => {
                    if !auth_vars.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            format!("unknown secret var `secret.{}`", id),
                        ));
                    }
                }
                ValueKind::EpField(id) => {
                    let ep = endpoint_vars
                        .ok_or_else(|| syn::Error::new(id.span(), "`ep.*` is not allowed here"))?;
                    if !ep.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            format!("unknown endpoint var `ep.{}`", id),
                        ));
                    }
                }
                ValueKind::OtherExpr(e) => {
                    if emit_helpers::contains_cx_or_ep(e) {
                        return Err(syn::Error::new(
                            e.span(),
                            "nested `vars`/`ep` usage is not supported; use plain `vars.x`, `ep.y`, or a pure expression without them",
                        ));
                    }
                }
                ValueKind::LitStr(_) => {}
                ValueKind::Fmt(_) => {}
            }
        }
    }

    Ok(ops)
}

fn key_spec_span(k: &KeySpec) -> Span {
    match k {
        KeySpec::Ident(id) => id.span(),
        KeySpec::Str(s) => s.span(),
    }
}

fn policy_stmt_span(s: &PolicyStmt) -> Span {
    match s {
        PolicyStmt::Remove { key } => key_spec_span(key),
        PolicyStmt::Set {
            key: _,
            value,
            op: _,
        } => value.span(),
    }
}

fn resolve_key(k: &KeySpec) -> KeyResolved {
    match k {
        KeySpec::Ident(id) => KeyResolved::Ident(id.clone()),
        KeySpec::Str(s) => KeyResolved::Static(s.clone()),
    }
}

fn resolve_value_kind(
    expr: &Expr,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    _span: Span,
) -> Result<ValueKind> {
    if let Expr::Lit(l) = expr
        && let syn::Lit::Str(s) = &l.lit
    {
        return Ok(ValueKind::LitStr(s.clone()));
    }

    if let Some(id) = emit_helpers::is_cx_field(expr) {
        // validate later at block-level
        let _ = client_vars;
        return Ok(ValueKind::CxField(id));
    }
    if let Some(id) = emit_helpers::is_auth_field(expr) {
        let _ = auth_vars;
        return Ok(ValueKind::AuthField(id));
    }
    if let Some(id) = emit_helpers::is_ep_field(expr) {
        let _ = endpoint_vars;
        return Ok(ValueKind::EpField(id));
    }

    Ok(ValueKind::OtherExpr(expr.clone()))
}

fn resolve_route_fmt_spec(
    spec: &FmtSpec,
    client_vars: Option<&BTreeMap<String, VarInfo>>,
    ep_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<FmtResolved> {
    let mut pieces: Vec<FmtResolvedPiece> = Vec::new();

    for p in &spec.pieces {
        match p {
            FmtPiece::Lit(l) => pieces.push(FmtResolvedPiece::Lit(l.clone())),
            FmtPiece::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let cv = client_vars
                        .and_then(|m| m.get(&r.ident.to_string()))
                        .ok_or_else(|| {
                            syn::Error::new(
                                r.ident.span(),
                                format!("unknown client var `vars.{}`", r.ident),
                            )
                        })?;
                    pieces.push(FmtResolvedPiece::Var {
                        source: FmtVarSource::Cx,
                        field: r.ident.clone(),
                        optional: cv.optional,
                    });
                }
                RefScope::Ep => {
                    let ev_opt = ep_vars.and_then(|m| m.get(&r.ident.to_string()));
                    let optional = if let Some(ev) = ev_opt {
                        ev.optional
                    } else {
                        false
                    };
                    pieces.push(FmtResolvedPiece::Var {
                        source: FmtVarSource::Ep,
                        field: r.ident.clone(),
                        optional,
                    });
                }
                RefScope::Auth => {
                    return Err(syn::Error::new(
                        r.ident.span(),
                        "{secret.*} is not allowed in routes (headers/query only)",
                    ));
                }
            },
        }
    }

    Ok(FmtResolved {
        require_all: spec.require_all,
        pieces,
    })
}

fn resolve_policy_value_kind(
    v: &crate::ast::PolicyValue,
    _owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    span: proc_macro2::Span,
) -> Result<ValueKind> {
    match v {
        crate::ast::PolicyValue::Expr(e) => {
            resolve_value_kind(e, client_vars, auth_vars, endpoint_vars, span)
        }
        crate::ast::PolicyValue::Fmt(fmt) => {
            let mut pieces: Vec<FmtResolvedPiece> = Vec::new();
            let mut has_optional = false;

            for p in &fmt.pieces {
                match p {
                    crate::ast::FmtPiece::Lit(s) => pieces.push(FmtResolvedPiece::Lit(s.clone())),
                    crate::ast::FmtPiece::Ref(r) => match r.scope {
                        RefScope::Cx => {
                            let v = client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                syn::Error::new(
                                    r.ident.span(),
                                    format!("unknown client var `vars.{}`", r.ident),
                                )
                            })?;
                            has_optional |= v.optional;
                            pieces.push(FmtResolvedPiece::Var {
                                source: FmtVarSource::Cx,
                                field: r.ident.clone(),
                                optional: v.optional,
                            });
                        }
                        RefScope::Ep => {
                            let ep = endpoint_vars.ok_or_else(|| {
                                syn::Error::new(r.ident.span(), "`ep.*` is not allowed here")
                            })?;
                            let v = ep.get(&r.ident.to_string()).ok_or_else(|| {
                                syn::Error::new(
                                    r.ident.span(),
                                    format!("unknown endpoint var `ep.{}`", r.ident),
                                )
                            })?;
                            has_optional |= v.optional;
                            pieces.push(FmtResolvedPiece::Var {
                                source: FmtVarSource::Ep,
                                field: r.ident.clone(),
                                optional: v.optional,
                            });
                        }
                        RefScope::Auth => {
                            let v = auth_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                syn::Error::new(
                                    r.ident.span(),
                                    format!("unknown secret var `secret.{}`", r.ident),
                                )
                            })?;
                            has_optional |= v.optional;
                            pieces.push(FmtResolvedPiece::Var {
                                source: FmtVarSource::Auth,
                                field: r.ident.clone(),
                                optional: v.optional,
                            });
                        }
                    },
                }
            }

            if !fmt.require_all && has_optional {
                return Err(syn::Error::new(
                    span,
                    "optional placeholders are not allowed in this template context",
                ));
            }

            Ok(ValueKind::Fmt(FmtResolved {
                require_all: fmt.require_all,
                pieces,
            }))
        }
    }
}

#[derive(Debug, Clone)]
pub struct FmtResolved {
    pub require_all: bool,
    pub pieces: Vec<FmtResolvedPiece>,
}

#[derive(Debug, Clone)]
pub enum FmtResolvedPiece {
    Lit(syn::LitStr),
    Var {
        source: FmtVarSource,
        field: syn::Ident,
        optional: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmtVarSource {
    Cx,
    Ep,
    Auth,
}
