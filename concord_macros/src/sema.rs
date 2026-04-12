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
    pub kind: LayerKind,
    pub prefix_pieces: Vec<PrefixPiece>, // if Prefix
    pub path_pieces: Vec<PathPiece>,     // if Path
    pub policy: PolicyBlocksResolved,
    pub auth_uses: Vec<AuthUseIr>,
    pub decls: Vec<VarInfo>, // endpoint vars declared by this layer (placeholders + binds)
}

#[derive(Debug)]
pub struct EndpointIr {
    pub name: Ident,
    pub method: Ident,
    pub route_pieces: Vec<PathPiece>,

    pub ancestry: Vec<usize>, // layer ids in nesting order (outer -> inner)

    pub vars: Vec<VarInfo>, // endpoint vars (union, stable)
    pub body: Option<CodecSpec>,
    pub response: CodecSpec,

    pub policy: PolicyBlocksResolved,
    pub auth_uses: Vec<AuthUseIr>,

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
    ApiKey { secret: Ident },
    StaticBearer { secret: Ident },
    Basic { username: Ident, password: Ident },
    OAuth2ClientCredentials {
        token_url: LitStr,
        client_id: Ident,
        client_secret: Ident,
        scope: Option<LitStr>,
    },
    Custom {
        provider_ty: Type,
        provider: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct AuthUseIr {
    pub kind: AuthUseKindIr,
}

#[derive(Debug, Clone)]
pub enum AuthUseKindIr {
    Bearer { credential: Ident },
    Header { header: LitStr, credential: Ident },
    Query { key: LitStr, credential: Ident },
    Basic { credential: Ident },
    Custom {
        usage_ty: Type,
        usage: Expr,
        credential: Ident,
    },
}

#[derive(Debug, Clone)]
pub enum PrefixPiece {
    Static(String),
    Var {
        wire: String,
        field: Ident,
        optional: bool,
    },
    CxVar {
        field: Ident,
        optional: bool,
    },
    EpVar {
        field: Ident,
    },
    Fmt(FmtResolved),
}

#[derive(Debug, Clone)]
pub enum PathPiece {
    Static(String),
    Var { field: Ident, optional: bool },
    CxVar { field: Ident, optional: bool },
    EpVar { field: Ident },
    Fmt(FmtResolved),
}

#[derive(Debug, Default)]
pub struct PolicyBlocksResolved {
    pub headers: Vec<PolicyOp>,
    pub query: Vec<PolicyOp>,
    pub timeout: Option<ValueKind>,
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
    Bind {
        key: KeyResolved,
        kind: PolicyKeyKind,
        field: Ident,
        optional: bool,
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

    // client vars: start from explicit `vars {}` then merge binds/fmt decls from client policy.
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
    collect_client_binds(&ast.client.policy, &mut client_vars_map)?;

    // auth vars: ONLY from `auth_vars {}`.
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

    let client_auth_credentials =
        analyze_auth_credentials(ast.client.auth.as_ref(), &auth_vars_map)?;
    let auth_credential_map: BTreeMap<String, AuthCredentialIr> = client_auth_credentials
        .iter()
        .map(|c| (c.name.to_string(), c.clone()))
        .collect();
    let client_auth_uses = resolve_auth_uses(&ast.client.auth_uses, &auth_credential_map)?;

    // validate client policy + resolve
    let client_policy = resolve_policy_blocks(
        &ast.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        &auth_vars_map,
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
        &mut layers,
        &mut endpoints,
    )?;

    Ok(Ir {
        mod_name,
        client_name,
        scheme: ast.client.scheme,
        domain: ast.client.host,
        client_vars,
        client_auth_vars,
        client_auth_credentials,
        client_policy,
        layers,
        endpoints,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyOwner {
    Client,
    Endpoint,
    Layer,
}

fn collect_client_binds(policy: &PolicyBlocks, out: &mut BTreeMap<String, VarInfo>) -> Result<()> {
    // existant: binds
    for blk in policy.headers.iter().chain(policy.query.iter()) {
        for stmt in &blk.stmts {
            match stmt {
                PolicyStmt::Bind { decl, .. } => {
                    upsert_var(
                        out,
                        &decl.rust,
                        decl.optional,
                        &decl.ty,
                        decl.default.as_ref(),
                    )?;
                }
                PolicyStmt::BindShort { ident_key, decl } => {
                    upsert_var(
                        out,
                        ident_key,
                        decl.optional,
                        &decl.ty,
                        decl.default.as_ref(),
                    )?;
                }
                _ => {}
            }
        }
    }

    // nouveau: fmt decls
    for blk in policy.headers.iter().chain(policy.query.iter()) {
        for stmt in &blk.stmts {
            if let PolicyStmt::Set { value, .. } = stmt
                && let crate::ast::PolicyValue::Fmt(fmt) = value
            {
                for p in &fmt.pieces {
                    if let crate::ast::FmtPiece::Var(d) = p {
                        upsert_var(out, &d.rust, d.optional, &d.ty, d.default.as_ref())?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_policy_fmt_decls(policy: &crate::ast::PolicyBlocks, out: &mut Vec<VarInfo>) {
    for blk in policy.headers.iter().chain(policy.query.iter()) {
        for stmt in &blk.stmts {
            if let crate::ast::PolicyStmt::Set { value, .. } = stmt
                && let crate::ast::PolicyValue::Fmt(fmt) = value
            {
                for p in &fmt.pieces {
                    if let crate::ast::FmtPiece::Var(d) = p {
                        out.push(VarInfo {
                            rust: d.rust.clone(),
                            optional: d.optional,
                            ty: d.ty.clone(),
                            default: d.default.clone(),
                        });
                    }
                }
            }
        }
    }
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

fn validate_required_secret(
    secret: &SecretRef,
    auth_vars: &BTreeMap<String, VarInfo>,
) -> Result<()> {
    let Some(info) = auth_vars.get(&secret.ident.to_string()) else {
        return Err(syn::Error::new(
            secret.ident.span(),
            format!("unknown secret `secret.{}` in auth credential", secret.ident),
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
) -> Result<Vec<AuthUseIr>> {
    uses.iter()
        .map(|u| {
            let credential = auth_use_credential_ident(u);
            let cred = credentials.get(&credential.to_string()).ok_or_else(|| {
                syn::Error::new(
                    credential.span(),
                    format!("unknown auth credential `{credential}`"),
                )
            })?;
            validate_auth_usage_compatibility(u, cred)?;

            let kind = match &u.kind {
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
            Ok(AuthUseIr { kind })
        })
        .collect()
}

fn auth_use_credential_ident(u: &AuthUseDecl) -> &Ident {
    match &u.kind {
        AuthUseKind::Bearer { credential }
        | AuthUseKind::Header { credential, .. }
        | AuthUseKind::Query { credential, .. }
        | AuthUseKind::Basic { credential }
        | AuthUseKind::Custom { credential, .. } => credential,
    }
}

fn validate_auth_usage_compatibility(u: &AuthUseDecl, cred: &AuthCredentialIr) -> Result<()> {
    match (&u.kind, &cred.kind) {
        (AuthUseKind::Custom { .. }, _) | (_, AuthCredentialKindIr::Custom { .. }) => Ok(()),
        (
            AuthUseKind::Bearer { .. },
            AuthCredentialKindIr::StaticBearer { .. }
            | AuthCredentialKindIr::OAuth2ClientCredentials { .. },
        ) => Ok(()),
        (
            AuthUseKind::Header { .. } | AuthUseKind::Query { .. },
            AuthCredentialKindIr::ApiKey { .. }
            | AuthCredentialKindIr::StaticBearer { .. }
            | AuthCredentialKindIr::OAuth2ClientCredentials { .. },
        ) => Ok(()),
        (AuthUseKind::Basic { .. }, AuthCredentialKindIr::Basic { .. }) => Ok(()),
        (AuthUseKind::Bearer { credential }, _) => Err(syn::Error::new(
            credential.span(),
            format!(
                "BearerAuth requires an access-token credential; `{}` is not compatible",
                cred.name
            ),
        )),
        (AuthUseKind::Header { credential, .. } | AuthUseKind::Query { credential, .. }, _) => {
            Err(syn::Error::new(
                credential.span(),
                format!(
                    "header/query auth requires a secret credential; `{}` is not compatible",
                    cred.name
                ),
            ))
        }
        (AuthUseKind::Basic { credential }, _) => Err(syn::Error::new(
            credential.span(),
            format!("BasicAuth requires a Basic credential; `{}` is not compatible", cred.name),
        )),
    }
}

fn walk_items(
    items: &[Item],
    ancestry: &mut Vec<usize>,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    auth_credentials: &BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &[AuthUseIr],
    layers: &mut Vec<LayerIr>,
    endpoints: &mut Vec<EndpointIr>,
) -> Result<()> {
    for it in items {
        match it {
            Item::Layer(ld) => {
                let id = layers.len();
                let (prefix_pieces, path_pieces, decls) = analyze_layer_route_and_decls(ld)?;
                let policy = resolve_policy_blocks(
                    &ld.policy,
                    PolicyOwner::Layer,
                    client_vars,
                    auth_vars,
                    None, // endpoint vars not known at layer-level alone (validated per endpoint)
                )?;
                let auth_uses = resolve_auth_uses(&ld.auth_uses, auth_credentials)?;

                layers.push(LayerIr {
                    kind: ld.kind,
                    prefix_pieces,
                    path_pieces,
                    policy,
                    auth_uses,
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
                "{ctx} string literals must not contain `{{` or `}}`; use separate atoms (e.g. \"a\" / {{id:Ty}} / \"b\" or {{x:Ty}} . \"api\")"
            ),
        ));
    }
    Ok(())
}

fn varinfo_from_decl(d: &TemplateVarDecl) -> VarInfo {
    VarInfo {
        rust: d.rust.clone(),
        optional: d.optional,
        ty: d.ty.clone(),
        default: d.default.clone(),
    }
}

fn analyze_layer_route_and_decls(
    ld: &LayerDef,
) -> Result<(Vec<PrefixPiece>, Vec<PathPiece>, Vec<VarInfo>)> {
    let mut decls: Vec<VarInfo> = ld
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
                    RouteAtom::Var(d) => {
                        decls.push(varinfo_from_decl(d));
                        prefix_pieces.push(PrefixPiece::Var {
                            wire: d.wire.to_string(),
                            field: d.rust.clone(),
                            optional: d.optional,
                        });
                    }
                    RouteAtom::Fmt(spec) => {
                        let (resolved, fmt_decls) = resolve_route_fmt_spec(spec, None, None)?;
                        decls.extend(fmt_decls);
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
                                    "{auth.*} is not allowed in prefix route (headers/query only)",
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
                    RouteAtom::Var(d) => {
                        decls.push(varinfo_from_decl(d));
                        path_pieces.push(PathPiece::Var {
                            field: d.rust.clone(),
                            optional: d.optional,
                        });
                    }
                    RouteAtom::Fmt(spec) => {
                        let (resolved, fmt_decls) = resolve_route_fmt_spec(spec, None, None)?;
                        decls.extend(fmt_decls);
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
                                    "{auth.*} is not allowed in path/prefix (headers/query only)",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect endpoint-var binds declared in this layer's policy
    for blk in ld.policy.headers.iter().chain(ld.policy.query.iter()) {
        for stmt in &blk.stmts {
            match stmt {
                PolicyStmt::Bind { decl, .. } => decls.push(VarInfo {
                    rust: decl.rust.clone(),
                    optional: decl.optional,
                    ty: decl.ty.clone(),
                    default: decl.default.clone(),
                }),
                PolicyStmt::BindShort { ident_key, decl } => decls.push(VarInfo {
                    rust: ident_key.clone(),
                    optional: decl.optional,
                    ty: decl.ty.clone(),
                    default: decl.default.clone(),
                }),
                _ => {}
            }
        }
    }
    collect_policy_fmt_decls(&ld.policy, &mut decls);
    Ok((prefix_pieces, path_pieces, decls))
}

fn analyze_endpoint(
    ed: &EndpointDef,
    ancestry: &[usize],
    client_vars: &std::collections::BTreeMap<String, VarInfo>,
    auth_vars: &std::collections::BTreeMap<String, VarInfo>,
    auth_credentials: &std::collections::BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &[AuthUseIr],
    layers: &[LayerIr],
) -> syn::Result<EndpointIr> {
    use std::collections::BTreeMap;

    // 1) Start endpoint var registry from ancestor layers.
    //    This defines what `ep.<field>` will contain (plus endpoint-local vars).
    let mut ep_vars: BTreeMap<String, VarInfo> = BTreeMap::new();

    for &lid in ancestry {
        for v in &layers[lid].decls {
            upsert_var(&mut ep_vars, &v.rust, v.optional, &v.ty, v.default.as_ref())?;
        }
    }
    for d in &ed.params {
        upsert_var(
            &mut ep_vars,
            &d.rust,
            d.optional,
            &d.ty,
            d.default.as_ref(),
        )?;
    }

    // 2) Build endpoint route pieces and collect vars declared in the route.
    let mut route_pieces: Vec<PathPiece> = Vec::new();

    for atom in &ed.route.atoms {
        match atom {
            RouteAtom::Static(lit) => {
                // Keep existing restriction for route literals.
                reject_formatted_lit(lit, "endpoint route")?;
                route_pieces.push(PathPiece::Static(lit.value()));
            }

            RouteAtom::Var(d) => {
                // Route placeholder declares a variable.
                upsert_var(&mut ep_vars, &d.rust, d.optional, &d.ty, d.default.as_ref())?;

                route_pieces.push(PathPiece::Var {
                    field: d.rust.clone(),
                    optional: d.optional,
                });
            }

            RouteAtom::Fmt(spec) => {
                // fmt[...] inside a route declares vars too.
                let (resolved, fmt_decls) =
                    resolve_route_fmt_spec(spec, Some(client_vars), Some(&ep_vars))?;

                for v in fmt_decls {
                    upsert_var(&mut ep_vars, &v.rust, v.optional, &v.ty, v.default.as_ref())?;
                }
                route_pieces.push(PathPiece::Fmt(resolved));
            }
            RouteAtom::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let v = client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                        syn::Error::new(
                            r.ident.span(),
                            format!("unknown client var `cx.{}`", r.ident),
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
                        "{auth.*} is not allowed in path/prefix (headers/query only)",
                    ));
                }
            },
        }
    }

    // 3) Collect endpoint-level policy binds into `ep_vars` so codegen has all fields.
    //    (headers/query bind syntaxes)
    for blk in ed.policy.headers.iter().chain(ed.policy.query.iter()) {
        for stmt in &blk.stmts {
            match stmt {
                PolicyStmt::Bind { decl, .. } => {
                    upsert_var(
                        &mut ep_vars,
                        &decl.rust,
                        decl.optional,
                        &decl.ty,
                        decl.default.as_ref(),
                    )?;
                }
                PolicyStmt::BindShort { ident_key, decl } => {
                    // `ident_key` is the rust field name in this form (per your snippet)
                    upsert_var(
                        &mut ep_vars,
                        ident_key,
                        decl.optional,
                        &decl.ty,
                        decl.default.as_ref(),
                    )?;
                }
                _ => {}
            }
        }
    }

    // 4) Collect any vars introduced by policy fmt[...] inside headers/query.
    {
        let mut fmt_decls: Vec<VarInfo> = Vec::new();
        collect_policy_fmt_decls(&ed.policy, &mut fmt_decls);
        for v in fmt_decls {
            upsert_var(&mut ep_vars, &v.rust, v.optional, &v.ty, v.default.as_ref())?;
        }
    }

    // 5) Resolve policy blocks now that endpoint vars are known.
    let policy = resolve_policy_blocks(
        &ed.policy,
        PolicyOwner::Endpoint,
        client_vars,
        auth_vars,
        Some(&ep_vars),
    )?;
    let mut auth_uses = client_auth_uses.to_vec();
    for &lid in ancestry {
        auth_uses.extend(layers[lid].auth_uses.iter().cloned());
    }
    auth_uses.extend(resolve_auth_uses(&ed.auth_uses, auth_credentials)?);

    // 6) Resolve paginate, if any.
    let paginate = match &ed.paginate {
        None => None,
        Some(p) => Some(resolve_paginate(p, client_vars, auth_vars, &ep_vars)?),
    };

    // 7) Resolve map block, if any.
    let map = ed.map.as_ref().map(|m| MapResolved {
        out_ty: m.out_ty.clone(),
        body: m.body.clone(),
    });

    // 8) Produce final IR.
    Ok(EndpointIr {
        name: ed.name.clone(),
        method: ed.method.clone(),
        route_pieces,
        ancestry: ancestry.to_vec(),

        // Stable order (BTreeMap order).
        vars: ep_vars.values().cloned().collect(),

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
        // rule: forbid `cx.*` and `auth.*` in pagination (controller config must not depend on runtime vars/secrets)
        if matches!(vk, ValueKind::CxField(_) | ValueKind::AuthField(_)) {
            return Err(syn::Error::new(
                a.value.span(),
                "paginate assignments must not reference `cx.*` or `auth.*`; use `ep.*` or constants",
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
        // timeout expr must not contain nested cx/ep; allow `cx.x` or `ep.y` only as root
        if emit_helpers::contains_cx_or_ep(t)
            && emit_helpers::is_cx_field(t).is_none()
            && emit_helpers::is_ep_field(t).is_none()
        {
            return Err(syn::Error::new(
                t.span(),
                "timeout expression cannot contain nested `cx`/`ep`; use a plain `cx.x`, `ep.y`, or a pure expression without them",
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

                // Optional-ref conditional set/remove for pure cx/ep refs
                let cond = match &vk {
                    ValueKind::CxField(id) => {
                        let v = client_vars.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(id.span(), format!("unknown client var `cx.{}`", id))
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
                            syn::Error::new(id.span(), format!("unknown auth var `auth.{}`", id))
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
            PolicyStmt::Bind { key, decl } => {
                if owner == PolicyOwner::Client {
                    // bind declares client var; validation done separately
                }
                ops.push(PolicyOp::Bind {
                    key: resolve_key(key),
                    kind,
                    field: decl.rust.clone(),
                    optional: decl.optional,
                });
            }
            PolicyStmt::BindShort { ident_key, decl } => {
                if kind == PolicyKeyKind::Header {
                    // ok
                }
                ops.push(PolicyOp::Bind {
                    key: KeyResolved::Ident(ident_key.clone()),
                    kind,
                    field: ident_key.clone(),
                    optional: decl.optional,
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

    // validate cx/ep existence
    for op in &ops {
        if let PolicyOp::Set { value, .. } = op {
            match value {
                ValueKind::CxField(id) => {
                    if !client_vars.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            format!("unknown client var `cx.{}`", id),
                        ));
                    }
                }
                ValueKind::AuthField(id) => {
                    if !auth_vars.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            format!("unknown auth var `auth.{}`", id),
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
                            "nested `cx`/`ep` usage is not supported; use plain `cx.x`, `ep.y`, or a pure expression without them",
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
        PolicyStmt::Bind { key: _, decl } => decl.rust.span(),
        PolicyStmt::BindShort { ident_key, decl: _ } => ident_key.span(),
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
) -> Result<(FmtResolved, Vec<VarInfo>)> {
    let mut decls: Vec<VarInfo> = Vec::new();
    let mut pieces: Vec<FmtResolvedPiece> = Vec::new();

    for p in &spec.pieces {
        match p {
            FmtPiece::Lit(l) => pieces.push(FmtResolvedPiece::Lit(l.clone())),
            FmtPiece::Var(d) => {
                // même déclaration qu’un placeholder
                decls.push(VarInfo {
                    rust: d.rust.clone(),
                    optional: d.optional,
                    ty: d.ty.clone(),
                    default: d.default.clone(),
                });
                pieces.push(FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    field: d.rust.clone(),
                    optional: d.optional,
                });
            }
            FmtPiece::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let cv = client_vars
                        .and_then(|m| m.get(&r.ident.to_string()))
                        .ok_or_else(|| {
                            syn::Error::new(
                                r.ident.span(),
                                format!("unknown client var `cx.{}`", r.ident),
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
                        "{auth.*} is not allowed in routes (headers/query only)",
                    ));
                }
            },
        }
    }

    Ok((
        FmtResolved {
            require_all: spec.require_all,
            pieces,
        },
        decls,
    ))
}

fn resolve_policy_value_kind(
    v: &crate::ast::PolicyValue,
    owner: PolicyOwner,
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
                    crate::ast::FmtPiece::Var(d) => {
                        has_optional |= d.optional;

                        // validation d’existence si possible (client et endpoint)
                        match owner {
                            PolicyOwner::Client => {
                                if !client_vars.contains_key(&d.rust.to_string()) {
                                    return Err(syn::Error::new(
                                        d.rust.span(),
                                        format!("unknown client var `{}`", d.rust),
                                    ));
                                }
                            }
                            PolicyOwner::Endpoint => {
                                let ep = endpoint_vars.ok_or_else(|| {
                                    syn::Error::new(d.rust.span(), "ep vars not available")
                                })?;
                                if !ep.contains_key(&d.rust.to_string()) {
                                    return Err(syn::Error::new(
                                        d.rust.span(),
                                        format!("unknown endpoint var `{}`", d.rust),
                                    ));
                                }
                            }
                            PolicyOwner::Layer => {
                                // layer-level: pas forcément de registry complet ici; l’existence est garantie
                                // par l’injection dans decls + union endpoint.
                            }
                        }

                        pieces.push(FmtResolvedPiece::Var {
                            source: match owner {
                                PolicyOwner::Client => FmtVarSource::Cx,
                                PolicyOwner::Endpoint => FmtVarSource::Ep,
                                PolicyOwner::Layer => FmtVarSource::Ep, // layer policy runs with ep in scope
                            },
                            field: d.rust.clone(),
                            optional: d.optional,
                        });
                    }
                    crate::ast::FmtPiece::Ref(r) => match r.scope {
                        RefScope::Cx => {
                            let v = client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                syn::Error::new(
                                    r.ident.span(),
                                    format!("unknown client var `cx.{}`", r.ident),
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
                                    format!("unknown auth var `auth.{}`", r.ident),
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
                    "fmt[...] forbids optional placeholders; use fmt?[...]",
                ));
            }

            Ok(ValueKind::Fmt(FmtResolved {
                require_all: fmt.require_all,
                pieces,
            }))
        }
    }
}

impl syn::parse::Parse for TemplateVarDecl {
    fn parse(input: syn::parse::ParseStream<'_>) -> Result<Self> {
        let wire: Ident = input.parse()?;
        let mut rust = wire.clone();
        if input.peek(syn::Token![as]) {
            input.parse::<syn::Token![as]>()?;
            rust = input.parse::<Ident>()?;
        }
        let optional = input.parse::<Option<syn::Token![?]>>()?.is_some();
        input.parse::<syn::Token![:]>()?;
        let ty: Type = input.parse()?;
        let default = if input.peek(syn::Token![=]) {
            input.parse::<syn::Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };
        Ok(TemplateVarDecl {
            wire,
            rust,
            optional,
            ty,
            default,
        })
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
