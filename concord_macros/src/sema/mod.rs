// concord_macros/src/sema.rs
use crate::ast::*;
use crate::emit_helpers;
use proc_macro2::Span;
use std::collections::BTreeMap;
use syn::{Expr, Ident, LitStr, Result, Type, spanned::Spanned};

include!("ir.rs");
include!("profiles.rs");
pub fn analyze(ast: ApiFile) -> Result<Ir> {
    let client_name = ast.client.name.clone();
    let mod_name_str = emit_helpers::to_snake(&client_name.to_string());
    let mod_name = Ident::new(&mod_name_str, client_name.span());

    // client vars: explicit `vars {}` only.
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

    // secret vars: only from `secret {}`.
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

    let endpoint_output_map = collect_endpoint_output_types(&ast.items)?;
    let client_auth_credentials = analyze_auth_credentials(
        ast.client.auth.as_ref(),
        &auth_vars_map,
        &endpoint_output_map,
    )?;
    let auth_credential_map: BTreeMap<String, AuthCredentialIr> = client_auth_credentials
        .iter()
        .map(|c| (c.name.to_string(), c.clone()))
        .collect();
    let client_auth_uses = resolve_auth_uses(
        &ast.client.auth_uses,
        &auth_credential_map,
        AuthUseProvenanceIr::Client,
    )?;

    let cache_profiles = resolve_cache_profiles(ast.client.cache_profiles.as_ref())?;
    let retry_profiles = resolve_retry_profiles(ast.client.retry_profiles.as_ref())?;
    let rate_limit_profiles = resolve_rate_limit_profiles(ast.client.rate_limit.as_ref())?;

    // validate client policy + resolve
    let mut client_policy = resolve_policy_blocks(
        &ast.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        &auth_vars_map,
        None,
    )?;
    client_policy.retry = resolve_client_retry(
        ast.client.retry.as_ref(),
        ast.client
            .retry_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &retry_profiles,
    )?;
    client_policy.cache = resolve_client_cache(
        ast.client.cache.as_ref(),
        ast.client
            .cache_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &cache_profiles,
    )?;
    client_policy.rate_limit = resolve_client_rate_limit(
        ast.client.rate_limit.as_ref(),
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;

    let client_vars: Vec<VarInfo> = client_vars_map.values().cloned().collect();
    let client_auth_vars: Vec<VarInfo> = auth_vars_map.values().cloned().collect();

    // walk layers/endpoints
    let mut layers: Vec<LayerIr> = Vec::new();
    let mut endpoints: Vec<EndpointIr> = Vec::new();

    let mut ancestry: Vec<usize> = Vec::new();
    let mut walk_ctx = WalkItemsCtx {
        client_vars: &client_vars_map,
        auth_vars: &auth_vars_map,
        auth_credentials: &auth_credential_map,
        client_auth_uses: &client_auth_uses,
        cache_profiles: &cache_profiles,
        retry_profiles: &retry_profiles,
        rate_limit_profiles: &rate_limit_profiles,
        layers: &mut layers,
        endpoints: &mut endpoints,
    };
    walk_items(&ast.items, &mut ancestry, &mut walk_ctx)?;

    let cache_store_enabled = policy_uses_cache(&client_policy)
        || layers.iter().any(|layer| policy_uses_cache(&layer.policy))
        || endpoints
            .iter()
            .any(|endpoint| policy_uses_cache(&endpoint.policy));
    let cache_store_config = match &client_policy.cache {
        Some(CacheResolved::Set(config)) => Some(config.clone()),
        Some(CacheResolved::Patch(patch)) => Some(cache_config_from_patch(patch)),
        Some(CacheResolved::Clear) | None => None,
    };

    Ok(Ir {
        mod_name,
        client_name,
        scheme: ast.client.scheme,
        domain: ast.client.host,
        client_vars,
        client_auth_vars,
        client_auth_credentials,
        client_policy,
        cache_store_enabled,
        cache_store_config,
        rate_limit_response_policy: ast
            .client
            .rate_limit
            .as_ref()
            .and_then(|block| block.response_policy.clone()),
        layers,
        endpoints,
    })
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("common.rs");
include!("auth.rs");
include!("retry.rs");
include!("cache.rs");
include!("rate_limit.rs");
include!("items.rs");
include!("policy.rs");
