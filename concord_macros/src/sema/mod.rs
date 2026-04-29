//! Semantic normalization and resolution for the v5 macro.
//!
//! This layer normalizes the raw parser API tree, validates names, resolves
//! inherited route/policy/auth state, and produces `ResolvedApi` /
//! `ResolvedEndpoint`. Codegen must consume this resolved model instead of raw
//! parser structures.

use crate::ast::{
    AuthCredentialKind, AuthCredentials, AuthUseKind, CacheDurationSpec, CacheOnErrorSpec,
    CachePatch, CacheProfilesBlock, CacheSpec, CodecSpec, FmtPiece, FmtSpec, KeySpec, PaginateSpec,
    PolicyBlock, PolicyBlocks, PolicyStmt, PolicyValue, RateLimitDurationUnit,
    RateLimitKeyBindingSpec, RateLimitKeySpec, RateLimitPlanSpec, RateLimitProfilesBlock,
    RateLimitSpec, RefScope, RetryIdempotencySpec, RetryPatch, RetryProfilesBlock, RetrySpec,
    RouteAtom, SecretRef,
};
use crate::emit_helpers;
use crate::model::*;
use proc_macro2::Span;
use std::collections::BTreeMap;
use syn::{Expr, Ident, LitStr, Result, Type, spanned::Spanned};

include!("ir.rs");
include!("profiles.rs");
include!("normalize.rs");

#[cfg(test)]
pub(crate) fn analyze_tokens_for_test(input: proc_macro2::TokenStream) -> ResolvedApi {
    let ast = syn::parse2::<crate::ast::RawApi>(input).expect("parse api");
    analyze(ast).expect("resolve api")
}

pub fn analyze(ast: crate::ast::RawApi) -> Result<ResolvedApi> {
    let norm = normalize_api(ast)?;
    resolve(norm)
}

fn resolve(norm: NormApiTree) -> Result<ResolvedApi> {
    let client_name = norm.client.name.clone();
    let mod_name_str = emit_helpers::to_snake(&client_name.to_string());
    let mod_name = Ident::new(&mod_name_str, client_name.span());

    // client vars: explicit `vars {}` only.
    let mut client_vars_map: BTreeMap<String, VarInfo> = BTreeMap::new();
    if let Some(vb) = &norm.client.vars {
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
    if let Some(vb) = &norm.client.auth_vars {
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

    let endpoint_output_map = collect_endpoint_output_types(&norm.items)?;
    let client_auth_credentials = analyze_auth_credentials(
        norm.client.auth.as_ref(),
        &auth_vars_map,
        &endpoint_output_map,
    )?;
    let auth_credential_map: BTreeMap<String, AuthCredentialIr> = client_auth_credentials
        .iter()
        .map(|c| (c.name.to_string(), c.clone()))
        .collect();
    let client_auth = resolve_auth_requirements(
        &norm.client.auth_uses,
        &auth_credential_map,
        AuthUseProvenanceIr::Client,
    )?;

    let cache_profiles = resolve_cache_profiles(norm.client.cache_profiles.as_ref())?;
    let retry_profiles = resolve_retry_profiles(norm.client.retry_profiles.as_ref())?;
    let rate_limit_profiles = resolve_rate_limit_profiles(norm.client.rate_limit.as_ref())?;

    // validate client policy + resolve
    let mut client_policy = resolve_policy_blocks(
        &norm.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        &auth_vars_map,
        None,
    )?;
    client_policy.retry = resolve_client_retry(
        norm.client.retry.as_ref(),
        norm.client
            .retry_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &retry_profiles,
    )?;
    client_policy.cache = resolve_client_cache(
        norm.client.cache.as_ref(),
        norm.client
            .cache_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &cache_profiles,
    )?;
    client_policy.rate_limit = resolve_client_rate_limit(
        norm.client.rate_limit.as_ref(),
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;

    let client_vars: Vec<VarInfo> = client_vars_map.values().cloned().collect();
    let client_auth_vars: Vec<VarInfo> = auth_vars_map.values().cloned().collect();

    // walk layers/endpoints
    let mut layers: Vec<LayerIr> = Vec::new();
    let mut endpoints: Vec<ResolvedEndpoint> = Vec::new();

    let mut ancestry: Vec<usize> = Vec::new();
    let mut walk_ctx = WalkItemsCtx {
        client_vars: &client_vars_map,
        auth_vars: &auth_vars_map,
        auth_credentials: &auth_credential_map,
        client_auth: &client_auth,
        cache_profiles: &cache_profiles,
        retry_profiles: &retry_profiles,
        rate_limit_profiles: &rate_limit_profiles,
        layers: &mut layers,
        endpoints: &mut endpoints,
    };
    walk_items(&norm.items, &mut ancestry, &mut walk_ctx)?;

    let cache_store_enabled = policy_uses_cache(&client_policy)
        || layers.iter().any(|layer| policy_uses_cache(&layer.policy))
        || endpoints.iter().any(endpoint_uses_cache);
    let cache_store_config = match &client_policy.cache {
        Some(CacheResolved::Set(config)) => Some(config.clone()),
        Some(CacheResolved::Patch(patch)) => Some(cache_config_from_patch(patch)),
        Some(CacheResolved::Clear) | None => None,
    };

    Ok(ResolvedApi {
        mod_name,
        client_name,
        scheme: norm.client.scheme,
        domain: norm.client.host,
        client_vars,
        client_auth_vars,
        client_auth_credentials,
        client_policy,
        cache_store_enabled,
        cache_store_config,
        rate_limit_response_policy: norm
            .client
            .rate_limit
            .as_ref()
            .and_then(|block| block.response_policy.clone()),
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

#[cfg(test)]
fn debug_norm_tree(norm: &NormApiTree) -> String {
    fn walk(items: &[NormNode], depth: usize, out: &mut String) {
        for item in items {
            let indent = "  ".repeat(depth);
            match item {
                NormNode::Layer(scope) => {
                    out.push_str(&format!(
                        "{indent}scope {:?} kind={:?} params={} auth={} headers={} query={} retry={} cache={} rate_limit={}\n",
                        scope.scope_name.as_ref().map(ToString::to_string),
                        scope.kind,
                        scope.params.len(),
                        scope.auth_uses.len(),
                        scope.policy.headers.as_ref().map_or(0, |h| h.stmts.len()),
                        scope.policy.query.as_ref().map_or(0, |q| q.stmts.len()),
                        scope.retry.is_some(),
                        scope.cache.is_some(),
                        scope.rate_limit.is_some(),
                    ));
                    walk(&scope.items, depth + 1, out);
                }
                NormNode::Endpoint(endpoint) => {
                    out.push_str(&format!(
                        "{indent}endpoint {} method={} alias={:?} params={} body={} query={} paginate={} map={}\n",
                        endpoint.name,
                        endpoint.method,
                        endpoint.alias.as_ref().map(ToString::to_string),
                        endpoint.params.len(),
                        endpoint.body.is_some(),
                        endpoint.policy.query.as_ref().map_or(0, |q| q.stmts.len()),
                        endpoint.paginate.is_some(),
                        endpoint.map.is_some(),
                    ));
                }
            }
        }
    }

    let mut out = format!(
        "client {} vars={} secrets={} auth={} retry_profiles={} cache_profiles={} rate_profiles={}\n",
        norm.client.name,
        norm.client.vars.as_ref().map_or(0, |v| v.decls.len()),
        norm.client.auth_vars.as_ref().map_or(0, |v| v.decls.len()),
        norm.client.auth_uses.len(),
        norm.client
            .retry_profiles
            .as_ref()
            .map_or(0, |v| v.profiles.len()),
        norm.client
            .cache_profiles
            .as_ref()
            .map_or(0, |v| v.profiles.len()),
        norm.client
            .rate_limit
            .as_ref()
            .map_or(0, |v| v.profiles.len()),
    );
    walk(&norm.items, 0, &mut out);
    out
}

#[cfg(test)]
fn debug_resolved_endpoints(resolved_api: &ResolvedApi) -> String {
    let mut out = String::new();
    for ep in &resolved_api.endpoints {
        let route = format!(
            "prefix={:?} scope_path={:?} endpoint={:?}",
            ep.prefix_pieces, ep.scope_path_pieces, ep.route_pieces
        );
        let policy = format!(
            "scopes={} headers={} query={} auth={} retry={} cache={} rate_limit={}",
            ep.policy.scopes.len(),
            ep.policy.endpoint.headers.len(),
            ep.policy.endpoint.query.len(),
            ep.policy.auth.len(),
            ep.policy.endpoint.retry.is_some(),
            ep.policy.endpoint.cache.is_some(),
            ep.policy.endpoint.rate_limit.is_some()
        );
        let params = ep
            .vars
            .iter()
            .map(|v| v.rust.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let facade = if ep.scope_modules.is_empty() {
            ep.name.to_string()
        } else {
            format!(
                "{}::{}",
                ep.scope_modules
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("::"),
                ep.name
            )
        };
        out.push_str(&format!(
            "{} method={} route=[{}] params=[{}] policy=[{}] facade={} response={:?} body={} pagination={} map={}\n",
            ep.name,
            ep.method,
            route,
            params,
            policy,
            facade,
            ep.response,
            ep.body.is_some(),
            ep.paginate.is_some(),
            ep.map.is_some()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_tree_snapshot_contains_v5_shape_without_legacy_auth_groups() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client NormApi {
                base https "example.com"
                var tenant: String
                secret token: String
                credential key = api_key(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                }
            }

            scope protected(user_id: u64) {
                path ["users", user_id]
                auth header "X-Token" = key

                GET Show(count: u64 = 20)
                    as show
                    path ["profile"]
                    query {
                        count
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect("valid api syntax");
        let norm = normalize_api(ast).expect("normalization succeeds");
        let snapshot = debug_norm_tree(&norm);

        assert!(snapshot.contains("client NormApi"));
        assert!(snapshot.contains("scope Some(\"protected\")"));
        assert!(snapshot.contains("endpoint Show method=GET"));
        assert!(snapshot.contains("alias=Some(\"show\")"));
        assert!(snapshot.contains("query=1"));
    }

    #[test]
    fn resolved_endpoint_debug_includes_inherited_tree_state() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client Api {
                base https "example.com"
                secret token: String
                credential key = api_key(secret.token)
            }

            scope protected {
                path ["v1"]
                auth header "X-Token" = key

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let snapshot = debug_resolved_endpoints(&resolved_api);

        assert!(snapshot.contains("Me method=GET"));
        assert!(snapshot.contains("scope_path=[Static(\"v1\")]"));
        assert!(snapshot.contains("endpoint=[Static(\"me\")]"));
        assert!(snapshot.contains("auth=1"));
        assert!(snapshot.contains("query=0"));
        assert!(snapshot.contains("facade=protected::Me"));
    }

    #[test]
    fn resolved_endpoint_snapshots_cover_v47_cases() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client SnapshotApi {
                base https "example.com"
                secret token: String
                credential key = api_key(secret.token)

                default {
                    retry read
                    rate_limit app
                }

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [429]
                    retry_after
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            GET Ping
                as ping
                path ["ping"]
                -> Json<String>;

            scope protected {
                path ["v1"]
                auth header "X-Token" = key

                GET Me(user_id: u64)
                    as me
                    path ["users", user_id]
                    -> Json<User>
            }

            GET Search(count: u64 = 20, page?: u64)
                as search
                path ["search"]
                query {
                    count
                    page
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>

            POST Login(body: Json<LoginRequest>)
                path ["login"]
                -> Json<LoginResponse>
                map AccessToken {
                    AccessToken::new(r.access_token)
                }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let snapshot = debug_resolved_endpoints(&resolved_api);

        assert!(snapshot.contains("Ping method=GET"));
        assert!(snapshot.contains("facade=Ping"));
        assert!(snapshot.contains("Me method=GET"));
        assert!(snapshot.contains("params=[user_id]"));
        assert!(snapshot.contains("auth=1"));
        assert!(snapshot.contains("Search method=GET"));
        assert!(snapshot.contains("query=2"));
        assert!(snapshot.contains("pagination=true"));
        assert!(snapshot.contains("Login method=POST"));
        assert!(snapshot.contains("body=true"));
        assert!(snapshot.contains("map=true"));
        assert!(snapshot.contains("scopes=1"));
    }
}
