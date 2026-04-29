fn normalize_api(raw: crate::ast::ApiFile) -> Result<NormApiTree> {
    reject_custom_credentials(raw.client.auth.as_ref())?;
    let client_auth_uses = normalize_auth_uses(raw.client.auth_uses)?;

    Ok(NormApiTree {
        client: NormClient {
            span: raw.client.span,
            name: raw.client.name,
            scheme: raw.client.scheme,
            host: raw.client.host,
            policy: raw.client.policy,
            vars: raw.client.vars.map(|vars| NormVars { decls: vars.decls }),
            auth_vars: raw
                .client
                .auth_vars
                .map(|vars| NormVars { decls: vars.decls }),
            auth: raw.client.auth,
            auth_uses: client_auth_uses,
            cache_profiles: raw.client.cache_profiles,
            cache: raw.client.cache,
            retry_profiles: raw.client.retry_profiles,
            retry: raw.client.retry,
            rate_limit: raw.client.rate_limit,
        },
        items: normalize_items(raw.items)?,
    })
}

fn normalize_items(items: Vec<crate::ast::Item>) -> Result<Vec<NormNode>> {
    items.into_iter().map(normalize_item).collect()
}

fn normalize_item(item: crate::ast::Item) -> Result<NormNode> {
    match item {
        crate::ast::Item::Layer(scope) => Ok(NormNode::Layer(Box::new(normalize_scope(*scope)?))),
        crate::ast::Item::Endpoint(endpoint) => {
            Ok(NormNode::Endpoint(Box::new(normalize_endpoint(*endpoint)?)))
        }
    }
}

fn normalize_scope(raw: crate::ast::LayerDef) -> Result<NormScope> {
    let auth_uses = normalize_auth_uses(raw.auth_uses)?;
    Ok(NormScope {
        span: raw.span,
        scope_name: raw.scope_name,
        kind: normalize_layer_kind(raw.kind),
        route: raw.route,
        params: raw.params,
        policy: raw.policy,
        auth_uses,
        cache: raw.cache,
        retry: raw.retry,
        rate_limit: raw.rate_limit,
        rate_limit_keys: raw.rate_limit_keys,
        items: normalize_items(raw.items)?,
    })
}

fn normalize_endpoint(raw: crate::ast::EndpointDef) -> Result<NormEndpoint> {
    let auth_uses = normalize_auth_uses(raw.auth_uses)?;
    Ok(NormEndpoint {
        span: raw.span,
        line: NormEndpointLine {
            span: raw.line.span,
            method: raw.line.method,
            name: raw.line.name,
            alias: raw.line.alias,
        },
        method: raw.method,
        name: raw.name,
        alias: raw.alias,
        route: raw.route,
        params: raw.params,
        policy: raw.policy,
        auth_uses,
        cache: raw.cache,
        retry: raw.retry,
        rate_limit: raw.rate_limit,
        rate_limit_keys: raw.rate_limit_keys,
        paginate: raw.paginate,
        body: raw.body,
        response: raw.response,
        map: raw.map,
    })
}

fn normalize_layer_kind(kind: crate::ast::LayerKind) -> RouteLayerKind {
    match kind {
        crate::ast::LayerKind::Prefix => RouteLayerKind::Prefix,
        crate::ast::LayerKind::Path => RouteLayerKind::Path,
    }
}

fn reject_custom_credentials(block: Option<&crate::ast::AuthBlock>) -> Result<()> {
    let Some(block) = block else {
        return Ok(());
    };
    for credential in &block.credentials {
        if let crate::ast::AuthCredentialKind::Custom {
            provider_ty,
            provider,
        } = &credential.kind
        {
            return Err(unsupported_custom_auth_credential_error(
                provider_ty
                    .span()
                    .join(provider.span())
                    .unwrap_or(provider_ty.span()),
            ));
        }
    }
    Ok(())
}

fn normalize_auth_uses(uses: Vec<crate::ast::AuthUseDecl>) -> Result<Vec<NormAuthUse>> {
    let mut out = Vec::with_capacity(uses.len());
    for auth_use in uses {
        match auth_use {
            crate::ast::AuthUseDecl::Single(kind) => {
                reject_custom_auth_use(&kind)?;
                out.push(NormAuthUse { kind: *kind });
            }
            crate::ast::AuthUseDecl::UnsupportedAllGroup(kinds)
            | crate::ast::AuthUseDecl::UnsupportedAnyGroup(kinds) => {
                return Err(unsupported_auth_group_error(
                    kinds
                        .first()
                        .map(auth_use_credential_ident)
                        .map(Ident::span)
                        .unwrap_or_else(Span::call_site),
                ));
            }
        }
    }
    Ok(out)
}

fn reject_custom_auth_use(kind: &crate::ast::AuthUseKind) -> Result<()> {
    if let crate::ast::AuthUseKind::Custom { usage, .. } = kind {
        return Err(unsupported_custom_auth_placement_error(usage.span()));
    }
    Ok(())
}
