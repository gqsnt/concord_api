use super::*;

pub(super) fn lower_public_policy_expr_checked(expr: &Expr) -> Result<Expr> {
    reject_direct_secret_expr(expr)?;
    Ok(lower_public_policy_expr(expr.clone()))
}

pub(super) fn lower_public_policy_expr(expr: Expr) -> Expr {
    match expr {
        Expr::Path(p) => lower_public_policy_expr_path(p),
        Expr::Field(mut f) => {
            if let Expr::Path(base_path) = &*f.base
                && base_path.qself.is_none()
                && base_path.path.segments.len() == 1
                && emit_helpers::is_public_expr_reserved_root(&base_path.path.segments[0].ident)
            {
                return Expr::Field(f);
            }
            f.base = Box::new(lower_public_policy_expr(*f.base));
            Expr::Field(f)
        }
        Expr::Cast(mut c) => {
            c.expr = Box::new(lower_public_policy_expr(*c.expr));
            Expr::Cast(c)
        }
        Expr::Paren(mut p) => {
            p.expr = Box::new(lower_public_policy_expr(*p.expr));
            Expr::Paren(p)
        }
        Expr::Reference(mut r) => {
            r.expr = Box::new(lower_public_policy_expr(*r.expr));
            Expr::Reference(r)
        }
        Expr::Unary(mut u) => {
            u.expr = Box::new(lower_public_policy_expr(*u.expr));
            Expr::Unary(u)
        }
        Expr::Binary(mut b) => {
            b.left = Box::new(lower_public_policy_expr(*b.left));
            b.right = Box::new(lower_public_policy_expr(*b.right));
            Expr::Binary(b)
        }
        other => other,
    }
}

pub(super) fn lower_public_policy_expr_path(path: syn::ExprPath) -> Expr {
    if path.qself.is_none() && path.path.segments.len() == 1 {
        let seg = &path.path.segments[0];
        let id = &seg.ident;
        if !emit_helpers::is_public_expr_reserved_root(id)
            && id
                .to_string()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
        {
            return syn::parse_quote!(ep.#id);
        }
    }
    Expr::Path(path)
}

pub(super) fn reject_direct_secret_expr(expr: &Expr) -> Result<()> {
    match expr {
        Expr::Field(field) => {
            if let Expr::Path(base_path) = &*field.base
                && base_path.qself.is_none()
                && base_path.path.segments.len() == 1
                && base_path.path.segments[0].ident == "secret"
            {
                return Err(syn::Error::new(
                    field.member.span(),
                    "DSL-010 direct `secret.*` is not allowed in policy expressions; declare an auth credential",
                ));
            }
            reject_direct_secret_expr(&field.base)
        }
        Expr::Path(path)
            if path.qself.is_none()
                && path
                    .path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "secret") =>
        {
            Err(syn::Error::new(
                path.path.segments[0].ident.span(),
                "DSL-010 direct `secret.*` is not allowed in policy expressions; declare an auth credential",
            ))
        }
        Expr::Cast(cast) => reject_direct_secret_expr(&cast.expr),
        Expr::Paren(paren) => reject_direct_secret_expr(&paren.expr),
        Expr::Reference(reference) => reject_direct_secret_expr(&reference.expr),
        Expr::Unary(unary) => reject_direct_secret_expr(&unary.expr),
        Expr::Binary(binary) => {
            reject_direct_secret_expr(&binary.left)?;
            reject_direct_secret_expr(&binary.right)
        }
        Expr::MethodCall(call) => {
            reject_direct_secret_expr(&call.receiver)?;
            for arg in &call.args {
                reject_direct_secret_expr(arg)?;
            }
            Ok(())
        }
        Expr::Call(call) => {
            reject_direct_secret_expr(&call.func)?;
            for arg in &call.args {
                reject_direct_secret_expr(arg)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn resolve_paginate(
    p: &PaginateSpec,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    ep_vars: &BTreeMap<String, VarInfo>,
) -> Result<PaginateResolved> {
    let mut assigns = Vec::new();
    let mut bindings = Vec::new();
    for a in &p.assigns {
        let lowered = lower_public_policy_expr_checked(&a.value)?;
        let vk = resolve_value_kind(
            &lowered,
            client_vars,
            auth_vars,
            Some(ep_vars),
            lowered.span(),
        )?;
        let vk = pagination_value_from_value_kind(vk, lowered.span())?;
        if let PaginationValueKind::EpField(endpoint_field) = &vk {
            let endpoint_info = ep_vars.get(&endpoint_field.to_string()).ok_or_else(|| {
                syn::Error::new(
                    endpoint_field.span(),
                    unknown_scoped_name_message("endpoint var", "ep", endpoint_field, ep_vars),
                )
            })?;
            bindings.push(PaginationBindingIr {
                controller_field: a.key.clone(),
                endpoint_rust_field: endpoint_info.rust.clone(),
            });
        }
        assigns.push(PaginationAssignmentResolved {
            field: a.key.clone(),
            value: vk,
        });
    }
    Ok(PaginateResolved {
        controller_ty: p.ctrl_ty.clone(),
        assigns,
        bindings,
    })
}

pub(super) fn resolve_policy_blocks(
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
        let lowered = lower_public_policy_expr_checked(t)?;
        let timeout = resolve_value_kind(
            &lowered,
            client_vars,
            auth_vars,
            endpoint_vars,
            lowered.span(),
        )?;
        out.timeout = Some(public_value_from_value_kind(timeout, lowered.span())?);
    }

    Ok(out)
}

pub(super) fn resolve_policy_block(
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
            PolicyStmt::Set { key, value } => {
                let lowered_value = match value {
                    PolicyValue::Expr(expr) => {
                        PolicyValue::Expr(lower_public_policy_expr_checked(expr)?)
                    }
                    PolicyValue::Fmt(fmt) => PolicyValue::Fmt(fmt.clone()),
                };
                let vk = resolve_policy_value_kind(
                    &lowered_value,
                    owner,
                    client_vars,
                    auth_vars,
                    endpoint_vars,
                    lowered_value.span(),
                )?;

                let set_value = match vk {
                    ValueKind::CxField(id) => {
                        let v = client_vars.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(
                                id.span(),
                                unknown_scoped_name_message("client var", "vars", &id, client_vars),
                            )
                        })?;
                        if v.optional {
                            PolicySetValue::OptionalCxField(id)
                        } else {
                            PolicySetValue::Value(PublicValueKind::CxField(id))
                        }
                    }
                    ValueKind::EpField(id) => {
                        let ep = endpoint_vars.ok_or_else(|| {
                            syn::Error::new(id.span(), "ep.* is not allowed here")
                        })?;
                        let v = ep.get(&id.to_string()).ok_or_else(|| {
                            syn::Error::new(
                                id.span(),
                                unknown_scoped_name_message("endpoint var", "ep", &id, ep),
                            )
                        })?;
                        if v.optional {
                            PolicySetValue::OptionalEpField(id)
                        } else {
                            PolicySetValue::Value(PublicValueKind::EpField(id))
                        }
                    }
                    other => PolicySetValue::Value(public_value_from_value_kind(
                        other,
                        lowered_value.span(),
                    )?),
                };

                let cardinality = match &set_value {
                    PolicySetValue::OptionalCxField(id) => client_vars
                        .get(&id.to_string())
                        .map(|v| query_cardinality(kind, true, &v.ty))
                        .unwrap_or(QueryValueCardinality::OptionalScalar),
                    PolicySetValue::OptionalEpField(id) => endpoint_vars
                        .and_then(|vars| vars.get(&id.to_string()))
                        .map(|v| query_cardinality(kind, true, &v.ty))
                        .unwrap_or(QueryValueCardinality::OptionalScalar),
                    PolicySetValue::Value(PublicValueKind::CxField(id)) => client_vars
                        .get(&id.to_string())
                        .map(|v| query_cardinality(kind, false, &v.ty))
                        .unwrap_or(QueryValueCardinality::Scalar),
                    PolicySetValue::Value(PublicValueKind::EpField(id)) => endpoint_vars
                        .and_then(|vars| vars.get(&id.to_string()))
                        .map(|v| query_cardinality(kind, false, &v.ty))
                        .unwrap_or(QueryValueCardinality::Scalar),
                    _ => QueryValueCardinality::Scalar,
                };

                ops.push(PolicyOp::Set {
                    key: resolve_key(key),
                    value: set_value,
                    cardinality,
                });
            }
        }
    }

    // validate references to ep in non-endpoint contexts
    if owner == PolicyOwner::Client {
        for op in &ops {
            if let PolicyOp::Set { value, .. } = op
                && matches!(
                    value,
                    PolicySetValue::Value(PublicValueKind::EpField(_))
                        | PolicySetValue::OptionalEpField(_)
                )
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
                PolicySetValue::Value(PublicValueKind::CxField(id))
                | PolicySetValue::OptionalCxField(id) => {
                    if !client_vars.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            unknown_scoped_name_message("client var", "vars", id, client_vars),
                        ));
                    }
                }
                PolicySetValue::Value(PublicValueKind::EpField(id))
                | PolicySetValue::OptionalEpField(id) => {
                    let ep = endpoint_vars
                        .ok_or_else(|| syn::Error::new(id.span(), "`ep.*` is not allowed here"))?;
                    if !ep.contains_key(&id.to_string()) {
                        return Err(syn::Error::new(
                            id.span(),
                            unknown_scoped_name_message("endpoint var", "ep", id, ep),
                        ));
                    }
                }
                PolicySetValue::Value(PublicValueKind::OtherExpr(e)) => {
                    validate_public_expr(e)?;
                }
                PolicySetValue::Value(PublicValueKind::LitStr(lit))
                    if kind == PolicyKeyKind::Header =>
                {
                    if http::HeaderValue::from_str(&lit.value()).is_err() {
                        return Err(syn::Error::new(
                            lit.span(),
                            "header value literal is not a valid HTTP header value",
                        ));
                    }
                }
                PolicySetValue::Value(PublicValueKind::LitStr(_) | PublicValueKind::Fmt(_)) => {}
            }
        }
    }

    if kind == PolicyKeyKind::Header {
        reject_duplicate_header_sets(&ops)?;
    }

    Ok(ops)
}

fn query_cardinality(kind: PolicyKeyKind, optional: bool, ty: &Type) -> QueryValueCardinality {
    if kind != PolicyKeyKind::Query {
        return if optional {
            QueryValueCardinality::OptionalScalar
        } else {
            QueryValueCardinality::Scalar
        };
    }
    if is_standard_vec_type(ty) {
        if optional {
            QueryValueCardinality::OptionalVector
        } else {
            QueryValueCardinality::Vector
        }
    } else if optional {
        QueryValueCardinality::OptionalScalar
    } else {
        QueryValueCardinality::Scalar
    }
}

pub(super) fn is_standard_vec_type(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    if path.qself.is_some() {
        return false;
    }
    let segments = &path.path.segments;
    let is_standard_prefix = match segments.len() {
        1 => path.path.leading_colon.is_none() && segments[0].ident == "Vec",
        3 => {
            matches!(segments[0].arguments, syn::PathArguments::None)
                && matches!(segments[1].arguments, syn::PathArguments::None)
                && segments[0].ident == "std"
                && segments[1].ident == "vec"
                && segments[2].ident == "Vec"
        }
        _ => false,
    };
    if !is_standard_prefix {
        return false;
    }
    let last = segments.last().expect("standard Vec path has a segment");
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    args.args.len() == 1 && matches!(args.args.first(), Some(syn::GenericArgument::Type(_)))
}

pub(super) fn collect_endpoint_query_cardinalities(
    scopes: &[PolicyBlocksResolved],
    endpoint: &PolicyBlocksResolved,
) -> BTreeMap<String, QueryValueCardinality> {
    let mut out = BTreeMap::new();
    for policy in scopes.iter().chain(std::iter::once(endpoint)) {
        for op in &policy.query {
            let PolicyOp::Set {
                value, cardinality, ..
            } = op
            else {
                continue;
            };
            let field = match value {
                PolicySetValue::Value(PublicValueKind::EpField(field))
                | PolicySetValue::OptionalEpField(field) => Some(field),
                _ => None,
            };
            if let Some(field) = field {
                out.insert(field.to_string(), *cardinality);
            }
        }
    }
    out
}

pub(super) fn collect_client_query_cardinalities(
    client_policy: &PolicyBlocksResolved,
    endpoints: &[ResolvedEndpoint],
) -> BTreeMap<String, QueryValueCardinality> {
    let mut out = BTreeMap::new();
    let policies = std::iter::once(client_policy).chain(endpoints.iter().flat_map(|endpoint| {
        endpoint
            .policy
            .scopes
            .iter()
            .chain(std::iter::once(&endpoint.policy.endpoint))
    }));
    for policy in policies {
        for op in &policy.query {
            let PolicyOp::Set {
                value, cardinality, ..
            } = op
            else {
                continue;
            };
            let field = match value {
                PolicySetValue::Value(PublicValueKind::CxField(field))
                | PolicySetValue::OptionalCxField(field) => Some(field),
                _ => None,
            };
            if let Some(field) = field {
                out.insert(field.to_string(), *cardinality);
            }
        }
    }
    out
}

pub(super) fn reject_duplicate_header_sets(ops: &[PolicyOp]) -> Result<()> {
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    for op in ops {
        let PolicyOp::Set { key, .. } = op else {
            continue;
        };
        let normalized = header_key_for_duplicate_check(key);
        if seen
            .insert(normalized.clone(), key_resolved_span(key))
            .is_some()
        {
            return Err(syn::Error::new(
                key_resolved_span(key),
                format!("duplicate header `{normalized}` in the same policy layer"),
            ));
        }
    }
    Ok(())
}

pub(super) fn header_key_for_duplicate_check(key: &KeyResolved) -> String {
    match key {
        KeyResolved::Static(lit) => lit.value().to_ascii_lowercase(),
        KeyResolved::Ident(ident) => emit_helpers::to_kebab(ident).to_ascii_lowercase(),
    }
}

pub(super) fn key_resolved_span(key: &KeyResolved) -> Span {
    match key {
        KeyResolved::Static(lit) => lit.span(),
        KeyResolved::Ident(ident) => ident.span(),
    }
}

pub(super) fn key_spec_span(k: &KeySpec) -> Span {
    match k {
        KeySpec::Ident(id) => id.span(),
        KeySpec::Str(s) => s.span(),
    }
}

pub(super) fn policy_stmt_span(s: &PolicyStmt) -> Span {
    match s {
        PolicyStmt::Remove { key } => key_spec_span(key),
        PolicyStmt::Set { key: _, value } => value.span(),
    }
}

pub(super) fn resolve_key(k: &KeySpec) -> KeyResolved {
    match k {
        KeySpec::Ident(id) => KeyResolved::Ident(id.clone()),
        KeySpec::Str(s) => KeyResolved::Static(s.clone()),
    }
}

pub(super) fn resolve_value_kind(
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
    let _ = auth_vars;
    if let Some(id) = emit_helpers::is_ep_field(expr) {
        let _ = endpoint_vars;
        return Ok(ValueKind::EpField(id));
    }

    Ok(ValueKind::OtherExpr(expr.clone()))
}

pub(super) fn resolve_route_fmt_spec(
    spec: &FmtSpec,
    client_vars: Option<&BTreeMap<String, VarInfo>>,
    ep_vars: Option<&BTreeMap<String, VarInfo>>,
    allow_explicit_ep: bool,
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
                            let msg = client_vars.map_or_else(
                                || format!("unknown client var `vars.{}`", r.ident),
                                |vars| {
                                    unknown_scoped_name_message(
                                        "client var",
                                        "vars",
                                        &r.ident,
                                        vars,
                                    )
                                },
                            );
                            syn::Error::new(r.ident.span(), msg)
                        })?;
                    pieces.push(FmtResolvedPiece::Var {
                        source: FmtVarSource::Cx,
                        field: r.ident.clone(),
                        optional: cv.optional,
                    });
                }
                RefScope::Ep => {
                    if r.explicit && !allow_explicit_ep {
                        return Err(syn::Error::new(
                            r.ident.span(),
                            "`ep.*` is not allowed in scope route fmt[...] specs; use the scope parameter name directly",
                        ));
                    }
                    let vars = ep_vars.ok_or_else(|| {
                        syn::Error::new(r.ident.span(), "`ep.*` is not allowed here")
                    })?;
                    let ev = vars.get(&r.ident.to_string()).ok_or_else(|| {
                        syn::Error::new(
                            r.ident.span(),
                            unknown_scoped_name_message("endpoint var", "ep", &r.ident, vars),
                        )
                    })?;
                    pieces.push(FmtResolvedPiece::Var {
                        source: FmtVarSource::Ep,
                        field: r.ident.clone(),
                        optional: ev.optional,
                    });
                }
                RefScope::Auth => {
                    return Err(syn::Error::new(
                        r.ident.span(),
                        "secret references are only allowed in credential declarations",
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

pub(super) fn resolve_policy_value_kind(
    v: &PolicyValue,
    _owner: PolicyOwner,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    span: proc_macro2::Span,
) -> Result<ValueKind> {
    match v {
        PolicyValue::Expr(e) => resolve_value_kind(e, client_vars, auth_vars, endpoint_vars, span),
        PolicyValue::Fmt(fmt) => {
            let mut pieces: Vec<FmtResolvedPiece> = Vec::new();
            let mut has_optional = false;

            for p in &fmt.pieces {
                match p {
                    FmtPiece::Lit(s) => pieces.push(FmtResolvedPiece::Lit(s.clone())),
                    FmtPiece::Ref(r) => match r.scope {
                        RefScope::Cx => {
                            let v = client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                syn::Error::new(
                                    r.ident.span(),
                                    unknown_scoped_name_message(
                                        "client var",
                                        "vars",
                                        &r.ident,
                                        client_vars,
                                    ),
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
                                    unknown_scoped_name_message("endpoint var", "ep", &r.ident, ep),
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
                            let _ = auth_vars;
                            return Err(direct_secret_policy_error(r.ident.span()));
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

pub(super) fn public_value_from_value_kind(
    value: ValueKind,
    _span: Span,
) -> Result<PublicValueKind> {
    match value {
        ValueKind::LitStr(value) => Ok(PublicValueKind::LitStr(value)),
        ValueKind::CxField(value) => Ok(PublicValueKind::CxField(value)),
        ValueKind::EpField(value) => Ok(PublicValueKind::EpField(value)),
        ValueKind::OtherExpr(value) => {
            validate_public_expr(&value)?;
            Ok(PublicValueKind::OtherExpr(value))
        }
        ValueKind::Fmt(value) => Ok(PublicValueKind::Fmt(value)),
    }
}

pub(super) fn pagination_value_from_value_kind(
    value: ValueKind,
    span: Span,
) -> Result<PaginationValueKind> {
    match value {
        ValueKind::LitStr(value) => Ok(PaginationValueKind::LitStr(value)),
        ValueKind::EpField(value) => Ok(PaginationValueKind::EpField(value)),
        ValueKind::OtherExpr(value) => {
            validate_public_expr(&value)?;
            Ok(PaginationValueKind::OtherExpr(value))
        }
        ValueKind::Fmt(value) => {
            if let Some(field) = value.pieces.iter().find_map(|piece| match piece {
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Cx,
                    field,
                    ..
                } => Some(field),
                FmtResolvedPiece::Lit(_)
                | FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    ..
                } => None,
            }) {
                return Err(pagination_scoped_ref_error(field.span()));
            }
            Ok(PaginationValueKind::Fmt(value))
        }
        ValueKind::CxField(_) => Err(pagination_scoped_ref_error(span)),
    }
}

pub(super) fn validate_public_expr(expr: &Expr) -> Result<()> {
    if let Some(found) = emit_helpers::public_expr_forbidden(expr)? {
        return Err(public_expr_forbidden_error(found));
    }
    Ok(())
}

pub(super) fn public_expr_forbidden_error(found: emit_helpers::PublicExprForbidden) -> syn::Error {
    let msg = match found.kind {
        emit_helpers::PublicExprForbiddenKind::Auth => {
            "auth references are not allowed in public policy expressions; use an auth declaration/use instead".to_string()
        }
        emit_helpers::PublicExprForbiddenKind::Secret => {
            "secret references are only allowed in credential declarations".to_string()
        }
        emit_helpers::PublicExprForbiddenKind::GeneratedLocal => {
            format!(
                "generated implementation local `{}` is not part of the public DSL expression scope",
                found.ident
            )
        }
        emit_helpers::PublicExprForbiddenKind::SecretExposure => {
            "secret exposure methods are not allowed in public policy expressions".to_string()
        }
    };
    syn::Error::new(found.span, msg)
}

pub(super) fn direct_secret_policy_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "direct secret.* is not allowed in policy expressions; declare an auth credential",
    )
}

pub(super) fn pagination_scoped_ref_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "paginate assignments must not reference client variables or secrets; use `ep.*` or constants",
    )
}

#[cfg(test)]
mod pagination_value_tests {
    use super::*;

    fn lit(value: &str) -> syn::LitStr {
        syn::LitStr::new(value, Span::call_site())
    }

    #[test]
    fn pagination_fmt_rejects_client_vars() {
        let value = ValueKind::Fmt(FmtResolved {
            require_all: false,
            pieces: vec![
                FmtResolvedPiece::Lit(lit("page-")),
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Cx,
                    field: emit_helpers::ident("cursor", Span::call_site()),
                    optional: false,
                },
            ],
        });

        let err = pagination_value_from_value_kind(value, Span::call_site()).unwrap_err();

        assert!(
            err.to_string()
                .contains("paginate assignments must not reference client variables or secrets"),
            "{err}"
        );
    }

    #[test]
    fn pagination_fmt_allows_endpoint_vars() {
        let value = ValueKind::Fmt(FmtResolved {
            require_all: false,
            pieces: vec![
                FmtResolvedPiece::Lit(lit("page-")),
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    field: emit_helpers::ident("cursor", Span::call_site()),
                    optional: false,
                },
            ],
        });

        assert!(matches!(
            pagination_value_from_value_kind(value, Span::call_site()),
            Ok(PaginationValueKind::Fmt(_))
        ));
    }

    #[test]
    fn pagination_expr_rejects_nested_client_vars() {
        let expr: syn::Expr = syn::parse_quote! { format!("{}", cx.cursor) };

        let err = pagination_value_from_value_kind(ValueKind::OtherExpr(expr), Span::call_site())
            .unwrap_err();

        assert!(
            err.to_string().contains(
                "generated implementation local `cx` is not part of the public DSL expression scope"
            ),
            "{err}"
        );
    }

    #[test]
    fn pagination_expr_rejects_nested_auth_vars() {
        let expr: syn::Expr = syn::parse_quote! { format!("{}", auth.cursor) };

        let err = pagination_value_from_value_kind(ValueKind::OtherExpr(expr), Span::call_site())
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("auth references are not allowed in public policy expressions"),
            "{err}"
        );
    }

    #[test]
    fn standard_vec_classifier_accepts_only_supported_shapes() {
        for source in [
            "Vec<String>",
            "std::vec::Vec<String>",
            "::std::vec::Vec<String>",
        ] {
            let ty: Type = syn::parse_str(source).expect("valid vector type");
            assert!(is_standard_vec_type(&ty), "expected standard Vec: {source}");
        }

        for source in [
            "custom::Vec<String>",
            "crate::Vec<String>",
            "foo::bar::Vec<String>",
            "std::<u8>::vec::Vec<String>",
            "Vec<String, u8>",
            "Vec<'a>",
            "Vec<3>",
            "::Vec<String>",
        ] {
            let ty: Type = syn::parse_str(source).expect("valid rejected type shape");
            assert!(
                !is_standard_vec_type(&ty),
                "unexpected standard Vec: {source}"
            );
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
}
