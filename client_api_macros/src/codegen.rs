use crate::ir::*;
use crate::parse;
use heck::ToSnakeCase;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, ExprLit, Ident, Lit, LitStr, Type};

pub fn emit(ir: Ir) -> syn::Result<TokenStream2> {
    let client_name = ir.client_name.clone();
    // derive internal type names to avoid collisions if multiple api_client! in same module
    let cx_name = format_ident!("{}Cx", client_name);
    let vars_name = format_ident!("{}Vars", client_name);

    let scheme_tokens = scheme_to_http_scheme(&ir.scheme_ident)?;
    let domain_lit = ir.host.clone();

    // Vars fields + constructor
    let vars_fields = ir.vars.iter().map(|v| {
        let n = &v.name;
        let ty = &v.ty;
        let ty = if v.optional {
            quote!(::core::option::Option<#ty>)
        } else {
            quote!(#ty)
        };
        quote! { pub #n: #ty }
    });

    let required_vars = ir
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect::<Vec<_>>();
    let vars_new_args: Vec<TokenStream2> = required_vars
        .iter()
        .map(|v| {
            let n = &v.name;
            let ty = &v.ty;
            quote! { #n: #ty }
        })
        .collect();

    let vars_new_inits = ir.vars.iter().map(|v| {
        let n = &v.name;
        if v.optional {
            if let Some(def) = &v.default {
                quote! { #n: ::core::option::Option::Some(#def) }
            } else {
                quote! { #n: ::core::option::Option::None }
            }
        } else if v.default.is_some() {
            let def = v.default.as_ref().unwrap();
            quote! { #n: #def }
        } else {
            quote! { #n }
        }
    });

    let vars_impl = quote! {
        #[derive(Clone)]
        pub struct #vars_name {
            #(#vars_fields,)*
        }

        impl #vars_name {
            pub fn new(#(#vars_new_args),*) -> Self {
                Self {
                    #(#vars_new_inits,)*
                }
            }
        }
    };

    // ClientContext + base_policy
    let base_policy = emit_base_policy(&ir.client_headers, &ir.vars)?;

    let cx_impl = quote! {
        pub struct #cx_name;

        impl ::client_api_lib::prelude::ClientContext for #cx_name {
            type Vars = #vars_name;
            const SCHEME: ::http::uri::Scheme = #scheme_tokens;
            const DOMAIN: &'static str = #domain_lit;

            fn base_policy(vars: &Self::Vars) -> ::client_api_lib::prelude::Policy {
                #base_policy
            }
        }
    };

    // Client wrapper (no clone)
    let client_ctor_sig = quote! { pub fn new(#(#vars_new_args),*) -> Self };

    // The above uses an iterator in quote; simpler explicit:
    let required_arg_idents: Vec<Ident> = required_vars.iter().map(|v| v.name.clone()).collect();
    let client_ctor_body = quote! {
        let vars = #vars_name::new(#(#required_arg_idents),*);
        Self { inner: ::client_api_lib::prelude::ApiClient::<#cx_name>::new(vars) }
    };

    let client_wrapper = quote! {
        pub struct #client_name {
            inner: ::client_api_lib::prelude::ApiClient<#cx_name>,
        }

        impl #client_name {
            #client_ctor_sig {
                #client_ctor_body
            }

            pub fn with_vars(vars: #vars_name) -> Self {
                Self { inner: ::client_api_lib::prelude::ApiClient::<#cx_name>::new(vars) }
            }

            pub fn execute<E>(
                &self,
                ep: E,
            ) -> impl ::core::future::Future<
                Output = ::core::result::Result<
                    <E::Response as ::client_api_lib::internal::ResponseSpec>::Output,
                    ::client_api_lib::prelude::ApiClientError
                >
            >
            where
                E: ::client_api_lib::prelude::Endpoint<#cx_name>,
            {
                self.inner.execute(ep)
            }
        }
    };

    // __internal module with per-endpoint route/policy/body/map structs
    let mut internal_items = Vec::new();
    for ep in &ir.endpoints {
        internal_items.push(emit_internal_for_endpoint(&ir, ep, &cx_name)?);
    }
    let internal_mod = quote! {
        #[doc(hidden)]
        pub mod __internal {
             use super::*;
            #(#internal_items)*
        }
    };

    // endpoints module
    let endpoints_items: Vec<TokenStream2> = ir
        .endpoints
        .iter()
        .map(|ep| emit_endpoint_module_item(&ir, ep, &cx_name))
        .collect::<syn::Result<Vec<_>>>()?;
    let endpoints_mod = quote! {
        pub mod endpoints {
             use super::*;
            #(#endpoints_items)*
        }
    };

    Ok(quote! {
        #vars_impl
        #cx_impl
        #client_wrapper
        #internal_mod
        #endpoints_mod
    })
}

fn scheme_to_http_scheme(s: &Ident) -> syn::Result<TokenStream2> {
    let v = s.to_string();
    match v.as_str() {
        "https" => Ok(quote!(::http::uri::Scheme::HTTPS)),
        "http" => Ok(quote!(::http::uri::Scheme::HTTP)),
        _ => Err(syn::Error::new_spanned(s, "scheme must be http or https")),
    }
}

fn emit_base_policy(headers: &[parse::HeaderRule], vars: &[IrVar]) -> syn::Result<TokenStream2> {
    if headers.is_empty() {
        return Ok(quote! {
            let mut p = ::client_api_lib::prelude::Policy::new();
            p
        });
    }
    let mut var_types: BTreeMap<String, &Type> = BTreeMap::new();
    for v in vars {
        var_types.insert(v.name.to_string().to_snake_case(), &v.ty);
    }
    let vars_fields = vars_name_set_from_vars(vars);
    let stmts = headers.iter().map(|h| match h {
        parse::HeaderRule::Remove { name } => {
            let n = lower_header_name(name);
            quote! {
                p.headers.remove(::http::header::HeaderName::from_static(#n));
            }
        }
        parse::HeaderRule::Set { name, value } => {
            let n = lower_header_name(name);
            // value is a template string (may contain {var}); v1: only supports "{var}" and plain literal.
            // If plain literal: from_static
            // If contains '{': build String then from_str unwrap (base_policy cannot fail)
            if let Some(var) = single_placeholder(value) {
                // If template is exactly "{var}" and var is HeaderValue => insert directly.
                let key = var.to_snake_case();
                if let Some(ty) = var_types.get(&key)
                    && is_header_value_type(ty)
                {
                    let ident = Ident::new(&key, value.span());
                    return quote! {
                        p.headers.insert(
                            ::http::header::HeaderName::from_static(#n),
                            vars.#ident.clone(),
                        );
                    };
                }
            }

            if value.value().contains('{') {
                let build = emit_template_build_scoped(
                    value,
                    &vars_fields,
                    None,
                    quote!(vars),
                    None,
                ).unwrap_or_else(|e| e.to_compile_error());
                quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        let __hv = ::http::header::HeaderValue::from_str(&__s).unwrap_or_else(|_| {
                            panic!(
                                "invalid header value for client header `{}` from template `{}`",
                                #n,
                                #value
                            )
                        });
                        p.headers.insert(::http::header::HeaderName::from_static(#n), __hv);
                    }
                }
            } else {
                let v = value.value();
                quote! {
                    p.headers.insert(
                        ::http::header::HeaderName::from_static(#n),
                        ::http::header::HeaderValue::from_static(#v),
                    );
                }
            }
        }
    });

    Ok(quote! {
        let mut p = ::client_api_lib::prelude::Policy::new();
        #(#stmts)*
        p
    })
}

fn emit_internal_for_endpoint(
    ir: &Ir,
    ep: &IrEndpoint,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let route_name = format_ident!("Route{}", ep.name);
    let policy_name = format_ident!("Policy{}", ep.name);
    let body_name = format_ident!("Body{}", ep.name);
    let map_name = format_ident!("Map{}", ep.name);

    // codec mapping: allow Json<T> / Text<T> shorthands by mapping codec path ident
    let resp_ty = &ep.resp.ty;

    let route_impl = emit_route_impl(ir, ep, &route_name, cx_name)?;
    let policy_impl = emit_policy_impl(ir, ep, &policy_name, cx_name)?;

    let body_impl = if let Some(body) = &ep.body {
        let body_codec = &body.codec;
        let body_ty = &body.ty;
        let ep_ident = &ep.name;
        // endpoint struct field is always named "body"
        quote! {
            pub struct #body_name;

            impl ::client_api_lib::internal::BodyPart<super::endpoints::#ep_ident> for #body_name {
                type Body = #body_ty;
                type Enc = #body_codec;

                fn body(ep: &super::endpoints::#ep_ident) -> ::core::option::Option<&Self::Body> {
                    ::core::option::Option::Some(&ep.body)
                }
            }
        }
    } else {
        quote! {}
    };

    let response_ty = if let Some(map) = &ep.map {
        let out_ty = &map.out_ty;
        let expr = &map.expr;
        // map function gets `v` as decoded, bind to `r`
        quote! {
            pub struct #map_name;

            impl ::client_api_lib::internal::Transform<#resp_ty> for #map_name {
                type Out = #out_ty;
                fn map(v: #resp_ty) -> ::core::result::Result<Self::Out, ::client_api_lib::prelude::FxError> {
                    let r = v;
                    let out: Self::Out = { #expr };
                    ::core::result::Result::Ok(out)
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #route_impl
        #policy_impl
        #body_impl
        #response_ty
    })
}

fn emit_route_impl(
    ir: &Ir,
    ep: &IrEndpoint,
    route_name: &Ident,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let ep_ident = &ep.name;
    let vars_fields = vars_name_set(ir);
    let ep_fields_route = endpoint_route_fields(ep)?;

    let mut host_pushes = Vec::new();
    for t in &ep.full_host_prefix {
        if t.value().contains('{') {
            let build = emit_template_build_scoped(
                t,
                &vars_fields,
                None,
                quote!(client.vars()),
                None,
            )?;
            host_pushes.push(quote! {
                let mut __s = ::std::string::String::new();
                #build
                route.host.push_label(__s);
            })
        } else {
            let lit = t.value();
            host_pushes.push(quote! { route.host.push_label(#lit); })
        }
    }

    let path_pushes = ep.full_path_prefix.iter().map(|t| {
        if t.value().contains('{') {
            let build = emit_template_build_scoped(
                t,
                &vars_fields,
                None,
                quote!(client.vars()),
                None,
            ).unwrap_or_else(|e| e.to_compile_error());
            quote! {
                let mut __s = ::std::string::String::new();
                #build
                route.path.push_raw(&__s);
            }
        } else {
            let lit = t.value();
            quote! { route.path.push_raw(#lit); }
        }
    });

    let ep_path = &ep.endpoint_path;
    let endpoint_push = if ep_path.value().contains('{') {
        let build = emit_template_build_scoped(
            ep_path,
            &vars_fields,
            Some(&ep_fields_route),
            quote!(client.vars()),
            Some(quote!(ep)),
        )?;
        quote! {
            let mut __s = ::std::string::String::new();
            #build
            route.path.push_raw(&__s);
        }
    } else {
        let lit = ep_path.value();
        quote! { route.path.push_raw(#lit); }
    };

    Ok(quote! {
        pub struct #route_name;

      impl ::client_api_lib::internal::RoutePart<super::#cx_name, super::endpoints::#ep_ident> for #route_name {
            fn apply(
                ep: &super::endpoints::#ep_ident,
                client: &::client_api_lib::prelude::ApiClient<super::#cx_name>,
                route: &mut ::client_api_lib::prelude::RouteParts,
            ) -> ::core::result::Result<(), ::client_api_lib::prelude::BuildError> {
                #(#host_pushes)*
                #(#path_pushes)*
                #endpoint_push
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn emit_policy_impl(
    ir: &Ir,
    ep: &IrEndpoint,
    policy_name: &Ident,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let ep_ident = &ep.name;
    let vars_set = vars_name_set(ir);
    let ep_fields = endpoint_policy_fields(ir, ep)?;
    // headers: support remove and set; set supports templates
    // headers: support remove and set; set supports templates
    let mut header_stmts: Vec<TokenStream2> = Vec::new();
    for h in &ep.headers {
        match h {
            parse::HeaderRule::Remove { name } => {
                let n = lower_header_name(name);
                header_stmts.push(quote! {
                    policy.headers.remove(::http::header::HeaderName::from_static(#n));
                });
            }
            parse::HeaderRule::Set { name, value } => {
                let n = lower_header_name(name);

                if let Some(inner) = single_placeholder_inner(value) {
                    let spec = parse_placeholder_spec(&inner, value.span())?;
                    let key = spec.name_snake.clone();

                    let ep_field = Ident::new(&key, value.span());
                    if ep_fields.contains(&key) {
                        if is_bool_type(&spec.ty) {
                            if spec.optional {
                                header_stmts.push(quote! {
                                    if let ::core::option::Option::Some(v) = ep.#ep_field {
                                        if v {
                                            policy.headers.insert(
                                                ::http::header::HeaderName::from_static(#n),
                                                ::http::header::HeaderValue::from_static("1"),
                                            );
                                        }
                                    }
                                });
                            } else {
                                header_stmts.push(quote! {
                                    if ep.#ep_field {
                                        policy.headers.insert(
                                            ::http::header::HeaderName::from_static(#n),
                                            ::http::header::HeaderValue::from_static("1"),
                                        );
                                    }
                                });
                            }
                            continue;
                        }

                        if spec.optional {
                            header_stmts.push(quote! {
                                if let ::core::option::Option::Some(v) = &ep.#ep_field {
                                    let __s = ::std::string::ToString::to_string(v);
                                    let __hv = ::http::header::HeaderValue::from_str(&__s)
                                       .map_err(|_| ::client_api_lib::prelude::BuildError::InvalidParam(concat!("header:", #n)))?;
                                    policy.headers.insert(::http::header::HeaderName::from_static(#n), __hv);
                                }
                            });
                        } else {
                            header_stmts.push(quote! {
                                {
                                    let __s = ::std::string::ToString::to_string(&ep.#ep_field);
                                    let __hv = ::http::header::HeaderValue::from_str(&__s)
                                       .map_err(|_| ::client_api_lib::prelude::BuildError::InvalidParam(concat!("header:", #n)))?;
                                    policy.headers.insert(::http::header::HeaderName::from_static(#n), __hv);
                                }
                            });
                        }
                        continue;
                    }
                    if vars_set.contains(&key) {
                        let ident = Ident::new(&key, value.span());
                        header_stmts.push(quote! {
                            {
                                let __s = ::std::string::ToString::to_string(&client.vars().#ident);
                                let __hv = ::http::header::HeaderValue::from_str(&__s)
                                    .map_err(|_| ::client_api_lib::prelude::BuildError::InvalidParam(concat!("header:", #n)))?;
                                policy.headers.insert(::http::header::HeaderName::from_static(#n), __hv);
                            }
                        });
                        continue;
                    }
                }

                if value.value().contains('{') {
                    let build = emit_template_build_scoped(
                        value,
                        &vars_set,
                        Some(&ep_fields),
                        quote!(client.vars()),
                        Some(quote!(ep)),
                    )
                        .unwrap_or_else(|e| e.to_compile_error());
                    header_stmts.push(quote! {
                        {
                            let mut __s = ::std::string::String::new();
                            #build
                            let __hv = ::http::header::HeaderValue::from_str(&__s)
                                .map_err(|_| ::client_api_lib::prelude::BuildError::InvalidParam(concat!("header:", #n)))?;
                            policy.headers.insert(::http::header::HeaderName::from_static(#n), __hv);
                        }
                    });
                } else {
                    let v = value.value();
                    header_stmts.push(quote! {
                        policy.headers.insert(
                            ::http::header::HeaderName::from_static(#n),
                            ::http::header::HeaderValue::from_static(#v),
                        );
                    });
                }
            }
        }
    }

    // query: optional => push only if Some, defaulted optional => Some(default), required => always push
    let query_stmts = ep.query.iter().map(|q| {
        let n = q.name.to_string();
        let field = Ident::new(&q.name.to_string().to_snake_case(), q.name.span());
        if q.optional {
            quote! {
                if let ::core::option::Option::Some(v) = &ep.#field {
                    policy.query.push((#n.to_string(), v.to_string()));
                }
            }
        } else {
            quote! {
                policy.query.push((#n.to_string(), ep.#field.to_string()));
            }
        }
    });

    Ok(quote! {
        pub struct #policy_name;

       impl ::client_api_lib::internal::PolicyPart<super::#cx_name, super::endpoints::#ep_ident> for #policy_name {
            fn apply(
                ep: &super::endpoints::#ep_ident,
               client: &::client_api_lib::prelude::ApiClient<super::#cx_name>,
                policy: &mut ::client_api_lib::prelude::Policy,
            ) -> ::core::result::Result<(), ::client_api_lib::prelude::BuildError> {
                #(#header_stmts)*
                #(#query_stmts)*
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn emit_endpoint_module_item(
    ir: &Ir,
    ep: &IrEndpoint,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let ep_ident = &ep.name;

    // fields: body, plus query fields
    let mut fields = Vec::<TokenStream2>::new();

    let path_params = collect_path_params(&ep.endpoint_path)?;
    for p in &path_params {
        let ident = Ident::new(&p.name_snake, ep.endpoint_path.span());
        let ty = &p.ty;
        fields.push(quote! { pub(super) #ident: #ty });
    }

    // 2) Body (fix: no stray '#')
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        fields.push(quote! { pub(super) body: #ty });
    }

    // 3) Query fields (snake_case) to allow `.user_id(...)` for `userId`
    for q in &ep.query {
        let n = Ident::new(&q.name.to_string().to_snake_case(), q.name.span());
        let ty = &q.ty;
        if q.optional {
            fields.push(quote! { pub(super) #n: ::core::option::Option<#ty> });
        } else {
            fields.push(quote! { pub(super) #n: #ty });
        }
    }

    // 4) Header params single-placeholder:
    //    - requis si !optional && !default
    //    - optional si '?'
    //    - default si '=expr'
    let vars_set = vars_name_set(ir);
    #[derive(Clone)]
    struct HeaderParam {
        name_snake: String,
        ty: Type,
        optional: bool,
        default: Option<Expr>,
    }
    let mut header_params: Vec<HeaderParam> = Vec::new();
    let mut header_seen: BTreeSet<String> = BTreeSet::new();
    for h in &ep.headers {
        let parse::HeaderRule::Set { value, .. } = h else {
            continue;
        };
        let Some(inner) = single_placeholder_inner(value) else {
            continue;
        };
        let spec = parse_placeholder_spec(&inner, value.span())?;
        let name_snake = spec.name_snake.clone();
        if vars_set.contains(&name_snake) {
            continue;
        } // fourni par client vars
        // ne pas dupliquer si déjà query/path
        if path_params.iter().any(|p| p.name_snake == name_snake) {
            continue;
        }
        if ep
            .query
            .iter()
            .any(|q| q.name.to_string().to_snake_case() == name_snake)
        {
            continue;
        }
        if !header_seen.insert(name_snake.clone()) {
            continue;
        }
        header_params.push(HeaderParam {
            name_snake,
            ty: spec.ty,
            optional: spec.optional,
            default: spec.default,
        });
    }
    for hp in &header_params {
        let ident = Ident::new(&hp.name_snake, ep_ident.span());
        let ty = &hp.ty;
        if is_bool_type(ty) {
            if hp.optional {
                fields.push(quote! { pub(super) #ident: ::core::option::Option<bool> });
            } else {
                fields.push(quote! { pub(super) #ident: bool });
            }
        } else if hp.optional {
            fields.push(quote! { pub(super) #ident: ::core::option::Option<#ty> });
        } else {
            fields.push(quote! { pub(super) #ident: #ty });
        }
    }

    // Build new() signature:
    // - required body => arg
    // - required query => arg
    let mut new_args = Vec::<TokenStream2>::new();
    let mut init_stmts = Vec::<TokenStream2>::new();
    for p in &path_params {
        let ident = Ident::new(&p.name_snake, ep.endpoint_path.span());
        let ty = &p.ty;
        if is_string_type(ty) {
            new_args.push(quote! { #ident: impl ::core::convert::Into<::std::string::String> });
            init_stmts.push(quote! { #ident: #ident.into() });
        } else {
            new_args.push(quote! { #ident: #ty });
            init_stmts.push(quote! { #ident });
        }
    }
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        new_args.push(quote! { body: #ty });
        init_stmts.push(quote! { body });
    }

    for q in &ep.query {
        let n = Ident::new(&q.name.to_string().to_snake_case(), q.name.span());
        let ty = &q.ty;
        if !q.optional && q.default.is_none() {
            new_args.push(quote! { #n: #ty });
            init_stmts.push(quote! { #n });
        } else if q.optional {
            if let Some(def) = &q.default {
                init_stmts.push(quote! { #n: ::core::option::Option::Some(#def) });
            } else {
                init_stmts.push(quote! { #n: ::core::option::Option::None });
            }
        } else {
            let def = q.default.as_ref().unwrap();
            init_stmts.push(quote! { #n: #def });
        }
    }

    // Headers: requis si !optional && default == None
    for hp in &header_params {
        let ident = Ident::new(&hp.name_snake, ep_ident.span());
        let ty = &hp.ty;
        let is_bool = is_bool_type(ty);
        let required = !hp.optional && hp.default.is_none();

        if required {
            if is_bool {
                new_args.push(quote! { #ident: bool });
                init_stmts.push(quote! { #ident });
            } else if is_string_type(ty) {
                new_args.push(quote! { #ident: impl ::core::convert::Into<::std::string::String> });
                init_stmts.push(quote! { #ident: #ident.into() });
            } else {
                new_args.push(quote! { #ident: #ty });
                init_stmts.push(quote! { #ident });
            }
        } else if is_bool {
            if hp.optional {
                if let Some(def) = &hp.default {
                    let def_ts = coerce_default_expr(&syn::parse_str::<Type>("bool").unwrap(), def);
                    init_stmts.push(quote! { #ident: ::core::option::Option::Some(#def_ts) });
                } else {
                    init_stmts.push(quote! { #ident: ::core::option::Option::None });
                }
            } else if let Some(def) = &hp.default {
                let def_ts = coerce_default_expr(&syn::parse_str::<Type>("bool").unwrap(), def);
                init_stmts.push(quote! { #ident: #def_ts });
            } else {
                // bool non-optional sans default ne devrait pas arriver ici (required)
                init_stmts.push(quote! { #ident: false });
            }
        } else if hp.optional {
            if let Some(def) = &hp.default {
                let def_ts = coerce_default_expr(ty, def);
                init_stmts.push(quote! { #ident: ::core::option::Option::Some(#def_ts) });
            } else {
                init_stmts.push(quote! { #ident: ::core::option::Option::None });
            }
        } else if let Some(def) = &hp.default {
            let def_ts = coerce_default_expr(ty, def);
            init_stmts.push(quote! { #ident: #def_ts });
        } else {
            // non-bool non-optional sans default ne devrait pas arriver ici (required)
        }
    }
    // setters for optional/defaulted query
    let setters = ep.query.iter().filter_map(|q| {
        let n = Ident::new(&q.name.to_string().to_snake_case(), q.name.span());
        let ty = &q.ty;
        if q.optional {
            Some(quote! {
                pub fn #n(mut self, v: #ty) -> Self {
                    self.#n = ::core::option::Option::Some(v);
                    self
                }
            })
        } else if q.default.is_some() {
            Some(quote! {
                pub fn #n(mut self, v: #ty) -> Self {
                    self.#n = v;
                    self
                }
            })
        } else {
            None
        }
    });
    let header_setters = header_params.iter().map(|hp| {
        let n = Ident::new(&hp.name_snake, ep_ident.span());
        let ty = &hp.ty;
        let is_bool = is_bool_type(ty);
        if is_bool {
            if hp.optional {
                quote! {
                    pub fn #n(mut self, v: bool) -> Self {
                        self.#n = ::core::option::Option::Some(v);
                        self
                    }
                }
            } else {
                quote! {
                    pub fn #n(mut self, v: bool) -> Self {
                        self.#n = v;
                        self
                    }
                }
            }
        } else if hp.optional {
            if is_string_type(ty) {
                quote! {
                    pub fn #n(mut self, v: impl ::core::convert::Into<::std::string::String>) -> Self {
                        self.#n = ::core::option::Option::Some(v.into());
                        self
                    }
                }
            } else {
                quote! {
                    pub fn #n(mut self, v: #ty) -> Self {
                        self.#n = ::core::option::Option::Some(v);
                        self
                    }
                }
            }
        } else if is_string_type(ty) {
            quote! {
                pub fn #n(mut self, v: impl ::core::convert::Into<::std::string::String>) -> Self {
                    self.#n = v.into();
                    self
                }
            }
        } else {
            quote! {
                pub fn #n(mut self, v: #ty) -> Self {
                    self.#n = v;
                    self
                }
            }
        }
    });

    let route_name = format_ident!("Route{}", ep_ident);
    let policy_name = format_ident!("Policy{}", ep_ident);
    let body_name = format_ident!("Body{}", ep_ident);
    let map_name = format_ident!("Map{}", ep_ident);

    // response spec
    let (resp_codec, resp_ty) = (&ep.resp.codec, &ep.resp.ty);

    let response_spec_ty = if ep.map.is_some() {
        quote! {
            ::client_api_lib::internal::Mapped<
                ::client_api_lib::internal::Decoded<#resp_codec, #resp_ty>,
                super::__internal::#map_name
            >
        }
    } else {
        quote! { ::client_api_lib::internal::Decoded<#resp_codec, #resp_ty> }
    };

    let body_part_ty = if ep.body.is_some() {
        quote! { super::__internal::#body_name }
    } else {
        quote! { ::client_api_lib::internal::NoBody }
    };
    let method_ident = &ep.method;
    Ok(quote! {
        pub struct #ep_ident {
            #(#fields,)*
        }

        impl #ep_ident {
            pub fn new(#(#new_args),*) -> Self {
                Self { #(#init_stmts,)* }
            }

            #(#setters)*
             #(#header_setters)*
        }

         impl ::client_api_lib::prelude::Endpoint<super::#cx_name> for #ep_ident {
            const METHOD: ::http::Method = ::http::Method::#method_ident;
            type Route = super::__internal::#route_name;
            type Policy = super::__internal::#policy_name;
            type Body = #body_part_ty;
            type Response = #response_spec_ty;
        }
    })
}

// ----- helpers -----

fn lower_header_name(n: &LitStr) -> LitStr {
    let s = n.value().to_ascii_lowercase();
    LitStr::new(&s, n.span())
}

fn vars_name_set_from_vars(vars: &[IrVar]) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for v in vars {
        s.insert(v.name.to_string().to_snake_case());
    }
    s
}

fn vars_name_set(ir: &Ir) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for v in &ir.vars {
        s.insert(v.name.to_string().to_snake_case());
    }
    s
}

fn single_placeholder(tpl: &LitStr) -> Option<String> {
    let s = tpl.value().trim().to_string();
    if s.starts_with('{') && s.ends_with('}') && s.matches('{').count() == 1 {
        Some(s.trim_matches('{').trim_matches('}').trim().to_string())
    } else {
        None
    }
}

fn is_header_value_type(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident == "HeaderValue")
            .unwrap_or(false),
        _ => false,
    }
}

fn endpoint_route_fields(ep: &IrEndpoint) -> syn::Result<BTreeSet<String>> {
    let mut ep_fields: BTreeSet<String> = BTreeSet::new();
    for p in collect_path_params(&ep.endpoint_path)? {
        ep_fields.insert(p.name_snake);
    }
    Ok(ep_fields)
}
fn endpoint_policy_fields(ir: &Ir, ep: &IrEndpoint) -> syn::Result<BTreeSet<String>> {
    let vars_fields = vars_name_set(ir);
    let mut ep_fields: BTreeSet<String> = BTreeSet::new();
    for p in collect_path_params(&ep.endpoint_path)? {
        ep_fields.insert(p.name_snake);
    }
    for q in &ep.query {
        ep_fields.insert(q.name.to_string().to_snake_case());
    }
    for h in &ep.headers {
        if let parse::HeaderRule::Set { value, .. } = h
            && let Some(inner) = single_placeholder_inner(value)
        {
            let k = placeholder_name_snake(&inner);
            if !vars_fields.contains(&k) {
                ep_fields.insert(k);
            }
        }
    }
    Ok(ep_fields)
}

#[derive(Clone)]
struct PlaceholderSpec {
    name_snake: String,
    optional: bool,
    ty: Type,
    default: Option<Expr>,
}

fn placeholder_name_snake(inner: &str) -> String {
    let left = inner.split('=').next().unwrap_or(inner).trim();
    let name = left.split(':').next().unwrap_or(left).trim();
    name.trim_end_matches('?').trim().to_snake_case()
}

fn parse_placeholder_spec(inner: &str, span: proc_macro2::Span) -> syn::Result<PlaceholderSpec> {
    let inner = inner.trim();
    let (lhs, def_part) = match inner.split_once('=') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (inner, None),
    };

    let default = if let Some(def) = def_part {
        Some(syn::parse_str::<Expr>(def).map_err(|e| syn::Error::new(span, e.to_string()))?)
    } else {
        None
    };

    let (name_part, ty_part) = match lhs.split_once(':') {
        Some((n, t)) => (n.trim(), Some(t.trim())),
        None => (lhs.trim(), None),
    };

    let optional = name_part.ends_with('?');
    let name = name_part.trim_end_matches('?').trim();
    if name.is_empty() {
        return Err(syn::Error::new(span, "empty placeholder name"));
    }

    let ty_str = ty_part.unwrap_or("String");
    let ty = syn::parse_str::<Type>(ty_str).map_err(|e| syn::Error::new(span, e.to_string()))?;

    Ok(PlaceholderSpec {
        name_snake: name.to_snake_case(),
        optional,
        ty,
        default,
    })
}

fn is_bool_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().map(|s| s.ident == "bool").unwrap_or(false))
}

fn is_string_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().map(|s| s.ident == "String").unwrap_or(false))
}

fn coerce_default_expr(ty: &Type, expr: &Expr) -> TokenStream2 {
    if is_string_type(ty)
        && matches!(
            expr,
            Expr::Lit(ExprLit {
                lit: Lit::Str(_),
                ..
            })
        )
    {
        return quote! { (#expr).to_string() };
    }
    quote! { #expr }
}

#[derive(Clone)]
struct PathParam {
    name_snake: String,
    ty: Type,
}

fn collect_path_params(tpl: &LitStr) -> syn::Result<Vec<PathParam>> {
    let s = tpl.value();
    let mut out: Vec<PathParam> = Vec::new();
    let mut i = 0usize;
    while i < s.len() {
        let rest = &s[i..];
        let Some(open) = rest.find('{') else {
            break;
        };
        let after_open = i + open + 1;
        let rest2 = &s[after_open..];
        let Some(close) = rest2.find('}') else {
            break;
        };
        let inner = rest2[..close].trim();
        if !inner.is_empty() {
            let spec = parse_placeholder_spec(inner, tpl.span())?;
            let name_snake = spec.name_snake;
            let ty = spec.ty;
            // dédoublonnage simple : si déjà présent, vérifier type identique
            if let Some(prev) = out.iter().find(|p| p.name_snake == name_snake) {
                // même nom => même type attendu
                // (comparaison stringifiée, suffisant ici)
                let prev_ty = &prev.ty;
                if quote!(#ty).to_string() != quote!(#prev_ty).to_string() {
                    return Err(syn::Error::new(
                        tpl.span(),
                        format!("path param `{}` has conflicting types", name_snake),
                    ));
                }
            } else {
                out.push(PathParam { name_snake, ty });
            }
        }
        i = after_open + close + 1;
    }
    Ok(out)
}

fn single_placeholder_inner(tpl: &LitStr) -> Option<String> {
    let v = tpl.value().trim().to_string();
    if v.starts_with('{') && v.ends_with('}') && v.matches('{').count() == 1 {
        Some(
            v.trim_start_matches('{')
                .trim_end_matches('}')
                .trim()
                .to_string(),
        )
    } else {
        None
    }
}

/// Template engine v1:
/// - supports literal text + placeholders like {name} or {name:Type=default}
/// - lookup order: endpoint fields first (if provided), then vars
/// - unknown keys => compile error with available keys listed
fn emit_template_build_scoped(
    tpl: &LitStr,
    vars_fields: &BTreeSet<String>,
    ep_fields: Option<&BTreeSet<String>>,
    vars_expr: TokenStream2,
    ep_expr: Option<TokenStream2>,
) -> syn::Result<TokenStream2> {
    let s = tpl.value();
    let mut parts = Vec::<TemplatePart>::new();

    let mut i = 0usize;
    while i < s.len() {
        let rest = &s[i..];
        if let Some(open) = rest.find('{') {
            if open > 0 {
                parts.push(TemplatePart::Lit(rest[..open].to_string()));
            }
            let after_open = i + open + 1;
            let rest2 = &s[after_open..];
            if let Some(close) = rest2.find('}') {
                let inner = rest2[..close].trim();
                let key = placeholder_name_snake(inner);
                parts.push(TemplatePart::Var(key));
                i = after_open + close + 1;
            } else {
                // no closing brace; treat remaining as literal
                parts.push(TemplatePart::Lit(rest[open..].to_string()));
                break;
            }
        } else {
            parts.push(TemplatePart::Lit(rest.to_string()));
            break;
        }
    }

    let mut stmts = Vec::<TokenStream2>::new();
    for p in parts {
        match p {
            TemplatePart::Lit(l) => {
                stmts.push(quote! { __s.push_str(#l); });
            }
            TemplatePart::Var(name) => {
                let ident = Ident::new(&name, tpl.span());
                if let Some(ep_set) = ep_fields
                    && ep_set.contains(&name)
                {
                    let Some(ep) = ep_expr.clone() else {
                        return Err(syn::Error::new_spanned(tpl, format!(
                            "template references endpoint field `{}` but no endpoint scope was provided",
                            name
                        )));
                    };
                    stmts.push(quote! {
     use ::core::fmt::Write as _;
     let _ = write!(__s, "{}", #ep.#ident);
   });
                } else if vars_fields.contains(&name) {
                    stmts.push(quote! {
     use ::core::fmt::Write as _;
     let _ = write!(__s, "{}", #vars_expr.#ident);
   });
                } else {
                    let mut ep_list = String::new();
                    if let Some(ep_set) = ep_fields {
                        ep_list = ep_set.iter().cloned().collect::<Vec<_>>().join(", ");
                    }
                    let vars_list = vars_fields.iter().cloned().collect::<Vec<_>>().join(", ");
                    let msg = if ep_fields.is_some() {
                        format!(
                            "unknown template key: {{{}}}. available endpoint fields: [{}]. available client vars: [{}]",
                            name, ep_list, vars_list
                        )
                    } else {
                        format!(
                            "unknown template key: {{{}}}. available client vars: [{}]",
                            name, vars_list
                        )
                    };
                    return Err(syn::Error::new_spanned(tpl, msg));
                }
            }
        }
    }

    Ok(quote! { #(#stmts)* })
}

enum TemplatePart {
    Lit(String),
    Var(String),
}
