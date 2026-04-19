use crate::ast::SetOp;
use crate::emit_helpers;
use crate::sema::*;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

#[inline]
fn client_prefixed_ident(client: &Ident, suffix: &str) -> Ident {
    // Example: RiotClient + "Vars" => RiotClientVars
    emit_helpers::ident(&format!("{}{}", client, suffix), client.span())
}

#[inline]
fn value_uses_auth(v: &ValueKind) -> bool {
    match v {
        ValueKind::AuthField(_) => true,
        ValueKind::Fmt(fmt) => fmt.pieces.iter().any(|p| {
            matches!(
                p,
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Auth,
                    ..
                }
            )
        }),
        _ => false,
    }
}

#[inline]
fn policy_uses_auth(policy: &PolicyBlocksResolved) -> bool {
    let ops_use = |op: &PolicyOp| match op {
        PolicyOp::Set { value, .. } => value_uses_auth(value),
        _ => false,
    };
    policy.headers.iter().any(ops_use)
        || policy.query.iter().any(ops_use)
        || policy.timeout.as_ref().is_some_and(value_uses_auth)
}

fn ep_optionals(ep: &EndpointIr) -> std::collections::BTreeMap<String, bool> {
    ep.vars
        .iter()
        .map(|v| (v.rust.to_string(), v.optional))
        .collect()
}

pub fn emit(ir: Ir) -> TokenStream2 {
    let mod_name = ir.mod_name.clone();
    let scheme = emit_scheme(ir.scheme);
    let domain = ir.domain.clone();

    let vars_ty = client_prefixed_ident(&ir.client_name, "Vars");
    let auth_inner_ty = client_prefixed_ident(&ir.client_name, "AuthInner");
    let auth_vars_ty = client_prefixed_ident(&ir.client_name, "AuthVars");
    let auth_state_ty = client_prefixed_ident(&ir.client_name, "AuthState");
    let cx_ty = client_prefixed_ident(&ir.client_name, "Cx");

    let vars_struct = emit_client_vars(&ir.client_vars, &vars_ty);
    let auth_vars_struct =
        emit_client_auth_vars(&ir.client_auth_vars, &auth_inner_ty, &auth_vars_ty);
    let auth_state_struct = emit_client_auth_state(&ir, &auth_state_ty, &cx_ty);
    let cx_struct = emit_client_context(
        &scheme,
        &domain,
        &ir,
        &ir.client_policy,
        &vars_ty,
        &auth_vars_ty,
        &auth_state_ty,
        &cx_ty,
    );
    let client_wrapper = emit_client_wrapper(&ir, &vars_ty, &auth_vars_ty, &cx_ty);
    let internal_mod = emit_internal(&ir, &vars_ty, &auth_vars_ty, &cx_ty);
    let endpoints_mod = emit_endpoints(&ir, &cx_ty);

    quote! {
        mod #mod_name {
            use super::*;

            #vars_struct
            #auth_vars_struct
            #auth_state_struct
            #cx_struct

            #client_wrapper

            #endpoints_mod
            #internal_mod
        }
    }
}

fn emit_scheme(s: crate::ast::SchemeLit) -> TokenStream2 {
    match s {
        crate::ast::SchemeLit::Http => quote! { ::http::uri::Scheme::HTTP },
        crate::ast::SchemeLit::Https => quote! { ::http::uri::Scheme::HTTPS },
    }
}

fn emit_client_vars(vars: &[VarInfo], vars_ty: &Ident) -> TokenStream2 {
    let fields = vars.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            quote! { pub #name: ::core::option::Option<#ty> }
        } else {
            quote! { pub #name: #ty }
        }
    });

    let required: Vec<&VarInfo> = vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();

    let new_args = required.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        quote! { #name: #ty }
    });

    let init_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #name }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #name: ::core::option::Option::Some(#d) }
            } else {
                quote! { #name: ::core::option::Option::None }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #name: #d }
        }
    });

    let setters = vars.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            let clear = emit_helpers::ident(&format!("clear_{name}"), name.span());
            quote! {
                #[inline]
                pub fn #name(mut self, v: #ty) -> Self { self.#name = ::core::option::Option::Some(v); self }
                #[inline]
                pub fn #clear(mut self) -> Self { self.#name = ::core::option::Option::None; self }
            }
        } else {
            quote! {
                #[inline]
                pub fn #name(mut self, v: #ty) -> Self { self.#name = v; self }
            }
        }
    });

    quote! {
        #[derive(Clone)]
       pub struct #vars_ty {
            #( #fields, )*
        }

       impl #vars_ty {
            #[inline]
            pub fn new( #( #new_args ),* ) -> Self {
                Self { #( #init_fields, )* }
            }

            #( #setters )*
       }
    }
}

fn emit_client_auth_state(ir: &Ir, auth_state_ty: &Ident, cx_ty: &Ident) -> TokenStream2 {
    if ir.client_auth_credentials.is_empty() {
        return quote! {};
    }

    let fields = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let provider_ty = emit_auth_provider_ty(&c.kind);
        quote! {
            pub(crate) #name: ::std::sync::Arc<::concord_core::prelude::CredentialSlot<#cx_ty, #provider_ty>>
        }
    });

    quote! {
        #[derive(Clone)]
        pub struct #auth_state_ty {
            #( #fields, )*
        }
    }
}

fn emit_client_auth_state_init(ir: &Ir, auth_state_ty: &Ident) -> (TokenStream2, TokenStream2) {
    if ir.client_auth_credentials.is_empty() {
        return (
            quote! { ::concord_core::prelude::NoAuthState },
            quote! {
                let _ = vars;
                let _ = auth;
                ::concord_core::prelude::NoAuthState
            },
        );
    }

    let client_ns = LitStr::new(&ir.client_name.to_string(), ir.client_name.span());
    let init_fields = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let provider = emit_auth_provider_init(&client_ns, c);
        quote! {
            #name: ::std::sync::Arc::new(::concord_core::prelude::CredentialSlot::new(#provider))
        }
    });
    let auth_bind = if ir.client_auth_vars.is_empty() {
        quote! { let _ = auth; }
    } else {
        quote! {
            let auth = auth.read().unwrap();
            #[allow(unused_variables)]
            let secret = &auth;
        }
    };

    (
        quote! { #auth_state_ty },
        quote! {
            let _ = vars;
            #auth_bind
            #auth_state_ty {
                #( #init_fields, )*
            }
        },
    )
}

fn emit_auth_provider_ty(kind: &AuthCredentialKindIr) -> TokenStream2 {
    match kind {
        AuthCredentialKindIr::ApiKey { .. } => {
            quote! { ::concord_core::prelude::StaticApiKeyProvider }
        }
        AuthCredentialKindIr::StaticBearer { .. } => {
            quote! { ::concord_core::prelude::StaticBearerProvider }
        }
        AuthCredentialKindIr::Basic { .. } => {
            quote! { ::concord_core::prelude::StaticBasicProvider }
        }
        AuthCredentialKindIr::OAuth2ClientCredentials { .. } => {
            quote! { ::concord_core::prelude::OAuth2ClientCredentialsProvider }
        }
        AuthCredentialKindIr::Endpoint { output_ty, .. } => {
            quote! { ::concord_core::prelude::ManualCredentialProvider<#output_ty> }
        }
        AuthCredentialKindIr::Custom { provider_ty, .. } => quote! { #provider_ty },
    }
}

fn emit_auth_provider_init(client_ns: &LitStr, credential: &AuthCredentialIr) -> TokenStream2 {
    let name = &credential.name;
    let name_lit = LitStr::new(&name.to_string(), name.span());
    let credential_id =
        quote! { ::concord_core::prelude::CredentialId::new(#client_ns, #name_lit) };

    match &credential.kind {
        AuthCredentialKindIr::ApiKey { secret } => quote! {
            ::concord_core::prelude::StaticApiKeyProvider::new(
                #credential_id,
                ::concord_core::prelude::ApiKey::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::StaticBearer { secret } => quote! {
            ::concord_core::prelude::StaticBearerProvider::new(
                #credential_id,
                ::concord_core::prelude::AccessToken::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::Basic { username, password } => quote! {
            ::concord_core::prelude::StaticBasicProvider::new(
                #credential_id,
                ::concord_core::prelude::BasicCredential::new(
                    auth.#username.expose().to_string(),
                    auth.#password.clone(),
                ),
            )
        },
        AuthCredentialKindIr::OAuth2ClientCredentials {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            let provider = quote! {
                ::concord_core::prelude::OAuth2ClientCredentialsProvider::new(
                    #credential_id,
                    #token_url.parse().expect("valid OAuth2ClientCredentials token_url"),
                    auth.#client_id.clone(),
                    auth.#client_secret.clone(),
                )
            };
            if let Some(scope) = scope {
                quote! { #provider.scope(#scope) }
            } else {
                provider
            }
        }
        AuthCredentialKindIr::Endpoint { .. } => {
            let acquire_name = emit_helpers::ident(&format!("acquire_auth_{name}"), name.span());
            let hint = LitStr::new(&format!("client.{acquire_name}(...)"), Span::call_site());
            quote! {
                ::concord_core::prelude::ManualCredentialProvider::new(#credential_id)
                    .with_missing_hint(#hint)
            }
        }
        AuthCredentialKindIr::Custom { provider, .. } => quote! { #provider },
    }
}

fn auth_credential_secret_names(ir: &Ir) -> (std::collections::BTreeSet<String>, bool) {
    let mut out = std::collections::BTreeSet::new();
    let mut has_custom = false;
    for c in &ir.client_auth_credentials {
        match &c.kind {
            AuthCredentialKindIr::ApiKey { secret }
            | AuthCredentialKindIr::StaticBearer { secret } => {
                out.insert(secret.to_string());
            }
            AuthCredentialKindIr::Basic { username, password } => {
                out.insert(username.to_string());
                out.insert(password.to_string());
            }
            AuthCredentialKindIr::OAuth2ClientCredentials {
                client_id,
                client_secret,
                ..
            } => {
                out.insert(client_id.to_string());
                out.insert(client_secret.to_string());
            }
            AuthCredentialKindIr::Endpoint { .. } => {}
            AuthCredentialKindIr::Custom { .. } => {
                has_custom = true;
            }
        }
    }
    (out, has_custom)
}

fn emit_client_context(
    scheme: &TokenStream2,
    domain: &LitStr,
    ir: &Ir,
    policy: &PolicyBlocksResolved,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    auth_state_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let base_policy = emit_policy_fn_base(policy);
    let (auth_state_assoc_ty, init_auth_state) = emit_client_auth_state_init(ir, auth_state_ty);

    quote! {
        #[derive(Clone)]
        pub struct #cx_ty;

        impl ::concord_core::prelude::ClientContext for #cx_ty {
            type Vars = #vars_ty;
            type AuthVars = #auth_vars_ty;
            type AuthState = #auth_state_assoc_ty;
            const SCHEME: ::http::uri::Scheme = #scheme;
            const DOMAIN: &'static str = #domain;

            fn init_auth_state(
                vars: &Self::Vars,
                auth: &Self::AuthVars,
            ) -> Self::AuthState {
                #init_auth_state
            }

            fn base_policy(
                vars: &Self::Vars,
                auth: &Self::AuthVars,
                ctx: &::concord_core::prelude::ErrorContext,
            ) -> ::core::result::Result<::concord_core::prelude::Policy, ::concord_core::prelude::ApiClientError> {
                #base_policy
            }
        }
    }
}

fn emit_policy_fn_base(policy: &PolicyBlocksResolved) -> TokenStream2 {
    let mut ops = Vec::new();
    ops.extend(emit_policy_ops(
        policy,
        PolicyKeyKind::Header,
        PolicyEmitCtx::ClientBase,
    ));
    ops.extend(emit_policy_ops(
        policy,
        PolicyKeyKind::Query,
        PolicyEmitCtx::ClientBase,
    ));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, PolicyEmitCtx::ClientBase);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    if let Some(cache) = emit_cache_op(&policy.cache) {
        ops.push(cache);
    }
    if let Some(retry) = emit_retry_op(&policy.retry) {
        ops.push(retry);
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, PolicyEmitCtx::ClientBase) {
        ops.push(rate_limit);
    }

    let lock_auth = if policy_uses_auth(policy) {
        quote! { let auth = auth.read().unwrap(); }
    } else {
        quote! {}
    };

    quote! {
        let mut policy = ::concord_core::prelude::Policy::new();
        let ctx = ctx.clone();
        #[allow(unused_variables)]
        let cx = vars;
        #[allow(unused_variables)]
        let auth = auth;
        #lock_auth
        #( #ops )*
        ::core::result::Result::Ok(policy)
    }
}

fn emit_client_auth_vars(
    vars: &[VarInfo],
    auth_inner_ty: &Ident,
    auth_vars_ty: &Ident,
) -> TokenStream2 {
    use quote::quote;

    // empty auth vars => unit struct (no lock)
    if vars.is_empty() {
        return quote! {
            #[derive(Clone, Default)]
            pub struct #auth_vars_ty;
            impl #auth_vars_ty {
                #[inline]
                pub fn new() -> Self { Self }
            }
        };
    }

    let inner_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if v.optional {
            quote! { pub #name: ::core::option::Option<::concord_core::prelude::SecretString> }
        } else {
            quote! { pub #name: ::concord_core::prelude::SecretString }
        }
    });

    let required: Vec<&VarInfo> = vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
    let new_args = required.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        quote! { #name: #ty }
    });
    let inner_init_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #name: ::concord_core::prelude::SecretString::new(#name) }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #name: ::core::option::Option::Some(::concord_core::prelude::SecretString::new(#d)) }
            } else {
                quote! { #name: ::core::option::Option::None }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #name: ::concord_core::prelude::SecretString::new(#d) }
        }
    });

    // Default if no required args
    let can_default = required.is_empty();
    let default_impl = if can_default {
        quote! {
            impl ::core::default::Default for #auth_vars_ty {
                fn default() -> Self { Self::new() }
            }
        }
    } else {
        quote! {}
    };

    let new_sig = if required.is_empty() {
        quote! { pub fn new() -> Self }
    } else {
        quote! { pub fn new( #( #new_args ),* ) -> Self }
    };
    quote! {
        #[derive(Clone)]
        pub struct #auth_inner_ty {
            #( #inner_fields, )*
        }
        #[derive(Clone)]
        pub struct #auth_vars_ty(pub ::std::sync::Arc<::std::sync::RwLock<#auth_inner_ty>>);

        impl ::core::ops::Deref for #auth_vars_ty {
            type Target = ::std::sync::RwLock<#auth_inner_ty>;
            #[inline]
            fn deref(&self) -> &Self::Target { &self.0 }
        }

        impl #auth_vars_ty {
            #[inline]
            #new_sig {
                let inner = #auth_inner_ty { #( #inner_init_fields, )* };
                Self(::std::sync::Arc::new(::std::sync::RwLock::new(inner)))
            }
        }
        #default_impl
    }
}

fn emit_internal(ir: &Ir, vars_ty: &Ident, auth_vars_ty: &Ident, cx_ty: &Ident) -> TokenStream2 {
    let endpoint_parts = ir
        .endpoints
        .iter()
        .map(|ep| emit_endpoint_parts(ir, ep, vars_ty, auth_vars_ty, cx_ty));

    quote! {
        mod __internal {
            use super::*;
            #( #endpoint_parts )*
        }
    }
}

fn emit_endpoint_parts(
    ir: &Ir,
    ep: &EndpointIr,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let name = &ep.name;
    let method = &ep.method;
    let route_ty = emit_helpers::ident(&format!("__Route_{name}"), Span::call_site());
    let policy_ty = emit_helpers::ident(&format!("__Policy_{name}"), Span::call_site());

    let ep_opt = ep_optionals(ep);

    let mut prefix_layer_route_ops: Vec<TokenStream2> = Vec::new();
    let mut path_layer_route_ops: Vec<TokenStream2> = Vec::new();
    let mut layer_policy_ops: Vec<TokenStream2> = Vec::new();
    for &lid in &ep.ancestry {
        let layer = &ir.layers[lid];
        let layer_route = match layer.kind {
            crate::ast::LayerKind::Prefix => {
                emit_prefix_route_apply(&layer.prefix_pieces, Some(&ep_opt))
            }
            crate::ast::LayerKind::Path => emit_path_route_apply(&layer.path_pieces, Some(&ep_opt)),
        };
        match layer.kind {
            crate::ast::LayerKind::Prefix => prefix_layer_route_ops.push(layer_route),
            crate::ast::LayerKind::Path => path_layer_route_ops.push(layer_route),
        }

        let layer_policy_apply = emit_policy_apply_fn(&layer.policy, PolicyEmitCtx::Layer);
        layer_policy_ops.push(quote! {
            {
                let __prev = policy.layer();
                policy.set_layer(::concord_core::prelude::PolicyLayer::PrefixPath);
                #layer_policy_apply
                policy.set_layer(__prev);
            }
        });
    }

    let endpoint_route_apply = emit_path_route_apply(&ep.route_pieces, Some(&ep_opt));
    let route_apply = quote! {
        #( #prefix_layer_route_ops )*
        #( #path_layer_route_ops )*
        #endpoint_route_apply
    };

    let endpoint_policy_apply = emit_policy_apply_fn(&ep.policy, PolicyEmitCtx::Endpoint);
    let policy_apply = quote! {
        #( #layer_policy_ops )*
        {
            let __prev = policy.layer();
            policy.set_layer(::concord_core::prelude::PolicyLayer::Endpoint);
            #endpoint_policy_apply
            policy.set_layer(__prev);
        }
    };

    let paginate_ty = emit_helpers::ident(&format!("__Pag_{name}"), Span::call_site());
    let paginate_impl = emit_paginate_part(ep, &paginate_ty, cx_ty, vars_ty);

    let map_ty = emit_helpers::ident(&format!("__Map_{name}"), Span::call_site());
    let map_impl = emit_map_part(ep, &map_ty);

    let auth_impl = emit_auth_parts(ir, ep, cx_ty);

    quote! {
        pub struct #route_ty;
        impl ::concord_core::internal::RoutePart<super::#cx_ty, super::endpoints::#name> for #route_ty {
            fn apply(
                    ep: &super::endpoints::#name,
                    vars: &super::#vars_ty,
                    auth: &super::#auth_vars_ty,
                    route: &mut ::concord_core::prelude::RouteParts
                )
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                let _ = vars;
                let _ = auth;
                #route_apply
                ::core::result::Result::Ok(())
            }
        }

        pub struct #policy_ty;
        impl ::concord_core::internal::PolicyPart<super::#cx_ty, super::endpoints::#name> for #policy_ty {
            fn apply(ep: &super::endpoints::#name, vars: &super::#vars_ty, auth: &super::#auth_vars_ty, policy: &mut ::concord_core::prelude::Policy)
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                let ctx = ::concord_core::prelude::ErrorContext {
                    endpoint: ep.name(),
                    method: ::http::Method::#method,
                };
                #policy_apply
                ::core::result::Result::Ok(())
            }
        }

        #auth_impl
        #paginate_impl
        #map_impl
    }
}

fn auth_use_part_ident(ep_name: &Ident, index: usize) -> Ident {
    emit_helpers::ident(&format!("__Auth_{ep_name}_{index}"), Span::call_site())
}

fn auth_one_of_alt_part_ident(ep_name: &Ident, index: usize, alt_index: usize) -> Ident {
    emit_helpers::ident(
        &format!("__Auth_{ep_name}_{index}_Alt_{alt_index}"),
        Span::call_site(),
    )
}

fn emit_endpoint_auth_item_ty(ep: &EndpointIr, index: usize, plan: &AuthUsePlanIr) -> TokenStream2 {
    match plan {
        AuthUsePlanIr::Use(_) => {
            let ident = auth_use_part_ident(&ep.name, index);
            quote! { super::__internal::#ident }
        }
        AuthUsePlanIr::OneOf(alts) => {
            let mut iter = (0..alts.len()).rev();
            let last = iter
                .next()
                .expect("one_of must contain at least one alternative");
            let last_ident = auth_one_of_alt_part_ident(&ep.name, index, last);
            let mut out = quote! { super::__internal::#last_ident };
            for alt in iter {
                let ident = auth_one_of_alt_part_ident(&ep.name, index, alt);
                out =
                    quote! { ::concord_core::internal::OneOfAuth<super::__internal::#ident, #out> };
            }
            out
        }
    }
}

fn emit_endpoint_auth_ty(ep: &EndpointIr) -> TokenStream2 {
    if ep.auth_uses.is_empty() {
        return quote! { ::concord_core::internal::NoAuth };
    }

    let item_types: Vec<TokenStream2> = ep
        .auth_uses
        .iter()
        .enumerate()
        .map(|(idx, plan)| emit_endpoint_auth_item_ty(ep, idx, plan))
        .collect();

    let mut iter = item_types.into_iter().rev();
    let mut out = iter.next().expect("non-empty auth item types");
    for item in iter {
        out = quote! { ::concord_core::internal::AuthChain<#item, #out> };
    }
    out
}

fn emit_auth_parts(ir: &Ir, ep: &EndpointIr, cx_ty: &Ident) -> TokenStream2 {
    let mut parts = Vec::new();
    for (idx, plan) in ep.auth_uses.iter().enumerate() {
        match plan {
            AuthUsePlanIr::Use(auth_use) => {
                let part_ty = auth_use_part_ident(&ep.name, idx);
                parts.push(emit_auth_part_for_ident(
                    ir, ep, cx_ty, &part_ty, idx, None, auth_use,
                ));
            }
            AuthUsePlanIr::OneOf(alts) => {
                for (alt_idx, auth_use) in alts.iter().enumerate() {
                    let part_ty = auth_one_of_alt_part_ident(&ep.name, idx, alt_idx);
                    parts.push(emit_auth_part_for_ident(
                        ir,
                        ep,
                        cx_ty,
                        &part_ty,
                        idx,
                        Some(alt_idx),
                        auth_use,
                    ));
                }
            }
        }
    }

    quote! {
        #( #parts )*
    }
}

fn emit_auth_part_for_ident(
    ir: &Ir,
    ep: &EndpointIr,
    cx_ty: &Ident,
    part_ty: &Ident,
    idx: usize,
    alt_idx: Option<usize>,
    auth_use: &AuthUseIr,
) -> TokenStream2 {
    let ep_name = &ep.name;
    let credential = auth_use_credential_ident_ir(auth_use);
    let credential_ir = ir
        .client_auth_credentials
        .iter()
        .find(|c| c.name == *credential)
        .expect("auth use was validated by sema");
    let provider_ty = emit_auth_provider_ty(&credential_ir.kind);
    let usage_ty = emit_auth_usage_ty(auth_use);
    let usage_expr = emit_auth_usage_expr(ep, cx_ty, auth_use);
    let step_id = if let Some(alt) = alt_idx {
        LitStr::new(
            &format!("{}:{}:alt{}:{}", ep_name, idx, alt, credential),
            Span::call_site(),
        )
    } else {
        LitStr::new(
            &format!("{}:{}:{}", ep_name, idx, credential),
            Span::call_site(),
        )
    };
    let provenance_layer = match auth_use.provenance {
        AuthUseProvenanceIr::Client => LitStr::new("client", Span::call_site()),
        AuthUseProvenanceIr::Scope(scope_id) => {
            LitStr::new(&format!("scope:{scope_id}"), Span::call_site())
        }
        AuthUseProvenanceIr::Endpoint => LitStr::new("endpoint", Span::call_site()),
    };
    let manual_policy = match &credential_ir.kind {
        AuthCredentialKindIr::Endpoint { .. } => Some(quote! {
            ::concord_core::prelude::AuthStepPolicy {
                retry_on_unauthorized: false,
                retry_on_forbidden: false,
                retry_on_challenge_rejection: false,
                invalidate_on_unauthorized: true,
                invalidate_on_forbidden: true,
                invalidate_on_challenge_rejection: true,
                ..::concord_core::prelude::AuthStepPolicy::default()
            }
        }),
        _ => None,
    };
    let manual_policy_chain = manual_policy
        .as_ref()
        .map(|policy| quote! { .with_policy(#policy) })
        .unwrap_or_else(|| quote! {});

    quote! {
        pub struct #part_ty;

        impl ::concord_core::internal::AuthPart<super::#cx_ty, super::endpoints::#ep_name> for #part_ty {
            type Ctrl = ::concord_core::prelude::UseCredential<super::#cx_ty, #provider_ty, #usage_ty>;

            fn controller(
                ctx: ::concord_core::prelude::AuthBuildContext<'_, super::#cx_ty>,
                ep: &super::endpoints::#ep_name,
            ) -> ::core::result::Result<Self::Ctrl, ::concord_core::prelude::ApiClientError> {
                #usage_expr
                ::core::result::Result::Ok(::concord_core::prelude::UseCredential::new(
                    ctx.auth_state.#credential.clone(),
                    __usage,
                )
                #manual_policy_chain
                .with_step_id(#step_id)
                .with_provenance(::concord_core::prelude::AuthProvenance::new(#provenance_layer)))
            }
        }
    }
}

fn emit_auth_usage_ty(auth_use: &AuthUseIr) -> TokenStream2 {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => quote! { ::concord_core::prelude::BearerAuth },
        AuthUseKindIr::Header { .. } => quote! { ::concord_core::prelude::HeaderAuth },
        AuthUseKindIr::Query { .. } => quote! { ::concord_core::prelude::QueryAuth },
        AuthUseKindIr::Basic { .. } => quote! { ::concord_core::prelude::BasicAuth },
        AuthUseKindIr::Certificate { .. } => quote! { ::concord_core::prelude::CertificateAuth },
        AuthUseKindIr::Custom { usage_ty, .. } => quote! { #usage_ty },
    }
}

fn emit_auth_usage_expr(ep: &EndpointIr, cx_ty: &Ident, auth_use: &AuthUseIr) -> TokenStream2 {
    let ep_name = &ep.name;
    match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::BearerAuth::new();
        },
        AuthUseKindIr::Header { header, .. } => {
            let param = LitStr::new(&format!("auth header:{}", header.value()), header.span());
            quote! {
                let __usage = ::concord_core::prelude::HeaderAuth::new(
                    ::http::header::HeaderName::from_bytes(#header.as_bytes())
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                            ctx: ::concord_core::prelude::ErrorContext {
                                endpoint: <super::endpoints::#ep_name as ::concord_core::prelude::Endpoint<super::#cx_ty>>::name(ep),
                                method: <super::endpoints::#ep_name as ::concord_core::prelude::Endpoint<super::#cx_ty>>::METHOD,
                            },
                            param: #param,
                        })?
                );
            }
        }
        AuthUseKindIr::Query { key, .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::QueryAuth::new(#key);
        },
        AuthUseKindIr::Basic { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::BasicAuth::new();
        },
        AuthUseKindIr::Certificate { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::CertificateAuth::new();
        },
        AuthUseKindIr::Custom {
            usage_ty, usage, ..
        } => quote! {
            let _ = ep;
            let __usage: #usage_ty = (#usage);
        },
    }
}

fn auth_use_credential_ident_ir(auth_use: &AuthUseIr) -> &Ident {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { credential }
        | AuthUseKindIr::Header { credential, .. }
        | AuthUseKindIr::Query { credential, .. }
        | AuthUseKindIr::Basic { credential }
        | AuthUseKindIr::Certificate { credential }
        | AuthUseKindIr::Custom { credential, .. } => credential,
    }
}

fn emit_endpoints(ir: &Ir, cx_ty: &Ident) -> TokenStream2 {
    let endpoint_defs = ir.endpoints.iter().map(|ep| emit_endpoint_def(ep, cx_ty));
    let scope_modules = emit_endpoint_scope_modules(ir);
    quote! {
        pub mod endpoints {
            use super::*;
            #( #endpoint_defs )*
            #scope_modules
        }
    }
}

struct EndpointScopeModule {
    name: Ident,
    endpoints: Vec<Ident>,
    children: Vec<EndpointScopeModule>,
}

fn insert_endpoint_scope_module(
    modules: &mut Vec<EndpointScopeModule>,
    path: &[Ident],
    endpoint: &Ident,
) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };

    let index = if let Some(index) = modules.iter().position(|module| module.name == *head) {
        index
    } else {
        modules.push(EndpointScopeModule {
            name: head.clone(),
            endpoints: Vec::new(),
            children: Vec::new(),
        });
        modules.len() - 1
    };

    if tail.is_empty() {
        modules[index].endpoints.push(endpoint.clone());
    } else {
        insert_endpoint_scope_module(&mut modules[index].children, tail, endpoint);
    }
}

fn emit_endpoint_scope_modules(ir: &Ir) -> TokenStream2 {
    let mut modules = Vec::new();
    for endpoint in &ir.endpoints {
        if endpoint.scope_modules.is_empty() {
            continue;
        }
        insert_endpoint_scope_module(&mut modules, &endpoint.scope_modules, &endpoint.name);
    }

    let tokens = modules
        .iter()
        .map(|module| emit_endpoint_scope_module(module, 1));
    quote! { #( #tokens )* }
}

fn emit_endpoint_scope_module(module: &EndpointScopeModule, depth: usize) -> TokenStream2 {
    let name = &module.name;
    let endpoint_reexports = module.endpoints.iter().map(|endpoint| {
        let supers = (0..depth).map(|_| quote! { super:: });
        quote! { pub use #( #supers )* #endpoint; }
    });
    let children = module
        .children
        .iter()
        .map(|child| emit_endpoint_scope_module(child, depth + 1));

    quote! {
        pub mod #name {
            #( #endpoint_reexports )*
            #( #children )*
        }
    }
}

fn emit_client_wrapper(
    ir: &Ir,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    use quote::quote;

    let client_ty = &ir.client_name;

    // same "required vars" as Vars::new(...)
    let required: Vec<&VarInfo> = ir
        .client_vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();

    let new_args: Vec<TokenStream2> = required
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();

    let new_pass: Vec<TokenStream2> = required
        .iter()
        .map(|v| {
            let f = &v.rust;
            quote! { #f }
        })
        .collect();
    let new_pass = new_pass.as_slice();

    let required_auth: Vec<&VarInfo> = ir
        .client_auth_vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
    let new_auth_args: Vec<TokenStream2> = required_auth
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();
    let new_auth_pass: Vec<TokenStream2> = required_auth
        .iter()
        .map(|v| {
            let f = &v.rust;
            quote! { #f }
        })
        .collect();

    let mut ctor_args: Vec<TokenStream2> = Vec::new();
    ctor_args.extend(new_args.iter().cloned());
    ctor_args.extend(new_auth_args.iter().cloned());

    let var_setters = ir.client_vars.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        let set_name = emit_helpers::ident(&format!("set_{f}"), f.span());
        if v.optional {
            let clear_name = emit_helpers::ident(&format!("clear_{f}"), f.span());
            quote! {
                #[inline]
                pub fn #set_name(&mut self, v: #ty) -> &mut Self {
                    self.inner.vars_mut().#f = ::core::option::Option::Some(v);
                    self
                }
                #[inline]
                pub fn #clear_name(&mut self) -> &mut Self {
                    self.inner.vars_mut().#f = ::core::option::Option::None;
                    self
                }
            }
        } else {
            quote! {
                #[inline]
                pub fn #set_name(&mut self, v: #ty) -> &mut Self {
                    self.inner.vars_mut().#f = v;
                    self
                }
            }
        }
    });

    let (credential_secret_names, has_custom_credentials) = auth_credential_secret_names(ir);
    let configure_rate_limiter = if let Some(policy) = &ir.rate_limit_response_policy {
        quote! {
            __inner.set_rate_limiter(::std::sync::Arc::new(
                ::concord_core::prelude::GovernorRateLimiter::new()
                    .with_response_policy(::std::sync::Arc::new(#policy::default()))
            ));
        }
    } else {
        quote! {}
    };
    let configure_cache_store = if ir.cache_store_enabled {
        let configure_cache_store_body = if let Some(config) = &ir.cache_store_config {
            let config = emit_cache_config(config);
            quote! {
                let __cache_config = #config;
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::prelude::MokaCacheStore::new(
                        ::concord_core::prelude::MokaCacheConfig::from_cache_config(&__cache_config)
                    )
                ));
            }
        } else {
            quote! {
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::prelude::MokaCacheStore::default()
                ));
            }
        };
        quote! {
            #[cfg(not(feature = "cache-moka"))]
            compile_error!(
                "cache default backend requires a `cache-moka` crate feature that enables `concord_core/cache-moka`"
            );
            #[cfg(feature = "cache-moka")]
            {
                #configure_cache_store_body
            }
        }
    } else {
        quote! {}
    };
    let auth_setters = ir.client_auth_vars.iter().map(|v| {
        let f = &v.rust;
        let set_name = emit_helpers::ident(&format!("set_{f}"), f.span());
        let rebuild_auth_state =
            has_custom_credentials || credential_secret_names.contains(&f.to_string());
        if v.optional {
            let clear_name = emit_helpers::ident(&format!("clear_{f}"), f.span());
            if rebuild_auth_state {
                quote! {
            #[inline]
            pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = ::core::option::Option::Some(v.into());
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
            #[inline]
            pub fn #clear_name(&mut self) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = ::core::option::Option::None;
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
        }
            } else {
                quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
                let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = ::core::option::Option::Some(v.into());
                self
            }
            #[inline]
            pub fn #clear_name(&self) -> &Self {
                let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = ::core::option::Option::None;
                self
            }
        }
            }
        } else {
            if rebuild_auth_state {
                quote! {
            #[inline]
            pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = v.into();
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
        }
            } else {
                quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
               let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = v.into();
                self
            }
        }
            }
        }
    });
    let credential_lifecycle_methods = ir.client_auth_credentials.iter().filter_map(|credential| {
        let name = &credential.name;
        let AuthCredentialKindIr::Endpoint {
            endpoint,
            output_ty,
        } = &credential.kind
        else {
            return None;
        };
        let acquire_name = emit_helpers::ident(&format!("acquire_auth_{name}"), name.span());
        let set_name = emit_helpers::ident(&format!("set_auth_{name}_value"), name.span());
        let clear_name = emit_helpers::ident(&format!("clear_auth_{name}"), name.span());
        let has_name = emit_helpers::ident(&format!("has_auth_{name}"), name.span());
        Some(quote! {
            #[inline]
            pub async fn #acquire_name(
                &self,
                ep: endpoints::#endpoint,
            ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                let value: #output_ty = self.request(ep).execute().await?;
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.set_manual(value).await;
                Ok(())
            }

            #[inline]
            pub async fn #set_name(&self, value: #output_ty) {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.set_manual(value).await;
            }

            #[inline]
            pub async fn #clear_name(&self) {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.clear_manual().await;
            }

            #[inline]
            pub async fn #has_name(&self) -> bool {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.has_value().await
            }
        })
    });

    quote! {
        #[derive(Clone)]
        pub struct #client_ty<T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
            inner: ::concord_core::prelude::ApiClient<#cx_ty, T>,
        }
        impl #client_ty<::concord_core::prelude::ReqwestTransport> {
            #[inline]
            pub fn new( #( #ctor_args ),* ) -> Self {
               let vars = #vars_ty::new( #( #new_pass ),* );
                let auth_vars = #auth_vars_ty::new( #( #new_auth_pass ),* );
                let mut __inner = ::concord_core::prelude::ApiClient::<#cx_ty, ::concord_core::prelude::ReqwestTransport>::new(vars, auth_vars);
                #configure_rate_limiter
                #configure_cache_store
                Self { inner: __inner }
            }


            #[inline]
            pub fn new_with_transport<T2: ::concord_core::prelude::Transport>(
                #( #ctor_args, )*
                transport: T2
            ) -> #client_ty<T2> {
                let vars = #vars_ty::new( #( #new_pass ),* );
                let auth_vars = #auth_vars_ty::new( #( #new_auth_pass ),* );
                let mut __inner = ::concord_core::prelude::ApiClient::<#cx_ty, T2>::with_transport(vars, auth_vars, transport);
                #configure_rate_limiter
                #configure_cache_store
                #client_ty { inner: __inner }
            }


        }

        impl<T: ::concord_core::prelude::Transport> #client_ty<T> {
            #( #var_setters )*
            #( #auth_setters )*
            #( #credential_lifecycle_methods )*

            #[inline]
            pub fn debug_level(&self) -> ::concord_core::prelude::DebugLevel { self.inner.debug_level() }
            #[inline]
            pub fn set_debug_level(&mut self, level: ::concord_core::prelude::DebugLevel) { self.inner.set_debug_level(level); }
            #[inline]
            pub fn with_debug_level(mut self, level: ::concord_core::prelude::DebugLevel) -> Self { self.inner.set_debug_level(level); self }
            #[inline]
            pub fn pagination_caps(&self) -> ::concord_core::prelude::Caps { self.inner.pagination_caps() }
            #[inline]
            pub fn set_pagination_caps(&mut self, caps: ::concord_core::prelude::Caps) { self.inner.set_pagination_caps(caps); }
            #[inline]
            pub fn with_pagination_caps(mut self, caps: ::concord_core::prelude::Caps) -> Self { self.inner.set_pagination_caps(caps); self }
            #[inline]
            pub fn set_rate_limiter(&mut self, limiter: ::std::sync::Arc<dyn ::concord_core::prelude::RateLimiter>) { self.inner.set_rate_limiter(limiter); }
            #[inline]
            pub fn with_rate_limiter(mut self, limiter: ::std::sync::Arc<dyn ::concord_core::prelude::RateLimiter>) -> Self { self.inner.set_rate_limiter(limiter); self }
            #[inline]
            pub fn set_cache_store(&mut self, store: ::std::sync::Arc<dyn ::concord_core::prelude::CacheStore>) { self.inner.set_cache_store(store); }
            #[inline]
            pub fn with_cache_store(mut self, store: ::std::sync::Arc<dyn ::concord_core::prelude::CacheStore>) -> Self { self.inner.set_cache_store(store); self }
            #[inline]
            pub fn set_inflight_policy(&mut self, policy: ::std::sync::Arc<dyn ::concord_core::prelude::InflightPolicy>) { self.inner.set_inflight_policy(policy); }
            #[inline]
            pub fn with_inflight_policy(mut self, policy: ::std::sync::Arc<dyn ::concord_core::prelude::InflightPolicy>) -> Self { self.inner.set_inflight_policy(policy); self }
            #[inline]
            pub fn request<E>(&self, ep: E) -> ::concord_core::prelude::PendingRequest<'_, #cx_ty, E, T>
            where
                E: ::concord_core::prelude::Endpoint<#cx_ty>,
            {
                self.inner.request(ep)
            }
        }
    }
}

fn emit_endpoint_def(ep: &EndpointIr, cx_ty: &Ident) -> TokenStream2 {
    let name = &ep.name;
    let method = &ep.method;
    let endpoint_name = if ep.scope_modules.is_empty() {
        LitStr::new(&name.to_string(), name.span())
    } else {
        let mut qualified = ep
            .scope_modules
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("::");
        qualified.push_str("::");
        qualified.push_str(&name.to_string());
        LitStr::new(&qualified, name.span())
    };

    // fields (endpoint vars)
    let mut fields_ts = Vec::new();
    let mut setters_ts = Vec::new();

    for v in &ep.vars {
        let f = &v.rust;
        let ty = &v.ty;
        if v.optional {
            fields_ts.push(quote! { pub(crate) #f: ::core::option::Option<#ty> });
            let clear = emit_helpers::ident(&format!("clear_{f}"), f.span());
            setters_ts.push(quote! {
                #[inline]
                pub fn #f(mut self, v: #ty) -> Self { self.#f = ::core::option::Option::Some(v); self }
                #[inline]
                pub fn #clear(mut self) -> Self { self.#f = ::core::option::Option::None; self }
            });
        } else {
            fields_ts.push(quote! { pub(crate) #f: #ty });
            setters_ts.push(quote! {
                #[inline]
                pub fn #f(mut self, v: #ty) -> Self { self.#f = v; self }
            });
        }
    }

    // ctor args: required vars (non-optional, no default) + body
    let required_vars: Vec<&VarInfo> = ep
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();

    let _new_args = required_vars.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        quote! { #f: #ty }
    });

    let init_fields = ep.vars.iter().map(|v| {
        let f = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #f }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #f: ::core::option::Option::Some(#d) }
            } else {
                quote! { #f: ::core::option::Option::None }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #f: #d }
        }
    });

    let mut struct_fields: Vec<TokenStream2> = fields_ts;
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        struct_fields.push(quote! { pub(crate) body: #ty });
    }

    let mut fn_args: Vec<TokenStream2> = required_vars
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        fn_args.push(quote! { body: #ty });
    }

    let mut init_parts: Vec<TokenStream2> = init_fields.collect();
    if ep.body.is_some() {
        init_parts.push(quote! { body });
    }

    let route_ident = emit_helpers::ident(&format!("__Route_{name}"), Span::call_site());
    let policy_ident = emit_helpers::ident(&format!("__Policy_{name}"), Span::call_site());
    let route_ty = quote! { super::__internal::#route_ident };
    let policy_ty = quote! { super::__internal::#policy_ident };
    let auth_ty = emit_endpoint_auth_ty(ep);

    // pagination part
    let pagination_ty = if ep.paginate.is_some() {
        let p_ident = emit_helpers::ident(&format!("__Pag_{name}"), Span::call_site());
        quote! { super::__internal::#p_ident }
    } else {
        quote! { ::concord_core::internal::NoPagination }
    };

    // body part
    let body_ty = if ep.body.is_some() {
        let b_ident = emit_helpers::ident(&format!("__Body_{name}"), Span::call_site());
        quote! { #b_ident }
    } else {
        quote! { ::concord_core::internal::NoBody }
    };

    // response spec
    let dec_enc = &ep.response.enc;
    let decoded_ty = &ep.response.ty;
    let response_base = quote! { ::concord_core::internal::Decoded<#dec_enc, #decoded_ty> };

    let response_ty = if ep.map.is_some() {
        let m_ident = emit_helpers::ident(&format!("__Map_{name}"), Span::call_site());
        quote! { ::concord_core::internal::Mapped<#response_base, super::__internal::#m_ident> }
    } else {
        response_base
    };

    // BodyPart impl if needed
    let body_impl = if let Some(body) = &ep.body {
        let enc = &body.enc;
        let ty = &body.ty;
        let b_ident = emit_helpers::ident(&format!("__Body_{name}"), Span::call_site());
        quote! {
            pub struct #b_ident;
            impl ::concord_core::internal::BodyPart<#name> for #b_ident {
                type Body = #ty;
                type Enc = #enc;
                fn body(ep: &#name) -> ::core::option::Option<&Self::Body> {
                    ::core::option::Option::Some(&ep.body)
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        pub struct #name {
              #( #struct_fields, )*
        }

        impl #name {
            #[inline]
              pub fn new( #( #fn_args ),* ) -> Self {
                Self { #( #init_parts, )* }
            }

            #( #setters_ts )*
        }

        #body_impl

        impl ::concord_core::prelude::Endpoint<super::#cx_ty> for #name {
            const METHOD: ::http::Method = ::http::Method::#method;
            type Route = #route_ty;
            type Policy = #policy_ty;
            type Auth = #auth_ty;
            type Pagination = #pagination_ty;
            type Body = #body_ty;
            type Response = #response_ty;

            #[inline]
            fn name(&self) -> &'static str {
                #endpoint_name
            }
        }
    }
}

#[derive(Clone, Copy)]
enum PolicyEmitCtx {
    ClientBase,
    Layer,
    Endpoint,
}

fn emit_policy_apply_fn(policy: &PolicyBlocksResolved, ctx: PolicyEmitCtx) -> TokenStream2 {
    let mut ops = Vec::new();

    ops.push(quote! {
        #[allow(unused_variables)]
        let cx = vars;
        #[allow(unused_variables)]
        let ep = ep;
        #[allow(unused_variables)]
        let auth = auth;
    });

    if policy_uses_auth(policy) {
        // AuthVars is a single RwLock<AuthInner>; lock exactly once per request build.
        ops.push(quote! { let auth = auth.read().unwrap(); });
    }
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Header, ctx));
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Query, ctx));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, ctx);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    if let Some(cache) = emit_cache_op(&policy.cache) {
        ops.push(cache);
    }
    if let Some(retry) = emit_retry_op(&policy.retry) {
        ops.push(retry);
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, ctx) {
        ops.push(rate_limit);
    }
    quote! { #( #ops )* }
}

fn emit_cache_op(cache: &Option<CacheResolved>) -> Option<TokenStream2> {
    let cache = cache.as_ref()?;
    Some(match cache {
        CacheResolved::Clear => quote! {
            policy.clear_cache();
        },
        CacheResolved::Set(config) => {
            let config = emit_cache_config(config);
            quote! {
                policy.set_cache(#config);
            }
        }
        CacheResolved::Patch(patch) => {
            let ops = emit_cache_patch_ops(patch);
            quote! {
                let mut __cache = policy.cache().cloned().unwrap_or_default();
                #( #ops )*
                policy.set_cache(__cache);
            }
        }
    })
}

fn emit_cache_config(config: &CacheConfigResolved) -> TokenStream2 {
    let mut ops = Vec::new();
    if config.http {
        ops.push(quote! {
            __cache = __cache.with_http();
        });
    }
    if let Some(ttl_secs) = config.default_ttl_secs {
        ops.push(quote! {
            __cache = __cache.with_default_ttl(::std::time::Duration::from_secs(#ttl_secs));
        });
    }
    if let Some(capacity) = config.capacity {
        let op = emit_cache_capacity_op(capacity);
        ops.push(op);
    }
    if let Some(max_body_bytes) = config.max_body_bytes {
        ops.push(quote! {
            __cache = __cache.with_max_body_bytes(
                ::core::convert::TryFrom::try_from(#max_body_bytes)
                    .expect("validated cache max_body fits usize")
            );
        });
    }
    if let Some(revalidate) = config.revalidate {
        ops.push(quote! {
            __cache = __cache.with_revalidate(#revalidate);
        });
    }
    if let Some(shared) = config.shared {
        ops.push(quote! {
            __cache = __cache.with_shared(#shared);
        });
    }
    if let Some(failure_mode) = config.failure_mode {
        let failure_mode = emit_cache_failure_mode(failure_mode);
        ops.push(quote! {
            __cache = __cache.with_failure_mode(#failure_mode);
        });
    }
    quote! {{
        let mut __cache = ::concord_core::prelude::CacheConfig::new();
        #( #ops )*
        __cache
    }}
}

fn emit_cache_patch_ops(patch: &CacheConfigPatchResolved) -> Vec<TokenStream2> {
    let mut ops = Vec::new();
    if patch.http == Some(true) {
        ops.push(quote! {
            __cache = __cache.with_http();
        });
    }
    if let Some(ttl_secs) = patch.default_ttl_secs {
        ops.push(quote! {
            __cache = __cache.with_default_ttl(::std::time::Duration::from_secs(#ttl_secs));
        });
    }
    if let Some(capacity) = patch.capacity {
        ops.push(emit_cache_capacity_op(capacity));
    }
    if let Some(max_body_bytes) = patch.max_body_bytes {
        ops.push(quote! {
            __cache = __cache.with_max_body_bytes(
                ::core::convert::TryFrom::try_from(#max_body_bytes)
                    .expect("validated cache max_body fits usize")
            );
        });
    }
    if let Some(revalidate) = patch.revalidate {
        ops.push(quote! {
            __cache = __cache.with_revalidate(#revalidate);
        });
    }
    if let Some(shared) = patch.shared {
        ops.push(quote! {
            __cache = __cache.with_shared(#shared);
        });
    }
    if let Some(failure_mode) = patch.failure_mode {
        let failure_mode = emit_cache_failure_mode(failure_mode);
        ops.push(quote! {
            __cache = __cache.with_failure_mode(#failure_mode);
        });
    }
    ops
}

fn emit_cache_capacity_op(capacity: CacheCapacityResolved) -> TokenStream2 {
    match capacity {
        CacheCapacityResolved::Entries(entries) => quote! {
            __cache = __cache.with_capacity_entries(#entries);
        },
        CacheCapacityResolved::Bytes(bytes) => quote! {
            __cache = __cache.with_capacity_bytes(#bytes);
        },
    }
}

fn emit_cache_failure_mode(mode: CacheFailureModeResolved) -> TokenStream2 {
    match mode {
        CacheFailureModeResolved::Ignore => {
            quote! { ::concord_core::prelude::CacheFailureMode::Ignore }
        }
        CacheFailureModeResolved::ServeStaleOnError => {
            quote! { ::concord_core::prelude::CacheFailureMode::ServeStaleOnError }
        }
    }
}

fn emit_retry_op(retry: &Option<RetryResolved>) -> Option<TokenStream2> {
    let retry = retry.as_ref()?;
    Some(match retry {
        RetryResolved::Clear => quote! {
            policy.clear_retry();
        },
        RetryResolved::Set(config) => {
            let config = emit_retry_config(config);
            quote! {
                policy.set_retry(#config);
            }
        }
        RetryResolved::Patch(patch) => {
            let ops = emit_retry_patch_ops(patch);
            quote! {
                let mut __retry = policy.retry().cloned().unwrap_or_default();
                #( #ops )*
                policy.set_retry(__retry);
            }
        }
    })
}

fn emit_retry_config(config: &RetryConfigResolved) -> TokenStream2 {
    let attempts = config.attempts;
    let methods = config
        .methods
        .iter()
        .map(|method| quote! { ::http::Method::#method });
    let statuses = config.statuses.iter().map(
        |status| quote! { ::http::StatusCode::from_u16(#status).expect("valid retry status") },
    );
    let transport_errors = config.transport_errors.iter().map(|kind| {
        quote! { ::concord_core::transport::TransportErrorKind::#kind }
    });
    let backoff = emit_retry_backoff(&config.backoff);
    let respect_retry_after = config.respect_retry_after;
    let idempotency = emit_retry_idempotency(&config.idempotency);

    quote! {
        ::concord_core::prelude::RetryConfig {
            attempts: #attempts,
            methods: ::std::vec![ #( #methods ),* ],
            statuses: ::std::vec![ #( #statuses ),* ],
            transport_errors: ::std::vec![ #( #transport_errors ),* ],
            backoff: #backoff,
            respect_retry_after: #respect_retry_after,
            idempotency: #idempotency,
        }
    }
}

fn emit_retry_patch_ops(patch: &RetryPatchResolved) -> Vec<TokenStream2> {
    let mut ops = Vec::new();

    if let Some(attempts) = patch.attempts {
        ops.push(quote! { __retry.attempts = #attempts; });
    }
    if let Some(methods) = &patch.methods {
        let methods = methods
            .iter()
            .map(|method| quote! { ::http::Method::#method });
        ops.push(quote! { __retry.methods = ::std::vec![ #( #methods ),* ]; });
    }
    if let Some(statuses) = &patch.statuses {
        let statuses = statuses.iter().map(
            |status| quote! { ::http::StatusCode::from_u16(#status).expect("valid retry status") },
        );
        ops.push(quote! { __retry.statuses = ::std::vec![ #( #statuses ),* ]; });
    }
    if let Some(transport_errors) = &patch.transport_errors {
        let transport_errors = transport_errors.iter().map(|kind| {
            quote! { ::concord_core::transport::TransportErrorKind::#kind }
        });
        ops.push(quote! { __retry.transport_errors = ::std::vec![ #( #transport_errors ),* ]; });
    }
    if let Some(backoff) = &patch.backoff {
        let backoff = emit_retry_backoff(backoff);
        ops.push(quote! { __retry.backoff = #backoff; });
    }
    if let Some(respect_retry_after) = patch.respect_retry_after {
        ops.push(quote! { __retry.respect_retry_after = #respect_retry_after; });
    }
    if let Some(idempotency) = &patch.idempotency {
        let idempotency = emit_retry_idempotency(idempotency);
        ops.push(quote! { __retry.idempotency = #idempotency; });
    }

    ops
}

fn emit_retry_backoff(backoff: &RetryBackoffResolved) -> TokenStream2 {
    match backoff {
        RetryBackoffResolved::None => quote! { ::concord_core::prelude::RetryBackoff::None },
    }
}

fn emit_retry_idempotency(idempotency: &RetryIdempotencyResolved) -> TokenStream2 {
    match idempotency {
        RetryIdempotencyResolved::SafeMethodsOnly => {
            quote! { ::concord_core::prelude::RetryIdempotency::SafeMethodsOnly }
        }
        RetryIdempotencyResolved::Header(header) => {
            let name = emit_helpers::emit_header_name(&header.value(), header.span());
            quote! { ::concord_core::prelude::RetryIdempotency::Header(#name) }
        }
    }
}

fn emit_rate_limit_op(
    rate_limit: &Option<RateLimitResolved>,
    ctx: PolicyEmitCtx,
) -> Option<TokenStream2> {
    let rate_limit = rate_limit.as_ref()?;
    Some(match rate_limit {
        RateLimitResolved::Clear => quote! {
            policy.clear_rate_limit();
        },
        RateLimitResolved::Add(plan) => {
            let plan = emit_rate_limit_plan(plan, ctx);
            quote! {
                policy.add_rate_limit(#plan);
            }
        }
        RateLimitResolved::Replace(plan) => {
            let plan = emit_rate_limit_plan(plan, ctx);
            quote! {
                policy.replace_rate_limit(#plan);
            }
        }
    })
}

fn emit_rate_limit_plan(plan: &RateLimitPlanResolved, ctx: PolicyEmitCtx) -> TokenStream2 {
    let buckets = plan.buckets.iter().map(|bucket| {
        let kind = LitStr::new(&bucket.kind, Span::call_site());
        let name = LitStr::new(&bucket.name, Span::call_site());
        let key = emit_rate_limit_key(&bucket.key, ctx);
        let cost = bucket.cost;
        let windows = bucket.windows.iter().map(|window| {
            let max = window.max;
            let per_secs = window.per_secs;
            quote! {
                ::concord_core::prelude::RateLimitWindow::new(
                    ::std::num::NonZeroU32::new(#max).expect("validated non-zero rate limit max"),
                    ::std::time::Duration::from_secs(#per_secs),
                )
            }
        });
        quote! {
            ::concord_core::prelude::RateLimitBucketUse::new(#kind, #name, #key)
                .with_cost(::std::num::NonZeroU32::new(#cost).expect("validated non-zero rate limit cost"))
                .with_windows(::std::vec![ #( #windows ),* ])
        }
    });
    quote! {
        ::concord_core::prelude::RateLimitPlan::from_buckets(::std::vec![ #( #buckets ),* ])
    }
}

fn emit_rate_limit_key(keys: &[RateLimitKeyResolved], ctx: PolicyEmitCtx) -> TokenStream2 {
    let parts = keys.iter().map(|key| match key {
        RateLimitKeyResolved::RouteHost => {
            quote! { ::concord_core::prelude::RateLimitKeyPart::url_host() }
        }
        RateLimitKeyResolved::Endpoint => {
            quote! { ::concord_core::prelude::RateLimitKeyPart::endpoint() }
        }
        RateLimitKeyResolved::Method => {
            quote! { ::concord_core::prelude::RateLimitKeyPart::method() }
        }
        RateLimitKeyResolved::Named { name, .. } => {
            let name = LitStr::new(name, Span::call_site());
            quote! {
                compile_error!(concat!("unresolved rate_limit key `", #name, "`"))
            }
        }
        RateLimitKeyResolved::EpField { name, field } => {
            let name = LitStr::new(name, field.span());
            match ctx {
                PolicyEmitCtx::ClientBase => quote! {
                    compile_error!("endpoint/scope rate_limit key cannot be used in client base policy")
                },
                PolicyEmitCtx::Layer | PolicyEmitCtx::Endpoint => quote! {
                    ::concord_core::prelude::RateLimitKeyPart::static_value(
                        #name,
                        ::std::string::ToString::to_string(&ep.#field),
                    )
                },
            }
        }
        RateLimitKeyResolved::Static { name, value } => {
            let name = LitStr::new(name, Span::call_site());
            let value = LitStr::new(value, Span::call_site());
            quote! {
                ::concord_core::prelude::RateLimitKeyPart::static_value(#name, #value)
            }
        }
    });
    quote! {
        ::concord_core::prelude::RateLimitKey::new(::std::vec![ #( #parts ),* ])
    }
}

fn emit_policy_ops(
    policy: &PolicyBlocksResolved,
    kind: PolicyKeyKind,
    ctx: PolicyEmitCtx,
) -> Vec<TokenStream2> {
    let ops = match kind {
        PolicyKeyKind::Header => &policy.headers,
        PolicyKeyKind::Query => &policy.query,
    };

    ops.iter()
        .map(|op| match op {
            PolicyOp::Remove { key } => emit_remove_op(key, kind, ctx),
            PolicyOp::Bind {
                key,
                field,
                optional,
                kind: _,
            } => emit_bind_op(key, kind, field, *optional, ctx),
            PolicyOp::Set {
                key,
                value,
                op,
                conditional_on_optional_ref,
            } => emit_set_op(key, kind, value, *op, *conditional_on_optional_ref, ctx),
        })
        .collect()
}

fn emit_key_string(key: &KeyResolved, kind: PolicyKeyKind) -> (String, Span, TokenStream2) {
    match key {
        KeyResolved::Static(l) => (l.value(), l.span(), quote! { #l }),
        KeyResolved::Ident(id) => {
            let s = match kind {
                PolicyKeyKind::Header => emit_helpers::to_kebab(id),
                PolicyKeyKind::Query => id.to_string(),
            };
            let lit = emit_helpers::lit_str(&s, id.span());
            (s, id.span(), quote! { #lit })
        }
    }
}

fn emit_remove_op(key: &KeyResolved, kind: PolicyKeyKind, _ctx: PolicyEmitCtx) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let name = emit_helpers::emit_header_name(&ks, sp);
            quote! {
                policy.remove_header(#name);
            }
        }
        PolicyKeyKind::Query => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let lit = emit_helpers::lit_str(&ks, sp);
            quote! {
                policy.remove_query(#lit);
            }
        }
    }
}

fn emit_bind_op(
    key: &KeyResolved,
    kind: PolicyKeyKind,
    field: &Ident,
    optional: bool,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let name = emit_helpers::emit_header_name(&ks, sp);

            let value_expr = match ctx {
                PolicyEmitCtx::ClientBase => quote! { &vars.#field },
                _ => quote! { &ep.#field },
            };

            if optional {
                quote! {
                    if let ::core::option::Option::Some(__v) = #value_expr.as_ref() {
                        let __hv = ::http::HeaderValue::from_str(&__v.to_string())
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                                ctx: ctx.clone(),
                                param: concat!("header:", #ks),
                            })?;
                        policy.insert_header(#name, __hv);
                    } else {
                        policy.remove_header(#name);
                    }
                }
            } else {
                quote! {
                    let __hv = ::http::HeaderValue::from_str(&#value_expr.to_string())
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                            ctx: ctx.clone(),
                            param: concat!("header:", #ks),
                        })?;
                    policy.insert_header(#name, __hv);
                }
            }
        }
        PolicyKeyKind::Query => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let lit = emit_helpers::lit_str(&ks, sp);

            let value_expr = match ctx {
                PolicyEmitCtx::ClientBase => quote! { &vars.#field },
                _ => quote! { &ep.#field },
            };

            if optional {
                quote! {
                    if let ::core::option::Option::Some(__v) = #value_expr.as_ref() {
                        policy.set_query(#lit, __v.to_string());
                    } else {
                        policy.remove_query(#lit);
                    }
                }
            } else {
                quote! {
                    policy.set_query(#lit, #value_expr.to_string());
                }
            }
        }
    }
}

fn emit_set_op(
    key: &KeyResolved,
    kind: PolicyKeyKind,
    value: &ValueKind,
    op: SetOp,
    conditional: Option<OptionalRefKind>,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let name = emit_helpers::emit_header_name(&ks, sp);
            if let ValueKind::Fmt(fmt) = value {
                let err = syn::LitStr::new(&format!("header:{ks}"), sp);
                let build = emit_fmt_build_string(fmt);
                let insert = quote! {
                    let __hv = ::http::HeaderValue::from_str(&__fmt_s)
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err })?;
                    policy.insert_header(#name, __hv);
                };

                if fmt.require_all {
                    let checks = fmt.pieces.iter().filter_map(|p| {
                        let FmtResolvedPiece::Var {
                            source,
                            field,
                            optional: true,
                        } = p
                        else {
                            return None;
                        };
                        match source {
                            FmtVarSource::Cx => {
                                Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Ep => {
                                Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Auth => {
                                Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } })
                            }
                        }
                    });
                    return quote! {
                        let mut __fmt_ok: bool = true;
                        #( #checks )*
                        if __fmt_ok {
                            let __fmt_s: ::std::string::String = { #build };
                            #insert
                        } else {
                            policy.remove_header(#name);
                        }
                    };
                } else {
                    return quote! {
                        let __fmt_s: ::std::string::String = { #build };
                        #insert
                    };
                }
            }
            // auth direct (non-fmt)
            if let ValueKind::AuthField(fld) = value {
                let err = syn::LitStr::new(&format!("header:{ks}"), sp);
                return if let Some(OptionalRefKind::Auth) = conditional {
                    quote! {
                        {
                            if let ::core::option::Option::Some(__v) = auth.#fld.as_ref() {
                                let __hv = ::http::HeaderValue::from_str(__v.expose())
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err })?;
                                policy.insert_header(#name, __hv);
                            } else {
                                policy.remove_header(#name);
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let __hv = ::http::HeaderValue::from_str(auth.#fld.expose())
                               .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err })?;
                            policy.insert_header(#name, __hv);
                        }
                    }
                };
            }
            // conditional optional ref => if Some set else remove
            if let Some(_ref_kind) = conditional {
                let as_ref_expr = match value {
                    ValueKind::CxField(f) => match ctx {
                        PolicyEmitCtx::ClientBase => quote! { vars.#f.as_ref() },
                        _ => quote! { vars.#f.as_ref() },
                    },
                    ValueKind::EpField(f) => quote! { ep.#f.as_ref() },
                    _ => unreachable!(),
                };
                return quote! {
                    if let ::core::option::Option::Some(__v) = #as_ref_expr {
                        let __hv = ::http::HeaderValue::from_str(&__v.to_string())
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                                ctx: ctx.clone(),
                                param: concat!("header:", #ks),
                            })?;
                        policy.insert_header(#name, __hv);
                    } else {
                        policy.remove_header(#name);
                    }
                };
            }

            let hv = match value {
                ValueKind::LitStr(s) => emit_helpers::emit_header_value_from_static(s),
                _ => {
                    let ex = emit_value_expr(value, ctx);
                    emit_helpers::emit_header_value_from_expr(&syn::parse2(ex).unwrap(), &ks, sp)
                }
            };

            quote! {
                policy.insert_header(#name, #hv);
            }
        }
        PolicyKeyKind::Query => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let lit = emit_helpers::lit_str(&ks, sp);
            if let ValueKind::Fmt(fmt) = value {
                let build = emit_fmt_build_string(fmt);
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __fmt_s); },
                    SetOp::Push => quote! { policy.push_query(#lit, __fmt_s); },
                };
                if fmt.require_all {
                    let checks = fmt.pieces.iter().filter_map(|p| {
                        let FmtResolvedPiece::Var {
                            source,
                            field,
                            optional: true,
                        } = p
                        else {
                            return None;
                        };
                        match source {
                            FmtVarSource::Cx => {
                                Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Ep => {
                                Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Auth => {
                                Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } })
                            }
                        }
                    });
                    return quote! {
                        let mut __fmt_ok: bool = true;
                        #( #checks )*
                        if __fmt_ok {
                            let __fmt_s: ::std::string::String = { #build };
                            #setter
                        } else {
                            policy.remove_query(#lit);
                        }
                    };
                } else {
                    return quote! {
                        let __fmt_s: ::std::string::String = { #build };
                        #setter
                    };
                }
            }
            if let ValueKind::AuthField(fld) = value {
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __s); },
                    SetOp::Push => quote! { policy.push_query(#lit, __s); },
                };
                return if let Some(OptionalRefKind::Auth) = conditional {
                    quote! {
                        {
                           if let ::core::option::Option::Some(__v) = auth.#fld.as_ref() {
                                let __s = __v.expose();
                                #setter
                            } else {
                                policy.remove_query(#lit);
                            }
                        }
                    }
                } else {
                    quote! {
                    {
                        let __s = auth.#fld.expose();
                        #setter
                    }
                    }
                };
            }
            if let Some(_ref_kind) = conditional {
                let as_ref_expr = match value {
                    ValueKind::CxField(f) => match ctx {
                        PolicyEmitCtx::ClientBase => quote! { vars.#f.as_ref() },
                        _ => quote! { vars.#f.as_ref() },
                    },
                    ValueKind::EpField(f) => quote! { ep.#f.as_ref() },
                    _ => unreachable!(),
                };
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __v.to_string()); },
                    SetOp::Push => quote! { policy.push_query(#lit, __v.to_string()); },
                };
                return quote! {
                    if let ::core::option::Option::Some(__v) = #as_ref_expr {
                        #setter
                    } else {
                        policy.remove_query(#lit);
                    }
                };
            }

            let ex = emit_value_expr(value, ctx);
            match op {
                SetOp::Set => quote! { policy.set_query(#lit, (#ex).to_string()); },
                SetOp::Push => quote! { policy.push_query(#lit, (#ex).to_string()); },
            }
        }
    }
}

fn emit_fmt_build_string(fmt: &FmtResolved) -> proc_macro2::TokenStream {
    let mut ops: Vec<proc_macro2::TokenStream> = Vec::new();

    for p in &fmt.pieces {
        match p {
            FmtResolvedPiece::Lit(s) => {
                ops.push(quote! { __fmt_s.push_str(#s); });
            }
            FmtResolvedPiece::Var {
                source,
                field,
                optional,
            } => match source {
                FmtVarSource::Cx => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&vars.#field.to_string()); });
                    }
                }
                FmtVarSource::Ep => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&ep.#field.to_string()); });
                    }
                }
                FmtVarSource::Auth => {
                    if *optional {
                        ops.push(quote! {
                           if let ::core::option::Option::Some(__v) = auth.#field.as_ref() {
                                __fmt_s.push_str(__v.expose());
                            }
                        });
                    } else {
                        ops.push(quote! {
                            __fmt_s.push_str(auth.#field.expose());
                        });
                    }
                }
            },
        }
    }

    quote! {
        let mut __fmt_s = ::std::string::String::new();
        #( #ops )*
        __fmt_s
    }
}

fn emit_fmt_build_string_with_ep_optionals(
    fmt: &FmtResolved,
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> proc_macro2::TokenStream {
    let mut ops: Vec<proc_macro2::TokenStream> = Vec::new();
    for p in &fmt.pieces {
        match p {
            FmtResolvedPiece::Lit(s) => {
                ops.push(quote! { __fmt_s.push_str(#s); });
            }
            FmtResolvedPiece::Var {
                source,
                field,
                optional,
            } => match source {
                FmtVarSource::Cx => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&vars.#field.to_string()); });
                    }
                }
                FmtVarSource::Ep => {
                    let is_optional = ep_optionals
                        .and_then(|m| m.get(&field.to_string()).copied())
                        .unwrap_or(*optional);
                    if is_optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&ep.#field.to_string()); });
                    }
                }
                FmtVarSource::Auth => {
                    if *optional {
                        ops.push(quote! {
                           if let ::core::option::Option::Some(__v) = auth.#field.as_ref() {
                                __fmt_s.push_str(__v.expose());
                            }
                        });
                    } else {
                        ops.push(quote! {
                            __fmt_s.push_str(auth.#field.expose());
                        });
                    }
                }
            },
        }
    }
    quote! {
        let mut __fmt_s = ::std::string::String::new();
        #( #ops )*
        __fmt_s
    }
}

fn emit_value_expr(v: &ValueKind, ctx: PolicyEmitCtx) -> TokenStream2 {
    match v {
        ValueKind::LitStr(s) => quote! { #s },
        ValueKind::CxField(f) => match ctx {
            PolicyEmitCtx::ClientBase => quote! { &vars.#f },
            _ => quote! { &vars.#f },
        },
        ValueKind::EpField(f) => quote! { &ep.#f },
        ValueKind::AuthField(f) => quote! { auth.#f.expose() },
        ValueKind::OtherExpr(e) => quote! { (#e) },
        ValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
        }
    }
}

fn emit_prefix_route_apply(
    pieces: &[PrefixPiece],
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    // HostParts joins labels in natural insertion order.
    let mut ops = Vec::new();
    for p in pieces {
        match p {
            PrefixPiece::Static(s) => {
                let lit = LitStr::new(s, Span::call_site());
                ops.push(quote! { route.host_mut().push_label_static(#lit); });
            }
            PrefixPiece::Var {
                wire,
                field,
                optional,
            } => {
                let wire_lit = LitStr::new(wire, Span::call_site());
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.host_mut().push_label(__v.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(ep.#field.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                    });
                }
            }
            PrefixPiece::CxVar { field, optional } => {
                let wire_lit = LitStr::new(&format!("cx.{}", field), Span::call_site());
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                            route.host_mut().push_label(__v.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(vars.#field.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                    });
                }
            }
            PrefixPiece::EpVar { field } => {
                let is_optional = ep_optionals
                    .and_then(|m| m.get(&field.to_string()).copied())
                    .unwrap_or(false);
                let wire_lit = LitStr::new(&format!("ep.{}", field), Span::call_site());
                if is_optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.host_mut().push_label(__v.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(ep.#field.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                    });
                }
            }
            PrefixPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string_with_ep_optionals(fmt, ep_optionals);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard_with_ep_optionals(fmt, ep_optionals);
                    ops.push(quote! {
                        {
                            if { #guard } {
                                let __fmt_s: ::std::string::String = { #build };
                                route.host_mut().push_label(
                                    __fmt_s,
                                    ::concord_core::prelude::HostLabelSource::Mixed
                                );
                            }
                        }
                    });
                } else {
                    ops.push(quote! {
                        {
                            let __fmt_s: ::std::string::String = { #build };
                            route.host_mut().push_label(
                                __fmt_s,
                                ::concord_core::prelude::HostLabelSource::Mixed
                            );
                        }
                    });
                }
            }
        }
    }
    quote! { #( #ops )* }
}

fn emit_path_route_apply(
    pieces: &[PathPiece],
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    let mut ops = Vec::new();
    for p in pieces {
        match p {
            PathPiece::Static(s) => {
                let lit = LitStr::new(s, Span::call_site());
                ops.push(quote! { route.path_mut().push_raw(#lit); });
            }
            PathPiece::Var { field, optional } => {
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.path_mut().push_segment_encoded(&__v.to_string());
                        }
                    });
                } else {
                    ops.push(
                        quote! { route.path_mut().push_segment_encoded(&ep.#field.to_string()); },
                    );
                }
            }
            PathPiece::CxVar { field, optional } => {
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                            route.path_mut().push_segment_encoded(&__v.to_string());
                        }
                    });
                } else {
                    ops.push(
                        quote! { route.path_mut().push_segment_encoded(&vars.#field.to_string()); },
                    );
                }
            }
            PathPiece::EpVar { field } => {
                let is_optional = ep_optionals
                    .and_then(|m| m.get(&field.to_string()).copied())
                    .unwrap_or(false);
                if is_optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.path_mut().push_segment_encoded(&__v.to_string());
                        }
                    });
                } else {
                    ops.push(
                        quote! { route.path_mut().push_segment_encoded(&ep.#field.to_string()); },
                    );
                }
            }
            PathPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string_with_ep_optionals(fmt, ep_optionals);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard_with_ep_optionals(fmt, ep_optionals);
                    ops.push(quote! {
                        {
                            if { #guard } {
                                let __fmt_s: ::std::string::String = { #build };
                                route.path_mut().push_segment_encoded(&__fmt_s);
                            }
                        }
                    });
                } else {
                    ops.push(quote! {
                        {
                            let __fmt_s: ::std::string::String = { #build };
                            route.path_mut().push_segment_encoded(&__fmt_s);
                        }
                    });
                }
            }
        }
    }
    quote! { #( #ops )* }
}

fn find_query_key_for_ep_field<'a>(ep: &'a EndpointIr, field: &Ident) -> Option<&'a KeyResolved> {
    // Take the last matching query op (closest to the endpoint) if multiple exist.
    ep.policy.query.iter().rev().find_map(|op| match op {
        PolicyOp::Bind {
            key,
            kind: PolicyKeyKind::Query,
            field: f,
            ..
        } if f == field => Some(key),
        PolicyOp::Set {
            key,
            value: ValueKind::EpField(f),
            ..
        } if f == field => Some(key),
        _ => None,
    })
}

fn emit_paginate_part(
    ep: &EndpointIr,
    paginate_ty: &Ident,
    cx_ty: &Ident,
    vars_ty: &Ident,
) -> TokenStream2 {
    let name = &ep.name;

    let Some(p) = &ep.paginate else {
        return quote! {
            pub struct #paginate_ty;
            impl ::concord_core::internal::PaginationPart<super::#cx_ty, super::endpoints::#name> for #paginate_ty {
                type Ctrl = ::concord_core::internal::NoController;
                fn controller(
                    _vars: &super::#vars_ty,
                    _ep: &super::endpoints::#name
                ) -> ::core::result::Result<Self::Ctrl, ::concord_core::prelude::ApiClientError> {
                    ::core::result::Result::Ok(::concord_core::internal::NoController)
                }
            }
        };
    };

    let ctrl_ty = &p.ctrl_ty;
    let ctrl_last = ctrl_ty
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();

    let is_cursor = ctrl_last == "CursorPagination";
    let is_offset_limit = ctrl_last == "OffsetLimitPagination";
    let is_paged = ctrl_last == "PagedPagination";

    // Auto key-hints (query key inference from ep field binds).
    let auto_key_assigns = p.assigns.iter().filter_map(|(k, v)| {
        let ValueKind::EpField(f) = v else {
            return None;
        };

        let key_res = find_query_key_for_ep_field(ep, f)?;
        let (_ks, _sp, key_ts) = emit_key_string(key_res, PolicyKeyKind::Query);
        let k_str = k.to_string();

        if is_cursor {
            if k_str == "cursor" {
                return Some(quote! { ctrl.cursor_key = ::std::borrow::Cow::from(#key_ts); });
            }
            if k_str == "per_page" {
                return Some(quote! { ctrl.per_page_key = ::std::borrow::Cow::from(#key_ts); });
            }
        }
        if is_offset_limit {
            if k_str == "offset" {
                return Some(quote! { ctrl.offset_key = ::std::borrow::Cow::from(#key_ts); });
            }
            if k_str == "limit" {
                return Some(quote! { ctrl.limit_key = ::std::borrow::Cow::from(#key_ts); });
            }
        }
        if is_paged {
            if k_str == "page" {
                return Some(quote! { ctrl.page_key = ::std::borrow::Cow::from(#key_ts); });
            }
            if k_str == "per_page" {
                return Some(quote! { ctrl.per_page_key = ::std::borrow::Cow::from(#key_ts); });
            }
        }

        None
    });

    // Typed controller init: assign fields directly (no ControllerBuild/ControllerValue).
    let assigns = p.assigns.iter().map(|(k, v)| {
        let val = match v {
            ValueKind::EpField(f) => quote! { ep.#f.clone() },
            // Prefer Cow for string literals; if the field expects String, user must write `"x".to_string()`.
            ValueKind::LitStr(s) => quote! { ::std::borrow::Cow::from(#s) },
            ValueKind::CxField(f) => quote! { cx.#f.clone() },
            ValueKind::AuthField(_) => quote! {{
                compile_error!(
                    "paginate: auth vars are not accessible in PaginationPart::controller (only cx vars + endpoint are passed)"
                );
                ::core::unreachable!()
            }},
            ValueKind::OtherExpr(e) => quote! { (#e) },
            ValueKind::Fmt(fmt) => {
                let build = emit_fmt_build_string(fmt);
                quote! { { #build } }
            }
        };

        quote! { ctrl.#k = #val; }
    });

    quote! {
        pub struct #paginate_ty;

        impl ::concord_core::internal::PaginationPart<super::#cx_ty, super::endpoints::#name> for #paginate_ty {
            type Ctrl = #ctrl_ty;

            fn controller(
                vars: &super::#vars_ty,
                ep: &super::endpoints::#name
            ) -> ::core::result::Result<Self::Ctrl, ::concord_core::prelude::ApiClientError> {
                #[allow(unused_variables)]
                let cx = vars;
                let mut ctrl: Self::Ctrl = ::core::default::Default::default();
                #( #auto_key_assigns )*
                #( #assigns )*
                ::core::result::Result::Ok(ctrl)
            }
        }
    }
}

fn emit_map_part(ep: &EndpointIr, map_ty: &Ident) -> TokenStream2 {
    let _name = &ep.name;
    let Some(m) = &ep.map else {
        return quote! {};
    };

    let dec_ty = &ep.response.ty;
    let out_ty = &m.out_ty;
    let body = &m.body;

    quote! {
        pub struct #map_ty;

        impl ::concord_core::internal::Transform<#dec_ty> for #map_ty {
            type Out = #out_ty;
            fn map(v: #dec_ty) -> ::core::result::Result<Self::Out, ::concord_core::prelude::FxError> {
                let r: #dec_ty = v;
                let out: #out_ty = (#body);
                ::core::result::Result::Ok(out)
            }
        }
    }
}

fn emit_fmt_require_all_guard_with_ep_optionals(
    fmt: &FmtResolved,
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    let checks = fmt.pieces.iter().filter_map(|p| {
        let FmtResolvedPiece::Var {
            source,
            field,
            optional,
        } = p
        else {
            return None;
        };
        let effective_optional = match source {
            FmtVarSource::Ep => ep_optionals
                .and_then(|m| m.get(&field.to_string()).copied())
                .unwrap_or(*optional),
            _ => *optional,
        };
        if !effective_optional {
            return None;
        }
        match source {
            FmtVarSource::Cx => Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Auth => Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } }),
        }
    });

    quote! {
        let mut __fmt_ok: bool = true;
        #( #checks )*
        __fmt_ok
    }
}
