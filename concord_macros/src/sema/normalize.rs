use super::*;
use crate::limits::check_dsl_scope_depth;

pub(super) fn normalize_api(raw: crate::ast::RawApi) -> Result<NormApiTree> {
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
        items: normalize_items(raw.items, 0)?,
    })
}

pub(super) fn normalize_items(
    items: Vec<crate::ast::RawItem>,
    scope_depth: usize,
) -> Result<Vec<NormNode>> {
    items
        .into_iter()
        .map(|item| normalize_item(item, scope_depth))
        .collect()
}

pub(super) fn normalize_item(item: crate::ast::RawItem, scope_depth: usize) -> Result<NormNode> {
    match item {
        crate::ast::RawItem::Layer(scope) => Ok(NormNode::Layer(Box::new(normalize_scope(
            *scope,
            scope_depth + 1,
        )?))),
        crate::ast::RawItem::Endpoint(endpoint) => {
            Ok(NormNode::Endpoint(Box::new(normalize_endpoint(*endpoint)?)))
        }
    }
}

pub(super) fn normalize_scope(raw: crate::ast::RawScope, scope_depth: usize) -> Result<NormScope> {
    check_dsl_scope_depth(scope_depth, raw.span)?;
    let auth_uses = normalize_auth_uses(raw.auth_uses)?;
    let items = normalize_items(raw.items, scope_depth)?;

    let scope_name = raw.scope_name;
    let params = raw.params;
    let policy = raw.policy;
    let behavior_uses = raw.behavior_uses;
    let retry = raw.retry;
    let rate_limit = raw.rate_limit;
    let rate_limit_keys = raw.rate_limit_keys;

    match (raw.host_route, raw.path_route) {
        (Some(host), Some(path)) => Ok(NormScope {
            span: raw.span,
            scope_name,
            kind: RouteLayerKind::Prefix,
            route: host,
            params,
            policy,
            behavior_uses,
            auth_uses,
            retry,
            rate_limit,
            rate_limit_keys,
            items: vec![NormNode::Layer(Box::new(NormScope {
                span: raw.scope_span,
                scope_name: None,
                kind: RouteLayerKind::Path,
                route: path,
                params: Vec::new(),
                policy: PolicyBlocks::default(),
                behavior_uses: Vec::new(),
                auth_uses: Vec::new(),
                retry: None,
                rate_limit: None,
                rate_limit_keys: Vec::new(),
                items,
            }))],
        }),
        (Some(host), None) => Ok(NormScope {
            span: raw.span,
            scope_name,
            kind: RouteLayerKind::Prefix,
            route: host,
            params,
            policy,
            behavior_uses,
            auth_uses,
            retry,
            rate_limit,
            rate_limit_keys,
            items,
        }),
        (None, Some(path)) => Ok(NormScope {
            span: raw.span,
            scope_name,
            kind: RouteLayerKind::Path,
            route: path,
            params,
            policy,
            behavior_uses,
            auth_uses,
            retry,
            rate_limit,
            rate_limit_keys,
            items,
        }),
        (None, None) => Ok(NormScope {
            span: raw.span,
            scope_name,
            kind: RouteLayerKind::Path,
            route: crate::ast::RouteExpr { atoms: Vec::new() },
            params,
            policy,
            behavior_uses,
            auth_uses,
            retry,
            rate_limit,
            rate_limit_keys,
            items,
        }),
    }
}

pub(super) fn normalize_endpoint(raw: crate::ast::RawEndpoint) -> Result<NormEndpoint> {
    let auth_uses = normalize_auth_uses(raw.auth_uses)?;
    Ok(NormEndpoint {
        span: raw.span,
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
    })
}

pub(super) fn normalize_auth_uses(uses: Vec<crate::ast::AuthUseDecl>) -> Result<Vec<NormAuthUse>> {
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
