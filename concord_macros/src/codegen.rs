use crate::ast::SetOp;
use crate::emit_helpers;
use crate::sema::*;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

pub fn emit(ir: Ir) -> TokenStream2 {
    let mod_name = ir.mod_name.clone();
    let scheme = emit_scheme(ir.scheme);
    let domain = ir.domain.clone();

    let vars_struct = emit_client_vars(&ir.client_vars);
    let auth_vars_struct = emit_client_auth_vars(&ir.client_auth_vars);
    let cx_struct = emit_client_context(&scheme, &domain, &ir.client_policy);
    let client_wrapper = emit_client_wrapper(&ir);
    let internal_mod = emit_internal(&ir);
    let endpoints_mod = emit_endpoints(&ir);

    quote! {
        mod #mod_name {
            use super::*;

            #vars_struct
            #auth_vars_struct
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

fn emit_client_vars(vars: &[VarInfo]) -> TokenStream2 {
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
        pub struct Vars {
            #( #fields, )*
        }

        impl Vars {
            #[inline]
            pub fn new( #( #new_args ),* ) -> Self {
                Self { #( #init_fields, )* }
            }

            #( #setters )*
        }
    }
}

fn emit_client_context(
    scheme: &TokenStream2,
    domain: &LitStr,
    policy: &PolicyBlocksResolved,
) -> TokenStream2 {
    let base_policy = emit_policy_fn_base(policy);

    quote! {
        #[derive(Clone)]
        pub struct Cx;

        impl ::concord_core::prelude::ClientContext for Cx {
            type Vars = Vars;
            type AuthVars = AuthVars;
            const SCHEME: ::http::uri::Scheme = #scheme;
            const DOMAIN: &'static str = #domain;

            fn base_policy(
                vars: &Self::Vars,
                auth: &Self::AuthVars
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

    quote! {
        let mut policy = ::concord_core::prelude::Policy::new();
        #[allow(unused_variables)]
        let cx = vars;
        #[allow(unused_variables)]
        let auth = auth;
        #( #ops )*
        ::core::result::Result::Ok(policy)
    }
}

fn emit_client_auth_vars(vars: &[VarInfo]) -> TokenStream2 {
    use quote::quote;
    let fields = vars.iter().map(|v| {
        let name = &v.rust;
        if v.optional {
            quote! { pub #name: ::std::sync::Arc<::std::sync::RwLock<::core::option::Option<::concord_core::prelude::SecretString>>> }
        } else {
            quote! { pub #name: ::std::sync::Arc<::std::sync::RwLock<::concord_core::prelude::SecretString>> }
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
            quote! { #name: ::std::sync::Arc::new(::std::sync::RwLock::new(::concord_core::prelude::SecretString::from(#name))) }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #name: ::std::sync::Arc::new(::std::sync::RwLock::new(::core::option::Option::Some(::concord_core::prelude::SecretString::from(#d)))) }
            } else {
                quote! { #name: ::std::sync::Arc::new(::std::sync::RwLock::new(::core::option::Option::None)) }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #name: ::std::sync::Arc::new(::std::sync::RwLock::new(::concord_core::prelude::SecretString::from(#d))) }
        }
    });
    // Default if no required args
    let can_default = required.is_empty();
    let default_impl = if can_default {
        quote! {
            impl ::core::default::Default for AuthVars {
                fn default() -> Self { Self::new() }
            }
        }
    } else {
        quote! {}
    };
    // empty auth vars => unit struct
    if vars.is_empty() {
        return quote! {
            #[derive(Clone, Default)]
            pub struct AuthVars;
            impl AuthVars {
                #[inline]
                pub fn new() -> Self { Self }
            }
        };
    }
    let new_sig = if required.is_empty() {
        quote! { pub fn new() -> Self }
    } else {
        quote! { pub fn new( #( #new_args ),* ) -> Self }
    };
    quote! {
        #[derive(Clone)]
        pub struct AuthVars {
            #( #fields, )*
        }
        impl AuthVars {
            #[inline]
            #new_sig {
                Self { #( #init_fields, )* }
            }
        }
        #default_impl
    }
}

fn emit_internal(ir: &Ir) -> TokenStream2 {
    let wrappers = quote! {
        pub struct __LayerPrefix<P>(::core::marker::PhantomData<P>);
        pub struct __LayerEndpoint<P>(::core::marker::PhantomData<P>);

        impl<P> __LayerPrefix<P> { const LAYER: ::concord_core::prelude::PolicyLayer = ::concord_core::prelude::PolicyLayer::PrefixPath; }
        impl<P> __LayerEndpoint<P> { const LAYER: ::concord_core::prelude::PolicyLayer = ::concord_core::prelude::PolicyLayer::Endpoint; }

        impl<Cx, E, P> ::concord_core::internal::PolicyPart<Cx, E> for __LayerPrefix<P>
        where
            Cx: ::concord_core::prelude::ClientContext,
            P: ::concord_core::internal::PolicyPart<Cx, E>,
        {
           fn apply(ep: &E, vars: &Cx::Vars, auth: &Cx::AuthVars, policy: &mut ::concord_core::prelude::Policy)
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                let prev = policy.layer();
                policy.set_layer(Self::LAYER);
                let r = P::apply(ep, vars, auth, policy);
                policy.set_layer(prev);
                r
            }
        }

        impl<Cx, E, P> ::concord_core::internal::PolicyPart<Cx, E> for __LayerEndpoint<P>
        where
            Cx: ::concord_core::prelude::ClientContext,
            P: ::concord_core::internal::PolicyPart<Cx, E>,
        {
            fn apply(ep: &E, vars: &Cx::Vars, auth: &Cx::AuthVars, policy: &mut ::concord_core::prelude::Policy)
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                let prev = policy.layer();
                policy.set_layer(Self::LAYER);
                let r = P::apply(ep, vars, auth, policy);
                policy.set_layer(prev);
                r
            }
        }
    };

    let layer_route_policy = ir.layers.iter().map(|l| emit_layer_parts(ir, l));
    let endpoint_parts = ir.endpoints.iter().map(emit_endpoint_parts);

    quote! {
        mod __internal {
            use super::*;
            #wrappers
            #( #layer_route_policy )*
            #( #endpoint_parts )*
        }
    }
}

fn emit_layer_parts(ir: &Ir, layer: &LayerIr) -> TokenStream2 {
    let id = layer.id;
    let route_ty = emit_helpers::ident(&format!("__Route_L{id}"), Span::call_site());
    let policy_ty = emit_helpers::ident(&format!("__Policy_L{id}"), Span::call_site());

    let route_apply = match layer.kind {
        crate::ast::LayerKind::Prefix => emit_prefix_route_apply(&layer.prefix_pieces),
        crate::ast::LayerKind::Path => emit_path_route_apply(&layer.path_pieces),
    };

    let policy_apply = emit_policy_apply_fn(&layer.policy, PolicyEmitCtx::Layer);

    let mut route_impls: Vec<TokenStream2> = Vec::new();
    let mut policy_impls: Vec<TokenStream2> = Vec::new();

    for ep in &ir.endpoints {
        if !ep.ancestry.contains(&id) {
            continue;
        }
        let ep_name = &ep.name;

        route_impls.push(quote! {
            impl ::concord_core::internal::RoutePart<super::Cx, super::endpoints::#ep_name> for #route_ty {
                fn apply(
                    ep: &super::endpoints::#ep_name,
                    vars: &super::Vars,
                    auth: &super::AuthVars,
                    route: &mut ::concord_core::prelude::RouteParts
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                    let _ = auth;
                    #route_apply
                    ::core::result::Result::Ok(())
                }
            }
        });

        policy_impls.push(quote! {
            impl ::concord_core::internal::PolicyPart<super::Cx, super::endpoints::#ep_name> for #policy_ty {
                fn apply(
                    ep: &super::endpoints::#ep_name,
                    vars: &super::Vars,
                    auth: &super::AuthVars,
                    policy: &mut ::concord_core::prelude::Policy
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                    #policy_apply
                    ::core::result::Result::Ok(())
                }
            }
        });
    }

    quote! {
        pub struct #route_ty;
        pub struct #policy_ty;
        #( #route_impls )*
        #( #policy_impls )*
    }
}

fn emit_endpoint_parts(ep: &EndpointIr) -> TokenStream2 {
    let name = &ep.name;
    let route_ty = emit_helpers::ident(&format!("__Route_{name}"), Span::call_site());
    let policy_ty = emit_helpers::ident(&format!("__Policy_{name}"), Span::call_site());

    let route_apply = emit_path_route_apply(&ep.route_pieces);
    let policy_apply = emit_policy_apply_fn(&ep.policy, PolicyEmitCtx::Endpoint);

    let paginate_ty = emit_helpers::ident(&format!("__Pag_{name}"), Span::call_site());
    let paginate_impl = emit_paginate_part(ep, &paginate_ty);

    let map_ty = emit_helpers::ident(&format!("__Map_{name}"), Span::call_site());
    let map_impl = emit_map_part(ep, &map_ty);

    quote! {
        pub struct #route_ty;
        impl<Cx> ::concord_core::internal::RoutePart<Cx, super::endpoints::#name> for #route_ty
        where
            Cx: ::concord_core::prelude::ClientContext,
        {
            fn apply(
                    ep: &super::endpoints::#name,
                    vars: &Cx::Vars,
                    auth: &Cx::AuthVars,
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
        impl<Cx> ::concord_core::internal::PolicyPart<Cx, super::endpoints::#name> for #policy_ty
        where
            Cx: ::concord_core::prelude::ClientContext,
        {
            fn apply(ep: &super::endpoints::#name, vars: &Cx::Vars, auth: &Cx::AuthVars, policy: &mut ::concord_core::prelude::Policy)
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                #policy_apply
                ::core::result::Result::Ok(())
            }
        }

        #paginate_impl
        #map_impl
    }
}

fn emit_endpoints(ir: &Ir) -> TokenStream2 {
    let endpoint_defs = ir.endpoints.iter().map(|ep| emit_endpoint_def(ir, ep));
    quote! {
        pub mod endpoints {
            use super::*;
            #( #endpoint_defs )*
        }
    }
}

fn emit_client_wrapper(ir: &Ir) -> TokenStream2 {
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

    let auth_setters = ir.client_auth_vars.iter().map(|v| {
        let f = &v.rust;
        let set_name = emit_helpers::ident(&format!("set_{f}"), f.span());
        if v.optional {
            let clear_name = emit_helpers::ident(&format!("clear_{f}"), f.span());
            quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
                *self.inner.auth_vars().#f.write().unwrap() = ::core::option::Option::Some(v.into());
                self
            }
            #[inline]
            pub fn #clear_name(&self) -> &Self {
                *self.inner.auth_vars().#f.write().unwrap() = ::core::option::Option::None;
                self
            }
        }
        } else {
            quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
                *self.inner.auth_vars().#f.write().unwrap() = v.into();
                self
            }
        }
        }
    });

    quote! {
        #[derive(Clone)]
        pub struct #client_ty<T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
            inner: ::concord_core::prelude::ApiClient<Cx, T>,
        }
        impl #client_ty<::concord_core::prelude::ReqwestTransport> {
            #[inline]
            pub fn new( #( #ctor_args ),* ) -> Self {
                let vars = Vars::new( #( #new_pass ),* );
                let auth_vars = AuthVars::new( #( #new_auth_pass ),* );
               Self { inner: ::concord_core::prelude::ApiClient::<Cx, ::concord_core::prelude::ReqwestTransport>::new(vars, auth_vars) }
            }


            #[inline]
            pub fn new_with_transport<T2: ::concord_core::prelude::Transport>(
                #( #ctor_args, )*
                transport: T2
            ) -> #client_ty<T2> {
                let vars = Vars::new( #( #new_pass ),* );
                let auth_vars = AuthVars::new( #( #new_auth_pass ),* );
                #client_ty { inner: ::concord_core::prelude::ApiClient::<Cx, T2>::with_transport(vars, auth_vars, transport) }
            }


        }

        impl<T: ::concord_core::prelude::Transport> #client_ty<T> {
            #( #var_setters )*
            #( #auth_setters )*

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
            pub fn request<E>(&self, ep: E) -> ::concord_core::prelude::PendingRequest<'_, Cx, E, T>
            where
                E: ::concord_core::prelude::Endpoint<Cx>,
            {
                self.inner.request(ep)
            }
        }
    }
}

fn emit_endpoint_def(ir: &Ir, ep: &EndpointIr) -> TokenStream2 {
    let name = &ep.name;
    let method = &ep.method;

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

    // route chain ordering:
    // - prefix route parts applied inner->outer for correct HostParts reversal semantics
    // - path route parts applied outer->inner
    let mut prefix_parts: Vec<TokenStream2> = Vec::new();
    let mut path_parts: Vec<TokenStream2> = Vec::new();
    for &lid in &ep.ancestry {
        let l = &ir.layers[lid];
        let r_ident = emit_helpers::ident(&format!("__Route_L{lid}"), Span::call_site());
        let r_path = quote! { super::__internal::#r_ident };
        match l.kind {
            crate::ast::LayerKind::Prefix => prefix_parts.push(r_path),
            crate::ast::LayerKind::Path => path_parts.push(r_path),
        }
    }
    // prefix should be inner->outer => reverse
    prefix_parts.reverse();
    let endpoint_route_part = {
        let r_ident = emit_helpers::ident(&format!("__Route_{name}"), Span::call_site());
        quote! { super::__internal::#r_ident }
    };

    let mut route_chain = Vec::new();
    route_chain.extend(prefix_parts);
    route_chain.extend(path_parts);
    route_chain.push(endpoint_route_part);

    let route_ty =
        emit_helpers::nested_chain(&route_chain, quote! { ::concord_core::internal::NoRoute });

    // policy chain: strict nesting order outer->inner, then endpoint
    let mut policy_chain = Vec::new();
    for &lid in &ep.ancestry {
        let p_ident = emit_helpers::ident(&format!("__Policy_L{lid}"), Span::call_site());
        policy_chain.push(quote! {
            super::__internal::__LayerPrefix<super::__internal::#p_ident>
        });
    }
    let ep_pol_ident = emit_helpers::ident(&format!("__Policy_{name}"), Span::call_site());
    policy_chain.push(quote! {
        super::__internal::__LayerEndpoint<super::__internal::#ep_pol_ident>
    });
    let policy_ty =
        emit_helpers::nested_chain(&policy_chain, quote! { ::concord_core::internal::NoPolicy });

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

        impl ::concord_core::prelude::Endpoint<super::Cx> for #name {
            const METHOD: ::http::Method = ::http::Method::#method;
            type Route = #route_ty;
            type Policy = #policy_ty;
            type Pagination = #pagination_ty;
            type Body = #body_ty;
            type Response = #response_ty;
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
        let auth = auth;
        #[allow(unused_variables)]
        let ep = ep;
    });
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Header, ctx));
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Query, ctx));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, ctx);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    quote! { #( #ops )* }
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
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #ks)))?;
                        policy.insert_header(#name, __hv);
                    } else {
                        policy.remove_header(#name);
                    }
                }
            } else {
                quote! {
                    let __hv = ::http::HeaderValue::from_str(&#value_expr.to_string())
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #ks)))?;
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
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(#err))?;
                    policy.insert_header(#name, __hv);
                };

                if fmt.require_all {
                    let checks = fmt.pieces.iter().filter_map(|p| {
                        let FmtResolvedPiece::Var { source, field, optional: true } = p else { return None; };
                        match source {
                            FmtVarSource::Cx => Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } }),
                            FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
                            FmtVarSource::Auth => Some(quote! { if auth.#field.read().unwrap().is_none() { __fmt_ok = false; } }),
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
                            let __g = auth.#fld.read().unwrap();
                            if let ::core::option::Option::Some(__v) = __g.as_ref() {
                                let __hv = ::http::HeaderValue::from_str(__v.expose())
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(#err))?;
                                policy.insert_header(#name, __hv);
                            } else {
                                policy.remove_header(#name);
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let __g = auth.#fld.read().unwrap();
                            let __hv = ::http::HeaderValue::from_str(__g.expose())
                                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(#err))?;
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
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #ks)))?;
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
                        let FmtResolvedPiece::Var { source, field, optional: true } = p else { return None; };
                        match source {
                            FmtVarSource::Cx => Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } }),
                            FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
                            FmtVarSource::Auth => Some(quote! { if auth.#field.read().unwrap().is_none() { __fmt_ok = false; } }),
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
                            let __g = auth.#fld.read().unwrap();
                            if let ::core::option::Option::Some(__v) = __g.as_ref() {
                                let __s: ::std::string::String = __v.expose().to_owned();
                                #setter
                            } else {
                                policy.remove_query(#lit);
                            }
                        }
                    }
                } else {
                    quote! {
                    {
                        let __g = auth.#fld.read().unwrap();
                        let __s: ::std::string::String = __g.expose().to_owned();
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
                            let __g = auth.#field.read().unwrap();
                            if let ::core::option::Option::Some(__v) = __g.as_ref() {
                                __fmt_s.push_str(__v.expose());
                            }
                        });
                    } else {
                        ops.push(quote! {
                            let __g = auth.#field.read().unwrap();
                            __fmt_s.push_str(__g.expose());
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
        ValueKind::AuthField(f) => quote! { auth.#f.read().unwrap().expose().to_owned() },
        ValueKind::OtherExpr(e) => quote! { (#e) },
        ValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
        }
    }
}

fn emit_prefix_route_apply(pieces: &[PrefixPiece]) -> TokenStream2 {
    // HostParts joins labels in reverse insertion order.
    // To preserve the textual order `a.b.c` => `a.b.c.domain`,
    // insert labels in reverse textual order: `c`, `b`, `a`.
    let mut ops = Vec::new();
    for p in pieces.iter().rev() {
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
            PrefixPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string(fmt);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard(fmt);
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

fn emit_path_route_apply(pieces: &[PathPiece]) -> TokenStream2 {
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
            PathPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string(fmt);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard(fmt);
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
    // Take the last bind (closest to the endpoint) if multiple exist.
    ep.policy.query.iter().rev().find_map(|op| match op {
        PolicyOp::Bind {
            key,
            kind: PolicyKeyKind::Query,
            field: f,
            ..
        } if f == field => Some(key),
        _ => None,
    })
}

fn emit_paginate_part(ep: &EndpointIr, paginate_ty: &Ident) -> TokenStream2 {
    let name = &ep.name;
    let Some(p) = &ep.paginate else {
        return quote! {};
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

    // Typed controller init: assign fields directly (no ControllerBuild / ControllerValue / hints).
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

        impl ::concord_core::internal::PaginationPart<super::Cx, super::endpoints::#name> for #paginate_ty {
            type Ctrl = #ctrl_ty;

            fn controller(
                vars: &super::Vars,
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

fn emit_fmt_require_all_guard(fmt: &FmtResolved) -> TokenStream2 {
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
            FmtVarSource::Cx => Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Auth => {
                Some(quote! { if auth.#field.read().unwrap().is_none() { __fmt_ok = false; } })
            }
        }
    });

    quote! {
        let mut __fmt_ok: bool = true;
        #( #checks )*
        __fmt_ok
    }
}
