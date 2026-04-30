fn resolve_paginate(
    p: &PaginateSpec,
    client_vars: &BTreeMap<String, VarInfo>,
    auth_vars: &BTreeMap<String, VarInfo>,
    ep_vars: &BTreeMap<String, VarInfo>,
) -> Result<PaginateResolved> {
    let built_in = is_builtin_paginate_controller(&p.ctrl_ty);
    if !built_in && !p.assigns.is_empty() {
        return Err(syn::Error::new_spanned(
            &p.ctrl_ty,
            "custom pagination controllers use `paginate TypePath` without a configuration block",
        ));
    }
    if built_in {
        validate_paginate_controller(&p.ctrl_ty)?;
    }
    let mut assigns = Vec::new();
    for a in &p.assigns {
        validate_paginate_assignment_key(&p.ctrl_ty, &a.key)?;
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

fn validate_paginate_controller(ctrl_ty: &Path) -> Result<()> {
    if is_builtin_paginate_controller(ctrl_ty) {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        ctrl_ty,
        "unknown pagination controller; expected OffsetLimitPagination, CursorPagination, or PagedPagination",
    ))
}

fn is_builtin_paginate_controller(ctrl_ty: &Path) -> bool {
    matches!(
        paginate_controller_name(ctrl_ty).as_deref(),
        Some("OffsetLimitPagination" | "CursorPagination" | "PagedPagination")
    )
}

fn validate_paginate_assignment_key(ctrl_ty: &Path, key: &Ident) -> Result<()> {
    let Some(controller) = paginate_controller_name(ctrl_ty) else {
        return validate_paginate_controller(ctrl_ty);
    };
    let key_name = key.to_string();
    let allowed = match controller.as_str() {
        "OffsetLimitPagination" => [
            "stop",
            "offset_key",
            "limit_key",
            "offset",
            "limit",
            "stop_on_short_page",
        ]
        .as_slice(),
        "CursorPagination" => [
            "stop",
            "cursor_key",
            "per_page_key",
            "cursor",
            "per_page",
            "send_cursor_on_first",
            "stop_when_cursor_missing",
        ]
        .as_slice(),
        "PagedPagination" => [
            "stop",
            "page_key",
            "per_page_key",
            "page",
            "per_page",
            "stop_on_short_page",
        ]
        .as_slice(),
        _ => return validate_paginate_controller(ctrl_ty),
    };
    if allowed.contains(&key_name.as_str()) {
        return Ok(());
    }
    Err(syn::Error::new(
        key.span(),
        format!(
            "unknown pagination field `{key_name}` for {controller}; allowed fields: {}",
            allowed.join(", ")
        ),
    ))
}

fn paginate_controller_name(ctrl_ty: &Path) -> Option<String> {
    ctrl_ty
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
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
                            syn::Error::new(
                                id.span(),
                                unknown_scoped_name_message("client var", "vars", id, client_vars),
                            )
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
                            syn::Error::new(
                                id.span(),
                                unknown_scoped_name_message("endpoint var", "ep", id, ep),
                            )
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
                            unknown_scoped_name_message("client var", "vars", id, client_vars),
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
                            unknown_scoped_name_message("endpoint var", "ep", id, ep),
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
