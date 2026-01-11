// concord_macros/src/sema.rs
use crate::ast::*;
use crate::emit_helpers;
use proc_macro2::Span;
use std::collections::{BTreeMap};
use std::str::FromStr;
use syn::{spanned::Spanned, Expr, Ident, LitStr, Result, Type};

#[derive(Debug)]
pub struct Ir {
    pub mod_name: Ident,
    pub client_name: Ident,
    pub scheme: SchemeLit,
    pub domain: LitStr,

    pub client_vars: Vec<VarInfo>, // stable order
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
    pub id: usize,
    pub kind: LayerKind,
    pub template: LitStr,
    pub prefix_pieces: Vec<PrefixPiece>, // if Prefix
    pub path_pieces: Vec<PathPiece>,     // if Path
    pub policy: PolicyBlocksResolved,
    pub decls: Vec<VarInfo>, // endpoint vars declared by this layer (placeholders + binds)
}

#[derive(Debug)]
pub struct EndpointIr {
    pub name: Ident,
    pub method: Ident,
    pub route: LitStr,
    pub route_pieces: Vec<PathPiece>,

    pub ancestry: Vec<usize>, // layer ids in nesting order (outer -> inner)

    pub vars: Vec<VarInfo>, // endpoint vars (union, stable)
    pub body: Option<CodecSpec>,
    pub response: CodecSpec,

    pub policy: PolicyBlocksResolved,

    pub paginate: Option<PaginateResolved>,
    pub map: Option<MapResolved>,
}

#[derive(Debug, Clone)]
pub enum PrefixPiece {
    Static(String),
    Var {
        wire: String,
        field: Ident,
        optional: bool,
    },
}

#[derive(Debug, Clone)]
pub enum PathPiece {
    Static(String),
    Var {
        field: Ident,
        optional: bool,
    },
}

#[derive(Debug, Default)]
pub struct PolicyBlocksResolved {
    pub headers: Vec<PolicyOp>,
    pub query: Vec<PolicyOp>,
    pub timeout: Option<ValueKind>,
}

#[derive(Debug, Clone)]
pub enum PolicyOp {
    Remove { key: KeyResolved, kind: PolicyKeyKind },
    Set {
        key: KeyResolved,
        kind: PolicyKeyKind,
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

    // client policy: collect client vars from binds in client headers/query
    let mut client_vars_map: BTreeMap<String, VarInfo> = BTreeMap::new();

    collect_client_binds(&ast.client.policy, &mut client_vars_map)?;

    // validate client policy + resolve
    let client_policy = resolve_policy_blocks(
        &ast.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        None,
    )?;

    let client_vars: Vec<VarInfo> = client_vars_map.values().cloned().collect();

    // walk layers/endpoints
    let mut layers: Vec<LayerIr> = Vec::new();
    let mut endpoints: Vec<EndpointIr> = Vec::new();

    let mut ancestry: Vec<usize> = Vec::new();
    walk_items(
        &ast.items,
        &mut ancestry,
        &client_vars_map,
        &mut layers,
        &mut endpoints,
    )?;

    Ok(Ir {
        mod_name,
        client_name,
        scheme: ast.client.scheme,
        domain: ast.client.host,
        client_vars,
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
                    upsert_var(out, &decl.rust, decl.optional, &decl.ty, decl.default.as_ref())?;
                }
                PolicyStmt::BindShort { ident_key, decl } => {
                    upsert_var(out, ident_key, decl.optional, &decl.ty, decl.default.as_ref())?;
                }
                _ => {}
            }
        }
    }

    // nouveau: fmt decls
    for blk in policy.headers.iter().chain(policy.query.iter()) {
        for stmt in &blk.stmts {
            if let PolicyStmt::Set { value, .. } = stmt {
                if let crate::ast::PolicyValue::Fmt(fmt) = value {
                    for p in &fmt.pieces {
                        if let crate::ast::FmtPiece::Var(d) = p {
                            upsert_var(out, &d.rust, d.optional, &d.ty, d.default.as_ref())?;
                        }
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
            if let crate::ast::PolicyStmt::Set { value, .. } = stmt {
                if let crate::ast::PolicyValue::Fmt(fmt) = value {
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
        if prev.default.is_some() && default.is_some() && prev.default.as_ref().unwrap() != default.unwrap() {
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

fn walk_items(
    items: &[Item],
    ancestry: &mut Vec<usize>,
    client_vars: &BTreeMap<String, VarInfo>,
    layers: &mut Vec<LayerIr>,
    endpoints: &mut Vec<EndpointIr>,
) -> Result<()> {
    for it in items {
        match it {
            Item::Layer(ld) => {
                let id = layers.len();
                let (prefix_pieces, path_pieces, decls) = analyze_layer_template_and_decls(ld)?;

                let policy = resolve_policy_blocks(
                    &ld.policy,
                    PolicyOwner::Layer,
                    client_vars,
                    None, // endpoint vars not known at layer-level alone (validated per endpoint)
                )?;

                layers.push(LayerIr {
                    id,
                    kind: ld.kind,
                    template: ld.template.clone(),
                    prefix_pieces,
                    path_pieces,
                    policy,
                    decls,
                });

                ancestry.push(id);
                walk_items(&ld.items, ancestry, client_vars, layers, endpoints)?;
                ancestry.pop();
            }
            Item::Endpoint(ed) => {
                let endpoint_ir = analyze_endpoint(ed, ancestry, client_vars, layers)?;
                endpoints.push(endpoint_ir);
            }
        }
    }
    Ok(())
}

fn analyze_layer_template_and_decls(ld: &LayerDef) -> Result<(Vec<PrefixPiece>, Vec<PathPiece>, Vec<VarInfo>)> {
    let span = ld.template.span();
    let s = ld.template.value();
    let mut decls: Vec<VarInfo> = Vec::new();

    let (prefix_pieces, path_pieces) = match ld.kind {
        LayerKind::Prefix => {
            let pieces = parse_prefix_template(&s, span)?;
            (pieces, Vec::new())
        }
        LayerKind::Path => {
            let pieces = parse_path_template(&s, span)?;
            (Vec::new(), pieces)
        }
    };

    // Collect declared vars from placeholders WITH full type/default by parsing again.
    // Prefix:
    if ld.kind == LayerKind::Prefix {
        for part in s.split('.') {
            let part = part.trim();
            if is_braced(part) {
                let decl = parse_template_var_decl_str(strip_braces(part), span)?;
                decls.push(VarInfo {
                    rust: decl.rust,
                    optional: decl.optional,
                    ty: decl.ty,
                    default: decl.default,
                });
            }
        }
    } else {
        for part in s.split('/') {
            let part = part.trim();
            if is_braced(part) {
                let decl = parse_template_var_decl_str(strip_braces(part), span)?;
                decls.push(VarInfo {
                    rust: decl.rust,
                    optional: decl.optional,
                    ty: decl.ty,
                    default: decl.default,
                });
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
    client_vars: &BTreeMap<String, VarInfo>,
    layers: &[LayerIr],
) -> Result<EndpointIr> {
    let span = ed.route.span();
    let route_str = ed.route.value();
    let route_pieces = parse_path_template(&route_str, span)?;

    // Build endpoint var registry: collect vars from all ancestry layer decls + endpoint route decls + endpoint binds
    let mut ep_vars: BTreeMap<String, VarInfo> = BTreeMap::new();

    // ancestry decls
    for &lid in ancestry {
        for v in &layers[lid].decls {
            upsert_var(&mut ep_vars, &v.rust, v.optional, &v.ty, v.default.as_ref())?;
        }
    }

    // endpoint route placeholders decls
    for part in route_str.split('/') {
        let part = part.trim();
        if is_braced(part) {
            let decl = parse_template_var_decl_str(strip_braces(part), span)?;
            upsert_var(&mut ep_vars, &decl.rust, decl.optional, &decl.ty, decl.default.as_ref())?;
        }
    }

    // endpoint binds decls
    for blk in ed.policy.headers.iter().chain(ed.policy.query.iter()) {
        for stmt in &blk.stmts {
            match stmt {
                PolicyStmt::Bind { decl, .. } => {
                    upsert_var(&mut ep_vars, &decl.rust, decl.optional, &decl.ty, decl.default.as_ref())?;
                }
                PolicyStmt::BindShort { ident_key, decl } => {
                    upsert_var(&mut ep_vars, ident_key, decl.optional, &decl.ty, decl.default.as_ref())?;
                }
                _ => {}
            }
        }
    }

    {
        let mut fmt_decls: Vec<VarInfo> = Vec::new();
        collect_policy_fmt_decls(&ed.policy, &mut fmt_decls);
        for v in fmt_decls {
            upsert_var(&mut ep_vars, &v.rust, v.optional, &v.ty, v.default.as_ref())?;
        }
    }
    // resolve policy blocks (now we can validate ep refs)
    let policy = resolve_policy_blocks(&ed.policy, PolicyOwner::Endpoint, client_vars, Some(&ep_vars))?;

    // pagination
    let paginate = match &ed.paginate {
        None => None,
        Some(p) => Some(resolve_paginate(p, client_vars, &ep_vars)?),
    };

    // map closure requires explicit `-> OutTy`
    let map = ed.map.as_ref().map(|m| MapResolved {
            out_ty: m.out_ty.clone(),
            body: m.body.clone(),
        });

    Ok(EndpointIr {
        name: ed.name.clone(),
        method: ed.method.clone(),
        route: ed.route.clone(),
        route_pieces,
        ancestry: ancestry.to_vec(),
        vars: ep_vars.values().cloned().collect(),
        body: ed.body.clone(),
        response: ed.response.clone(),
        policy,
        paginate,
        map,
    })
}

fn resolve_paginate(
    p: &PaginateSpec,
    client_vars: &BTreeMap<String, VarInfo>,
    ep_vars: &BTreeMap<String, VarInfo>,
) -> Result<PaginateResolved> {
    let mut assigns = Vec::new();
    for a in &p.assigns {
        let vk = resolve_value_kind(&a.value, client_vars, Some(ep_vars), a.value.span())?;
        // rule: forbid `cx.` in pagination (controller config must not depend on runtime client vars)
        if matches!(vk, ValueKind::CxField(_)) {
            return Err(syn::Error::new(
                a.value.span(),
                "paginate assignments must not reference `cx.*`; use `ep.*` or constants",
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
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<PolicyBlocksResolved> {
    let mut out = PolicyBlocksResolved::default();

    if let Some(h) = &policy.headers {
        out.headers = resolve_policy_block(h, PolicyKeyKind::Header, owner, client_vars, endpoint_vars)?;
    }
    if let Some(q) = &policy.query {
        out.query = resolve_policy_block(q, PolicyKeyKind::Query, owner, client_vars, endpoint_vars)?;
    }
    if let Some(t) = &policy.timeout {
        // timeout expr must not contain nested cx/ep; allow `cx.x` or `ep.y` only as root
        if emit_helpers::contains_cx_or_ep(t) && emit_helpers::is_cx_field(t).is_none() && emit_helpers::is_ep_field(t).is_none() {
            return Err(syn::Error::new(
                t.span(),
                "timeout expression cannot contain nested `cx`/`ep`; use a plain `cx.x`, `ep.y`, or a pure expression without them",
            ));
        }
        out.timeout = Some(resolve_value_kind(t, client_vars, endpoint_vars, t.span())?);
    }

    Ok(out)
}

fn resolve_policy_block(
    blk: &PolicyBlock,
    kind: PolicyKeyKind,
    owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Vec<PolicyOp>> {
    let mut ops = Vec::new();

    for stmt in &blk.stmts {
        match stmt {
            PolicyStmt::Remove { key } => {
                ops.push(PolicyOp::Remove {
                    key: resolve_key(key),
                    kind,
                });
            }
            PolicyStmt::Set { key, value, op } => {
                if kind == PolicyKeyKind::Header && *op == SetOp::Push {
                    return Err(syn::Error::new(value.span(), "`+=` is not allowed in headers; only in query"));
                }
                let vk = resolve_policy_value_kind(value, owner, client_vars, endpoint_vars, value.span())?;

                // Optional-ref conditional set/remove for pure cx/ep refs
                let cond = match &vk {
                    ValueKind::CxField(id) => {
                        let v = client_vars.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(id.span(), format!("unknown client var `cx.{}`", id))
                        })?;
                        if v.optional { Some(OptionalRefKind::Cx) } else { None }
                    }
                    ValueKind::EpField(id) => {
                        let ep = endpoint_vars.ok_or_else(|| syn::Error::new(id.span(), "ep.* is not allowed here"))?;
                        let v = ep.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(id.span(), format!("unknown endpoint var `ep.{}`", id))
                        })?;
                        if v.optional { Some(OptionalRefKind::Ep) } else { None }
                    }
                    _ => None,
                };

                ops.push(PolicyOp::Set {
                    key: resolve_key(key),
                    kind,
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
                && matches!(value, ValueKind::EpField(_)) {
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
                        return Err(syn::Error::new(id.span(), format!("unknown client var `cx.{}`", id)));
                    }
                }
                ValueKind::EpField(id) => {
                    let ep = endpoint_vars.ok_or_else(|| syn::Error::new(id.span(), "`ep.*` is not allowed here"))?;
                    if !ep.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(id.span(), format!("unknown endpoint var `ep.{}`", id)));
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
        PolicyStmt::Set { key: _, value, op: _ } => value.span(),
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
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    _span: Span,
) -> Result<ValueKind> {
    if let Expr::Lit(l) = expr
        && let syn::Lit::Str(s) = &l.lit {
            return Ok(ValueKind::LitStr(s.clone()));
        }

    if let Some(id) = emit_helpers::is_cx_field(expr) {
        // validate later at block-level
        let _ = client_vars;
        return Ok(ValueKind::CxField(id));
    }
    if let Some(id) = emit_helpers::is_ep_field(expr) {
        let _ = endpoint_vars;
        return Ok(ValueKind::EpField(id));
    }

    Ok(ValueKind::OtherExpr(expr.clone()))
}


fn resolve_policy_value_kind(
    v: &crate::ast::PolicyValue,
    owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    span: proc_macro2::Span,
) -> Result<ValueKind> {
    match v {
        crate::ast::PolicyValue::Expr(e) => resolve_value_kind(e, client_vars, endpoint_vars, span),
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
                                    return Err(syn::Error::new(d.rust.span(), format!("unknown client var `{}`", d.rust)));
                                }
                            }
                            PolicyOwner::Endpoint => {
                                let ep = endpoint_vars.ok_or_else(|| syn::Error::new(d.rust.span(), "ep vars not available"))?;
                                if !ep.contains_key(&d.rust.to_string()) {
                                    return Err(syn::Error::new(d.rust.span(), format!("unknown endpoint var `{}`", d.rust)));
                                }
                            }
                            PolicyOwner::Layer => {
                                // layer-level: pas forcément de registry complet ici; l’existence est garantie
                                // par l’injection dans decls + union endpoint.
                            }
                        }

                        pieces.push(FmtResolvedPiece::Var {
                            field: d.rust.clone(),
                            optional: d.optional,
                        });
                    }
                }
            }

            if !fmt.require_all && has_optional {
                return Err(syn::Error::new(span, "fmt[...] forbids optional placeholders; use fmt?[...]"));
            }

            Ok(ValueKind::Fmt(FmtResolved {
                require_all: fmt.require_all,
                pieces,
            }))
        }
    }
}



fn parse_prefix_template(s: &str, span: Span) -> Result<Vec<PrefixPiece>> {
    let parts: Vec<&str> = s.split('.').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return Err(syn::Error::new(span, "empty prefix template"));
    }
    let mut out = Vec::new();
    for part in parts {
        if part.is_empty() {
            return Err(syn::Error::new(span, "empty label in prefix template"));
        }
        if is_braced(part) {
            let decl = parse_template_var_decl_str(strip_braces(part), span)?;
            out.push(PrefixPiece::Var {
                wire: decl.wire.to_string(),
                field: decl.rust,
                optional: decl.optional,
            });
        } else {
            out.push(PrefixPiece::Static(part.to_string()));
        }
    }
    Ok(out)
}

fn parse_path_template(s: &str, span: Span) -> Result<Vec<PathPiece>> {
    let raw = s.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = raw.split('/').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    let mut out = Vec::new();
    for part in parts {
        if is_braced(part) {
            let decl = parse_template_var_decl_str(strip_braces(part), span)?;
            out.push(PathPiece::Var {
                field: decl.rust,
                optional: decl.optional,
            });
        } else {
            out.push(PathPiece::Static(part.to_string()));
        }
    }
    Ok(out)
}

fn is_braced(s: &str) -> bool {
    s.starts_with('{') && s.ends_with('}') && s.len() >= 2
}

fn strip_braces(s: &str) -> &str {
    &s[1..s.len() - 1]
}

fn parse_template_var_decl_str(s: &str, span: Span) -> Result<TemplateVarDecl> {
    // Parse with syn from a string token stream.
    let ts = proc_macro2::TokenStream::from_str(s)
        .map_err(|e| syn::Error::new(span, format!("invalid placeholder decl: {e}")))?;
    syn::parse2::<TemplateVarDecl>(ts)
        .map_err(|e| syn::Error::new(span, format!("invalid placeholder decl: {e}")))
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
    Var { field: syn::Ident, optional: bool },
}
