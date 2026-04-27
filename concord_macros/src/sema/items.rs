struct WalkItemsCtx<'a> {
    client_vars: &'a BTreeMap<String, VarInfo>,
    auth_vars: &'a BTreeMap<String, VarInfo>,
    auth_credentials: &'a BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &'a [AuthUsePlanIr],
    cache_profiles: &'a BTreeMap<String, CacheConfigResolved>,
    retry_profiles: &'a BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &'a BTreeMap<String, RateLimitPlanResolved>,
    layers: &'a mut Vec<LayerIr>,
    endpoints: &'a mut Vec<EndpointIr>,
}

struct EndpointAnalysisCtx<'a> {
    client_vars: &'a BTreeMap<String, VarInfo>,
    auth_vars: &'a BTreeMap<String, VarInfo>,
    auth_credentials: &'a BTreeMap<String, AuthCredentialIr>,
    client_auth_uses: &'a [AuthUsePlanIr],
    cache_profiles: &'a BTreeMap<String, CacheConfigResolved>,
    retry_profiles: &'a BTreeMap<String, RetryConfigResolved>,
    rate_limit_profiles: &'a BTreeMap<String, RateLimitPlanResolved>,
    layers: &'a [LayerIr],
}

fn walk_items(
    items: &[Item],
    ancestry: &mut Vec<usize>,
    ctx: &mut WalkItemsCtx<'_>,
) -> Result<()> {
    for it in items {
        match it {
            Item::Layer(ld) => {
                let id = ctx.layers.len();
                let (prefix_pieces, path_pieces, decls) = analyze_layer_route_and_decls(ld)?;
                let key_bindings = resolve_rate_limit_key_bindings(&ld.rate_limit_keys, &decls)?;
                let mut policy = resolve_policy_blocks(
                    &ld.policy,
                    PolicyOwner::Layer,
                    ctx.client_vars,
                    ctx.auth_vars,
                    None, // endpoint vars not known at layer-level alone (validated per endpoint)
                )?;
                policy.retry = resolve_retry_spec(ld.retry.as_ref(), ctx.retry_profiles)?;
                policy.cache = resolve_cache_spec(ld.cache.as_ref(), ctx.cache_profiles)?;
                let mut visible_keys = rate_limit_key_bindings_for_ancestry(ancestry, ctx.layers);
                for binding in &key_bindings {
                    visible_keys.insert(binding.name.clone(), binding.clone());
                }
                policy.rate_limit = resolve_rate_limit_spec(
                    ld.rate_limit.as_ref(),
                    ctx.rate_limit_profiles,
                    &visible_keys,
                    None,
                )?;
                let auth_uses = resolve_auth_uses(
                    &ld.auth_uses,
                    ctx.auth_credentials,
                    AuthUseProvenanceIr::Scope(id),
                )?;

                ctx.layers.push(LayerIr {
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
                walk_items(&ld.items, ancestry, ctx)?;
                ancestry.pop();
            }
            Item::Endpoint(ed) => {
                let analysis_ctx = EndpointAnalysisCtx {
                    client_vars: ctx.client_vars,
                    auth_vars: ctx.auth_vars,
                    auth_credentials: ctx.auth_credentials,
                    client_auth_uses: ctx.client_auth_uses,
                    cache_profiles: ctx.cache_profiles,
                    retry_profiles: ctx.retry_profiles,
                    rate_limit_profiles: ctx.rate_limit_profiles,
                    layers: ctx.layers.as_slice(),
                };
                let endpoint_ir = analyze_endpoint(ed, ancestry, &analysis_ctx)?;
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
    ctx: &EndpointAnalysisCtx<'_>,
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
                    resolve_route_fmt_spec(spec, Some(ctx.client_vars), Some(&ep_vars))?;
                route_pieces.push(PathPiece::Fmt(resolved));
            }
            RouteAtom::Ref(r) => match r.scope {
                RefScope::Cx => {
                    let v = ctx.client_vars.get(&r.ident.to_string()).ok_or_else(|| {
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
        ctx.client_vars,
        ctx.auth_vars,
        Some(&ep_vars),
    )?;
    policy.retry = resolve_retry_spec(ed.retry.as_ref(), ctx.retry_profiles)?;
    policy.cache = resolve_cache_spec(ed.cache.as_ref(), ctx.cache_profiles)?;
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
    policy.rate_limit = resolve_rate_limit_spec(
        ed.rate_limit.as_ref(),
        ctx.rate_limit_profiles,
        &visible_keys,
        Some(&ep_vars),
    )?;
    let mut auth_uses = ctx.client_auth_uses.to_vec();
    for &lid in ancestry {
        auth_uses.extend(ctx.layers[lid].auth_uses.iter().cloned());
    }
    auth_uses.extend(resolve_auth_uses(
        &ed.auth_uses,
        ctx.auth_credentials,
        AuthUseProvenanceIr::Endpoint,
    )?);
    let scope_modules: Vec<Ident> = ancestry
        .iter()
        .filter_map(|&lid| ctx.layers[lid].scope_name.clone())
        .collect();
    let mut prefix_pieces = Vec::new();
    let mut path_layer_pieces = Vec::new();
    let mut policy_layers = Vec::new();
    for &lid in ancestry {
        let layer = &ctx.layers[lid];
        match layer.kind {
            LayerKind::Prefix => prefix_pieces.extend(layer.prefix_pieces.iter().cloned()),
            LayerKind::Path => path_layer_pieces.extend(layer.path_pieces.iter().cloned()),
        }
        policy_layers.push(layer.policy.clone());
    }
    let current_endpoint_key = endpoint_scope_key(&scope_modules, &ed.name);
    for credential in ctx.auth_credentials.values() {
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
        Some(p) => Some(resolve_paginate(p, ctx.client_vars, ctx.auth_vars, &ep_vars)?),
    };

    // 5) Resolve map block, if any.
    let map = ed.map.as_ref().map(|m| MapResolved {
        out_ty: m.out_ty.clone(),
        body: m.body.clone(),
    });

    // 6) Produce final IR.
    Ok(EndpointIr {
        name: ed.name.clone(),
        alias: ed.alias.clone(),
        scope_modules,
        method: ed.method.clone(),
        prefix_pieces,
        path_layer_pieces,
        route_pieces,
        policy_layers,
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

