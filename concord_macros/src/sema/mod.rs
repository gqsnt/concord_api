//! Semantic normalization and resolution for the Concord macro.
//!
//! This layer normalizes the raw parser API tree, validates names, resolves
//! inherited route/policy/auth state, and produces `ResolvedApi` /
//! `ResolvedEndpoint`. Codegen must consume this resolved model instead of raw
//! parser structures.

use crate::ast::{
    AuthCredentialKind, AuthCredentials, AuthUseKind, BehaviorProfileDef, BehaviorProfilesBlock,
    BehaviorUseSpec, CacheDurationSpec, CacheOnErrorSpec, CachePatch, CacheProfilesBlock,
    CacheSpec, CodecSpec, FmtPiece, FmtSpec, KeySpec, PaginateSpec, PolicyBlock, PolicyBlocks,
    PolicyStmt, PolicyValue, RateLimitDurationUnit, RateLimitKeyBindingSpec, RateLimitKeySpec,
    RateLimitPlanSpec, RateLimitProfilesBlock, RateLimitSpec, RefScope, RetryIdempotencySpec,
    RetryPatch, RetryProfilesBlock, RetrySpec, RouteAtom, SecretRef,
};
use crate::emit_helpers;
use crate::model::*;
use proc_macro2::Span;
use std::collections::BTreeMap;
use syn::{Expr, Ident, LitStr, Path, Result, Type, spanned::Spanned};

include!("ir.rs");
include!("profiles.rs");
include!("behavior.rs");
include!("normalize.rs");
#[path = "resolve.rs"]
mod resolve_stage;

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

    let cache_profiles = resolve_cache_profiles(norm.client.cache_profiles.as_ref())?;
    let retry_profiles = resolve_retry_profiles(norm.client.retry_profiles.as_ref())?;
    let rate_limit_profiles = resolve_rate_limit_profiles(norm.client.rate_limit.as_ref())?;
    let behavior_profiles = resolve_behavior_profiles(
        norm.client.behavior_profiles.as_ref(),
        &cache_profiles,
        &retry_profiles,
    )?;
    let default_behavior =
        resolve_behavior_uses(&norm.client.default_behavior_uses, &behavior_profiles)?;
    let default_behavior_rate_limit = resolve_behavior_rate_limit_specs(
        &default_behavior.rate_limit_specs,
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;
    let mut client_auth_uses = default_behavior.auth_uses;
    client_auth_uses.extend(norm.client.auth_uses.iter().cloned());
    let client_auth = resolve_auth_requirements(
        &client_auth_uses,
        &auth_credential_map,
        AuthUseProvenanceIr::Client,
    )?;

    // validate client policy + resolve
    let mut client_policy = resolve_policy_blocks(
        &norm.client.policy,
        PolicyOwner::Client,
        &client_vars_map,
        &auth_vars_map,
        None,
    )?;
    let explicit_client_retry = resolve_client_retry(
        norm.client.retry.as_ref(),
        norm.client
            .retry_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &retry_profiles,
    )?;
    client_policy.retry = match explicit_client_retry {
        Some(_) => explicit_client_retry,
        None => default_behavior.policy.retry.clone(),
    };
    let explicit_client_cache = resolve_client_cache(
        norm.client.cache.as_ref(),
        norm.client
            .cache_profiles
            .as_ref()
            .and_then(|block| block.default.as_ref()),
        &cache_profiles,
    )?;
    client_policy.cache = match explicit_client_cache {
        Some(_) => explicit_client_cache,
        None => default_behavior.policy.cache.clone(),
    };
    let explicit_default_rate_limit = resolve_client_rate_limit(
        norm.client.rate_limit.as_ref(),
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;
    client_policy.rate_limit =
        merge_rate_limit_resolved(default_behavior_rate_limit, explicit_default_rate_limit);

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
        behavior_profiles: &behavior_profiles,
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
    fn normalized_tree_snapshot_contains_current_shape_without_raw_auth_groups() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client NormApi {
                base "https://example.com"
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
    fn normalized_tree_contains_only_canonical_endpoint_constructs() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client NormCanonical {
                    base "https://example.com"
                    var trace_id: String
                }

                POST Create(q: String, tag?: String, body: Json<CreateBody>)
                    as create
                    path [fmt["items-", q]]
                    query {
                        q
                        "tag" += tag
                    }
                    headers {
                        "x-trace" = vars.trace_id
                    }
                    -> Json<CreateResponse>
            }
            "#,
        )
        .expect("valid current api syntax");
        let norm = normalize_api(ast).expect("normalization succeeds");
        let NormNode::Endpoint(endpoint) = &norm.items[0] else {
            panic!("expected endpoint");
        };

        assert!(matches!(
            endpoint.route.atoms.as_slice(),
            [RouteAtom::Fmt(_)]
        ));
        assert!(
            endpoint.body.is_some(),
            "body is normalized from signature only"
        );
        assert_eq!(endpoint.params.len(), 2);

        let query = endpoint.policy.query.as_ref().expect("query policy");
        assert_eq!(query.stmts.len(), 2);
        match &query.stmts[0] {
            PolicyStmt::Set {
                key: KeySpec::Ident(key),
                value: PolicyValue::Expr(Expr::Field(field)),
                op: SetOp::Set,
            } => {
                assert_eq!(key.to_string(), "q");
                match &field.member {
                    syn::Member::Named(member) => assert_eq!(member.to_string(), "q"),
                    other => panic!("expected named query field, got {other:?}"),
                }
            }
            other => panic!("query shorthand was not canonicalized: {other:?}"),
        }
        assert!(matches!(
            &query.stmts[1],
            PolicyStmt::Set {
                key: KeySpec::Str(_),
                op: SetOp::Push,
                ..
            }
        ));

        let headers = endpoint.policy.headers.as_ref().expect("headers policy");
        assert!(matches!(
            &headers.stmts[0],
            PolicyStmt::Set {
                key: KeySpec::Str(_),
                op: SetOp::Set,
                ..
            }
        ));
    }

    #[test]
    fn resolved_endpoint_debug_includes_inherited_tree_state() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client Api {
                base "https://example.com"
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
    fn route_resolution_accepts_scope_and_endpoint_params() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                scope users(tenant_id: String) {
                    path ["tenants", tenant_id]

                    GET Show(user_id: String)
                        path ["users", user_id]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let snapshot = debug_resolved_endpoints(&resolved_api);
        let endpoint = &resolved_api.endpoints[0];

        assert!(snapshot.contains("scope_path=[Static(\"tenants\"), EpVar"));
        assert!(snapshot.contains("endpoint=[Static(\"users\"), EpVar"));
        assert!(endpoint.vars.iter().any(|var| var.rust == "tenant_id"));
        assert!(endpoint.vars.iter().any(|var| var.rust == "user_id"));
    }

    #[test]
    fn explicit_ep_reference_in_scope_route_fails() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                scope users(user_id: String) {
                    path [ep.user_id]

                    GET Show
                        path ["show"]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("explicit ep refs in scope route must fail");

        assert!(
            err.to_string()
                .contains("`ep.*` is not allowed in scope routes")
        );
    }

    #[test]
    fn unknown_route_reference_fails_during_resolution() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Show(user_id: String)
                    path [fmt["user-", missing]]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("unknown endpoint route refs must fail");

        assert!(err.to_string().contains("unknown endpoint var"));
    }

    #[test]
    fn optional_fmt_route_reference_resolves_as_optional() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Show(prefix?: String)
                    path [fmt["user-", prefix]]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = &resolved_api.endpoints[0];
        let PathPiece::Fmt(fmt) = &endpoint.route_pieces[0] else {
            panic!("expected fmt route piece");
        };

        assert!(fmt.pieces.iter().any(|piece| matches!(
            piece,
            FmtResolvedPiece::Var {
                source: FmtVarSource::Ep,
                optional: true,
                ..
            }
        )));
    }

    #[test]
    fn resolved_query_and_header_ops_preserve_order_and_optional_conditions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    var trace_id: String
                }

                GET Search(q: String, maybe?: String)
                    path ["search"]
                    query {
                        "q" = q,
                        "tag" += q,
                        "maybe" = maybe,
                        -"old"
                    }
                    headers {
                        "x-trace" = vars.trace_id,
                        -"x-old"
                    }
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint_policy = &resolved_api.endpoints[0].policy.endpoint;

        assert!(matches!(
            &endpoint_policy.query[0],
            PolicyOp::Set {
                key: KeyResolved::Static(key),
                op: SetOp::Set,
                ..
            } if key.value() == "q"
        ));
        assert!(matches!(
            &endpoint_policy.query[1],
            PolicyOp::Set {
                key: KeyResolved::Static(key),
                op: SetOp::Push,
                ..
            } if key.value() == "tag"
        ));
        assert!(matches!(
            &endpoint_policy.query[2],
            PolicyOp::Set {
                key: KeyResolved::Static(key),
                conditional_on_optional_ref: Some(OptionalRefKind::Ep),
                ..
            } if key.value() == "maybe"
        ));
        assert!(matches!(
            &endpoint_policy.query[3],
            PolicyOp::Remove {
                key: KeyResolved::Static(key)
            } if key.value() == "old"
        ));
        assert!(matches!(
            &endpoint_policy.headers[0],
            PolicyOp::Set {
                key: KeyResolved::Static(key),
                ..
            } if key.value() == "x-trace"
        ));
        assert!(matches!(
            &endpoint_policy.headers[1],
            PolicyOp::Remove {
                key: KeyResolved::Static(key)
            } if key.value() == "x-old"
        ));
    }

    #[test]
    fn duplicate_header_names_in_same_layer_fail_case_insensitively() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search
                    path ["search"]
                    headers {
                        "X-Trace" = "one",
                        "x-trace" = "two"
                    }
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("duplicate header names must fail");

        assert!(err.to_string().contains("duplicate header `x-trace`"));
    }

    #[test]
    fn static_and_bearer_auth_credentials_resolve() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret api_key: String
                    secret token: String
                    credential key = api_key(secret.api_key)
                    credential bearer_token = bearer(secret.token)
                }

                GET Search
                    path ["search"]
                    auth header "X-Api-Key" = key
                    auth bearer bearer_token
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        assert!(matches!(
            resolved_api.client_auth_credentials[0].kind,
            AuthCredentialKindIr::ApiKey { .. }
        ));
        assert!(matches!(
            resolved_api.client_auth_credentials[1].kind,
            AuthCredentialKindIr::StaticBearer { .. }
        ));
        assert_eq!(resolved_api.endpoints[0].policy.auth.len(), 2);
    }

    #[test]
    fn auth_requirements_combine_in_client_scope_endpoint_order() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret api_key: String
                    secret token: String
                    secret scope_key: String
                    credential client_key = api_key(secret.api_key)
                    credential scope_key = api_key(secret.scope_key)
                    credential token = bearer(secret.token)
                    auth header "X-Client" = client_key
                }

                scope protected {
                    path ["protected"]
                    auth query "scope_key" = scope_key

                    GET Me
                        path ["me"]
                        auth bearer token
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let auth = &resolved_api.endpoints[0].policy.auth;

        assert_eq!(auth.len(), 3);
        let names = auth
            .iter()
            .map(|plan| {
                let AuthUsePlanIr::Use(auth_use) = plan;
                auth_use_credential_ident_ir(auth_use).to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["client_key", "scope_key", "token"]);
        let provenances = auth
            .iter()
            .map(|plan| {
                let AuthUsePlanIr::Use(auth_use) = plan;
                auth_use.provenance
            })
            .collect::<Vec<_>>();
        assert!(matches!(provenances[0], AuthUseProvenanceIr::Client));
        assert!(matches!(provenances[1], AuthUseProvenanceIr::Scope(_)));
        assert!(matches!(provenances[2], AuthUseProvenanceIr::Endpoint));
    }

    #[test]
    fn endpoint_backed_credential_resolves_to_endpoint_output() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret upstream_key: String
                    credential upstream = api_key(secret.upstream_key)
                    credential session = endpoint auth_api::Login
                }

                scope auth_api {
                    path ["auth"]

                    POST Login(body: Json<LoginRequest>)
                        path ["login"]
                        auth header "X-Upstream-Key" = upstream
                        -> Json<LoginResponse>
                        map AccessToken {
                            AccessToken::new(r.access_token)
                        }
                }

                scope protected {
                    path ["protected"]
                    auth bearer session

                    GET Me
                        path ["me"]
                        -> Json<User>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let session = resolved_api
            .client_auth_credentials
            .iter()
            .find(|credential| credential.name == "session")
            .expect("session credential");

        let AuthCredentialKindIr::Endpoint {
            endpoint_key,
            output_ty,
            ..
        } = &session.kind
        else {
            panic!("expected endpoint-backed credential");
        };
        assert_eq!(endpoint_key, "auth_api::Login");
        assert!(
            quote::quote!(#output_ty)
                .to_string()
                .contains("AccessToken")
        );
    }

    #[test]
    fn policy_profiles_defaults_and_endpoint_overrides_resolve() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    default {
                        retry read_child
                        cache standard
                        rate_limit app
                    }

                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    retry read_child extends read {
                        on [429]
                        retry_after
                    }

                    cache standard {
                        ttl 30s
                        revalidate
                    }

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }
                }

                GET Ping
                    path ["ping"]
                    retry off
                    cache off
                    rate_limit only app
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected default client retry");
        };
        assert_eq!(client_retry.max_attempts, 2);
        assert_eq!(client_retry.statuses, [429]);
        assert!(client_retry.respect_retry_after);

        let Some(CacheResolved::Set(client_cache)) = &resolved_api.client_policy.cache else {
            panic!("expected default client cache");
        };
        assert_eq!(client_cache.default_ttl_secs, Some(30));
        assert_eq!(client_cache.revalidate, Some(true));

        let Some(RateLimitResolved::Add(client_rate_limit)) =
            &resolved_api.client_policy.rate_limit
        else {
            panic!("expected default client rate limit");
        };
        assert_eq!(client_rate_limit.buckets.len(), 1);

        let endpoint_policy = &resolved_api.endpoints[0].policy.endpoint;
        assert!(matches!(endpoint_policy.retry, Some(RetryResolved::Clear)));
        assert!(matches!(endpoint_policy.cache, Some(CacheResolved::Clear)));
        assert!(matches!(
            endpoint_policy.rate_limit,
            Some(RateLimitResolved::Replace(_))
        ));
    }

    #[test]
    fn behavior_cache_profile_resolves_before_local_override() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    cache standard {
                        ttl 30s
                    }

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    behavior protected_read {
                        retry read
                        cache standard
                        rate_limit app
                    }
                }

                scope users {
                    path ["users"]
                    behavior protected_read

                    GET Me
                        path ["me"]
                        cache off
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = &resolved_api.endpoints[0];

        assert!(matches!(
            endpoint
                .policy
                .scopes
                .first()
                .and_then(|scope| scope.cache.as_ref()),
            Some(CacheResolved::Set(_))
        ));
        assert!(matches!(
            endpoint.policy.endpoint.cache,
            Some(CacheResolved::Clear)
        ));
    }

    #[test]
    fn behavior_rate_limit_merges_with_local_rate_limit() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    rate_limit users {
                        bucket method by [host, endpoint] {
                            5 / 1s
                        }
                    }

                    behavior base_read {
                        rate_limit app
                    }
                }

                scope users {
                    path ["users"]
                    behavior base_read
                    rate_limit users

                    GET List
                    path []
                    -> Json<()>
                }

                GET RootList
                path ["root"]
                behavior base_read
                rate_limit users
                -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let scope_endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "List")
            .expect("scope endpoint");
        let scope_rate_limit = scope_endpoint
            .policy
            .scopes
            .first()
            .and_then(|scope| scope.rate_limit.as_ref())
            .expect("scope rate limit");
        let scope_bucket_names = match scope_rate_limit {
            RateLimitResolved::Add(plan) | RateLimitResolved::Replace(plan) => plan
                .buckets
                .iter()
                .map(|bucket| bucket.name.clone())
                .collect::<Vec<_>>(),
            RateLimitResolved::Clear => panic!("expected merged scope rate limit"),
        };
        assert_eq!(
            scope_bucket_names,
            vec!["app_0".to_string(), "users_0".to_string()]
        );

        let root_endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "RootList")
            .expect("root endpoint");
        let root_rate_limit = root_endpoint
            .policy
            .endpoint
            .rate_limit
            .as_ref()
            .expect("endpoint rate limit");
        let root_bucket_names = match root_rate_limit {
            RateLimitResolved::Add(plan) | RateLimitResolved::Replace(plan) => plan
                .buckets
                .iter()
                .map(|bucket| bucket.name.clone())
                .collect::<Vec<_>>(),
            RateLimitResolved::Clear => panic!("expected merged endpoint rate limit"),
        };
        assert_eq!(
            root_bucket_names,
            vec!["app_0".to_string(), "users_0".to_string()]
        );
    }

    #[test]
    fn behavior_rate_limit_resolves_with_scope_key_binding() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit tenant_bucket {
                        bucket method by [tenant_key] {
                            5 / 1s
                        }
                    }

                    behavior tenant_read {
                        rate_limit tenant_bucket
                    }
                }

                scope tenants(tenant: String) {
                    path ["tenants", tenant]
                    rate_limit key tenant_key = tenant
                    behavior tenant_read

                    GET List
                    path ["items"]
                    -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "List")
            .expect("List endpoint");
        let scope_policy = endpoint.policy.scopes.first().expect("scope policy");
        let rate_limit = scope_policy
            .rate_limit
            .as_ref()
            .expect("resolved scope rate limit");
        let plan = match rate_limit {
            RateLimitResolved::Add(plan) | RateLimitResolved::Replace(plan) => plan,
            RateLimitResolved::Clear => panic!("expected resolved rate limit"),
        };
        assert_eq!(plan.buckets.len(), 1);
        let bucket = &plan.buckets[0];
        assert!(bucket.key.iter().any(|key| matches!(
            key,
            RateLimitKeyResolved::EpField { name, field }
                if name == "tenant_key" && field.to_string() == "tenant"
        )));
    }

    #[test]
    fn default_behavior_applies_to_client_policy() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    behavior protected_read {
                        retry read
                        rate_limit app
                    }

                    default {
                        behavior protected_read
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected default behavior retry");
        };
        assert_eq!(client_retry.max_attempts, 2);

        let Some(RateLimitResolved::Add(client_rate_limit)) =
            &resolved_api.client_policy.rate_limit
        else {
            panic!("expected default behavior rate limit");
        };
        assert_eq!(client_rate_limit.buckets.len(), 1);
    }

    #[test]
    fn explicit_default_cache_overrides_default_behavior_cache() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    cache standard {
                        ttl 30s
                    }

                    behavior cached {
                        cache standard
                    }

                    default {
                        behavior cached
                        cache off
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        assert!(matches!(
            resolved_api.client_policy.cache,
            Some(CacheResolved::Clear)
        ));
    }

    #[test]
    fn explicit_default_retry_and_cache_still_override_default_behavior() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry from_behavior {
                        max_attempts 5
                        methods [GET]
                    }

                    retry explicit {
                        max_attempts 2
                        methods [GET]
                    }

                    cache standard {
                        ttl 30s
                    }

                    behavior cached_read {
                        retry from_behavior
                        cache standard
                    }

                    default {
                        behavior cached_read
                        retry explicit
                        cache off
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected explicit default retry");
        };
        assert_eq!(client_retry.max_attempts, 2);

        assert!(matches!(
            resolved_api.client_policy.cache,
            Some(CacheResolved::Clear)
        ));
    }

    #[test]
    fn default_behavior_auth_applies_to_endpoint() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                    credential session = bearer(secret.token)

                    behavior protected {
                        auth bearer session
                    }

                    default {
                        behavior protected
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = &resolved_api.endpoints[0];

        assert_eq!(endpoint.policy.auth.len(), 1);
        let AuthUsePlanIr::Use(auth_use) = &endpoint.policy.auth[0];
        assert_eq!(
            auth_use_credential_ident_ir(auth_use).to_string(),
            "session"
        );
        assert!(matches!(auth_use.provenance, AuthUseProvenanceIr::Client));
    }

    #[test]
    fn unknown_policy_profile_fails_during_resolution() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    path ["ping"]
                    retry missing
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("unknown retry profile must fail");

        assert!(err.to_string().contains("unknown retry profile"));
    }

    #[test]
    fn rate_limit_observer_path_is_resolved_on_api() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    observe rate_limit crate::Observer

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }
                }

                GET Ping
                    path ["ping"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let observer = resolved_api
            .rate_limit_response_policy
            .as_ref()
            .expect("rate limit observer");

        assert!(quote::quote!(#observer).to_string().contains("Observer"));
    }

    #[test]
    fn retry_attempts_rejected_before_resolution() {
        let err = syn::parse_str::<crate::ast::RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry read {
                        attempts 2
                    }
                }
            }
            "#,
        )
        .expect_err("attempts syntax must fail");

        assert!(err.to_string().contains("`attempts` is not supported"));
    }

    #[test]
    fn body_signature_response_and_map_resolve_into_endpoint_model() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client BodyApi {
                base "https://example.com"
            }

            POST Create(body: Json<CreateBody>)
                as create
                path ["items"]
                -> Json<CreateResponse>
                map Created {
                    Created::from(r)
                }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Create")
            .expect("Create endpoint");

        let body = endpoint.body.as_ref().expect("body resolved");
        let body_ty = &body.ty;
        let response_ty = &endpoint.response.ty;
        let map = endpoint.map.as_ref().expect("map resolved");
        let map_ty = &map.out_ty;

        assert_eq!(quote::quote!(#body_ty).to_string(), "CreateBody");
        assert_eq!(quote::quote!(#response_ty).to_string(), "CreateResponse");
        assert_eq!(quote::quote!(#map_ty).to_string(), "Created");
    }

    #[test]
    fn pagination_controllers_resolve_into_endpoint_model() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET Offset(start: u64 = 0, count: u64 = 20)
                path ["offset"]
                query {
                    start
                    count
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>

            GET Cursor(cursor?: String, count: u64 = 20)
                path ["cursor"]
                query {
                    cursor
                    count
                }
                paginate CursorPagination {
                    cursor = cursor,
                    per_page = count
                }
                -> Json<Vec<String>>

            GET Paged(page: u64 = 1, count: u64 = 20)
                path ["paged"]
                query {
                    page
                    count
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let controllers = resolved_api
            .endpoints
            .iter()
            .map(|ep| {
                let pagination = ep.paginate.as_ref().expect("pagination resolved");
                (
                    ep.name.to_string(),
                    pagination
                        .ctrl_ty
                        .segments
                        .last()
                        .expect("controller type")
                        .ident
                        .to_string(),
                    pagination.assigns.len(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            controllers,
            vec![
                ("Offset".to_string(), "OffsetLimitPagination".to_string(), 2),
                ("Cursor".to_string(), "CursorPagination".to_string(), 2),
                ("Paged".to_string(), "PagedPagination".to_string(), 2),
            ]
        );
    }

    #[test]
    fn unknown_pagination_field_fails_resolution() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET Offset(count: u64 = 20)
                path ["offset"]
                query {
                    count
                }
                paginate OffsetLimitPagination {
                    per_page = count
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("unknown pagination assignment must fail");

        assert!(
            err.to_string()
                .contains("unknown pagination field `per_page` for OffsetLimitPagination"),
            "{err}"
        );
    }

    #[test]
    fn resolved_endpoint_snapshots_cover_v47_cases() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client SnapshotApi {
                base "https://example.com"
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
