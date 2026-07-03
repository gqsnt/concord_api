//! Semantic normalization and resolution for the Concord macro.
//!
//! This layer normalizes the raw parser API tree, validates names, resolves
//! inherited route/policy/auth state, and produces `ResolvedApi` /
//! `ResolvedEndpoint`. Codegen must consume this resolved model instead of raw
//! parser structures.

use crate::ast::{
    AuthCredentialKind, AuthCredentials, AuthUseKind, BehaviorProfileDef, BehaviorProfilesBlock,
    BehaviorUseSpec, FmtPiece, FmtSpec, KeySpec, PaginateSpec, PolicyBlock, PolicyBlocks,
    PolicyStmt, PolicyValue, RateLimitDurationUnit, RateLimitKeyBindingSpec, RateLimitKeySpec,
    RateLimitPlanSpec, RateLimitProfilesBlock, RateLimitSpec, RawIoSpec, RawResponseIo, RefScope,
    RetryIdempotencySpec, RetryPatch, RetryProfilesBlock, RetrySpec, RouteAtom, SecretRef,
};
use crate::emit_helpers;
use crate::model::facade::{
    build_facade_ir, client_prefixed_type_name, generated_acquire_as_trait_type_name,
    generated_auth_facade_type_name, generated_auth_handle_type_name,
    generated_endpoint_request_ext_trait_type_name, ident_path_strings,
};
use crate::model::*;
use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet as PublicNameSet};
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
    let mut client_vars: Vec<VarInfo> = Vec::new();
    if let Some(vb) = &norm.client.vars {
        for d in &vb.decls {
            let was_present = client_vars_map.contains_key(&d.rust.to_string());
            upsert_var(
                &mut client_vars_map,
                &d.rust,
                d.optional,
                &d.ty,
                d.default.as_ref(),
            )?;
            if !was_present {
                let resolved = client_vars_map.get(&d.rust.to_string()).ok_or_else(|| {
                    syn::Error::new(
                        d.rust.span(),
                        "internal resolver error: inserted client var missing from resolution map",
                    )
                })?;
                client_vars.push(resolved.clone());
            }
        }
    }

    // secret vars: only from `secret {}`.
    let mut auth_vars_map: BTreeMap<String, VarInfo> = BTreeMap::new();
    let mut client_auth_vars: Vec<VarInfo> = Vec::new();
    if let Some(vb) = &norm.client.auth_vars {
        for d in &vb.decls {
            let was_present = auth_vars_map.contains_key(&d.rust.to_string());
            upsert_var(
                &mut auth_vars_map,
                &d.rust,
                d.optional,
                &d.ty,
                d.default.as_ref(),
            )?;
            if !was_present {
                let resolved = auth_vars_map.get(&d.rust.to_string()).ok_or_else(|| {
                    syn::Error::new(
                        d.rust.span(),
                        "internal resolver error: inserted auth var missing from resolution map",
                    )
                })?;
                client_auth_vars.push(resolved.clone());
            }
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

    let retry_profiles = resolve_retry_profiles(norm.client.retry_profiles.as_ref())?;
    let rate_limit_profiles = resolve_rate_limit_profiles(norm.client.rate_limit.as_ref())?;
    let behavior_profiles = resolve_behavior_profiles(
        norm.client.behavior_profiles.as_ref(),
        &retry_profiles,
        &rate_limit_profiles,
    )?;
    validate_behavior_uses_unique_at_site(&norm.client.default_behavior_uses)?;
    let client_default_behavior_names = behavior_use_names(&norm.client.default_behavior_uses);
    let default_behavior =
        resolve_behavior_uses(&norm.client.default_behavior_uses, &behavior_profiles)?;
    let default_behavior_rate_limit = resolve_behavior_rate_limit_specs(
        &default_behavior.rate_limit_specs,
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
        RateLimitAttachmentContext::ClientBase,
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
    let client_retry_directive = explicit_client_retry.or(default_behavior.retry.clone());
    let (client_retry, inherited_retry) = materialize_retry_directive(None, client_retry_directive);
    client_policy.retry = client_retry;
    let explicit_default_rate_limit = resolve_client_rate_limit(
        norm.client.rate_limit.as_ref(),
        &rate_limit_profiles,
        &BTreeMap::new(),
        None,
    )?;
    client_policy.rate_limit =
        merge_rate_limit_resolved(default_behavior_rate_limit, explicit_default_rate_limit);

    // walk layers/endpoints
    let mut layers: Vec<LayerIr> = Vec::new();
    let mut endpoints: Vec<ResolvedEndpoint> = Vec::new();

    let mut ancestry: Vec<usize> = Vec::new();
    let mut walk_ctx = WalkItemsCtx {
        client_vars: &client_vars_map,
        auth_vars: &auth_vars_map,
        auth_credentials: &auth_credential_map,
        client_auth: &client_auth,
        client_default_behavior_names: &client_default_behavior_names,
        retry_profiles: &retry_profiles,
        rate_limit_profiles: &rate_limit_profiles,
        behavior_profiles: &behavior_profiles,
        layers: &mut layers,
        endpoints: &mut endpoints,
    };
    walk_items(&norm.items, &mut ancestry, &mut walk_ctx, inherited_retry)?;

    let resolved_api = ResolvedApi {
        mod_name,
        client_name,
        scheme: norm.client.scheme,
        domain: norm.client.host,
        client_vars,
        client_auth_vars,
        client_auth_credentials,
        client_policy,
        rate_limit_response_policy: norm
            .client
            .rate_limit
            .as_ref()
            .and_then(|block| block.response_policy.clone()),
        endpoints,
    };

    validate_generated_public_api(&resolved_api)?;

    Ok(resolved_api)
}

fn validate_generated_public_api(api: &ResolvedApi) -> Result<()> {
    let mut errors = None;
    let facade_ir = build_facade_ir(api);

    reject_raw_ident(&mut errors, &api.client_name, "client type");
    for var in api.client_vars.iter().chain(api.client_auth_vars.iter()) {
        reject_raw_ident(&mut errors, &var.rust, "client parameter");
    }
    for credential in &api.client_auth_credentials {
        reject_raw_ident(&mut errors, &credential.name, "credential");
    }
    for endpoint in &api.endpoints {
        reject_raw_ident(&mut errors, &endpoint.name, "endpoint");
        if let Some(alias) = &endpoint.alias {
            reject_raw_ident(&mut errors, alias, "endpoint alias");
        }
        for scope in &endpoint.scope_modules {
            reject_raw_ident(&mut errors, scope, "scope");
        }
        for var in &endpoint.vars {
            reject_raw_ident(&mut errors, &var.rust, "endpoint parameter");
        }
        for group in &endpoint.facade_param_groups {
            for var in group {
                reject_raw_ident(&mut errors, &var.rust, "scope parameter");
            }
        }
    }

    validate_client_method_namespace(api, &facade_ir, &mut errors);
    validate_builder_namespace(api, &mut errors);
    validate_auth_facade_namespace(api, &mut errors);
    validate_scope_method_namespaces(api, &facade_ir, &mut errors);
    validate_generated_type_namespace(api, &facade_ir, &mut errors);

    match errors {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn validate_client_method_namespace(
    api: &ResolvedApi,
    facade_ir: &crate::model::facade::FacadeIr,
    errors: &mut Option<syn::Error>,
) {
    let mut ns = PublicNameNamespace::new("generated client");
    for name in [
        "new",
        "new_with_transport",
        "builder",
        "debug_level",
        "set_debug_level",
        "with_debug_level",
        "pagination_detect_loops",
        "set_pagination_detect_loops",
        "with_pagination_detect_loops",
        "configure",
        "configure_mut",
        "request",
        "auth_state",
    ] {
        ns.reserve(name, "generated client method", api.client_name.span());
    }
    let root_auth_scope_exists = facade_ir
        .scopes
        .iter()
        .any(|scope| scope.path.first().is_some_and(|part| part == "auth"));
    if !root_auth_scope_exists {
        ns.reserve("auth", "generated client method", api.client_name.span());
    }

    for var in &api.client_vars {
        ns.add(
            errors,
            format!("set_{}", var.rust),
            var.rust.span(),
            "client parameter setter",
        );
        if var.optional {
            ns.add(
                errors,
                format!("clear_{}", var.rust),
                var.rust.span(),
                "client parameter clearer",
            );
        }
    }
    for var in &api.client_auth_vars {
        ns.add(
            errors,
            format!("set_{}", var.rust),
            var.rust.span(),
            "client secret setter",
        );
        if var.optional {
            ns.add(
                errors,
                format!("clear_{}", var.rust),
                var.rust.span(),
                "client secret clearer",
            );
        }
    }
    for methods in &facade_ir.credential_methods {
        for public_name in [
            &methods.acquire_name,
            &methods.set_name,
            &methods.clear_name,
            &methods.has_name,
        ] {
            ns.add(
                errors,
                public_name.to_string(),
                public_name.span(),
                "endpoint-backed credential helper",
            );
        }
    }

    for scope in facade_ir
        .scopes
        .iter()
        .filter(|scope| scope.path.len() == 1)
    {
        let span =
            scope_public_name_span(api, &scope.path).unwrap_or_else(|| scope.public_method.span());
        ns.add(
            errors,
            scope.public_method.to_string(),
            span,
            "root scope accessor",
        );
    }

    for endpoint in facade_ir
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.scope_path.is_empty())
    {
        let span = resolved_endpoint_for_facade(api, endpoint)
            .map(endpoint_public_name_span)
            .unwrap_or_else(|| endpoint.public_method.span());
        ns.add(
            errors,
            endpoint.public_method.to_string(),
            span,
            "root endpoint method",
        );
    }
}

fn validate_builder_namespace(api: &ResolvedApi, errors: &mut Option<syn::Error>) {
    let mut ns = PublicNameNamespace::new("generated client builder");
    for name in ["new", "build"] {
        ns.reserve(name, "generated builder method", api.client_name.span());
    }
    for var in api
        .client_vars
        .iter()
        .chain(api.client_auth_vars.iter())
        .filter(|var| !var.optional && var.default.is_none())
    {
        ns.add(
            errors,
            var.rust.to_string(),
            var.rust.span(),
            "required constructor parameter builder setter",
        );
    }
}

fn validate_auth_facade_namespace(api: &ResolvedApi, errors: &mut Option<syn::Error>) {
    let mut ns = PublicNameNamespace::new("generated auth-state facade");
    for credential in &api.client_auth_credentials {
        if matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
            ns.add(
                errors,
                credential.name.to_string(),
                credential.name.span(),
                "endpoint-backed credential auth-state accessor",
            );
        }
    }
}

fn validate_scope_method_namespaces(
    api: &ResolvedApi,
    facade_ir: &crate::model::facade::FacadeIr,
    errors: &mut Option<syn::Error>,
) {
    let mut namespaces: BTreeMap<Vec<String>, PublicNameNamespace> = BTreeMap::new();

    for scope in &facade_ir.scopes {
        let path = scope.path.clone();
        for setter in &scope.setters {
            scope_namespace(&mut namespaces, &path).add(
                errors,
                setter.set_name.to_string(),
                setter.set_name.span(),
                "scope parameter setter",
            );
            if setter
                .forms
                .contains(&crate::model::facade::SetterForm::Clear)
            {
                scope_namespace(&mut namespaces, &path).add(
                    errors,
                    setter.clear_name.to_string(),
                    setter.clear_name.span(),
                    "scope parameter clearer",
                );
            }
        }
        for method in &scope.methods {
            scope_namespace(&mut namespaces, &path).add(
                errors,
                method.public_name.to_string(),
                method.public_name.span(),
                "scope accessor",
            );
        }
    }

    for endpoint in &facade_ir.endpoints {
        if !endpoint.scope_path.is_empty() {
            let span = resolved_endpoint_for_facade(api, endpoint)
                .map(endpoint_public_name_span)
                .unwrap_or_else(|| endpoint.public_method.span());
            scope_namespace(&mut namespaces, &endpoint.scope_path).add(
                errors,
                endpoint.public_method.to_string(),
                span,
                "endpoint method",
            );
        }
    }
}

fn resolved_endpoint_for_facade<'a>(
    api: &'a ResolvedApi,
    facade_endpoint: &crate::model::facade::FacadeEndpoint,
) -> Option<&'a ResolvedEndpoint> {
    api.endpoints.iter().find(|endpoint| {
        endpoint.scope_modules == facade_endpoint.target.scope_path
            && endpoint.name == facade_endpoint.target.endpoint
    })
}

fn endpoint_public_name_span(endpoint: &ResolvedEndpoint) -> Span {
    endpoint.alias.as_ref().unwrap_or(&endpoint.name).span()
}

fn scope_public_name_span(api: &ResolvedApi, path: &[Ident]) -> Option<Span> {
    if path.is_empty() {
        return None;
    }
    api.endpoints.iter().find_map(|endpoint| {
        if endpoint.scope_modules.len() < path.len() {
            return None;
        }
        let matches_path = endpoint
            .scope_modules
            .iter()
            .zip(path.iter())
            .all(|(actual, expected)| actual == expected);
        matches_path.then(|| endpoint.scope_modules[path.len() - 1].span())
    })
}

fn validate_generated_type_namespace(
    api: &ResolvedApi,
    facade_ir: &crate::model::facade::FacadeIr,
    errors: &mut Option<syn::Error>,
) {
    let mut ns = PublicNameNamespace::new("generated module type namespace");
    let span = api.client_name.span();
    ns.add(errors, api.client_name.to_string(), span, "client type");
    for suffix in [
        "Vars",
        "AuthInner",
        "AuthVars",
        "AuthState",
        "Cx",
        "Builder",
    ] {
        ns.add(
            errors,
            client_prefixed_type_name(&api.client_name, suffix),
            span,
            "generated client support type",
        );
    }
    ns.add(
        errors,
        generated_auth_facade_type_name(&api.client_name),
        span,
        "generated auth-state facade type",
    );

    for credential in &api.client_auth_credentials {
        if matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
            ns.add(
                errors,
                generated_auth_handle_type_name(&api.client_name, &credential.name),
                credential.name.span(),
                "endpoint-backed credential auth handle type",
            );
            ns.add(
                errors,
                generated_acquire_as_trait_type_name(&api.client_name, &credential.name),
                credential.name.span(),
                "endpoint-backed credential request extension trait",
            );
        }
    }

    for scope in &facade_ir.scopes {
        ns.add(
            errors,
            scope.rust_type_name.to_string(),
            scope.rust_type_name.span(),
            "scope facade type",
        );
    }

    for endpoint in &api.endpoints {
        ns.add(
            errors,
            generated_endpoint_request_ext_trait_type_name(endpoint),
            endpoint.name.span(),
            "endpoint request extension trait",
        );
    }

    validate_endpoint_public_type_namespaces(api, errors);
}

fn validate_endpoint_public_type_namespaces(api: &ResolvedApi, errors: &mut Option<syn::Error>) {
    let mut namespaces: BTreeMap<Vec<String>, PublicNameNamespace> = BTreeMap::new();
    let mut child_modules_seen: PublicNameSet<(Vec<String>, String)> = PublicNameSet::new();

    for endpoint in &api.endpoints {
        let path = endpoint.scope_modules.clone();
        endpoint_type_namespace(&mut namespaces, &path).add(
            errors,
            endpoint.name.to_string(),
            endpoint.name.span(),
            "endpoint marker type",
        );

        for idx in 0..endpoint.scope_modules.len() {
            let parent = endpoint.scope_modules[..idx].to_vec();
            let child = endpoint.scope_modules[idx].to_string();
            if child_modules_seen.insert((ident_path_strings(&parent), child.clone())) {
                endpoint_type_namespace(&mut namespaces, &parent).add(
                    errors,
                    child,
                    endpoint.scope_modules[idx].span(),
                    "endpoint scope module",
                );
            }
        }
    }
}

fn endpoint_type_namespace<'a>(
    namespaces: &'a mut BTreeMap<Vec<String>, PublicNameNamespace>,
    path: &[Ident],
) -> &'a mut PublicNameNamespace {
    namespaces
        .entry(ident_path_strings(path))
        .or_insert_with(|| PublicNameNamespace::new(endpoint_type_namespace_label(path)))
}

fn endpoint_type_namespace_label(path: &[Ident]) -> String {
    if path.is_empty() {
        "generated endpoints module".to_string()
    } else {
        format!(
            "generated endpoints::{} module",
            ident_path_strings(path).join("::")
        )
    }
}

fn scope_namespace<'a>(
    namespaces: &'a mut BTreeMap<Vec<String>, PublicNameNamespace>,
    path: &[Ident],
) -> &'a mut PublicNameNamespace {
    namespaces
        .entry(ident_path_strings(path))
        .or_insert_with(|| PublicNameNamespace::new(scope_namespace_label(path)))
}

fn scope_namespace_label(path: &[Ident]) -> String {
    if path.is_empty() {
        "generated client".to_string()
    } else {
        format!(
            "generated `{}` scope facade",
            ident_path_strings(path).join("::")
        )
    }
}

struct PublicNameNamespace {
    label: String,
    names: BTreeMap<String, PublicNameSource>,
}

impl PublicNameNamespace {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            names: BTreeMap::new(),
        }
    }

    fn reserve(&mut self, name: &'static str, kind: &'static str, span: Span) {
        self.names
            .insert(name.to_string(), PublicNameSource { kind, span });
    }

    fn add(
        &mut self,
        errors: &mut Option<syn::Error>,
        name: String,
        span: Span,
        kind: &'static str,
    ) {
        if let Some(existing) = self.names.get(&name) {
            push_error(
                errors,
                syn::Error::new(
                    span,
                    format!(
                        "generated public API name `{name}` for {kind} conflicts with {} in {}",
                        existing.kind, self.label
                    ),
                ),
            );
        } else {
            self.names.insert(name, PublicNameSource { kind, span });
        }
    }
}

struct PublicNameSource {
    kind: &'static str,
    #[allow(dead_code)]
    span: Span,
}

fn reject_raw_ident(errors: &mut Option<syn::Error>, ident: &Ident, kind: &'static str) {
    let name = ident.to_string();
    if name.starts_with("r#") {
        push_error(
            errors,
            syn::Error::new(
                ident.span(),
                format!(
                    "raw Rust identifier `{name}` is not supported for generated public API {kind} names"
                ),
            ),
        );
    }
}

fn push_error(errors: &mut Option<syn::Error>, error: syn::Error) {
    match errors {
        Some(existing) => existing.combine(error),
        None => *errors = Some(error),
    }
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("common.rs");
include!("auth.rs");
include!("retry.rs");
include!("rate_limit.rs");
include!("items.rs");
include!("policy.rs");

#[cfg(test)]
fn analyze_source(source: &str) -> ResolvedApi {
    let ast: crate::ast::RawApi = syn::parse_str(source).expect("valid api syntax");
    analyze(ast).expect("analysis succeeds")
}

#[cfg(test)]
fn endpoint_by_name<'a>(api: &'a ResolvedApi, name: &str) -> &'a ResolvedEndpoint {
    api.endpoints
        .iter()
        .find(|endpoint| endpoint.name == name)
        .unwrap_or_else(|| panic!("missing endpoint `{name}`"))
}

#[cfg(test)]
fn rate_limit_plan(rate_limit: &RateLimitResolved) -> &RateLimitPlanResolved {
    match rate_limit {
        RateLimitResolved::Add(plan) | RateLimitResolved::Replace(plan) => plan,
        RateLimitResolved::Clear => panic!("expected resolved rate limit"),
    }
}

#[cfg(test)]
fn effective_endpoint_rate_limit_bucket_names(
    api: &ResolvedApi,
    endpoint: &ResolvedEndpoint,
) -> Vec<String> {
    let mut bucket_names = Vec::new();
    let mut apply = |layer: &Option<RateLimitResolved>| match layer {
        Some(RateLimitResolved::Add(plan)) | Some(RateLimitResolved::Replace(plan)) => {
            bucket_names.extend(plan.buckets.iter().map(|bucket| bucket.name.clone()));
        }
        Some(RateLimitResolved::Clear) => bucket_names.clear(),
        None => {}
    };

    apply(&api.client_policy.rate_limit);
    for scope in &endpoint.policy.scopes {
        apply(&scope.rate_limit);
    }
    apply(&endpoint.policy.endpoint.rate_limit);

    bucket_names
}

#[cfg(test)]
fn auth_requirement_names(auth: &[AuthRequirementIr]) -> Vec<String> {
    auth.iter().map(|req| req.credential.to_string()).collect()
}

#[cfg(test)]
fn auth_requirement_provenances(auth: &[AuthRequirementIr]) -> Vec<AuthUseProvenanceIr> {
    auth.iter()
        .map(|req| match req.provenance.label.as_str() {
            "client" => AuthUseProvenanceIr::Client,
            "endpoint" => AuthUseProvenanceIr::Endpoint,
            label if label.starts_with("scope:") => {
                let scope_id = label
                    .trim_start_matches("scope:")
                    .parse::<usize>()
                    .expect("scope provenance parses");
                AuthUseProvenanceIr::Scope(scope_id)
            }
            other => panic!("unexpected auth provenance label: {other}"),
        })
        .collect()
}

#[cfg(test)]
fn auth_requirement_step_ids(auth: &[AuthRequirementIr]) -> Vec<String> {
    auth.iter().map(|req| req.step_id.clone()).collect()
}

#[cfg(test)]
fn debug_norm_tree(norm: &NormApiTree) -> String {
    fn walk(items: &[NormNode], depth: usize, out: &mut String) {
        for item in items {
            let indent = "  ".repeat(depth);
            match item {
                NormNode::Layer(scope) => {
                    out.push_str(&format!(
                        "{indent}scope {:?} kind={:?} params={} auth={} headers={} query={} retry={} rate_limit={}\n",
                        scope.scope_name.as_ref().map(ToString::to_string),
                        scope.kind,
                        scope.params.len(),
                        scope.auth_uses.len(),
                        scope.policy.headers.as_ref().map_or(0, |h| h.stmts.len()),
                        scope.policy.query.as_ref().map_or(0, |q| q.stmts.len()),
                        scope.retry.is_some(),
                        scope.rate_limit.is_some(),
                    ));
                    walk(&scope.items, depth + 1, out);
                }
                NormNode::Endpoint(endpoint) => {
                    out.push_str(&format!(
                        "{indent}endpoint {} method={} alias={:?} params={} body={} query={} paginate={}\n",
                        endpoint.name,
                        endpoint.method,
                        endpoint.alias.as_ref().map(ToString::to_string),
                        endpoint.params.len(),
                        endpoint.body.is_some(),
                        endpoint.policy.query.as_ref().map_or(0, |q| q.stmts.len()),
                        endpoint.paginate.is_some(),
                    ));
                }
            }
        }
    }

    let mut out = format!(
        "client {} vars={} secrets={} auth={} retry_profiles={} rate_profiles={}\n",
        norm.client.name,
        norm.client.vars.as_ref().map_or(0, |v| v.decls.len()),
        norm.client.auth_vars.as_ref().map_or(0, |v| v.decls.len()),
        norm.client.auth_uses.len(),
        norm.client
            .retry_profiles
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
            "scopes={} headers={} query={} auth={} retry={} rate_limit={}",
            ep.policy.scopes.len(),
            ep.policy.endpoint.headers.len(),
            ep.policy.endpoint.query.len(),
            ep.policy.auth.len(),
            ep.policy.endpoint.retry.is_some(),
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
            "{} method={} route=[{}] params=[{}] policy=[{}] facade={} response={:?} body={} pagination={}\n",
            ep.name,
            ep.method,
            route,
            params,
            policy,
            facade,
            ep.io.response_entity.public_output_ty,
            ep.io.request_entity.capabilities.has_body,
            ep.paginate.is_some(),
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
                value: PolicyValue::Expr(Expr::Path(path)),
                op: SetOp::Set,
            } => {
                assert_eq!(key.to_string(), "q");
                assert_eq!(path.path.segments.len(), 1);
                assert_eq!(path.path.segments[0].ident, "q");
            }
            other => panic!("query shorthand should remain raw in normalized tree: {other:?}"),
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
    fn generated_public_name_collisions_are_rejected() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client Collision {
                base "https://example.com"
                var debug_level: u8
            }

            GET Ping
                path ["ping"]
                -> Json<String>
            "#,
        )
        .expect("valid api syntax");

        let err = analyze(ast).expect_err("generated public name collisions must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("set_debug_level"),
            "collision should mention the generated setter name: {msg}"
        );
        assert!(
            msg.contains("generated client method"),
            "collision should mention the namespace: {msg}"
        );
    }

    #[test]
    fn resolved_query_shorthand_lowers_to_endpoint_field_semantics() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search(q: String)
                    path ["search"]
                    query {
                        q
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Search")
            .expect("Search endpoint");

        let query = endpoint
            .policy
            .endpoint
            .query
            .first()
            .expect("resolved query op");
        match query {
            PolicyOp::Set {
                value: PolicySetValue::Value(PublicValueKind::EpField(field)),
                ..
            } => assert_eq!(field.to_string(), "q"),
            other => panic!("query shorthand did not lower to endpoint field semantics: {other:?}"),
        }
    }

    #[test]
    fn direct_secret_policy_expressions_are_rejected_during_analysis() {
        for (label, source) in [
            (
                "headers",
                r#"
                api! {
                    client Api {
                        base "https://example.com"
                        secret token: String
                    }

                    GET HeaderRef
                        path ["header"]
                        headers {
                            "x-api-key" = secret.token
                        }
                        -> Json<String>
                }
                "#,
            ),
            (
                "query",
                r#"
                api! {
                    client Api {
                        base "https://example.com"
                        secret token: String
                    }

                    GET QueryRef
                        path ["query"]
                        query {
                            token = secret.token
                        }
                        -> Json<String>
                }
                "#,
            ),
            (
                "timeout",
                r#"
                api! {
                    client Api {
                        base "https://example.com"
                        secret token: String
                    }

                    GET TimeoutRef
                        path ["timeout"]
                        timeout: secret.token
                        -> Json<String>
                }
                "#,
            ),
            (
                "pagination",
                r#"
                api! {
                    client Api {
                        base "https://example.com"
                        secret token: String
                    }

                    GET PageRef(start: u64 = 0)
                        path ["page"]
                        query {
                            start
                        }
                        paginate OffsetLimitPagination {
                            offset = secret.token,
                            limit = start
                        }
                        -> Json<Vec<String>>
                }
                "#,
            ),
        ] {
            let ast: crate::ast::RawApi = syn::parse_str(source).expect("raw syntax parses");
            let err = analyze(ast).expect_err(label);
            assert!(
                err.to_string().contains("DSL-010"),
                "{label} should be rejected by sema: {err}"
            );
        }
    }

    #[test]
    fn normalized_tree_splits_raw_scope_host_and_path_into_canonical_layers() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client NormSplitApi {
                    base "https://example.com"
                    secret token: String
                    credential key = api_key(secret.token)
                }

                scope tenant(tenant_id: String) {
                    host [fmt["tenant-", tenant_id], "api"]
                    path ["v1"]
                    auth header "X-Token" = key

                    GET Show
                        path ["profile"]
                        -> Json<String>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let norm = normalize_api(ast).expect("normalization succeeds");

        assert_eq!(norm.items.len(), 1);
        let NormNode::Layer(outer) = &norm.items[0] else {
            panic!("expected outer scope layer");
        };
        assert_eq!(
            outer
                .scope_name
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("tenant")
        );
        assert!(matches!(outer.kind, RouteLayerKind::Prefix));
        assert_eq!(outer.auth_uses.len(), 1);
        assert_eq!(outer.items.len(), 1);
        let NormNode::Layer(inner) = &outer.items[0] else {
            panic!("expected inner path layer");
        };
        assert!(inner.scope_name.is_none());
        assert!(matches!(inner.kind, RouteLayerKind::Path));
        assert!(inner.auth_uses.is_empty());
        assert!(inner.policy.headers.is_none());
        assert!(inner.policy.query.is_none());
        assert!(inner.retry.is_none());
        assert!(inner.rate_limit.is_none());
        assert!(inner.rate_limit_keys.is_empty());
        assert_eq!(inner.items.len(), 1);
        let NormNode::Endpoint(endpoint) = &inner.items[0] else {
            panic!("expected endpoint under inner path layer");
        };
        assert_eq!(endpoint.name, "Show");
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
                host ["tenant"]
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
        assert!(snapshot.contains("prefix=[Static(\"tenant\")]"));
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
                value: PolicySetValue::OptionalEpField(field),
                ..
            } if key.value() == "maybe" && field == "maybe"
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
        assert_eq!(
            auth_requirement_names(auth),
            ["client_key", "scope_key", "token"]
        );
        assert_eq!(
            auth_requirement_step_ids(auth),
            [
                "protected::Me:0:client_key",
                "protected::Me:1:scope_key",
                "protected::Me:2:token"
            ]
        );
        assert_eq!(
            auth_requirement_provenances(auth),
            [
                AuthUseProvenanceIr::Client,
                AuthUseProvenanceIr::Scope(0),
                AuthUseProvenanceIr::Endpoint,
            ]
        );
        assert!(matches!(auth[0].placement, AuthPlacementIr::Header { .. }));
        assert!(matches!(auth[1].placement, AuthPlacementIr::Query { .. }));
        assert!(matches!(auth[2].placement, AuthPlacementIr::Bearer));
    }

    #[test]
    fn final_auth_materialization_rejects_case_insensitive_header_collisions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret client_token: String
                    secret scope_token: String
                    credential client_auth = api_key(secret.client_token)
                    credential scope_auth = api_key(secret.scope_token)
                    auth header "X-Token" = client_auth
                }

                scope protected {
                    path ["protected"]
                    auth header "x-token" = scope_auth

                    GET Show
                        path ["show"]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("final header collision must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("final endpoint `protected::Show`"));
        assert!(msg.contains("header `x-token`"));
        assert!(msg.contains("client"));
        assert!(msg.contains("scope:0"));
    }

    #[test]
    fn final_auth_materialization_rejects_query_collisions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret client_token: String
                    secret endpoint_token: String
                    credential client_auth = api_key(secret.client_token)
                    credential endpoint_auth = api_key(secret.endpoint_token)
                    auth query "api_key" = client_auth
                }

                scope protected {
                    path ["protected"]

                    GET Show
                        path ["show"]
                        auth query "api_key" = endpoint_auth
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("final query collision must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("final endpoint `protected::Show`"));
        assert!(msg.contains("query `api_key`"));
        assert!(msg.contains("client"));
        assert!(msg.contains("endpoint"));
    }

    #[test]
    fn final_auth_materialization_rejects_bearer_plus_basic_authorization_collisions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret bearer_token: String
                    secret basic_user: String
                    secret basic_password: String
                    credential bearer_auth = bearer(secret.bearer_token)
                    credential basic_auth = basic(secret.basic_user, secret.basic_password)
                    auth bearer bearer_auth
                }

                scope protected {
                    path ["protected"]
                    auth basic basic_auth

                    GET Show
                        path ["show"]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("authorization target collisions must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("final endpoint `protected::Show`"));
        assert!(msg.contains("Authorization"));
        assert!(msg.contains("client"));
        assert!(msg.contains("scope:0"));
        assert!(msg.contains("between `client` and `scope:0`"));
    }

    #[test]
    fn final_auth_materialization_rejects_bearer_plus_custom_authorization_header_collisions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret bearer_token: String
                    secret header_token: String
                    credential bearer_auth = bearer(secret.bearer_token)
                    credential header_auth = api_key(secret.header_token)
                    auth bearer bearer_auth
                }

                GET Show
                    path ["show"]
                    auth header "Authorization" = header_auth
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("authorization target collisions must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("final endpoint `Show`"));
        assert!(msg.contains("Authorization"));
        assert!(msg.contains("client"));
        assert!(msg.contains("endpoint"));
        assert!(msg.contains("between `client` and `endpoint`"));
    }

    #[test]
    fn final_auth_materialization_rejects_duplicate_bearer_across_layers() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret client_token: String
                    secret scope_token: String
                    credential client_auth = bearer(secret.client_token)
                    credential scope_auth = bearer(secret.scope_token)
                    auth bearer client_auth
                }

                scope protected {
                    path ["protected"]
                    auth bearer scope_auth

                    GET Show
                        path ["show"]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("duplicate bearer targets must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("Authorization"));
        assert!(msg.contains("client"));
        assert!(msg.contains("scope:0"));
        assert!(msg.contains("between `client` and `scope:0`"));
    }

    #[test]
    fn final_auth_materialization_rejects_duplicate_basic_across_layers() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret client_user: String
                    secret client_pass: String
                    secret endpoint_user: String
                    secret endpoint_pass: String
                    credential client_basic = basic(secret.client_user, secret.client_pass)
                    credential endpoint_basic = basic(secret.endpoint_user, secret.endpoint_pass)
                    auth basic client_basic
                }

                GET Show
                    path ["show"]
                    auth basic endpoint_basic
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("duplicate basic targets must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("Authorization"));
        assert!(msg.contains("client"));
        assert!(msg.contains("endpoint"));
        assert!(msg.contains("between `client` and `endpoint`"));
    }

    #[test]
    fn final_auth_materialization_rejects_certificate_collisions() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    credential client_cert = endpoint auth_a::IssueClientCert
                    credential scope_cert = endpoint auth_b::IssueScopeCert
                }

                scope auth_a {
                    path ["auth-a"]

                    GET IssueClientCert
                        path ["cert"]
                        -> Json<ClientCertificate>
                }

                scope auth_b {
                    path ["auth-b"]

                    GET IssueScopeCert
                        path ["cert"]
                        -> Json<ClientCertificate>
                }

                scope protected {
                    path ["protected"]
                    auth certificate client_cert

                    GET Show
                        path ["show"]
                        auth certificate scope_cert
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("certificate collisions must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("final endpoint `protected::Show`"));
        assert!(msg.contains("certificate"));
        assert!(msg.contains("endpoint"));
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
                        -> Json<AccessToken>
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
            target, output_ty, ..
        } = &session.kind
        else {
            panic!("expected endpoint-backed credential");
        };
        assert_eq!(
            target
                .scope_modules
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["auth_api".to_string()]
        );
        assert_eq!(target.endpoint.to_string(), "Login");
        assert!(
            quote::quote!(#output_ty)
                .to_string()
                .contains("AccessToken")
        );
    }

    #[test]
    fn endpoint_backed_credential_resolves_scoped_target_unambiguously() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    credential session = endpoint auth_b::Login
                }

                scope auth_a {
                    path ["auth-a"]

                    POST Login
                        path ["login"]
                        -> Json<String>
                }

                scope auth_b {
                    path ["auth-b"]

                    POST Login
                        path ["login"]
                        -> Json<AccessToken>
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
            target, output_ty, ..
        } = &session.kind
        else {
            panic!("expected endpoint-backed credential");
        };
        assert_eq!(
            target
                .scope_modules
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["auth_b".to_string()]
        );
        assert_eq!(target.endpoint.to_string(), "Login");
        assert!(
            quote::quote!(#output_ty)
                .to_string()
                .contains("AccessToken")
        );
    }

    #[test]
    fn endpoint_backed_credential_rejects_self_use() {
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

                    POST Login
                        path ["login"]
                        auth header "X-Upstream-Key" = upstream
                        auth bearer session
                        -> Json<AccessToken>
                }
            }
            "#,
        )
        .expect("valid api syntax");

        let err = analyze(ast).expect_err("self-use must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("cannot acquire via endpoint"));
        assert!(msg.contains("uses that credential"));
    }

    #[test]
    fn endpoint_backed_credential_rejects_inherited_client_auth_self_use() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret upstream_key: String
                    credential upstream = api_key(secret.upstream_key)
                    credential session = endpoint auth_api::Login
                    auth bearer session
                }

                scope auth_api {
                    path ["auth"]

                    POST Login
                        path ["login"]
                        auth header "X-Upstream-Key" = upstream
                        -> Json<AccessToken>
                }
            }
            "#,
        )
        .expect("valid api syntax");

        let err = analyze(ast).expect_err("inherited client auth self-use must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("cannot acquire via endpoint"));
        assert!(msg.contains("uses that credential"));
    }

    #[test]
    fn endpoint_backed_credential_rejects_inherited_behavior_auth_self_use() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret upstream_key: String
                    credential upstream = api_key(secret.upstream_key)
                    credential session = endpoint auth_api::Login

                    behaviors {
                        behavior default_auth {
                            auth bearer session
                        }
                    }

                    defaults {
                        behavior default_auth
                    }
                }

                scope auth_api {
                    path ["auth"]

                    POST Login
                        path ["login"]
                        auth header "X-Upstream-Key" = upstream
                        -> Json<AccessToken>
                }
            }
            "#,
        )
        .expect("valid api syntax");

        let err = analyze(ast).expect_err("inherited behavior auth self-use must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("cannot acquire via endpoint"));
        assert!(msg.contains("uses that credential"));
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

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }
                }

                GET Ping
                    path ["ping"]
                    retry off
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

        let Some(RateLimitResolved::Add(client_rate_limit)) =
            &resolved_api.client_policy.rate_limit
        else {
            panic!("expected default client rate limit");
        };
        assert_eq!(client_rate_limit.buckets.len(), 1);

        let endpoint_policy = &resolved_api.endpoints[0].policy.endpoint;
        assert!(matches!(endpoint_policy.retry, Some(RetryResolved::Clear)));
        assert!(matches!(
            endpoint_policy.rate_limit,
            Some(RateLimitResolved::Replace(_))
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
                if name == "tenant_key" && *field == "tenant"
        )));
    }

    #[test]
    fn client_default_behavior_applies_to_endpoint_policy() {
        let resolved_api = analyze_source(
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

                    behavior read_behavior {
                        retry read
                        rate_limit app
                    }

                    defaults {
                        behavior read_behavior
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected client default behavior retry");
        };
        assert_eq!(client_retry.max_attempts, 2);
        assert_eq!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, endpoint),
            vec!["app_0".to_string()]
        );
    }

    #[test]
    fn explicit_default_retry_overrides_default_behavior() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry behavior_retry {
                        max_attempts 5
                        methods [GET]
                    }

                    retry explicit_retry {
                        max_attempts 2
                        methods [GET]
                    }

                    behavior read_behavior {
                        retry behavior_retry
                    }

                    defaults {
                        behavior read_behavior
                        retry explicit_retry
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        );

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected explicit default retry");
        };
        assert_eq!(client_retry.max_attempts, 2);
    }

    #[test]
    fn endpoint_explicit_retry_overrides_endpoint_behavior() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry behavior_retry {
                        max_attempts 5
                        methods [GET]
                    }

                    retry explicit_retry {
                        max_attempts 2
                        methods [GET]
                    }

                    behavior read_behavior {
                        retry behavior_retry
                    }
                }

                GET Me
                    path ["me"]
                    behavior read_behavior
                    retry explicit_retry
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        let Some(RetryResolved::Set(endpoint_retry)) = &endpoint.policy.endpoint.retry else {
            panic!("expected explicit endpoint retry");
        };
        assert_eq!(endpoint_retry.max_attempts, 2);
    }

    #[test]
    fn endpoint_behavior_rate_limit_combines_with_explicit_rate_limit() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    rate_limit method {
                        bucket method by [host, endpoint] {
                            5 / 1s
                        }
                    }

                    behavior read_behavior {
                        rate_limit app
                    }
                }

                GET Me
                    path ["me"]
                    behavior read_behavior
                    rate_limit method
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        assert_eq!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, endpoint),
            vec!["app_0".to_string(), "method_0".to_string()]
        );
    }

    #[test]
    fn scope_behavior_rate_limit_combines_with_endpoint_rate_limit() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    rate_limit method {
                        bucket method by [host, endpoint] {
                            5 / 1s
                        }
                    }

                    behavior scope_read {
                        rate_limit app
                    }
                }

                scope users {
                    path ["users"]
                    behavior scope_read

                    GET Me
                        path ["me"]
                        rate_limit method
                        -> Json<()>
                }
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        assert_eq!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, endpoint),
            vec!["app_0".to_string(), "method_0".to_string()]
        );
    }

    #[test]
    fn rate_limit_off_clears_inherited_behavior_rate_limit() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    behavior read_behavior {
                        rate_limit app
                    }

                    defaults {
                        behavior read_behavior
                    }
                }

                GET Me
                    path ["me"]
                    rate_limit off
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        assert!(effective_endpoint_rate_limit_bucket_names(&resolved_api, endpoint).is_empty());
        assert!(matches!(
            endpoint.policy.endpoint.rate_limit,
            Some(RateLimitResolved::Clear)
        ));
    }

    #[test]
    fn scope_behavior_is_inherited_by_nested_endpoint() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    behavior scope_read {
                        retry read
                    }
                }

                scope users {
                    path ["users"]
                    behavior scope_read

                    GET Me
                        path ["me"]
                        -> Json<()>
                }
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        let Some(scope_policy) = endpoint.policy.scopes.first() else {
            panic!("expected scope policy");
        };
        let Some(RetryResolved::Set(scope_retry)) = &scope_policy.retry else {
            panic!("expected inherited scope retry");
        };
        assert_eq!(scope_retry.max_attempts, 2);
    }

    #[test]
    fn endpoint_behavior_adds_policy_without_losing_inherited_auth() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                    credential session = bearer(secret.token)

                    behavior default_auth {
                        auth bearer session
                    }

                    retry endpoint_retry {
                        max_attempts 2
                        methods [GET]
                    }

                    behavior endpoint_read {
                        retry endpoint_retry
                    }

                    defaults {
                        behavior default_auth
                    }
                }

                GET Me
                    path ["me"]
                    behavior endpoint_read
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Me");

        assert_eq!(endpoint.policy.auth.len(), 1);
        let auth = &endpoint.policy.auth[0];
        assert_eq!(auth.credential.to_string(), "session");
        assert_eq!(auth.placement, AuthPlacementIr::Bearer);
        assert_eq!(auth.usage_id, "bearer");
        assert_eq!(auth.step_id, "Me:0:session");
        assert_eq!(auth.provenance.label, "client");
        assert!(matches!(
            endpoint.policy.endpoint.retry,
            Some(RetryResolved::Set(_))
        ));
    }

    #[test]
    fn behavior_rate_limit_key_binding_resolves_at_endpoint_attachment() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit match_bucket {
                        bucket method by [match_key] {
                            5 / 1s
                        }
                    }

                    behavior match_read {
                        rate_limit match_bucket
                    }
                }

                GET Match(match_id: String)
                    path ["match", match_id]
                    rate_limit key match_key = match_id
                    behavior match_read
                    -> Json<()>
            }
            "#,
        );
        let endpoint = endpoint_by_name(&resolved_api, "Match");
        let rate_limit = endpoint
            .policy
            .endpoint
            .rate_limit
            .as_ref()
            .expect("endpoint rate limit");
        let plan = rate_limit_plan(rate_limit);
        assert_eq!(plan.buckets.len(), 1);
        let bucket = &plan.buckets[0];
        assert!(matches!(
            bucket.key.as_slice(),
            [RateLimitKeyResolved::EpField { name, field }]
                if name == "match_key" && *field == "match_id"
        ));
    }

    #[test]
    fn default_behavior_rate_limit_requiring_endpoint_key_fails() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit match_bucket {
                        bucket method by [match_key] {
                            5 / 1s
                        }
                    }

                    behavior match_read {
                        rate_limit match_bucket
                    }

                    defaults {
                        behavior match_read
                    }
                }

                GET Match(match_id: String)
                    path ["match", match_id]
                    rate_limit key match_key = match_id
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("default behavior rate limit should fail");
        let msg = err.to_string();

        assert!(msg.contains("rate_limit key"));
        assert!(msg.contains("client base policy"));
    }

    #[test]
    fn scope_rate_limit_key_without_binding_fails_before_codegen() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    rate_limit match_bucket {
                        bucket method by [match_key] {
                            5 / 1s
                        }
                    }

                    behavior match_read {
                        rate_limit match_bucket
                    }
                }

                scope MatchScope {
                    path ["match"]
                    behavior match_read

                    GET Match(match_id: String)
                        path [match_id]
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("scope rate limit key without binding should fail");
        let msg = err.to_string();
        assert!(msg.contains("rate_limit key"));
        assert!(msg.contains("match_key"));
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
    fn explicit_default_retry_still_overrides_default_behavior() {
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

                    behavior read_behavior {
                        retry from_behavior
                    }

                    default {
                        behavior read_behavior
                        retry explicit
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
        let auth = &endpoint.policy.auth[0];
        assert_eq!(auth.credential.to_string(), "session");
        assert_eq!(auth.placement, AuthPlacementIr::Bearer);
        assert_eq!(auth.usage_id, "bearer");
        assert_eq!(auth.step_id, "Me:0:session");
        assert_eq!(auth.provenance.label, "client");
    }

    #[test]
    fn behavior_doc_names_preserve_client_scope_endpoint_order() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    behavior client_read {
                        retry off
                    }

                    behavior scope_read {
                        retry off
                    }

                    behavior endpoint_read {
                        retry off
                    }

                    defaults {
                        behavior client_read
                    }
                }

                scope users {
                    path ["users"]
                    behavior scope_read

                    GET Me
                        path ["me"]
                        behavior endpoint_read
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = &resolved_api.endpoints[0];

        assert_eq!(
            endpoint.behavior_doc.names,
            vec![
                "client_read".to_string(),
                "scope_read".to_string(),
                "endpoint_read".to_string(),
            ]
        );
    }

    #[test]
    fn behavior_doc_names_are_deduped_in_stable_order() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    behavior read {
                        retry off
                    }

                    behavior match_read {
                        retry off
                    }

                    defaults {
                        behavior read
                    }
                }

                scope users {
                    path ["users"]
                    behavior read

                    GET Me
                        path ["me"]
                        behavior match_read
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("duplicate behavior across layers remains allowed");
        let endpoint = &resolved_api.endpoints[0];

        assert_eq!(
            endpoint.behavior_doc.names,
            vec!["read".to_string(), "match_read".to_string()]
        );
    }

    #[test]
    fn duplicate_behavior_across_layers_remains_allowed() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    behavior read {
                        retry off
                    }

                    defaults {
                        behavior read
                    }
                }

                scope users {
                    path ["users"]
                    behavior read

                    GET Me
                        path ["me"]
                        behavior read
                        -> Json<()>
                }
            }
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("cross-layer behavior reuse remains valid");
        let endpoint = &resolved_api.endpoints[0];

        assert_eq!(endpoint.behavior_doc.names, vec!["read".to_string()]);
    }

    #[test]
    fn behavior_merge_order_snapshot() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client MergeSnapshotApi {
                    base "https://example.com"
                    secret client_token: String
                    secret outer_token: String
                    secret inner_token: String
                    secret endpoint_token: String
                    secret direct_token: String

                    credential client_auth = bearer(secret.client_token)
                    credential outer_auth = api_key(secret.outer_token)
                    credential inner_auth = api_key(secret.inner_token)
                    credential endpoint_auth = api_key(secret.endpoint_token)
                    credential direct_auth = api_key(secret.direct_token)

                    retry client_retry {
                        max_attempts 2
                        methods [GET]
                        on [401, 403]
                        retry_after
                    }

                    retry outer_retry {
                        max_attempts 3
                        methods [GET]
                        on [429]
                    }

                    retry inner_retry {
                        max_attempts 4
                        methods [GET]
                        on [500]
                    }

                    retry endpoint_retry {
                        max_attempts 5
                        methods [GET]
                        on [502]
                    }

                    rate_limit client_limit {
                        bucket client by [host] {
                            1 / 1s
                        }
                    }

                    rate_limit outer_limit {
                        bucket outer by [host] {
                            2 / 1s
                        }
                    }

                    rate_limit inner_limit {
                        bucket inner by [host] {
                            3 / 1s
                        }
                    }

                    rate_limit endpoint_limit {
                        bucket endpoint by [host] {
                            4 / 1s
                        }
                    }

                    behaviors {
                        behavior client_behavior {
                            auth bearer client_auth
                            retry client_retry
                            rate_limit client_limit
                        }

                        behavior outer_behavior {
                            auth header "X-Outer" = outer_auth
                            retry outer_retry
                            rate_limit outer_limit
                        }

                        behavior inner_behavior {
                            auth query "inner" = inner_auth
                            retry inner_retry
                            rate_limit inner_limit
                        }

                        behavior endpoint_behavior {
                            auth header "X-Endpoint" = endpoint_auth
                            retry off
                        }
                    }

                    defaults {
                        behavior client_behavior
                    }
                }

                scope outer {
                    path ["outer"]
                    behavior outer_behavior

                    scope inner {
                        path ["inner"]
                        behavior inner_behavior

                        GET Show
                            path ["show"]
                            behavior endpoint_behavior
                            auth query "direct" = direct_auth
                            retry endpoint_retry
                            rate_limit endpoint_limit
                            -> Json<()>
                    }
                }
            }
            "#,
        );

        let endpoint = endpoint_by_name(&resolved_api, "Show");
        let auth_names = auth_requirement_names(&endpoint.policy.auth);
        assert_eq!(
            auth_names,
            vec![
                "client_auth".to_string(),
                "outer_auth".to_string(),
                "inner_auth".to_string(),
                "endpoint_auth".to_string(),
                "direct_auth".to_string(),
            ]
        );
        assert_eq!(
            endpoint.behavior_doc.names,
            vec![
                "client_behavior".to_string(),
                "outer_behavior".to_string(),
                "inner_behavior".to_string(),
                "endpoint_behavior".to_string(),
            ]
        );
        assert!(matches!(
            resolved_api.client_policy.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 2,
                ..
            }))
        ));
        assert!(matches!(
            resolved_api.client_policy.rate_limit,
            Some(RateLimitResolved::Add(_))
        ));
        let client_rate_limit = rate_limit_plan(
            resolved_api
                .client_policy
                .rate_limit
                .as_ref()
                .expect("client rate limit"),
        );
        assert_eq!(client_rate_limit.buckets.len(), 1);
        let client_bucket = &client_rate_limit.buckets[0];
        assert_eq!(client_bucket.kind, "client");
        assert_eq!(client_bucket.name, "client_limit_0");
        assert!(matches!(
            client_bucket.key.as_slice(),
            [RateLimitKeyResolved::RouteHost]
        ));
        assert_eq!(client_bucket.cost, 1);
        assert_eq!(
            client_bucket
                .windows
                .iter()
                .map(|window| (window.max, window.per_secs))
                .collect::<Vec<_>>(),
            vec![(1, 1)]
        );

        assert_eq!(endpoint.policy.scopes.len(), 2);
        let outer_scope = &endpoint.policy.scopes[0];
        let inner_scope = &endpoint.policy.scopes[1];
        assert!(matches!(
            outer_scope.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 3,
                ..
            }))
        ));
        assert!(matches!(
            outer_scope.rate_limit,
            Some(RateLimitResolved::Add(_))
        ));
        assert!(matches!(
            inner_scope.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 4,
                ..
            }))
        ));
        assert!(matches!(
            inner_scope.rate_limit,
            Some(RateLimitResolved::Add(_))
        ));

        assert!(matches!(
            endpoint.policy.endpoint.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 5,
                ..
            }))
        ));
        assert!(matches!(
            endpoint.policy.endpoint.rate_limit,
            Some(RateLimitResolved::Add(_))
        ));
        assert_eq!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, endpoint),
            vec![
                "client_limit_0".to_string(),
                "outer_limit_0".to_string(),
                "inner_limit_0".to_string(),
                "endpoint_limit_0".to_string(),
            ]
        );
    }

    #[test]
    fn auth_append_order_snapshot() {
        let source = r#"
            api! {
                client AuthOrderApi {
                    base "https://example.com"
                    secret client_token: String
                    secret scope_token: String
                    secret endpoint_token: String
                    secret direct_token: String

                    credential client_auth = bearer(secret.client_token)
                    credential scope_auth = api_key(secret.scope_token)
                    credential endpoint_auth = api_key(secret.endpoint_token)
                    credential direct_auth = api_key(secret.direct_token)

                    behaviors {
                        behavior client_behavior {
                            auth bearer client_auth
                        }

                        behavior scope_behavior {
                            auth header "X-Scope" = scope_auth
                        }

                        behavior endpoint_behavior {
                            auth query "X-Endpoint" = endpoint_auth
                        }
                    }

                    defaults {
                        behavior client_behavior
                    }
                }

                scope protected {
                    path ["protected"]
                    behavior scope_behavior

                    GET Show
                        path ["show"]
                        behavior endpoint_behavior
                        auth header "X-Direct" = direct_auth
                        -> Json<()>
                }
            }
            "#;
        let resolved_api = analyze_source(source);
        let endpoint = endpoint_by_name(&resolved_api, "Show");

        assert_eq!(
            auth_requirement_names(&endpoint.policy.auth),
            vec![
                "client_auth".to_string(),
                "scope_auth".to_string(),
                "endpoint_auth".to_string(),
                "direct_auth".to_string(),
            ]
        );
        assert_eq!(endpoint.policy.auth.len(), 4);
        assert!(matches!(
            endpoint.policy.auth.as_slice(),
            [client, scope, endpoint_behavior, direct] if
                matches!(client.provenance.label.as_str(), "client")
                && matches!(scope.provenance.label.as_str(), "scope:0")
                && matches!(endpoint_behavior.provenance.label.as_str(), "endpoint")
                && matches!(direct.provenance.label.as_str(), "endpoint")
        ));

        let emitted = crate::codegen::emit(analyze_source(source));
        let emitted = emitted
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        for needle in [
            "CredentialId::new(\"AuthOrderApi\",\"client_auth\")",
            "CredentialId::new(\"AuthOrderApi\",\"scope_auth\")",
            "CredentialId::new(\"AuthOrderApi\",\"endpoint_auth\")",
            "CredentialId::new(\"AuthOrderApi\",\"direct_auth\")",
            "AuthUsageId::new(\"bearer\")",
            "AuthUsageId::new(\"header\")",
            "AuthUsageId::new(\"query\")",
            "protected::Show:0:client_auth",
            "protected::Show:1:scope_auth",
            "protected::Show:2:endpoint_auth",
            "protected::Show:3:direct_auth",
        ] {
            assert!(
                emitted.contains(needle),
                "generated auth plan missing `{needle}`\n{emitted}"
            );
        }
        let mut last = 0;
        for needle in [
            "protected::Show:0:client_auth",
            "protected::Show:1:scope_auth",
            "protected::Show:2:endpoint_auth",
            "protected::Show:3:direct_auth",
        ] {
            let pos = emitted
                .find(needle)
                .unwrap_or_else(|| panic!("missing step id `{needle}`"));
            assert!(pos >= last, "step ids out of order around `{needle}`");
            last = pos;
        }
    }

    #[test]
    fn retry_replace_snapshot() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client RetrySnapshotApi {
                    base "https://example.com"

                    retry retry_a {
                        max_attempts 2
                        methods [GET]
                        on [429]
                    }

                    retry retry_b {
                        max_attempts 3
                        methods [GET]
                        on [500]
                    }

                    retry retry_c {
                        max_attempts 4
                        methods [GET]
                        on [502]
                    }

                    behaviors {
                        behavior client_retry {
                            retry retry_a
                        }

                        behavior scope_retry {
                            retry retry_b
                        }

                        behavior clear_retry {
                            retry off
                        }

                        behavior replace_retry {
                            retry retry_c
                        }
                    }

                    defaults {
                        behavior client_retry
                    }
                }

                scope protected {
                    path ["protected"]
                    behavior scope_retry

                    GET Clear
                        path ["clear"]
                        behavior clear_retry
                        -> Json<()>

                    GET Replace
                        path ["replace"]
                        behavior replace_retry
                        -> Json<()>
                }
            }
            "#,
        );
        assert!(matches!(
            resolved_api.client_policy.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 2,
                ..
            }))
        ));

        let clear_endpoint = endpoint_by_name(&resolved_api, "Clear");
        assert!(matches!(
            clear_endpoint.policy.scopes[0].retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 3,
                ..
            }))
        ));
        assert!(matches!(
            clear_endpoint.policy.endpoint.retry,
            Some(RetryResolved::Clear)
        ));

        let replace_endpoint = endpoint_by_name(&resolved_api, "Replace");
        assert!(matches!(
            replace_endpoint.policy.scopes[0].retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 3,
                ..
            }))
        ));
        assert!(matches!(
            replace_endpoint.policy.endpoint.retry,
            Some(RetryResolved::Set(RetryConfigResolved {
                max_attempts: 4,
                ..
            }))
        ));
    }

    #[test]
    fn retry_patches_materialize_inherited_and_after_clear() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client RetryPatchApi {
                    base "https://example.com"

                    retry base {
                        max_attempts 2
                        methods [GET]
                        on [429]
                    }

                    behaviors {
                        behavior client_base {
                            retry base
                        }

                        behavior patch_methods {
                            retry {
                                methods [POST]
                            }
                        }

                        behavior clear_retry {
                            retry off
                        }

                        behavior patch_after_clear {
                            retry {
                                max_attempts 7
                            }
                        }
                    }

                    defaults {
                        behavior client_base
                    }
                }

                scope inherited {
                    path ["inherited"]
                    behavior patch_methods

                    GET Patched
                        path ["patched"]
                        -> Json<()>
                }

                scope cleared {
                    path ["cleared"]
                    behavior clear_retry

                    GET Reenabled
                        path ["reenabled"]
                        behavior patch_after_clear
                        -> Json<()>
                }
            }
            "#,
        );

        let Some(RetryResolved::Set(client_retry)) = &resolved_api.client_policy.retry else {
            panic!("expected client retry to materialize as set");
        };
        assert_eq!(client_retry.max_attempts, 2);
        assert_eq!(
            client_retry
                .methods
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["GET".to_string()]
        );
        assert_eq!(client_retry.statuses, [429]);

        let patched_endpoint = endpoint_by_name(&resolved_api, "Patched");
        let Some(RetryResolved::Set(scope_retry)) = &patched_endpoint.policy.scopes[0].retry else {
            panic!("expected inherited patch to materialize as set");
        };
        assert_eq!(scope_retry.max_attempts, 2);
        assert_eq!(
            scope_retry
                .methods
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["POST".to_string()]
        );
        assert_eq!(scope_retry.statuses, [429]);
        assert!(patched_endpoint.policy.endpoint.retry.is_none());

        let reenabled_endpoint = endpoint_by_name(&resolved_api, "Reenabled");
        assert!(matches!(
            reenabled_endpoint.policy.scopes[0].retry,
            Some(RetryResolved::Clear)
        ));
        let Some(RetryResolved::Set(endpoint_retry)) = &reenabled_endpoint.policy.endpoint.retry
        else {
            panic!("expected patch after clear to re-enable retry");
        };
        assert_eq!(endpoint_retry.max_attempts, 7);
        assert!(endpoint_retry.methods.is_empty());
        assert!(endpoint_retry.statuses.is_empty());
    }

    #[test]
    fn rate_limit_append_off_snapshot() {
        let resolved_api = analyze_source(
            r#"
            api! {
                client RateLimitSnapshotApi {
                    base "https://example.com"

                    rate_limit client_limit {
                        bucket client by [host] {
                            1 / 1s
                        }
                    }

                    rate_limit scope_limit {
                        bucket scope by [host] {
                            2 / 1s
                        }
                    }

                    rate_limit endpoint_limit {
                        bucket endpoint by [host] {
                            3 / 1s
                        }
                    }

                    behaviors {
                        behavior client_limit_behavior {
                            rate_limit client_limit
                        }

                        behavior scope_limit_behavior {
                            rate_limit scope_limit
                        }

                        behavior endpoint_limit_behavior {
                            rate_limit endpoint_limit
                        }

                        behavior clear_limit_behavior {
                            rate_limit off
                        }
                    }

                    defaults {
                        behavior client_limit_behavior
                    }
                }

                scope protected {
                    path ["protected"]
                    behavior scope_limit_behavior

                    GET Append
                        path ["append"]
                        behavior endpoint_limit_behavior
                        -> Json<()>

                    GET Clear
                        path ["clear"]
                        behavior clear_limit_behavior
                        -> Json<()>
                }
            }
            "#,
        );

        let append_endpoint = endpoint_by_name(&resolved_api, "Append");
        assert_eq!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, append_endpoint),
            vec![
                "client_limit_0".to_string(),
                "scope_limit_0".to_string(),
                "endpoint_limit_0".to_string(),
            ]
        );
        assert!(matches!(
            append_endpoint.policy.scopes[0].rate_limit,
            Some(RateLimitResolved::Add(_))
        ));
        assert!(matches!(
            append_endpoint.policy.endpoint.rate_limit,
            Some(RateLimitResolved::Add(_))
        ));
        let append_plan = rate_limit_plan(
            append_endpoint
                .policy
                .endpoint
                .rate_limit
                .as_ref()
                .expect("append endpoint rate limit"),
        );
        assert_eq!(
            append_plan
                .buckets
                .iter()
                .map(|bucket| {
                    (
                        bucket.kind.as_str(),
                        bucket.name.as_str(),
                        bucket
                            .key
                            .iter()
                            .map(|key| match key {
                                RateLimitKeyResolved::RouteHost => "host",
                                RateLimitKeyResolved::Endpoint => "endpoint",
                                RateLimitKeyResolved::Method => "method",
                                RateLimitKeyResolved::EpField { name, .. } => name.as_str(),
                                RateLimitKeyResolved::Static { value, .. } => value.as_str(),
                            })
                            .collect::<Vec<_>>(),
                        bucket.cost,
                        bucket
                            .windows
                            .iter()
                            .map(|window| (window.max, window.per_secs))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![(
                "endpoint",
                "endpoint_limit_0",
                vec!["host"],
                1,
                vec![(3, 1)]
            )]
        );

        let clear_endpoint = endpoint_by_name(&resolved_api, "Clear");
        assert!(matches!(
            clear_endpoint.policy.endpoint.rate_limit,
            Some(RateLimitResolved::Clear)
        ));
        assert!(
            effective_endpoint_rate_limit_bucket_names(&resolved_api, clear_endpoint).is_empty()
        );
    }

    #[test]
    fn same_layer_duplicate_behavior_rejected() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client DuplicateBehaviorApi {
                    base "https://example.com"

                    behavior read {
                        retry off
                    }

                    defaults {
                        behavior read
                        behavior read
                    }
                }

                GET Me
                    path ["me"]
                    -> Json<()>
            }
            "#,
        )
        .expect("valid api syntax");

        let err = analyze(ast).expect_err("same-site duplicate behavior must fail");
        assert!(
            err.to_string()
                .contains("duplicate behavior `read` at this attachment site")
        );
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
    fn body_signature_response_resolve_into_endpoint_model() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client BodyApi {
                base "https://example.com"
            }

            POST Create(body: Json<CreateBody>)
                as create
                path ["items"]
                -> Json<CreateResponse>
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");
        let endpoint = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Create")
            .expect("Create endpoint");

        assert_eq!(
            ty_string(&endpoint.io.request_entity.adapter_ty),
            "::concord_core::advanced::EncodedRequest<Json<CreateBody>>"
        );
        assert_eq!(
            ty_string(
                endpoint
                    .io
                    .request_entity
                    .public_input_ty
                    .as_ref()
                    .expect("public input type")
            ),
            "CreateBody"
        );
        assert_eq!(
            ty_string(&endpoint.io.response_entity.adapter_ty),
            "::concord_core::advanced::BufferedResponse<Json<CreateResponse>>"
        );
        assert_eq!(
            ty_string(&endpoint.io.response_entity.public_output_ty),
            "CreateResponse"
        );
    }

    #[test]
    fn resolved_endpoint_exposes_closed_io_variants() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                POST Buffered(body: Json<Body>)
                    path ["buffered"]
                    -> Json<Resp>

                GET Streamed
                    path ["streamed"]
                    -> Stream<Bytes>

                GET Listed
                    path ["listed"]
                    -> Records<Item, NdJson>

                GET Multiparted
                    path ["multiparted"]
                    -> Multipart<Part>

                GET Ssed
                    path ["ssed"]
                    -> Sse<Event>
            }
            "#,
        )
        .expect("valid api syntax");

        let resolved_api = analyze(ast).expect("analysis succeeds");

        let buffered = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Buffered")
            .expect("buffered endpoint");
        assert_eq!(
            ty_string(&buffered.io.request_entity.adapter_ty),
            "::concord_core::advanced::EncodedRequest<Json<Body>>"
        );
        assert_eq!(
            ty_string(&buffered.io.response_entity.adapter_ty),
            "::concord_core::advanced::BufferedResponse<Json<Resp>>"
        );

        let streamed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Streamed")
            .expect("streamed endpoint");
        assert_eq!(
            ty_string(&streamed.io.response_entity.adapter_ty),
            "::concord_core::advanced::RawStreamResponse<Bytes>"
        );

        let listed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Listed")
            .expect("listed endpoint");
        assert_eq!(
            ty_string(&listed.io.response_entity.adapter_ty),
            "::concord_core::advanced::RecordResponse<Item,NdJson>"
        );

        let multiparted = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Multiparted")
            .expect("multiparted endpoint");
        assert_eq!(
            ty_string(&multiparted.io.response_entity.adapter_ty),
            "::concord_core::advanced::MultipartResponse<Part,::concord_core::advanced::FormData>"
        );

        let ssed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Ssed")
            .expect("ssed endpoint");
        assert_eq!(
            ty_string(&ssed.io.response_entity.adapter_ty),
            "::concord_core::advanced::SseResponse<Event,::concord_core::advanced::JsonSse>"
        );
    }

    fn ty_string(ty: &Type) -> String {
        quote::quote!(#ty).to_string().replace(' ', "")
    }

    #[test]
    fn resolved_endpoint_io_entity_metadata_covers_buffered_and_request_adapters() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client MetaApi {
                    base "https://example.com"
                }

                GET NoBody
                    path ["no-body"]
                    -> Json<NoBodyResponse>

                POST Buffered(body: Json<BufferedBody>)
                    path ["buffered"]
                    -> Json<BufferedResponse>

                POST StreamReq(body: Stream<OctetStream>)
                    path ["stream-req"]
                    -> Json<StreamResponse>
            }
            "#,
        )
        .expect("valid api syntax");

        let resolved_api = analyze(ast).expect("analysis succeeds");

        let no_body = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "NoBody")
            .expect("no body endpoint");
        assert_eq!(
            ty_string(&no_body.io.request_entity.adapter_ty),
            "::concord_core::advanced::NoRequestBody"
        );
        assert!(no_body.io.request_entity.public_input_ty.is_none());
        assert!(no_body.io.request_entity.body_field_ty.is_none());
        assert!(no_body.io.request_entity.doc.facade_summary.is_none());
        assert_eq!(
            ty_string(&no_body.io.response_entity.adapter_ty),
            "::concord_core::advanced::BufferedResponse<Json<NoBodyResponse>>"
        );
        assert_eq!(
            ty_string(&no_body.io.response_entity.public_output_ty),
            "NoBodyResponse"
        );
        assert!(no_body.io.response_entity.capabilities.supports_pagination);
        assert!(!no_body.io.response_entity.capabilities.is_streaming);
        assert!(!no_body.io.response_entity.capabilities.is_no_content);

        let buffered = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Buffered")
            .expect("buffered endpoint");
        assert_eq!(
            ty_string(&buffered.io.request_entity.adapter_ty),
            "::concord_core::advanced::EncodedRequest<Json<BufferedBody>>"
        );
        assert_eq!(
            ty_string(
                buffered
                    .io
                    .request_entity
                    .public_input_ty
                    .as_ref()
                    .expect("public input type")
            ),
            "BufferedBody"
        );
        assert_eq!(
            ty_string(
                buffered
                    .io
                    .request_entity
                    .body_field_ty
                    .as_ref()
                    .expect("body field type")
            ),
            "BufferedBody"
        );
        assert_eq!(
            buffered.io.request_entity.doc.facade_summary.as_deref(),
            Some("Body: Json<BufferedBody>")
        );
        assert_eq!(
            ty_string(&buffered.io.response_entity.adapter_ty),
            "::concord_core::advanced::BufferedResponse<Json<BufferedResponse>>"
        );
        assert_eq!(
            ty_string(&buffered.io.response_entity.public_output_ty),
            "BufferedResponse"
        );

        let stream_req = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "StreamReq")
            .expect("stream request endpoint");
        assert_eq!(
            ty_string(&stream_req.io.request_entity.adapter_ty),
            "::concord_core::advanced::RawStreamRequest<OctetStream>"
        );
        assert!(stream_req.io.request_entity.capabilities.has_body);
        assert!(stream_req.io.request_entity.capabilities.is_streaming);
        assert_eq!(
            ty_string(
                &stream_req
                    .io
                    .request_entity
                    .public_input_ty
                    .as_ref()
                    .expect("stream body type")
            ),
            "StreamBody"
        );
    }

    #[test]
    fn resolved_endpoint_io_entity_metadata_covers_streaming_response_families() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            api! {
                client StreamMetaApi {
                    base "https://example.com"
                }

                GET Streamed
                    path ["streamed"]
                    -> Stream<OctetStream>

                GET Listed
                    path ["listed"]
                    -> Records<Item, NdJson>

                GET Multiparted
                    path ["multiparted"]
                    -> Multipart<Part, Mixed>

                GET Ssed
                    path ["ssed"]
                    -> Sse<Event>

                GET NoContent
                    path ["no-content"]
                    -> NoContent
            }
            "#,
        )
        .expect("valid api syntax");

        let resolved_api = analyze(ast).expect("analysis succeeds");

        let streamed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Streamed")
            .expect("streamed endpoint");
        assert_eq!(
            ty_string(&streamed.io.response_entity.adapter_ty),
            "::concord_core::advanced::RawStreamResponse<OctetStream>"
        );
        assert_eq!(
            streamed.io.response_entity.doc.facade_summary.as_deref(),
            Some("Response: Stream<OctetStream>")
        );
        assert!(streamed.io.response_entity.capabilities.is_streaming);
        assert!(!streamed.io.response_entity.capabilities.supports_pagination);
        let listed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Listed")
            .expect("listed endpoint");
        assert_eq!(
            ty_string(&listed.io.response_entity.adapter_ty),
            "::concord_core::advanced::RecordResponse<Item,NdJson>"
        );
        assert!(listed.io.response_entity.capabilities.is_streaming);
        assert!(!listed.io.response_entity.capabilities.supports_pagination);
        let multiparted = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Multiparted")
            .expect("multiparted endpoint");
        assert_eq!(
            ty_string(&multiparted.io.response_entity.adapter_ty),
            "::concord_core::advanced::MultipartResponse<Part,Mixed>"
        );
        assert!(multiparted.io.response_entity.capabilities.is_streaming);
        assert!(
            !multiparted
                .io
                .response_entity
                .capabilities
                .supports_pagination
        );
        let ssed = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Ssed")
            .expect("ssed endpoint");
        assert_eq!(
            ty_string(&ssed.io.response_entity.adapter_ty),
            "::concord_core::advanced::SseResponse<Event,::concord_core::advanced::JsonSse>"
        );
        assert!(ssed.io.response_entity.capabilities.is_streaming);
        assert!(!ssed.io.response_entity.capabilities.supports_pagination);
        let no_content = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "NoContent")
            .expect("no content endpoint");
        assert_eq!(
            ty_string(&no_content.io.response_entity.adapter_ty),
            "::concord_core::advanced::NoContentResponse"
        );
        assert!(no_content.io.response_entity.capabilities.is_no_content);
        assert!(!no_content.io.response_entity.capabilities.is_streaming);
        assert!(
            !no_content
                .io
                .response_entity
                .capabilities
                .supports_pagination
        );
    }

    #[test]
    fn pagination_assignments_resolve_into_endpoint_model() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET Offset(start: u64 = 0, count: u64 = 20)
                path ["offset"]
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
                paginate CursorPagination<String> {
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

            GET Custom(page: u64 = 1)
                path ["custom"]
                paginate HeaderCursorPagination {
                    page = page
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let offset = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Offset")
            .and_then(|ep| ep.paginate.as_ref())
            .expect("offset pagination");
        let controller_ty = &offset.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "OffsetLimitPagination"
        );
        assert_eq!(offset.assigns.len(), 2);
        assert_eq!(offset.bindings.len(), 2);

        let cursor = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Cursor")
            .and_then(|ep| ep.paginate.as_ref())
            .expect("cursor pagination");
        let controller_ty = &cursor.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "CursorPagination < String >"
        );
        assert_eq!(cursor.assigns.len(), 2);
        assert_eq!(cursor.bindings.len(), 2);

        let paged = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Paged")
            .and_then(|ep| ep.paginate.as_ref())
            .expect("paged pagination");
        let controller_ty = &paged.controller_ty;
        assert_eq!(quote::quote!(#controller_ty).to_string(), "PagedPagination");
        assert_eq!(paged.assigns.len(), 2);
        assert_eq!(paged.bindings.len(), 2);

        let custom = resolved_api
            .endpoints
            .iter()
            .find(|ep| ep.name == "Custom")
            .and_then(|ep| ep.paginate.as_ref())
            .expect("custom pagination");
        let controller_ty = &custom.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "HeaderCursorPagination"
        );
        assert_eq!(custom.assigns.len(), 1);
        assert_eq!(custom.bindings.len(), 1);
    }

    #[test]
    fn custom_pagination_resolves() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(page: u64 = 1, count: u64 = 2)
                path ["items"]
                paginate HeaderPagePagination {
                    page = page,
                    count = count
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let resolved_api = analyze(ast).expect("analysis succeeds");

        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        let controller_ty = &pagination.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "HeaderPagePagination"
        );
        assert_eq!(pagination.assigns.len(), 2);
        assert_eq!(pagination.bindings.len(), 2);
        assert_eq!(pagination.bindings[0].controller_field.to_string(), "page");
        assert_eq!(pagination.bindings[1].controller_field.to_string(), "count");
        assert_eq!(
            pagination.bindings[0].endpoint_rust_field.to_string(),
            "page"
        );
        assert_eq!(
            pagination.bindings[1].endpoint_rust_field.to_string(),
            "count"
        );
    }

    #[test]
    fn custom_pagination_assignment_rejects_unknown_endpoint_field() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(count: u64 = 2)
                paginate HeaderPagePagination {
                    page = does_not_exist
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("unknown endpoint field should fail");

        assert!(
            err.to_string()
                .contains("unknown endpoint var `ep.does_not_exist`")
                || err.to_string().contains("available endpoint vars"),
            "{err}"
        );
    }

    #[test]
    fn custom_pagination_rejects_removed_syntax() {
        let src = format!(
            r#"
            client PageApi {{
                base "https://example.com"
            }}

            GET List
                paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {{
                    page = page
                }}
                -> Json<Vec<String>>
            "#
        );
        let err = syn::parse_str::<crate::ast::RawApi>(&src)
            .expect_err("custom pagination should be rejected");

        assert!(
            err.to_string()
                .contains("pagination no longer uses `endpoint_state ... bindings ...`; use `paginate Controller { ... }`"),
            "{err}"
        );
    }

    #[test]
    fn pagination_auto_key_bindings_are_resolved_in_sema() {
        let resolved_api = analyze_source(
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
            "#,
        );

        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        let controller_ty = &pagination.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "OffsetLimitPagination"
        );
        assert_eq!(pagination.assigns.len(), 2);
        assert_eq!(pagination.bindings.len(), 2);
    }

    #[test]
    fn pagination_bindings_do_not_infer_query_keys_from_endpoint_ops() {
        let resolved_api = analyze_source(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(start: u64 = 0, count: u64 = 20)
                query {
                    "from" = start
                    "pageSize" = count
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>
            "#,
        );

        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        assert_eq!(pagination.bindings.len(), 2);

        let offset = &pagination.bindings[0];
        assert_eq!(offset.controller_field, "offset");
        assert_eq!(offset.endpoint_rust_field, "start");

        let limit = &pagination.bindings[1];
        assert_eq!(limit.controller_field, "limit");
        assert_eq!(limit.endpoint_rust_field, "count");
        assert_ne!(offset.endpoint_rust_field, "from");
        assert_ne!(limit.endpoint_rust_field, "pageSize");
    }

    #[test]
    fn pagination_header_bound_bindings_do_not_require_query_keys() {
        let resolved_api = analyze_source(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(page: u64 = 1, count: u64 = 20)
                headers {
                    "X-Page" = page,
                    "X-Count" = count,
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>
            "#,
        );

        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        let controller_ty = &pagination.controller_ty;
        assert_eq!(quote::quote!(#controller_ty).to_string(), "PagedPagination");
        assert_eq!(pagination.assigns.len(), 2);
        assert_eq!(pagination.bindings.len(), 2);
        assert_eq!(pagination.bindings[0].controller_field, "page");
        assert_eq!(pagination.bindings[0].endpoint_rust_field, "page");
        assert_eq!(pagination.bindings[1].controller_field, "per_page");
        assert_eq!(pagination.bindings[1].endpoint_rust_field, "count");
    }

    #[test]
    fn pagination_cursor_assignments_resolve_from_paginate_block() {
        let resolved_api = analyze_source(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 20)
                paginate CursorPagination<String> {
                    cursor = cursor,
                    per_page = count,
                    send_cursor_on_first = true,
                    stop_when_cursor_missing = false
                }
                -> Json<Vec<String>>
            "#,
        );

        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        let controller_ty = &pagination.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "CursorPagination < String >"
        );
        assert_eq!(pagination.assigns.len(), 4);
        assert_eq!(pagination.bindings.len(), 2);
        assert_eq!(
            pagination.assigns[2].field.to_string(),
            "send_cursor_on_first"
        );
        assert_eq!(
            pagination.assigns[3].field.to_string(),
            "stop_when_cursor_missing"
        );
    }

    #[test]
    fn pagination_assignment_fields_are_type_driven() {
        let resolved_api = analyze_source(
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
        );
        let pagination = resolved_api.endpoints[0]
            .paginate
            .as_ref()
            .expect("pagination resolved");
        let controller_ty = &pagination.controller_ty;
        assert_eq!(
            quote::quote!(#controller_ty).to_string(),
            "OffsetLimitPagination"
        );
        assert_eq!(pagination.assigns.len(), 1);
        assert_eq!(pagination.assigns[0].field.to_string(), "per_page");
        assert_eq!(pagination.bindings.len(), 1);
        assert_eq!(
            pagination.bindings[0].controller_field.to_string(),
            "per_page"
        );
    }

    #[test]
    fn pagination_binding_unknown_endpoint_field_is_rejected() {
        let ast: crate::ast::RawApi = syn::parse_str(
            r#"
            client PageApi {
                base "https://example.com"
            }

            GET List(count: u64 = 20)
                paginate OffsetLimitPagination {
                    offset = does_not_exist,
                    limit = count
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("valid api syntax");
        let err = analyze(ast).expect_err("unknown endpoint field should fail");

        assert!(
            err.to_string()
                .contains("unknown endpoint var `ep.does_not_exist`")
                || err.to_string().contains("available endpoint vars"),
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
                -> Json<ApiKey>
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
        assert!(snapshot.contains("scopes=1"));
    }
}
