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
    walk_items(
        &norm.items,
        &mut ancestry,
        &mut walk_ctx,
        inherited_retry,
        0,
    )?;

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

#[cfg(test)]
#[path = "tests/mod.rs"]
mod file_tests;
