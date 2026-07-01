fn lower_public_policy_expr_checked(expr: &Expr) -> Result<Expr> {
    reject_direct_secret_expr(expr)?;
    Ok(lower_public_policy_expr(expr.clone()))
}

fn lower_public_policy_expr(expr: Expr) -> Expr {
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

fn lower_public_policy_expr_path(path: syn::ExprPath) -> Expr {
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

fn reject_direct_secret_expr(expr: &Expr) -> Result<()> {
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

fn resolve_paginate(
    p: &PaginateSpec,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    ep_vars: &BTreeMap<String, VarInfo>,
) -> Result<PaginateResolved> {
    let mut controller_kind = paginate_controller_kind(&p.ctrl_ty);
    if p.endpoint_state {
        if !matches!(controller_kind, PaginationControllerKind::Custom) {
            return Err(syn::Error::new_spanned(
                &p.ctrl_ty,
                "endpoint_state pagination is only supported for custom controllers",
            ));
        }
        controller_kind = PaginationControllerKind::CustomEndpointState;
    }
    if matches!(controller_kind, PaginationControllerKind::Custom) && !p.assigns.is_empty() {
        return Err(syn::Error::new_spanned(
            &p.ctrl_ty,
            "custom pagination controllers use `paginate TypePath` without a configuration block",
        ));
    }
    if matches!(controller_kind, PaginationControllerKind::CustomEndpointState)
        && p.bindings_ty.is_none()
    {
        return Err(syn::Error::new_spanned(
            &p.ctrl_ty,
            "endpoint_state custom pagination requires an explicit bindings type",
        ));
    }
    let mut assigns = Vec::new();
    let mut bindings = Vec::new();
    let mut send_cursor_on_first = false;
    let mut stop_when_cursor_missing = true;
    for a in &p.assigns {
        validate_paginate_assignment_key(controller_kind, &a.key)?;
        let lowered = lower_public_policy_expr_checked(&a.value)?;
        if matches!(controller_kind, PaginationControllerKind::CustomEndpointState) {
            let vk = resolve_value_kind(
                &lowered,
                client_vars,
                auth_vars,
                Some(ep_vars),
                lowered.span(),
            )?;
            let PaginationValueKind::EpField(endpoint_field) =
                pagination_value_from_value_kind(vk, lowered.span())?
            else {
                return Err(syn::Error::new(
                    lowered.span(),
                    "endpoint_state pagination bindings must reference endpoint variables",
                ));
            };
            let endpoint_info = ep_vars.get(&endpoint_field.to_string()).ok_or_else(|| {
                syn::Error::new(
                    endpoint_field.span(),
                    unknown_scoped_name_message("endpoint var", "ep", &endpoint_field, ep_vars),
                )
            })?;
            let endpoint_field_ty = if endpoint_info.optional {
                let ty = &endpoint_info.ty;
                syn::parse_quote!(::core::option::Option<#ty>)
            } else {
                endpoint_info.ty.clone()
            };
            bindings.push(PaginationBindingIr {
                controller_field: a.key.clone(),
                endpoint_field: endpoint_field.clone(),
                endpoint_rust_field: endpoint_info.rust.clone(),
                endpoint_field_ty,
                assignment_span: lowered.span(),
            });
            assigns.push(PaginationAssignmentResolved {
                field: a.key.clone(),
                value: PaginationValueKind::EpField(endpoint_field),
            });
            continue;
        }
        if matches!(controller_kind, PaginationControllerKind::Cursor) {
            match a.key.to_string().as_str() {
                "send_cursor_on_first" => {
                    send_cursor_on_first = parse_cursor_flag_value(&lowered, &a.key)?;
                    continue;
                }
                "stop_when_cursor_missing" => {
                    stop_when_cursor_missing = parse_cursor_flag_value(&lowered, &a.key)?;
                    continue;
                }
                _ => {}
            }
        }
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
            let endpoint_field_ty = if endpoint_info.optional {
                let ty = &endpoint_info.ty;
                syn::parse_quote!(::core::option::Option<#ty>)
            } else {
                endpoint_info.ty.clone()
            };
            bindings.push(PaginationBindingIr {
                controller_field: a.key.clone(),
                endpoint_field: endpoint_field.clone(),
                endpoint_rust_field: endpoint_info.rust.clone(),
                endpoint_field_ty,
                assignment_span: lowered.span(),
            });
        }
        assigns.push(PaginationAssignmentResolved {
            field: a.key.clone(),
            value: vk,
        });
    }
    let controller = match controller_kind {
        PaginationControllerKind::OffsetLimit => {
            PaginationControllerResolved::OffsetLimit(OffsetLimitPaginationResolved { assigns })
        }
        PaginationControllerKind::Cursor => PaginationControllerResolved::Cursor(
            CursorPaginationResolved {
                assigns,
                send_cursor_on_first,
                stop_when_cursor_missing,
            },
        ),
        PaginationControllerKind::Paged => {
            PaginationControllerResolved::Paged(PagedPaginationResolved { assigns })
        }
        PaginationControllerKind::CustomEndpointState => PaginationControllerResolved::CustomEndpointState {
            ctrl_ty: p.ctrl_ty.clone(),
            bindings_ty: p
                .bindings_ty
                .clone()
                .expect("endpoint_state bindings type was validated above"),
        },
        PaginationControllerKind::Custom => PaginationControllerResolved::Custom {
            ctrl_ty: p.ctrl_ty.clone(),
        },
    };
    Ok(PaginateResolved {
        controller,
        bindings,
    })
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PaginationControllerKind {
    OffsetLimit,
    Cursor,
    Paged,
    Custom,
    CustomEndpointState,
}

fn paginate_controller_kind(ctrl_ty: &Path) -> PaginationControllerKind {
    if ctrl_ty.leading_colon.is_some() || ctrl_ty.segments.len() != 1 {
        return PaginationControllerKind::Custom;
    }
    match ctrl_ty.segments[0].ident.to_string().as_str() {
        "OffsetLimitPagination" => PaginationControllerKind::OffsetLimit,
        "CursorPagination" => PaginationControllerKind::Cursor,
        "PagedPagination" => PaginationControllerKind::Paged,
        _ => PaginationControllerKind::Custom,
    }
}

fn validate_paginate_assignment_key(kind: PaginationControllerKind, key: &Ident) -> Result<()> {
    if matches!(kind, PaginationControllerKind::Custom) {
        return Err(syn::Error::new_spanned(
            key,
            "unknown pagination controller; expected OffsetLimitPagination, CursorPagination, or PagedPagination",
        ));
    }
    if matches!(kind, PaginationControllerKind::CustomEndpointState) {
        return Ok(());
    }
    let key_name = key.to_string();
    let allowed: &[&str] = match kind {
        PaginationControllerKind::OffsetLimit => &["offset_key", "limit_key", "offset", "limit"],
        PaginationControllerKind::Cursor => &[
            "cursor_key",
            "per_page_key",
            "cursor",
            "per_page",
            "send_cursor_on_first",
            "stop_when_cursor_missing",
        ],
        PaginationControllerKind::Paged => &["page_key", "per_page_key", "page", "per_page"],
        PaginationControllerKind::Custom | PaginationControllerKind::CustomEndpointState => &[],
    };
    if allowed.contains(&key_name.as_str()) {
        return Ok(());
    }
    let controller = match kind {
        PaginationControllerKind::OffsetLimit => "OffsetLimitPagination",
        PaginationControllerKind::Cursor => "CursorPagination",
        PaginationControllerKind::Paged => "PagedPagination",
        PaginationControllerKind::Custom | PaginationControllerKind::CustomEndpointState => unreachable!(),
    };
    Err(syn::Error::new(
        key.span(),
        format!("unknown pagination field `{key_name}` for {controller}; allowed fields: {}", allowed.join(", ")),
    ))
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
                let lowered_value = match value {
                    PolicyValue::Expr(expr) => PolicyValue::Expr(lower_public_policy_expr_checked(expr)?),
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
                                unknown_scoped_name_message(
                                    "client var",
                                    "vars",
                                    &id,
                                    client_vars,
                                ),
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
                    other => {
                        PolicySetValue::Value(public_value_from_value_kind(
                            other,
                            lowered_value.span(),
                        )?)
                    }
                };

                ops.push(PolicyOp::Set {
                    key: resolve_key(key),
                    value: set_value,
                    op: *op,
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
                PolicySetValue::Value(
                    PublicValueKind::LitStr(lit)
                ) if kind == PolicyKeyKind::Header => {
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

fn reject_duplicate_header_sets(ops: &[PolicyOp]) -> Result<()> {
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    for op in ops {
        let PolicyOp::Set { key, .. } = op else {
            continue;
        };
        let normalized = header_key_for_duplicate_check(key);
        if seen.insert(normalized.clone(), key_resolved_span(key)).is_some() {
            return Err(syn::Error::new(
                key_resolved_span(key),
                format!("duplicate header `{normalized}` in the same policy layer"),
            ));
        }
    }
    Ok(())
}

fn header_key_for_duplicate_check(key: &KeyResolved) -> String {
    match key {
        KeyResolved::Static(lit) => lit.value().to_ascii_lowercase(),
        KeyResolved::Ident(ident) => emit_helpers::to_kebab(ident).to_ascii_lowercase(),
    }
}

fn key_resolved_span(key: &KeyResolved) -> Span {
    match key {
        KeyResolved::Static(lit) => lit.span(),
        KeyResolved::Ident(ident) => ident.span(),
    }
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
    let _ = auth_vars;
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
                            syn::Error::new(
                                r.ident.span(),
                                msg,
                            )
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

fn resolve_policy_value_kind(
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
                                    unknown_scoped_name_message(
                                        "endpoint var",
                                        "ep",
                                        &r.ident,
                                        ep,
                                    ),
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

fn public_value_from_value_kind(value: ValueKind, _span: Span) -> Result<PublicValueKind> {
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

fn pagination_value_from_value_kind(value: ValueKind, span: Span) -> Result<PaginationValueKind> {
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

fn parse_cursor_flag_value(expr: &Expr, key: &Ident) -> Result<bool> {
    if let Expr::Lit(lit) = expr
        && let syn::Lit::Bool(value) = &lit.lit
    {
        return Ok(value.value);
    }
    Err(syn::Error::new_spanned(
        expr,
        format!("`{key}` must be a boolean literal"),
    ))
}

fn validate_public_expr(expr: &Expr) -> Result<()> {
    if let Some(found) = emit_helpers::public_expr_forbidden(expr) {
        return Err(public_expr_forbidden_error(found));
    }
    Ok(())
}

fn public_expr_forbidden_error(found: emit_helpers::PublicExprForbidden) -> syn::Error {
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

fn direct_secret_policy_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "direct secret.* is not allowed in policy expressions; declare an auth credential",
    )
}

fn pagination_scoped_ref_error(span: Span) -> syn::Error {
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

        let err =
            pagination_value_from_value_kind(ValueKind::OtherExpr(expr), Span::call_site())
                .unwrap_err();

        assert!(
            err.to_string()
                .contains("generated implementation local `cx` is not part of the public DSL expression scope"),
            "{err}"
        );
    }

    #[test]
    fn pagination_expr_rejects_nested_auth_vars() {
        let expr: syn::Expr = syn::parse_quote! { format!("{}", auth.cursor) };

        let err =
            pagination_value_from_value_kind(ValueKind::OtherExpr(expr), Span::call_site())
                .unwrap_err();

        assert!(
            err.to_string()
                .contains("auth references are not allowed in public policy expressions"),
            "{err}"
        );
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
