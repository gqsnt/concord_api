struct WalkItemsCtx<'a> {
    client_vars: &'a BTreeMap<String, VarInfo>,
    auth_vars: &'a BTreeMap<String, VarInfo>,
    auth_credentials: &'a BTreeMap<String, AuthCredentialIr>,
    client_auth: &'a [AuthUsePlanIr],
    client_default_behavior_names: &'a [String],
    retry_profiles: &'a BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &'a BTreeMap<String, RateLimitPlanResolved>,
    behavior_profiles: &'a BTreeMap<String, BehaviorResolved>,
    layers: &'a mut Vec<LayerIr>,
    endpoints: &'a mut Vec<ResolvedEndpoint>,
}

struct EndpointAnalysisCtx<'a> {
    client_vars: &'a BTreeMap<String, VarInfo>,
    auth_vars: &'a BTreeMap<String, VarInfo>,
    auth_credentials: &'a BTreeMap<String, AuthCredentialIr>,
    client_auth: &'a [AuthUsePlanIr],
    client_default_behavior_names: &'a [String],
    retry_profiles: &'a BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &'a BTreeMap<String, RateLimitPlanResolved>,
    behavior_profiles: &'a BTreeMap<String, BehaviorResolved>,
    layers: &'a [LayerIr],
}

fn walk_items(
    items: &[NormNode],
    ancestry: &mut Vec<usize>,
    ctx: &mut WalkItemsCtx<'_>,
    inherited_retry: Option<RetryConfigResolved>,
) -> Result<()> {
    for it in items {
        match it {
            NormNode::Layer(ld) => {
                let id = ctx.layers.len();
                let (prefix_pieces, path_pieces, decls) =
                    analyze_layer_route_and_decls(ld, ancestry, ctx.layers, ctx.client_vars)?;
                let key_bindings = resolve_rate_limit_key_bindings(&ld.rate_limit_keys, &decls)?;
                validate_behavior_uses_unique_at_site(&ld.behavior_uses)?;
                let behavior = resolve_behavior_uses(&ld.behavior_uses, ctx.behavior_profiles)?;
                let behavior_names = behavior_use_names(&ld.behavior_uses);
                let mut policy = resolve_policy_blocks(
                    &ld.policy,
                    PolicyOwner::Layer,
                    ctx.client_vars,
                    ctx.auth_vars,
                    None, // endpoint vars not known at layer-level alone (validated per endpoint)
                )?;
                let retry_directive = if ld.retry.is_some() {
                    resolve_retry_spec(ld.retry.as_ref(), ctx.retry_profiles)?
                } else {
                    behavior.retry.clone()
                };
                let (retry, next_retry) =
                    materialize_retry_directive(inherited_retry.clone(), retry_directive);
                policy.retry = retry;
                let mut visible_keys = rate_limit_key_bindings_for_ancestry(ancestry, ctx.layers);
                for binding in &key_bindings {
                    visible_keys.insert(binding.name.clone(), binding.clone());
                }
                let behavior_rate_limit = resolve_behavior_rate_limit_specs(
                    &behavior.rate_limit_specs,
                    ctx.rate_limit_profiles,
                    &visible_keys,
                    None,
                )?;
                let explicit_rate_limit = resolve_rate_limit_spec(
                    ld.rate_limit.as_ref(),
                    ctx.rate_limit_profiles,
                    &visible_keys,
                    None,
                )?;
                policy.rate_limit = merge_rate_limit_resolved(behavior_rate_limit, explicit_rate_limit);
                let mut auth_uses = behavior.auth_uses;
                auth_uses.extend(ld.auth_uses.iter().cloned());
                let auth = resolve_auth_requirements(
                    &auth_uses,
                    ctx.auth_credentials,
                    AuthUseProvenanceIr::Scope(id),
                )?;

                ctx.layers.push(LayerIr {
                    scope_name: ld.scope_name.clone(),
                    kind: ld.kind,
                    prefix_pieces,
                    path_pieces,
                    policy,
                    auth,
                    rate_limit_key_bindings: key_bindings,
                    behavior_names,
                    decls,
                });

                ancestry.push(id);
                walk_items(&ld.items, ancestry, ctx, next_retry)?;
                ancestry.pop();
            }
            NormNode::Endpoint(ed) => {
                let analysis_ctx = EndpointAnalysisCtx {
                    client_vars: ctx.client_vars,
                    auth_vars: ctx.auth_vars,
                    auth_credentials: ctx.auth_credentials,
                    client_auth: ctx.client_auth,
                    client_default_behavior_names: ctx.client_default_behavior_names,
                    retry_profiles: ctx.retry_profiles,
                    rate_limit_profiles: ctx.rate_limit_profiles,
                    behavior_profiles: ctx.behavior_profiles,
                    layers: ctx.layers.as_slice(),
                };
                let endpoint_ir =
                    analyze_endpoint(ed, ancestry, &analysis_ctx, inherited_retry.clone())?;
                ctx.endpoints.push(endpoint_ir);
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
                "{ctx} string literals must not contain `{{` or `}}`; use separate route atoms such as \"a\", id, \"b\", or fmt[\"x\", id]"
            ),
        ));
    }
    Ok(())
}

fn collect_endpoint_output_types(items: &[NormNode]) -> Result<BTreeMap<String, Type>> {
    let mut out = BTreeMap::new();
    let mut scope_stack: Vec<String> = Vec::new();
    collect_endpoint_output_types_into(items, &mut out, &mut scope_stack)?;
    Ok(out)
}

fn collect_endpoint_output_types_into(
    items: &[NormNode],
    out: &mut BTreeMap<String, Type>,
    scope_stack: &mut Vec<String>,
) -> Result<()> {
    for item in items {
        match item {
            NormNode::Layer(layer) => {
                if let Some(name) = &layer.scope_name {
                    scope_stack.push(name.to_string());
                    collect_endpoint_output_types_into(&layer.items, out, scope_stack)?;
                    let _ = scope_stack.pop();
                } else {
                    collect_endpoint_output_types_into(&layer.items, out, scope_stack)?;
                }
            }
            NormNode::Endpoint(endpoint) => {
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
                let output_ty = endpoint_public_output_ty(endpoint)?;
                out.insert(key, output_ty);
            }
        }
    }
    Ok(())
}

fn endpoint_public_output_ty(endpoint: &NormEndpoint) -> Result<Type> {
    if let Some(map) = &endpoint.map {
        return Ok(map.out_ty.clone());
    }

    let response_io = classify_http_response_io(&endpoint.response)?;
    Ok(match response_io {
        ResolvedResponseBodyIo::BufferedCodec(io) => io.value_ty,
        ResolvedResponseBodyIo::BufferedBytes => syn::parse_quote!(::bytes::Bytes),
        ResolvedResponseBodyIo::NoContent => syn::parse_quote!(()),
        ResolvedResponseBodyIo::RawStream { media_ty } => {
            syn::parse_quote!(::concord_core::advanced::StreamResponse<#media_ty>)
        }
        ResolvedResponseBodyIo::Records { item_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::RecordStream<#item_ty>)
        }
        ResolvedResponseBodyIo::Multipart { part_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::MultipartStream<#part_ty>)
        }
        ResolvedResponseBodyIo::Sse { event_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::SseStream<#event_ty>)
        }
    })
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
    ld: &NormScope,
    ancestry: &[usize],
    known_layers: &[LayerIr],
    client_vars: &BTreeMap<String, VarInfo>,
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
    let mut layer_vars: BTreeMap<String, VarInfo> = BTreeMap::new();
    for &layer_id in ancestry {
        for var in &known_layers[layer_id].decls {
            layer_vars.insert(var.rust.to_string(), var.clone());
        }
    }
    for var in &decls {
        layer_vars.insert(var.rust.to_string(), var.clone());
    }
    let mut prefix_pieces: Vec<PrefixPiece> = Vec::new();
    let mut path_pieces: Vec<PathPiece> = Vec::new();

    match ld.kind {
        RouteLayerKind::Prefix => {
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
                        let resolved =
                            resolve_route_fmt_spec(spec, Some(client_vars), Some(&layer_vars), false)?;
                        prefix_pieces.push(PrefixPiece::Fmt(resolved));
                    }
                    RouteAtom::Ref(r) => {
                        match r.scope {
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
                                prefix_pieces.push(PrefixPiece::CxVar {
                                    field: r.ident.clone(),
                                    optional: v.optional,
                                });
                            }
                            RefScope::Ep => {
                                if r.explicit {
                                    return Err(syn::Error::new(
                                        r.ident.span(),
                                        "`ep.*` is not allowed in scope routes; use the scope parameter name directly",
                                    ));
                                }
                                let _v = layer_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                    syn::Error::new(
                                        r.ident.span(),
                                        unknown_scoped_name_message(
                                            "scope param",
                                            "scope",
                                            &r.ident,
                                            &layer_vars,
                                        ),
                                    )
                                })?;
                                prefix_pieces.push(PrefixPiece::EpVar {
                                    field: r.ident.clone(),
                                });
                            }
                            RefScope::Auth => {
                                return Err(syn::Error::new(
                                    r.ident.span(),
                                    "secret references are only allowed in credential declarations",
                                ));
                            }
                        }
                    }
                }
            }
        }
        RouteLayerKind::Path => {
            for atom in &ld.route.atoms {
                match atom {
                    RouteAtom::Static(lit) => {
                        reject_formatted_lit(lit, "path")?;
                        path_pieces.push(PathPiece::Static(lit.value()));
                    }
                    RouteAtom::Fmt(spec) => {
                        let resolved =
                            resolve_route_fmt_spec(spec, Some(client_vars), Some(&layer_vars), false)?;
                        path_pieces.push(PathPiece::Fmt(resolved));
                    }
                    RouteAtom::Ref(r) => {
                        match r.scope {
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
                                path_pieces.push(PathPiece::CxVar {
                                    field: r.ident.clone(),
                                    optional: v.optional,
                                });
                            }
                            RefScope::Ep => {
                                if r.explicit {
                                    return Err(syn::Error::new(
                                        r.ident.span(),
                                        "`ep.*` is not allowed in scope routes; use the scope parameter name directly",
                                    ));
                                }
                                let _v = layer_vars.get(&r.ident.to_string()).ok_or_else(|| {
                                    syn::Error::new(
                                        r.ident.span(),
                                        unknown_scoped_name_message(
                                            "scope param",
                                            "scope",
                                            &r.ident,
                                            &layer_vars,
                                        ),
                                    )
                                })?;
                                path_pieces.push(PathPiece::EpVar {
                                    field: r.ident.clone(),
                                });
                            }
                            RefScope::Auth => {
                                return Err(syn::Error::new(
                                    r.ident.span(),
                                    "secret references are only allowed in credential declarations",
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
    ed: &NormEndpoint,
    ancestry: &[usize],
    ctx: &EndpointAnalysisCtx<'_>,
    inherited_retry: Option<RetryConfigResolved>,
) -> syn::Result<ResolvedEndpoint> {
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
        for v in &ctx.layers[lid].decls {
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
                let resolved =
                    resolve_route_fmt_spec(spec, Some(ctx.client_vars), Some(&ep_vars), true)?;
                route_pieces.push(PathPiece::Fmt(resolved));
            }
            RouteAtom::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let v = ctx.client_vars.get(&r.ident.to_string()).ok_or_else(|| {
                        syn::Error::new(
                            r.ident.span(),
                            unknown_scoped_name_message(
                                "client var",
                                "vars",
                                &r.ident,
                                ctx.client_vars,
                            ),
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
                            unknown_scoped_name_message("endpoint var", "ep", &r.ident, &ep_vars),
                        )
                    })?;
                    route_pieces.push(PathPiece::EpVar {
                        field: r.ident.clone(),
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

    // 3) Resolve policy blocks now that endpoint vars are known.
    let mut policy = resolve_policy_blocks(
        &ed.policy,
        PolicyOwner::Endpoint,
        ctx.client_vars,
        ctx.auth_vars,
        Some(&ep_vars),
    )?;
    validate_behavior_uses_unique_at_site(&ed.behavior_uses)?;
    let behavior = resolve_behavior_uses(&ed.behavior_uses, ctx.behavior_profiles)?;
    let retry_directive = if ed.retry.is_some() {
        resolve_retry_spec(ed.retry.as_ref(), ctx.retry_profiles)?
    } else {
        behavior.retry.clone()
    };
    let (retry, _next_retry) = materialize_retry_directive(inherited_retry, retry_directive);
    policy.retry = retry;
    let endpoint_decls = ep_var_order
        .iter()
        .filter_map(|key| ep_vars.get(key))
        .cloned()
        .collect::<Vec<_>>();
    let endpoint_key_bindings =
        resolve_rate_limit_key_bindings(&ed.rate_limit_keys, &endpoint_decls)?;
    let mut visible_keys = rate_limit_key_bindings_for_ancestry(ancestry, ctx.layers);
    for binding in endpoint_key_bindings {
        visible_keys.insert(binding.name.clone(), binding);
    }
    let behavior_rate_limit = resolve_behavior_rate_limit_specs(
        &behavior.rate_limit_specs,
        ctx.rate_limit_profiles,
        &visible_keys,
        Some(&ep_vars),
    )?;
    let explicit_rate_limit = resolve_rate_limit_spec(
        ed.rate_limit.as_ref(),
        ctx.rate_limit_profiles,
        &visible_keys,
        Some(&ep_vars),
    )?;
    policy.rate_limit = merge_rate_limit_resolved(behavior_rate_limit, explicit_rate_limit);
    let mut auth = ctx.client_auth.to_vec();
    for &lid in ancestry {
        auth.extend(ctx.layers[lid].auth.iter().cloned());
    }
    let mut auth_uses = behavior.auth_uses;
    auth_uses.extend(ed.auth_uses.iter().cloned());
    auth.extend(resolve_auth_requirements(
        &auth_uses,
        ctx.auth_credentials,
        AuthUseProvenanceIr::Endpoint,
    )?);
    let mut behavior_doc_names = Vec::new();
    behavior_doc_names.extend(ctx.client_default_behavior_names.iter().cloned());
    for &lid in ancestry {
        behavior_doc_names.extend(ctx.layers[lid].behavior_names.iter().cloned());
    }
    behavior_doc_names.extend(behavior_use_names(&ed.behavior_uses));
    let mut seen_behavior_doc_names = std::collections::BTreeSet::new();
    behavior_doc_names.retain(|name| seen_behavior_doc_names.insert(name.clone()));
    let mut scope_modules = Vec::new();
    let mut facade_param_groups = Vec::new();
    let mut prefix_pieces = Vec::new();
    let mut scope_path_pieces = Vec::new();
    let mut scope_policies = Vec::new();
    for &lid in ancestry {
        let layer = &ctx.layers[lid];
        if let Some(scope_name) = &layer.scope_name {
            scope_modules.push(scope_name.clone());
            facade_param_groups.push(layer.decls.clone());
        }
        match layer.kind {
            RouteLayerKind::Prefix => prefix_pieces.extend(layer.prefix_pieces.iter().cloned()),
            RouteLayerKind::Path => scope_path_pieces.extend(layer.path_pieces.iter().cloned()),
        }
        scope_policies.push(layer.policy.clone());
    }
    let current_endpoint_key = endpoint_scope_key(&scope_modules, &ed.name);
    for credential in ctx.auth_credentials.values() {
        let AuthCredentialKindIr::Endpoint { endpoint_key, .. } = &credential.kind else {
            continue;
        };
        if endpoint_key != &current_endpoint_key {
            continue;
        }
        if auth_plan_references_credential(&auth, &credential.name) {
            return Err(syn::Error::new(
                ed.name.span(),
                format!(
                    "credential `{}` cannot acquire via endpoint `{}` because the endpoint uses that credential",
                    credential.name, ed.name
            ),
        ));
        }
    }

    let method_name = ed.method.to_string();
    if !matches!(
        method_name.as_str(),
        "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "OPTIONS" | "PATCH"
    ) {
        return Err(syn::Error::new(
            ed.method.span(),
            "unsupported endpoint method",
        ));
    }

    let request_io = classify_request_io(ed.body.as_ref())?;
    let response_io = classify_http_response_io(&ed.response)?;
    if matches!(
        response_io,
        ResolvedResponseBodyIo::NoContent
            | ResolvedResponseBodyIo::RawStream { .. }
            | ResolvedResponseBodyIo::Records { .. }
            | ResolvedResponseBodyIo::Multipart { .. }
            | ResolvedResponseBodyIo::Sse { .. }
    ) {
        if ed.map.is_some() {
            return Err(syn::Error::new(
                ed.name.span(),
                "`map` is only supported for buffered responses",
            ));
        }
        if ed.paginate.is_some() {
            return Err(syn::Error::new(
                ed.name.span(),
                "pagination is only supported for buffered responses",
            ));
        }
    }
    if matches!(response_io, ResolvedResponseBodyIo::BufferedBytes) && ed.paginate.is_some() {
        return Err(syn::Error::new(
            ed.name.span(),
            "`Bytes` responses do not support pagination",
        ));
    }

    // 4) Resolve paginate, if any.
    if !request_io.is_none() && ed.paginate.is_some() {
        return Err(syn::Error::new(
            ed.name.span(),
            "paginated endpoints with request bodies are not supported in v1",
        ));
    }
    let paginate = match &ed.paginate {
        None => None,
        Some(p) => Some(resolve_paginate(p, ctx.client_vars, ctx.auth_vars, &ep_vars)?),
    };

    // 5) Resolve map block, if any.
    let map = ed.map.as_ref().map(|m| MapResolved {
        out_ty: m.out_ty.clone(),
        body: m.body.clone(),
    });

    // 6) Produce final resolved_api.
    Ok(ResolvedEndpoint {
        name: ed.name.clone(),
        alias: ed.alias.clone(),
        scope_modules,
        facade_param_groups,
        method: ed.method.clone(),
        prefix_pieces,
        scope_path_pieces,
        route_pieces,

        // Stable declaration order.
        vars: endpoint_decls,

        io: ResolvedHttpEndpointIo {
            request: request_io,
            response: response_io,
        },

        policy: ResolvedPolicySpec {
            scopes: scope_policies,
            endpoint: policy,
            auth,
        },
        behavior_doc: BehaviorDocMeta {
            names: behavior_doc_names,
        },
        paginate,
        map,
    })
}

#[derive(Copy, Clone)]
enum EndpointIoPosition {
    Request,
    Response,
}

fn endpoint_io_family_name(spec: &RawIoSpec) -> Option<&syn::Ident> {
    match &spec.marker {
        syn::Type::Path(tp) => tp.path.segments.last().map(|segment| &segment.ident),
        _ => None,
    }
}

fn endpoint_io_arg_count(spec: &RawIoSpec) -> usize {
    spec.args.len()
}

fn buffered_codec_io(spec: &RawIoSpec) -> BufferedCodecIo {
    BufferedCodecIo {
        marker: spec.marker.clone(),
        codec_path: spec.enc.clone(),
        value_ty: spec.ty.clone(),
    }
}

fn classify_request_io(spec: Option<&RawIoSpec>) -> Result<ResolvedRequestBodyIo> {
    let Some(spec) = spec else {
        return Ok(ResolvedRequestBodyIo::None);
    };
    let io = classify_endpoint_io(spec, EndpointIoPosition::Request)?;
    Ok(match io {
        EndpointIoClassification::BufferedCodec(io) => ResolvedRequestBodyIo::BufferedCodec(io),
        EndpointIoClassification::BufferedBytes => {
            return Err(syn::Error::new_spanned(
                spec.marker.clone(),
                "`Bytes` endpoint I/O is reserved but not supported yet",
            ));
        }
        EndpointIoClassification::NoContent => {
            return Err(syn::Error::new_spanned(
                spec.marker.clone(),
                "`NoContent` is not valid as an endpoint request",
            ));
        }
        EndpointIoClassification::RawStream { media_ty } => ResolvedRequestBodyIo::RawStream {
            media_ty,
        },
        EndpointIoClassification::Records { item_ty, format_ty } => {
            ResolvedRequestBodyIo::Records { item_ty, format_ty }
        }
        EndpointIoClassification::Multipart { value_ty, format_ty } => {
            ResolvedRequestBodyIo::Multipart { value_ty, format_ty }
        }
        EndpointIoClassification::Sse { .. } => {
            return Err(syn::Error::new_spanned(
                spec.marker.clone(),
                "`Sse` is only valid as an endpoint response",
            ));
        }
    })
}

fn classify_http_response_io(spec: &RawResponseIo) -> Result<ResolvedResponseBodyIo> {
    let io = classify_endpoint_io(spec, EndpointIoPosition::Response)?;
    Ok(match io {
        EndpointIoClassification::BufferedCodec(io) => ResolvedResponseBodyIo::BufferedCodec(io),
        EndpointIoClassification::BufferedBytes => ResolvedResponseBodyIo::BufferedBytes,
        EndpointIoClassification::NoContent => ResolvedResponseBodyIo::NoContent,
        EndpointIoClassification::RawStream { media_ty } => ResolvedResponseBodyIo::RawStream {
            media_ty,
        },
        EndpointIoClassification::Records { item_ty, format_ty } => {
            ResolvedResponseBodyIo::Records { item_ty, format_ty }
        }
        EndpointIoClassification::Multipart { value_ty, format_ty } => {
            ResolvedResponseBodyIo::Multipart {
                part_ty: value_ty,
                format_ty,
            }
        }
        EndpointIoClassification::Sse {
            event_ty,
            codec_ty,
        } => ResolvedResponseBodyIo::Sse {
            event_ty,
            codec_ty,
        },
    })
}

enum EndpointIoClassification {
    BufferedCodec(BufferedCodecIo),
    BufferedBytes,
    NoContent,
    RawStream {
        media_ty: Type,
    },
    Records {
        item_ty: Type,
        format_ty: Type,
    },
    Multipart {
        value_ty: Type,
        format_ty: Type,
    },
    Sse {
        event_ty: Type,
        codec_ty: Type,
    },
}

fn classify_endpoint_io(
    spec: &RawIoSpec,
    position: EndpointIoPosition,
) -> Result<EndpointIoClassification> {
    let family = endpoint_io_family_name(spec)
        .map(ToString::to_string)
        .unwrap_or_default();
    let arg_count = endpoint_io_arg_count(spec);

    match family.as_str() {
        "Bytes" => {
            if spec.had_angle_args || arg_count != 0 {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `Bytes` does not take type arguments",
                ));
            }
            Ok(EndpointIoClassification::BufferedBytes)
        }
        "NoContent" => {
            if spec.had_angle_args || arg_count != 0 {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `NoContent` does not take type arguments",
                ));
            }
            if matches!(position, EndpointIoPosition::Request) {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "`NoContent` is only valid as an endpoint response",
                ));
            }
            Ok(EndpointIoClassification::NoContent)
        }
        "Stream" => {
            if arg_count != 1 {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `Stream` expects exactly one type argument",
                ));
            }
            Ok(EndpointIoClassification::RawStream {
                media_ty: spec.args[0].clone(),
            })
        }
        "Records" => {
            if arg_count != 2 {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `Records` expects exactly two type arguments",
                ));
            }
            Ok(EndpointIoClassification::Records {
                item_ty: spec.args[0].clone(),
                format_ty: spec.args[1].clone(),
            })
        }
        "Multipart" => {
            if !(arg_count == 1 || arg_count == 2) {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `Multipart` expects one or two type arguments",
                ));
            }
            let value_ty = spec.args[0].clone();
            let format_ty = spec
                .args
                .get(1)
                .cloned()
                .unwrap_or_else(|| syn::parse_quote!(::concord_core::advanced::FormData));
            Ok(EndpointIoClassification::Multipart { value_ty, format_ty })
        }
        "Sse" => {
            if !(arg_count == 1 || arg_count == 2) {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "reserved endpoint I/O family `Sse` expects one or two type arguments",
                ));
            }
            if matches!(position, EndpointIoPosition::Request) {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "`Sse` is only valid as an endpoint response",
                ));
            }
            Ok(EndpointIoClassification::Sse {
                event_ty: spec.args[0].clone(),
                codec_ty: spec
                    .args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| syn::parse_quote!(::concord_core::advanced::JsonSse)),
            })
        }
        _ => {
            if arg_count > 1 {
                return Err(syn::Error::new_spanned(
                    spec.marker.clone(),
                    "codec spec expects exactly one type argument: `Enc<T>`",
                ));
            }
            Ok(EndpointIoClassification::BufferedCodec(buffered_codec_io(spec)))
        }
    }
}

