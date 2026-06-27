fn normalize_api(raw: crate::ast::RawApi) -> Result<NormApiTree> {
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
            default_behavior_uses: raw.client.default_behavior_uses,
            retry_profiles: raw.client.retry_profiles,
            retry: raw.client.retry,
            rate_limit: raw.client.rate_limit,
            behavior_profiles: raw.client.behavior_profiles,
        },
        items: normalize_items(raw.items)?,
    })
}

fn normalize_items(items: Vec<crate::ast::RawItem>) -> Result<Vec<NormNode>> {
    items.into_iter().map(normalize_item).collect()
}

fn normalize_item(item: crate::ast::RawItem) -> Result<NormNode> {
    match item {
        crate::ast::RawItem::Layer(scope) => Ok(NormNode::Layer(Box::new(normalize_scope(*scope)?))),
        crate::ast::RawItem::Endpoint(endpoint) => {
            Ok(NormNode::Endpoint(Box::new(normalize_endpoint(*endpoint)?)))
        }
    }
}

fn normalize_scope(raw: crate::ast::RawScope) -> Result<NormScope> {
    let auth_uses = normalize_auth_uses(raw.auth_uses)?;
    Ok(NormScope {
        span: raw.span,
        scope_name: raw.scope_name,
        kind: normalize_layer_kind(raw.kind),
        route: raw.route,
        params: raw.params,
        policy: raw.policy,
        behavior_uses: raw.behavior_uses,
        auth_uses,
        retry: raw.retry,
        rate_limit: raw.rate_limit,
        rate_limit_keys: raw.rate_limit_keys,
        items: normalize_items(raw.items)?,
    })
}

fn normalize_endpoint(raw: crate::ast::RawEndpoint) -> Result<NormEndpoint> {
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
        behavior_uses: raw.behavior_uses,
        auth_uses,
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

fn normalize_auth_uses(uses: Vec<crate::ast::AuthUseDecl>) -> Result<Vec<NormAuthUse>> {
    let mut out = Vec::with_capacity(uses.len());
    for auth_use in uses {
        match auth_use {
            crate::ast::AuthUseDecl::Single(kind) => {
                out.push(NormAuthUse { kind: *kind });
            }
        }
    }
    Ok(out)
}
