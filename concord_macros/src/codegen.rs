use crate::ir::*;
use crate::parse;
use crate::parse::RefExpr;
use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use syn::{Expr, ExprLit, Ident, Lit, LitStr, Type, spanned::Spanned as _};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum SharedRouteMode {
    Host,
    Path,
}

pub fn emit(ir: Ir) -> syn::Result<TokenStream2> {
    let client_name = ir.client_name.clone();
    let cx_name = format_ident!("{}Cx", client_name);
    let vars_name = format_ident!("{}Vars", client_name);
    let mod_ident = format_ident!("{}", client_name.to_string().to_snake_case());
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
        } else if let Some(def) = &v.default {
            quote! { #n: #def }
        } else {
            quote! { #n }
        }
    });
    let vars_impl = quote! {
        #[derive(Clone, Debug)]
        pub struct #vars_name {
            #(#vars_fields,)*
        }
        impl #vars_name {
            pub fn new(#(#vars_new_args),*) -> Self {
                Self { #(#vars_new_inits,)* }
            }
        }
    };

    // ClientContext + base_policy
    let base_policy = emit_base_policy(&ir.client_headers, &ir.vars, ir.client_timeout.as_ref())?;
    let cx_impl = quote! {
        #[derive(Clone)]
        pub struct #cx_name;
        impl ::concord_core::prelude::ClientContext for #cx_name {
            type Vars = #vars_name;
            const SCHEME: ::http::uri::Scheme = #scheme_tokens;
            const DOMAIN: &'static str = #domain_lit;
            fn base_policy(vars: &Self::Vars) -> Result<::concord_core::prelude::Policy, ::concord_core::prelude::ApiClientError> {
                #base_policy
            }
        }
    };

    // Client wrapper
    let client_ctor_sig = quote! { pub fn new(#(#vars_new_args),*) -> Self };
    let required_arg_idents: Vec<Ident> = required_vars.iter().map(|v| v.name.clone()).collect();
    let client_ctor_body = quote! {
        let vars = #vars_name::new(#(#required_arg_idents),*);
        Self { inner: ::concord_core::prelude::ApiClient::<#cx_name>::new(vars) }
    };
    let client_wrapper = quote! {
        #[derive(Clone)]
        pub struct #client_name {
            inner: ::concord_core::prelude::ApiClient<#cx_name>,
        }
        impl #client_name {
             #client_ctor_sig { #client_ctor_body }
            pub fn with_reqwest_client(#(#vars_new_args,)* client: ::reqwest::Client) -> Self {
                Self::new_with_reqwest_client(#(#required_arg_idents,)* client)
            }
             pub fn new_with_reqwest_client(#(#vars_new_args,)* client: ::reqwest::Client) -> Self {
                 let vars = #vars_name::new(#(#required_arg_idents),*);
                 Self { inner: ::concord_core::prelude::ApiClient::<#cx_name>::with_reqwest_client(vars, client) }
             }
            pub fn with_transport(#(#vars_new_args,)* transport: impl ::concord_core::prelude::Transport) -> Self {
                Self::new_with_transport(#(#required_arg_idents,)* transport)
            }
             pub fn new_with_transport(#(#vars_new_args,)* transport: impl ::concord_core::prelude::Transport) -> Self {
                 let vars = #vars_name::new(#(#required_arg_idents),*);
                 Self { inner: ::concord_core::prelude::ApiClient::<#cx_name>::with_transport(vars, transport) }
             }
            pub fn with_vars(vars: #vars_name) -> Self {
                Self { inner: ::concord_core::prelude::ApiClient::<#cx_name>::new(vars) }
            }

             pub fn vars(&self) -> &#vars_name { self.inner.vars() }

             pub fn api(&self) -> &::concord_core::prelude::ApiClient<#cx_name> { &self.inner }
             pub fn api_mut(&mut self) -> &mut ::concord_core::prelude::ApiClient<#cx_name> { &mut self.inner }

             pub fn pagination_caps(&self) -> ::concord_core::prelude::Caps { self.inner.pagination_caps() }
             pub fn set_pagination_caps(&mut self, caps: ::concord_core::prelude::Caps) { self.inner.set_pagination_caps(caps); }
             pub fn with_pagination_caps(mut self, caps: ::concord_core::prelude::Caps) -> Self { self.inner.set_pagination_caps(caps); self }

             pub fn execute_decoded_ref<E>(
                 &self,
                 ep: &E,
                 meta: ::concord_core::prelude::RequestMeta
             ) -> impl ::core::future::Future<
                 Output = ::core::result::Result<
                     ::concord_core::prelude::DecodedResponse<<E::Response as ::concord_core::internal::ResponseSpec>::Output>,
                     ::concord_core::prelude::ApiClientError
                 >
             >
             where E: ::concord_core::prelude::Endpoint<#cx_name> {
                 self.inner.execute_decoded_ref(ep, meta)
             }

             pub fn execute_decoded_ref_with<E, F>(
                 &self,
                 ep: &E,
                 meta: ::concord_core::prelude::RequestMeta,
                 patch: F,
             ) -> impl ::core::future::Future<
                 Output = ::core::result::Result<
                     ::concord_core::prelude::DecodedResponse<<E::Response as ::concord_core::internal::ResponseSpec>::Output>,
                     ::concord_core::prelude::ApiClientError
                 >
             >
             where
                 E: ::concord_core::prelude::Endpoint<#cx_name>,
                 F: for<'a> FnOnce(&mut ::concord_core::prelude::PolicyPatch<'a>) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>,
             {
                 self.inner.execute_decoded_ref_with(ep, meta, patch)
             }

            pub fn with_debug_level(mut self, level: ::concord_core::prelude::DebugLevel) -> Self {
                self.inner.set_debug_level(level);
                self
            }

            pub fn execute<E>(
                &self,
                ep: E,
            ) -> impl ::core::future::Future<
                Output = ::core::result::Result<
                    <E::Response as ::concord_core::internal::ResponseSpec>::Output,
                    ::concord_core::prelude::ApiClientError
                >
            >
            where
                E: ::concord_core::prelude::Endpoint<#cx_name>,
            {
                self.inner.execute(ep)
            }

            pub async fn execute_async<E>(
                &self,
                ep: E,
            ) -> ::core::result::Result<
                <E::Response as ::concord_core::internal::ResponseSpec>::Output,
                ::concord_core::prelude::ApiClientError
                >
            where
                E: ::concord_core::prelude::Endpoint<#cx_name>,
            {
                self.inner.execute(ep).await
            }

            pub fn collect_all_items<E>(&self, ep: E) -> ::concord_core::prelude::CollectAllItems<'_, #cx_name, E>
                where
                    E: ::concord_core::prelude::CollectAllItemsEndpoint<#cx_name>,
            {
                self.inner.collect_all_items(ep)
            }
        }
    };

    let shared = SharedRegistry::build(&ir)?;
    let shared_items = shared.emit_shared_module(&ir, &cx_name)?;

    // __internal module with per-endpoint route/policy/body/map
    let mut internal_items = Vec::new();
    for ep in &ir.endpoints {
        let plan = analyze_endpoint(&ir, ep)?;
        internal_items.push(emit_internal_for_endpoint(
            &ir, ep, &plan, &cx_name, &shared,
        )?);
    }
    let internal_mod = quote! {
        #[doc(hidden)]
        mod __internal {
            use super::*;
            pub mod __shared {
                use super::*;
                #shared_items
            }
            #(#internal_items)*
        }
    };

    // endpoints module
    let endpoints_items: Vec<TokenStream2> = ir
        .endpoints
        .iter()
        .map(|ep| {
            let plan = analyze_endpoint(&ir, ep)?;
            emit_endpoint_module_item(ep, &plan, &cx_name)
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let endpoints_mod = quote! {
        pub mod endpoints {
            use super::*;
            #(#endpoints_items)*
        }
    };

    Ok(quote! {
        pub mod #mod_ident {
            use super::*;

            #vars_impl
            #cx_impl
            #client_wrapper
            #internal_mod
            #endpoints_mod
        }
    })
}

// --------------------- planning ---------------------
#[derive(Clone)]
struct FieldDef {
    name_snake: String,
    orig: String,
    ty: Type,
    optional: bool,
    default: Option<Expr>,
    span: Span,
}
impl FieldDef {
    fn ident(&self) -> Ident {
        Ident::new(&self.name_snake, self.span)
    }
    fn is_required(&self) -> bool {
        !self.optional && self.default.is_none()
    }
}
struct EndpointPlan {
    fields: BTreeMap<String, FieldDef>,
    order: Vec<String>, // first-declaration order
    // subset order for route-declared fields (new() ordering)
    route_decl_order: Vec<String>,
}

fn analyze_endpoint(ir: &Ir, ep: &IrEndpoint) -> syn::Result<EndpointPlan> {
    let mut fields: BTreeMap<String, FieldDef> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut route_decl_order: Vec<String> = Vec::new();

    // 1) route: host prefix, path prefix, endpoint path
    for r in &ep.full_host_prefix {
        collect_decl_fields_from_route_expr(
            r,
            RouteCtx::Host,
            &mut fields,
            &mut order,
            &mut route_decl_order,
        )?;
    }
    for r in &ep.full_path_prefix {
        collect_decl_fields_from_route_expr(
            r,
            RouteCtx::Path,
            &mut fields,
            &mut order,
            &mut route_decl_order,
        )?;
    }
    collect_decl_fields_from_route_expr(
        &ep.endpoint_path,
        RouteCtx::Path,
        &mut fields,
        &mut order,
        &mut route_decl_order,
    )?;

    // 2) query
    for node in &ep.full_policy_prefix {
        for q in &node.query {
            let value = match q {
                parse::QueryEntry::Set { value, .. } => Some(value),
                parse::QueryEntry::Push { value, .. } => Some(value),
                parse::QueryEntry::Remove { .. } => None,
            };
            let Some(value) = value else { continue };
            match value {
                parse::ValueExpr::Decl(decl) => {
                    let eff = decl.alias.as_ref().unwrap_or(&decl.name);
                    let name_snake = eff.to_string().to_snake_case();
                    let orig = eff.to_string();
                    add_field(
                        &mut fields,
                        &mut order,
                        &name_snake,
                        decl.ty.clone(),
                        decl.optional,
                        decl.default.clone(),
                        decl.name.span(),
                        orig,
                    )?;
                }
                parse::ValueExpr::Format { atoms, mode: _ } => {
                    collect_decl_fields_from_atoms(
                        atoms,
                        RouteCtx::Policy,
                        &mut fields,
                        &mut order,
                    )?;
                }
                _ => {}
            }
        }
    }
    for q in &ep.query {
        let value = match q {
            parse::QueryEntry::Set { value, .. } => Some(value),
            parse::QueryEntry::Push { value, .. } => Some(value),
            parse::QueryEntry::Remove { .. } => None,
        };
        let Some(value) = value else { continue };

        match value {
            parse::ValueExpr::Decl(decl) => {
                let eff = decl.alias.as_ref().unwrap_or(&decl.name);
                let name_snake = eff.to_string().to_snake_case();
                let orig = eff.to_string();
                add_field(
                    &mut fields,
                    &mut order,
                    &name_snake,
                    decl.ty.clone(),
                    decl.optional,
                    decl.default.clone(),
                    decl.name.span(),
                    orig,
                )?;
            }
            parse::ValueExpr::Format { atoms, mode: _ } => {
                collect_decl_fields_from_atoms(atoms, RouteCtx::Policy, &mut fields, &mut order)?;
            }
            _ => {}
        }
    }

    // 3) headers
    for node in &ep.full_policy_prefix {
        for h in &node.headers {
            if let parse::HeaderRule::Set { value, .. } = h {
                match value.as_ref() {
                    parse::ValueExpr::Decl(decl) => {
                        let eff = decl.alias.as_ref().unwrap_or(&decl.name);
                        let name_snake = eff.to_string().to_snake_case();
                        let orig = eff.to_string();
                        add_field(
                            &mut fields,
                            &mut order,
                            &name_snake,
                            decl.ty.clone(),
                            decl.optional,
                            decl.default.clone(),
                            decl.name.span(),
                            orig,
                        )?;
                    }
                    parse::ValueExpr::Format { atoms, mode: _ } => {
                        collect_decl_fields_from_atoms(
                            atoms,
                            RouteCtx::Policy,
                            &mut fields,
                            &mut order,
                        )?;
                    }
                    _ => {}
                }
            }
        }
    }
    for h in &ep.headers {
        if let parse::HeaderRule::Set { value, .. } = h {
            match value.as_ref() {
                parse::ValueExpr::Decl(decl) => {
                    let eff = decl.alias.as_ref().unwrap_or(&decl.name);
                    let name_snake = eff.to_string().to_snake_case();
                    let orig = eff.to_string();
                    add_field(
                        &mut fields,
                        &mut order,
                        &name_snake,
                        decl.ty.clone(),
                        decl.optional,
                        decl.default.clone(),
                        decl.name.span(),
                        orig,
                    )?;
                }
                parse::ValueExpr::Format { atoms, mode: _ } => {
                    collect_decl_fields_from_atoms(
                        atoms,
                        RouteCtx::Policy,
                        &mut fields,
                        &mut order,
                    )?;
                }
                _ => {}
            }
        }
    }

    // 4) validate all reference placeholders resolve (and are not ambiguous)
    let vars_set = vars_name_set(ir);
    let ep_set: BTreeSet<String> = fields.keys().cloned().collect();

    for r in &ep.full_host_prefix {
        validate_route_expr_refs(r, RouteCtx::Host, &vars_set, &ep_set)?;
    }
    for r in &ep.full_path_prefix {
        validate_route_expr_refs(r, RouteCtx::Path, &vars_set, &ep_set)?;
    }
    validate_route_expr_refs(&ep.endpoint_path, RouteCtx::Path, &vars_set, &ep_set)?;

    // query refs
    for node in &ep.full_policy_prefix {
        for q in &node.query {
            match q {
                parse::QueryEntry::Remove { .. } => {}
                parse::QueryEntry::Set { value, .. } | parse::QueryEntry::Push { value, .. } => {
                    validate_value_expr_refs(value, &vars_set, &ep_set)?;
                    if let parse::ValueExpr::Format { atoms, mode: _ } = value {
                        validate_atoms_refs(atoms, RouteCtx::Policy, &vars_set, &ep_set)?;
                    }
                }
            }
        }
    }
    for q in &ep.query {
        match q {
            parse::QueryEntry::Remove { .. } => {}
            parse::QueryEntry::Set { value, .. } | parse::QueryEntry::Push { value, .. } => {
                validate_value_expr_refs(value, &vars_set, &ep_set)?;
                if let parse::ValueExpr::Format { atoms, mode: _ } = value {
                    validate_atoms_refs(atoms, RouteCtx::Policy, &vars_set, &ep_set)?;
                }
            }
        }
    }

    // header refs
    for node in &ep.full_policy_prefix {
        for h in &node.headers {
            if let parse::HeaderRule::Set { value, .. } = h {
                validate_value_expr_refs(value, &vars_set, &ep_set)?;
                if let parse::ValueExpr::Format { atoms, mode: _ } = value.as_ref() {
                    validate_atoms_refs(atoms, RouteCtx::Policy, &vars_set, &ep_set)?;
                }
            }
        }
    }
    for h in &ep.headers {
        if let parse::HeaderRule::Set { value, .. } = h {
            validate_value_expr_refs(value, &vars_set, &ep_set)?;
            if let parse::ValueExpr::Format { atoms, mode: _ } = value.as_ref() {
                validate_atoms_refs(atoms, RouteCtx::Policy, &vars_set, &ep_set)?;
            }
        }
    }

    Ok(EndpointPlan {
        fields,
        order,
        route_decl_order,
    })
}

#[derive(Copy, Clone)]
enum RouteCtx {
    Host,
    Path,
    Policy,
}

#[allow(clippy::too_many_arguments)]
fn add_field(
    fields: &mut BTreeMap<String, FieldDef>,
    order: &mut Vec<String>,
    name_snake: &str,
    ty: Type,
    optional: bool,
    default: Option<Expr>,
    span: Span,
    orig: String,
) -> syn::Result<()> {
    if is_reserved_placeholder_name(name_snake) {
        return Err(syn::Error::new(
            span,
            format!(
                "reserved placeholder name `{}` (would generate a reserved endpoint method/field `{}`); use `{} as <alias>: <Type>`",
                orig, name_snake, orig
            ),
        ));
    }
    if let Some(prev) = fields.get_mut(name_snake) {
        if prev.orig != orig {
            let mut e = syn::Error::new(
                prev.span,
                format!(
                    "first declaration maps to `{}`: `{}`",
                    name_snake, prev.orig
                ),
            );
            e.combine(syn::Error::new(
                span,
                format!(
                    "snake_case collision: `{}` and `{}` both map to `{}`; rename one placeholder (use `as` in a declaration)",
                    prev.orig, orig, name_snake
                ),
            ));
            return Err(e);
        }
        // type compatibility check (stringified tokens)
        let a = norm_type(&ty);
        let prev_ty = &prev.ty;
        let b = norm_type(prev_ty);
        if a != b {
            let mut e = syn::Error::new(
                prev.span,
                format!("`{}` first declared here with type `{}`", name_snake, b),
            );
            e.combine(syn::Error::new(
                span,
                format!("conflicting types for `{}`: `{}` vs `{}`", name_snake, b, a),
            ));
            return Err(e);
        }
        // optional mismatch => error (avoid silent behaviour changes)
        if prev.optional != optional {
            let mut e = syn::Error::new(
                prev.span,
                format!(
                    "`{}` first declared here with optionality={}",
                    name_snake, prev.optional
                ),
            );
            e.combine(syn::Error::new(
                span,
                format!(
                    "conflicting optionality for `{}` (previous={}, current={})",
                    name_snake, prev.optional, optional
                ),
            ));
            return Err(e);
        }
        // merge defaults: allow adding a default if missing; conflict if both present and differ
        match (&prev.default, &default) {
            (Some(a), Some(b)) => {
                let sa = norm_expr(a);
                let sb = norm_expr(b);
                if sa != sb {
                    let mut e = syn::Error::new(
                        prev.span,
                        format!("`{}` first declared here with a default", name_snake),
                    );
                    e.combine(syn::Error::new(
                        span,
                        format!("conflicting defaults for `{}`", name_snake),
                    ));
                    return Err(e);
                }
            }
            (None, Some(_)) => {
                prev.default = default;
            }
            _ => {}
        }
        Ok(())
    } else {
        fields.insert(
            name_snake.to_string(),
            FieldDef {
                name_snake: name_snake.to_string(),
                orig,
                ty,
                optional,
                default,
                span,
            },
        );
        order.push(name_snake.to_string());
        Ok(())
    }
}

fn is_reserved_placeholder_name(name_snake: &str) -> bool {
    matches!(
        name_snake,
        // core endpoint API surface
        "new" | "name" | "debug" | "timeout" | "body"
        // endpoint control methods
        | "with_debug_level" | "with_timeout" | "without_timeout"
        | "set_debug_level_opt" | "set_timeout_override" | "set_body"
        // internal fields
        | "__client_api_debug_level" | "__client_api_timeout"
        // misc: avoid collisions with constants/associated items
        | "endpoint_name" | "ENDPOINT_NAME"
    )
}

fn norm_type(ty: &Type) -> String {
    let mut s = quote!(#ty).to_string();
    s.retain(|c| !c.is_whitespace());
    for (from, to) in [
        ("std::string::String", "String"),
        ("alloc::string::String", "String"),
        ("::std::string::String", "String"),
        ("core::primitive::u64", "u64"),
        ("core::primitive::i64", "i64"),
        ("core::primitive::usize", "usize"),
        ("core::primitive::bool", "bool"),
    ] {
        s = s.replace(from, to);
    }
    s
}

fn norm_expr(e: &Expr) -> String {
    let mut s = quote!(#e).to_string();
    s.retain(|c| !c.is_whitespace());
    s
}

fn validate_value_expr_refs(
    v: &parse::ValueExpr,
    vars: &BTreeSet<String>,
    eps: &BTreeSet<String>,
) -> syn::Result<()> {
    match v {
        parse::ValueExpr::Lit(_) => Ok(()),
        parse::ValueExpr::Decl(_) => Ok(()),
        parse::ValueExpr::Format { atoms, mode: _ } => {
            validate_atoms_refs(atoms, RouteCtx::Policy, vars, eps)
        }
        parse::ValueExpr::Ref(r) => {
            let name = r.name.to_string().to_snake_case();
            match r.scope {
                Some(parse::RefScope::Vars) => {
                    if !vars.contains(&name) {
                        return Err(err_unknown_placeholder(
                            r.name.span(),
                            &name,
                            vars,
                            eps,
                            Some(parse::RefScope::Vars),
                        ));
                    }
                    Ok(())
                }
                Some(parse::RefScope::Ep) => {
                    if !eps.contains(&name) {
                        return Err(err_unknown_placeholder(
                            r.name.span(),
                            &name,
                            vars,
                            eps,
                            Some(parse::RefScope::Ep),
                        ));
                    }
                    Ok(())
                }
                None => {
                    let in_vars = vars.contains(&name);
                    let in_eps = eps.contains(&name);
                    if in_vars && in_eps {
                        return Err(err_ambiguous_placeholder(r.name.span(), &name));
                    }
                    if !(in_vars || in_eps) {
                        return Err(err_unknown_placeholder(
                            r.name.span(),
                            &name,
                            vars,
                            eps,
                            None,
                        ));
                    }
                    Ok(())
                }
            }
        }
    }
}

// --------------------- emit internal ---------------------
fn emit_internal_for_endpoint(
    ir: &Ir,
    ep: &IrEndpoint,
    plan: &EndpointPlan,
    cx_name: &Ident,
    shared: &SharedRegistry,
) -> syn::Result<TokenStream2> {
    let route_name = format_ident!("Route{}", ep.name);
    let policy_name = format_ident!("Policy{}", ep.name);
    let body_name = format_ident!("Body{}", ep.name);
    let map_name = format_ident!("Map{}", ep.name);
    let resp_ty = &ep.resp.ty;
    let route_chain = emit_route_chain_shared(ir, ep, &route_name, cx_name, shared)?;
    let policy_chain = emit_policy_chain_shared(ir, ep, plan, &policy_name, cx_name, shared)?;
    let field_impls = emit_field_trait_impls_for_endpoint(plan, &ep.name, shared)?;
    let pagination_impl = emit_pagination_impl(ir, ep, plan, cx_name)?;
    let body_impl = if let Some(body) = &ep.body {
        let body_codec = &body.codec;
        let body_ty = &body.ty;
        let ep_ident = &ep.name;
        quote! {
            pub struct #body_name;
            impl ::concord_core::internal::BodyPart<super::endpoints::#ep_ident> for #body_name {
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
        quote! {
            pub struct #map_name;
            impl ::concord_core::internal::Transform<#resp_ty> for #map_name {
                type Out = #out_ty;
                fn map(v: #resp_ty) -> ::core::result::Result<Self::Out, ::concord_core::prelude::FxError> {
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
        #route_chain
        #policy_chain
        #field_impls
        #body_impl
        #pagination_impl
        #response_ty
    })
}

fn build_chain_ty_ts(parts: &[TokenStream2], tail: TokenStream2) -> TokenStream2 {
    let mut ty = tail;
    for p in parts.iter().rev() {
        ty = quote!(::concord_core::internal::Chain<#p, #ty>);
    }
    ty
}

fn emit_route_chain_shared(
    ir: &Ir,
    ep: &IrEndpoint,
    route_name: &Ident,
    cx_name: &Ident,
    shared: &SharedRegistry,
) -> syn::Result<TokenStream2> {
    let _ = cx_name;
    let vars_set = vars_name_set(ir);
    let mut parts: Vec<TokenStream2> = Vec::new();
    for r in &ep.full_host_prefix {
        parts.push(shared.route_part_ty(SharedRouteMode::Host, r, &vars_set));
    }
    for r in &ep.full_path_prefix {
        parts.push(shared.route_part_ty(SharedRouteMode::Path, r, &vars_set));
    }
    if !is_empty_route_expr(&ep.endpoint_path) {
        parts.push(shared.route_part_ty(SharedRouteMode::Path, &ep.endpoint_path, &vars_set));
    }
    let chain_ty = build_chain_ty_ts(&parts, quote!(::concord_core::internal::NoRoute));
    Ok(quote! { pub type #route_name = #chain_ty; })
}

fn host_label_source_ts(atoms: &[parse::Atom]) -> TokenStream2 {
    if atoms.len() == 1
        && let parse::Atom::Param(ph) = &atoms[0]
    {
        let name = ph.name.to_string();
        let lit = LitStr::new(&name, ph.name.span());
        return quote!(::concord_core::prelude::HostLabelSource::Placeholder { name: #lit });
    }
    quote!(::concord_core::prelude::HostLabelSource::Mixed)
}

fn emit_policy_chain_shared(
    ir: &Ir,
    ep: &IrEndpoint,
    plan: &EndpointPlan,
    policy_name: &Ident,
    cx_name: &Ident,
    shared: &SharedRegistry,
) -> syn::Result<TokenStream2> {
    let ep_ident = &ep.name;
    let vars_set = vars_name_set(ir);
    let vars_opt_set = vars_optional_set(ir);
    let ep_set: BTreeSet<String> = plan.fields.keys().cloned().collect();
    let ep_opt_set: BTreeSet<String> = plan
        .fields
        .iter()
        .filter_map(|(k, f)| if f.optional { Some(k.clone()) } else { None })
        .collect();

    let mut part_tys: Vec<TokenStream2> = Vec::new();

    // Prefix/path shared policy nodes (only if non-empty)
    for node in &ep.full_policy_prefix {
        let has_any = !node.headers.is_empty() || !node.query.is_empty() || node.timeout.is_some();
        if !has_any {
            continue;
        }
        part_tys.push(shared.policy_part_ty(node, &vars_set));
    }

    // Endpoint-level policy part (per-endpoint, unchanged semantics)
    let endpoint_part = format_ident!("{}Endpoint", policy_name);
    {
        let mut stmts: Vec<TokenStream2> = Vec::new();
        stmts.push(quote! { policy.set_layer(::concord_core::prelude::PolicyLayer::Endpoint); });
        for h in &ep.headers {
            match h {
                parse::HeaderRule::Remove { name } => {
                    let n = lower_header_name(name);
                    stmts.push(quote! {
                        policy.remove_header(::http::header::HeaderName::from_static(#n));
                    });
                }
                parse::HeaderRule::Set { name, value } => {
                    let header_name = lower_header_name(name);
                    stmts.push(emit_header_set_stmt(
                        value,
                        &header_name,
                        &vars_set,
                        &vars_opt_set,
                        &ep_set,
                        &ep_opt_set,
                    )?);
                }
            }
        }
        for q in &ep.query {
            match q {
                parse::QueryEntry::Remove { key } => {
                    let k = key.value();
                    stmts.push(quote! { policy.remove_query(#k); });
                }
                parse::QueryEntry::Set { key, value } => {
                    stmts.push(emit_query_stmt(
                        key,
                        value,
                        false,
                        &vars_set,
                        &vars_opt_set,
                        &ep_set,
                        &ep_opt_set,
                    )?);
                }
                parse::QueryEntry::Push { key, value } => {
                    stmts.push(emit_query_stmt(
                        key,
                        value,
                        true,
                        &vars_set,
                        &vars_opt_set,
                        &ep_set,
                        &ep_opt_set,
                    )?);
                }
            }
        }
        if let Some(t) = ep.timeout.as_ref() {
            stmts.push(quote! { policy.set_timeout(#t); });
        }
        let p = endpoint_part.clone();
        let cx = cx_name;
        part_tys.push(quote!(#p));
        // define the struct/impl in this function output
        let runtime_part = format_ident!("{}Runtime", policy_name);
        // IMPORTANT: include runtime override layer in the policy chain
        part_tys.push(quote!(#runtime_part));
        let chain_ty = build_chain_ty_ts(&part_tys, quote!(::concord_core::internal::NoPolicy));
        Ok(quote! {
            pub struct #endpoint_part;
            impl ::concord_core::internal::PolicyPart<super::#cx, super::endpoints::#ep_ident> for #endpoint_part {
                fn apply(
                    ep: &super::endpoints::#ep_ident,
                    client: &::concord_core::prelude::ApiClient<super::#cx>,
                    policy: &mut ::concord_core::prelude::Policy,
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                    let _ = ep;
                    let _ = client;
                    #(#stmts)*
                    ::core::result::Result::Ok(())
                }
            }

            pub struct #runtime_part;
            impl ::concord_core::internal::PolicyPart<super::#cx, super::endpoints::#ep_ident> for #runtime_part {
                fn apply(
                    ep: &super::endpoints::#ep_ident,
                    _client: &::concord_core::prelude::ApiClient<super::#cx>,
                    policy: &mut ::concord_core::prelude::Policy,
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                    policy.set_layer(::concord_core::prelude::PolicyLayer::Runtime);
                    match ep.__client_api_timeout {
                        ::concord_core::prelude::TimeoutOverride::Inherit => {}
                        ::concord_core::prelude::TimeoutOverride::Clear => policy.clear_timeout(),
                        ::concord_core::prelude::TimeoutOverride::Set(t) => policy.set_timeout(t),
                    }
                    ::core::result::Result::Ok(())
                }
            }

            pub type #policy_name = #chain_ty;
        })
    }
}

fn emit_field_trait_impls_for_endpoint(
    plan: &EndpointPlan,
    ep_ident: &Ident,
    shared: &SharedRegistry,
) -> syn::Result<TokenStream2> {
    let mut impls: Vec<TokenStream2> = Vec::new();
    for (name, f) in &plan.fields {
        if !shared.field_names.contains(name) {
            continue;
        }
        let tr = SharedRegistry::field_trait_ident(name);
        let field_ident = Ident::new(name, f.span);
        let ty = &f.ty;
        if f.optional {
            impls.push(quote! {
                impl __shared::#tr for super::endpoints::#ep_ident {
                    type Ty = #ty;
                    fn get_opt(ep: &Self) -> ::core::option::Option<&Self::Ty> {
                        ep.#field_ident.as_ref()
                    }
                }
            });
        } else {
            impls.push(quote! {
                impl __shared::#tr for super::endpoints::#ep_ident {
                    type Ty = #ty;
                    fn get_opt(ep: &Self) -> ::core::option::Option<&Self::Ty> {
                        ::core::option::Option::Some(&ep.#field_ident)
                    }
                }
            });
        }
    }
    Ok(quote! { #(#impls)* })
}

// --------------------- shared route/policy emit helpers ---------------------

fn emit_route_expr_pushes_host_shared(
    r: &parse::RouteExpr,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    cx_name: &Ident,
) -> syn::Result<Vec<TokenStream2>> {
    let _ = cx_name;
    match r {
        parse::RouteExpr::Static(lit) => {
            Ok(vec![quote! { route.host_mut().push_label_static(#lit); }])
        }
        parse::RouteExpr::Segments(segs) => {
            let mut out = Vec::new();
            for seg in segs {
                if let Some(lit) = atoms_static_string(&seg.atoms) {
                    out.push(quote! { route.host_mut().push_label_static(#lit); });
                } else {
                    let source_ts = host_label_source_ts(&seg.atoms);
                    let build = emit_atoms_build_expr_shared_route(
                        &seg.atoms,
                        RouteCtx::Host,
                        vars_set,
                        vars_opt_set,
                    )?;
                    out.push(quote! {
                        {
                            let mut __s = ::std::string::String::new();
                            #build
                            route.host_mut().push_label(__s, #source_ts);
                        }
                    });
                }
            }
            Ok(out)
        }
    }
}

fn emit_route_expr_pushes_path_shared(
    r: &parse::RouteExpr,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    cx_name: &Ident,
) -> syn::Result<Vec<TokenStream2>> {
    let _ = cx_name;
    match r {
        parse::RouteExpr::Static(lit) => Ok(vec![quote! { route.path_mut().push_raw(#lit); }]),
        parse::RouteExpr::Segments(segs) => {
            let mut out = Vec::new();
            for seg in segs {
                if let Some(lit) = atoms_static_string(&seg.atoms) {
                    out.push(quote! { route.path_mut().push_raw(#lit); });
                } else {
                    let build = emit_atoms_build_expr_shared_route(
                        &seg.atoms,
                        RouteCtx::Path,
                        vars_set,
                        vars_opt_set,
                    )?;
                    out.push(quote! {
                        {
                            let mut __s = ::std::string::String::new();
                            #build
                            route.path_mut().push_segment_encoded(&__s);
                        }
                    });
                }
            }
            Ok(out)
        }
    }
}

fn emit_atoms_build_expr_shared_route(
    atoms: &[parse::Atom],
    ctx: RouteCtx,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    let mut stmts: Vec<TokenStream2> = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(l) => {
                let s = l.value();
                if !s.is_empty() {
                    stmts.push(quote! { __s.push_str(#s); });
                }
            }
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                if vars_set.contains(&name) {
                    if vars_opt_set.contains(&name) {
                        return Err(syn::Error::new(
                            r.name.span(),
                            "v2: optional vars cannot be used in routes",
                        ));
                    }
                    let ident = Ident::new(&name, r.name.span());
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", client.vars().#ident);
                    });
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", <E as __shared::#tr>::get(ep));
                    });
                }
                let _ = ctx;
            }
            parse::Atom::Param(ph) => {
                let name = if ph.is_decl() {
                    placeholder_eff_snake(ph)
                } else {
                    ph.name.to_string().to_snake_case()
                };
                if !ph.is_decl() && vars_set.contains(&name) {
                    if vars_opt_set.contains(&name) {
                        return Err(syn::Error::new(
                            ph.name.span(),
                            "v2: optional vars cannot be used in routes",
                        ));
                    }
                    let ident = Ident::new(&name, ph.name.span());
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", client.vars().#ident);
                    });
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", <E as __shared::#tr>::get(ep));
                    });
                }
            }
        }
    }
    Ok(quote! { #(#stmts)* })
}

fn emit_policy_node_apply_stmts_shared(
    node: &IrPolicyNode,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<Vec<TokenStream2>> {
    let mut stmts: Vec<TokenStream2> = Vec::new();
    stmts.push(quote! { policy.set_layer(::concord_core::prelude::PolicyLayer::PrefixPath); });
    // headers
    for h in &node.headers {
        match h {
            parse::HeaderRule::Remove { name } => {
                let n = lower_header_name(name);
                stmts.push(quote! {
                    policy.remove_header(::http::header::HeaderName::from_static(#n));
                });
            }
            parse::HeaderRule::Set { name, value } => {
                let header_name = lower_header_name(name);
                stmts.push(emit_header_set_stmt_shared(
                    value,
                    &header_name,
                    vars_set,
                    vars_opt_set,
                )?);
            }
        }
    }
    // query
    for q in &node.query {
        match q {
            parse::QueryEntry::Remove { key } => {
                let k = key.value();
                stmts.push(quote! { policy.remove_query(#k); });
            }
            parse::QueryEntry::Set { key, value } => {
                stmts.push(emit_query_stmt_shared(
                    key,
                    value,
                    false,
                    vars_set,
                    vars_opt_set,
                )?);
            }
            parse::QueryEntry::Push { key, value } => {
                stmts.push(emit_query_stmt_shared(
                    key,
                    value,
                    true,
                    vars_set,
                    vars_opt_set,
                )?);
            }
        }
    }
    if let Some(t) = &node.timeout {
        stmts.push(quote! { policy.set_timeout(#t); });
    }
    Ok(stmts)
}

fn resolve_ref_shared(r: &RefExpr, vars_set: &BTreeSet<String>) -> (String, bool) {
    let name = r.name.to_string().to_snake_case();
    let is_var = match r.scope {
        Some(parse::RefScope::Vars) => true,
        Some(parse::RefScope::Ep) => false,
        None => vars_set.contains(&name),
    };
    (name, is_var)
}

fn emit_header_set_stmt_shared(
    value: &parse::ValueExpr,
    header_name: &LitStr,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    match value {
        parse::ValueExpr::Lit(lit) => {
            let v = lit.value();
            Ok(quote! {
                {
                    let __hv = ::http::header::HeaderValue::from_str(#v)
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                    policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                }
            })
        }
        parse::ValueExpr::Ref(r) => {
            let (name, is_var) = resolve_ref_shared(r, vars_set);
            let ident = Ident::new(&name, r.name.span());
            if is_var {
                if vars_opt_set.contains(&name) {
                    Ok(quote! {
                        if let ::core::option::Option::Some(v) = &client.vars().#ident {
                            let __s = ::std::string::ToString::to_string(v);
                            let __hv = ::http::header::HeaderValue::from_str(&__s)
                                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                            policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                        }
                    })
                } else {
                    Ok(quote! {
                        {
                            let __s = ::std::string::ToString::to_string(&client.vars().#ident);
                            let __hv = ::http::header::HeaderValue::from_str(&__s)
                                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                            policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                        }
                    })
                }
            } else {
                let tr = SharedRegistry::field_trait_ident(&name);
                Ok(quote! {
                    if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                        let __s = ::std::string::ToString::to_string(v);
                        let __hv = ::http::header::HeaderValue::from_str(&__s)
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                        policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                    }
                })
            }
        }
        parse::ValueExpr::Decl(decl) => {
            let eff = decl.alias.as_ref().unwrap_or(&decl.name);
            let name = eff.to_string().to_snake_case();
            let tr = SharedRegistry::field_trait_ident(&name);
            Ok(quote! {
                if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                    let __s = ::std::string::ToString::to_string(v);
                    let __hv = ::http::header::HeaderValue::from_str(&__s)
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                    policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                }
            })
        }
        parse::ValueExpr::Format { atoms, mode } => {
            let gates = emit_atoms_optional_gates_shared(atoms, vars_set, vars_opt_set)?;
            let build = emit_atoms_build_expr_shared_policy(atoms, vars_set, vars_opt_set)?;
            match mode {
                parse::FormatMode::GateEntry => {
                    if gates.is_empty() {
                        Ok(quote! {
                            {
                                let mut __s = ::std::string::String::new();
                                #build
                                let __hv = ::http::header::HeaderValue::from_str(&__s)
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                            }
                        })
                    } else {
                        Ok(quote! {
                            if #(#gates)&&* {
                                let mut __s = ::std::string::String::new();
                                #build
                                let __hv = ::http::header::HeaderValue::from_str(&__s)
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                            }
                        })
                    }
                }
                parse::FormatMode::Partial => Ok(quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        if !__s.is_empty() {
                            let __hv = ::http::header::HeaderValue::from_str(&__s)
                                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                            policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                        }
                    }
                }),
            }
        }
    }
}

fn emit_query_stmt_shared(
    key: &LitStr,
    value: &parse::ValueExpr,
    push: bool,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    let k = key.value();
    let method = if push {
        format_ident!("push_query")
    } else {
        format_ident!("set_query")
    };
    match value {
        parse::ValueExpr::Lit(lit) => {
            let v = lit.value();
            Ok(quote! { policy.#method(#k, #v); })
        }
        parse::ValueExpr::Ref(r) => {
            let (name, is_var) = resolve_ref_shared(r, vars_set);
            let ident = Ident::new(&name, r.name.span());
            if is_var {
                if vars_opt_set.contains(&name) {
                    Ok(quote! {
                        if let ::core::option::Option::Some(v) = &client.vars().#ident {
                            policy.#method(#k, ::std::string::ToString::to_string(v));
                        }
                    })
                } else {
                    Ok(
                        quote! { policy.#method(#k, ::std::string::ToString::to_string(&client.vars().#ident)); },
                    )
                }
            } else {
                let tr = SharedRegistry::field_trait_ident(&name);
                Ok(quote! {
                    if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                        policy.#method(#k, ::std::string::ToString::to_string(v));
                    }
                })
            }
        }
        parse::ValueExpr::Decl(decl) => {
            let eff = decl.alias.as_ref().unwrap_or(&decl.name);
            let name = eff.to_string().to_snake_case();
            let tr = SharedRegistry::field_trait_ident(&name);
            Ok(quote! {
                if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                    policy.#method(#k, ::std::string::ToString::to_string(v));
                }
            })
        }
        parse::ValueExpr::Format { atoms, mode } => {
            let gates = emit_atoms_optional_gates_shared(atoms, vars_set, vars_opt_set)?;
            let build = emit_atoms_build_expr_shared_policy(atoms, vars_set, vars_opt_set)?;
            match mode {
                parse::FormatMode::GateEntry => {
                    if gates.is_empty() {
                        Ok(quote! {
                            {
                                let mut __s = ::std::string::String::new();
                                #build
                                policy.#method(#k, __s);
                            }
                        })
                    } else {
                        Ok(quote! {
                            if #(#gates)&&* {
                                let mut __s = ::std::string::String::new();
                                #build
                                policy.#method(#k, __s);
                            }
                        })
                    }
                }
                parse::FormatMode::Partial => Ok(quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        if !__s.is_empty() {
                            policy.#method(#k, __s);
                        }
                    }
                }),
            }
        }
    }
}

fn emit_atoms_optional_gates_shared(
    atoms: &[parse::Atom],
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<Vec<TokenStream2>> {
    let mut gates = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(_) => {}
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                if vars_set.contains(&name) {
                    if vars_opt_set.contains(&name) {
                        let ident = Ident::new(&name, r.name.span());
                        gates.push(quote!(client.vars().#ident.is_some()));
                    }
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    gates.push(quote!(<E as __shared::#tr>::get_opt(ep).is_some()));
                }
            }
            parse::Atom::Param(ph) => {
                let name = if ph.is_decl() {
                    placeholder_eff_snake(ph)
                } else {
                    ph.name.to_string().to_snake_case()
                };
                if !ph.is_decl() && vars_set.contains(&name) {
                    if vars_opt_set.contains(&name) {
                        let ident = Ident::new(&name, ph.name.span());
                        gates.push(quote!(client.vars().#ident.is_some()));
                    }
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    gates.push(quote!(<E as __shared::#tr>::get_opt(ep).is_some()));
                }
            }
        }
    }
    Ok(gates)
}

fn emit_atoms_build_expr_shared_policy(
    atoms: &[parse::Atom],
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    let mut stmts: Vec<TokenStream2> = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(l) => {
                let s = l.value();
                if !s.is_empty() {
                    stmts.push(quote! { __s.push_str(#s); });
                }
            }
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                if vars_set.contains(&name) {
                    let ident = Ident::new(&name, r.name.span());
                    if vars_opt_set.contains(&name) {
                        stmts.push(quote! {
                            if let ::core::option::Option::Some(v) = &client.vars().#ident {
                                use ::core::fmt::Write as _;
                                let _ = write!(__s, "{}", v);
                            }
                        });
                    } else {
                        stmts.push(quote! {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", client.vars().#ident);
                        });
                    }
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                }
            }
            parse::Atom::Param(ph) => {
                let name = if ph.is_decl() {
                    placeholder_eff_snake(ph)
                } else {
                    ph.name.to_string().to_snake_case()
                };
                if !ph.is_decl() && vars_set.contains(&name) {
                    let ident = Ident::new(&name, ph.name.span());
                    if vars_opt_set.contains(&name) {
                        stmts.push(quote! {
                            if let ::core::option::Option::Some(v) = &client.vars().#ident {
                                use ::core::fmt::Write as _;
                                let _ = write!(__s, "{}", v);
                            }
                        });
                    } else {
                        stmts.push(quote! {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", client.vars().#ident);
                        });
                    }
                } else {
                    let tr = SharedRegistry::field_trait_ident(&name);
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = <E as __shared::#tr>::get_opt(ep) {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                }
            }
        }
    }
    Ok(quote! { #(#stmts)* })
}

fn emit_header_set_stmt(
    value: &parse::ValueExpr,
    header_name: &LitStr,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    match value {
        parse::ValueExpr::Lit(lit) => {
            let v = lit.value();
            Ok(quote! {
                {
                    let __hv = ::http::header::HeaderValue::from_str(#v)
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                    policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                }
            })
        }
        parse::ValueExpr::Ref(r) => {
            let (src, is_opt) = resolve_ref_source(
                r,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            Ok(emit_scalar_header_from_src(header_name, src, is_opt))
        }
        parse::ValueExpr::Decl(decl) => {
            let name = decl.name.to_string().to_snake_case();
            let ident = Ident::new(&name, decl.name.span());
            let src = quote!(ep.#ident);
            let is_opt = ep_opt_set.contains(&name);
            Ok(emit_scalar_header_from_src(header_name, src, is_opt))
        }
        parse::ValueExpr::Format { atoms, mode } => {
            let gates = emit_atoms_optional_gates(
                atoms,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            let build = emit_atoms_build_expr(
                atoms,
                RouteCtx::Policy,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            match mode {
                parse::FormatMode::GateEntry => {
                    if gates.is_empty() {
                        Ok(quote! {
                            {
                                let mut __s = ::std::string::String::new();
                                #build
                                let __hv = ::http::header::HeaderValue::from_str(&__s)
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                            }
                        })
                    } else {
                        Ok(quote! {
                            if #(#gates)&&* {
                                let mut __s = ::std::string::String::new();
                                #build
                                let __hv = ::http::header::HeaderValue::from_str(&__s)
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                            }
                        })
                    }
                }
                parse::FormatMode::Partial => Ok(quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        if !__s.is_empty() {
                            let __hv = ::http::header::HeaderValue::from_str(&__s)
                                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                            policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                        }
                    }
                }),
            }
        }
    }
}

fn emit_scalar_header_from_src(
    header_name: &LitStr,
    src: TokenStream2,
    is_opt: bool,
) -> TokenStream2 {
    if is_opt {
        quote! {
            if let ::core::option::Option::Some(v) = &#src {
                let __s = ::std::string::ToString::to_string(v);
                let __hv = ::http::header::HeaderValue::from_str(&__s)
                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
            }
        }
    } else {
        quote! {
            {
                let __s = ::std::string::ToString::to_string(&#src);
                let __hv = ::http::header::HeaderValue::from_str(&__s)
                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                policy.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
            }
        }
    }
}

fn emit_query_stmt(
    key: &LitStr,
    value: &parse::ValueExpr,
    push: bool,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    let k = key.value();
    let method = if push {
        format_ident!("push_query")
    } else {
        format_ident!("set_query")
    };

    match value {
        parse::ValueExpr::Lit(lit) => {
            let v = lit.value();
            Ok(quote! {
                policy.#method(#k, #v);
            })
        }
        parse::ValueExpr::Ref(r) => {
            let (src, is_opt) = resolve_ref_source(
                r,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            if is_opt {
                Ok(quote! {
                    if let ::core::option::Option::Some(v) = &#src {
                        policy.#method(#k, ::std::string::ToString::to_string(v));
                    }
                })
            } else {
                Ok(quote! {
                    policy.#method(#k, ::std::string::ToString::to_string(&#src));
                })
            }
        }
        parse::ValueExpr::Decl(decl) => {
            let name = decl.name.to_string().to_snake_case();
            let ident = Ident::new(&name, decl.name.span());
            let src = quote!(ep.#ident);
            let is_opt = ep_opt_set.contains(&name);
            if is_opt {
                Ok(quote! {
                    if let ::core::option::Option::Some(v) = &#src {
                        policy.#method(#k, ::std::string::ToString::to_string(v));
                    }
                })
            } else {
                Ok(quote! {
                    policy.#method(#k, ::std::string::ToString::to_string(&#src));
                })
            }
        }
        parse::ValueExpr::Format { atoms, mode } => {
            let gates = emit_atoms_optional_gates(
                atoms,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            let build = emit_atoms_build_expr(
                atoms,
                RouteCtx::Policy,
                vars_set,
                vars_opt_set,
                ep_set,
                ep_opt_set,
                quote!(client.vars()),
                quote!(ep),
            )?;
            match mode {
                parse::FormatMode::GateEntry => {
                    if gates.is_empty() {
                        Ok(quote! {
                            {
                                let mut __s = ::std::string::String::new();
                                #build
                                policy.#method(#k, __s);
                            }
                        })
                    } else {
                        Ok(quote! {
                            if #(#gates)&&* {
                                let mut __s = ::std::string::String::new();
                                #build
                                policy.#method(#k, __s);
                            }
                        })
                    }
                }
                parse::FormatMode::Partial => Ok(quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        if !__s.is_empty() {
                            policy.#method(#k, __s);
                        }
                    }
                }),
            }
        }
    }
}

fn resolve_ref_source(
    r: &parse::RefExpr,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
    ep_expr: TokenStream2,
) -> syn::Result<(TokenStream2, bool)> {
    let name = r.name.to_string().to_snake_case();
    let ident = Ident::new(&name, r.name.span());
    match r.scope {
        Some(parse::RefScope::Vars) => {
            if !vars_set.contains(&name) {
                return Err(err_unknown_placeholder(
                    r.name.span(),
                    &name,
                    vars_set,
                    ep_set,
                    Some(parse::RefScope::Vars),
                ));
            }
            Ok((quote!(#vars_expr.#ident), vars_opt_set.contains(&name)))
        }
        Some(parse::RefScope::Ep) => {
            if !ep_set.contains(&name) {
                return Err(err_unknown_placeholder(
                    r.name.span(),
                    &name,
                    vars_set,
                    ep_set,
                    Some(parse::RefScope::Ep),
                ));
            }
            Ok((quote!(#ep_expr.#ident), ep_opt_set.contains(&name)))
        }
        None => {
            let in_vars = vars_set.contains(&name);
            let in_eps = ep_set.contains(&name);
            if in_vars && in_eps {
                return Err(err_ambiguous_placeholder(r.name.span(), &name));
            }
            if in_vars {
                return Ok((quote!(#vars_expr.#ident), vars_opt_set.contains(&name)));
            }
            if in_eps {
                return Ok((quote!(#ep_expr.#ident), ep_opt_set.contains(&name)));
            }
            Err(err_unknown_placeholder(
                r.name.span(),
                &name,
                vars_set,
                ep_set,
                None,
            ))
        }
    }
}

// --------------------- endpoint struct generation ---------------------
fn emit_endpoint_module_item(
    ep: &IrEndpoint,
    plan: &EndpointPlan,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let ep_ident = &ep.name;

    let debug_field = format_ident!("__client_api_debug_level");
    let timeout_field = format_ident!("__client_api_timeout");
    // Fields (stable order)
    let mut fields_ts: Vec<TokenStream2> = Vec::new();
    for name in &plan.order {
        let f = plan.fields.get(name).unwrap();
        let ident = f.ident();
        let ty = &f.ty;
        if f.optional {
            fields_ts.push(quote! { pub(super) #ident: ::core::option::Option<#ty> });
        } else {
            fields_ts.push(quote! { pub(super) #ident: #ty });
        }
    }

    fields_ts.push(quote! {
        pub(super) #debug_field: ::core::option::Option<::concord_core::prelude::DebugLevel>
    });

    fields_ts.push(quote! {
        pub(super) #timeout_field: ::concord_core::prelude::TimeoutOverride
    });
    // Body
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        fields_ts.push(quote! { pub(super) body: #ty });
    }

    // Build new() args:
    // - required route decls first (plan.route_decl_order)
    // - then body
    // - then remaining required in plan.order
    let mut required_names: BTreeSet<String> = BTreeSet::new();
    for (k, f) in &plan.fields {
        if f.is_required() {
            required_names.insert(k.clone());
        }
    }
    let mut new_args: Vec<TokenStream2> = Vec::new();
    let mut init_stmts: Vec<TokenStream2> = Vec::new();
    let mut done_inits: BTreeSet<String> = BTreeSet::new();

    for name in &plan.route_decl_order {
        if !required_names.contains(name) {
            continue;
        }
        let f = plan.fields.get(name).unwrap();
        let ident = f.ident();
        let ty = &f.ty;
        if is_string_type(ty) {
            new_args.push(quote! { #ident: impl ::core::convert::Into<::std::string::String> });
            init_stmts.push(quote! { #ident: #ident.into() });
        } else {
            new_args.push(quote! { #ident: #ty });
            init_stmts.push(quote! { #ident });
        }
        done_inits.insert(name.clone());
    }

    if let Some(body) = &ep.body {
        let ty = &body.ty;
        new_args.push(quote! { body: #ty });
        init_stmts.push(quote! { body });
    }

    for name in &plan.order {
        if done_inits.contains(name) {
            continue;
        }
        let f = plan.fields.get(name).unwrap();
        let ident = f.ident();
        let ty = &f.ty;
        if f.is_required() {
            if is_string_type(ty) {
                new_args.push(quote! { #ident: impl ::core::convert::Into<::std::string::String> });
                init_stmts.push(quote! { #ident: #ident.into() });
            } else {
                new_args.push(quote! { #ident: #ty });
                init_stmts.push(quote! { #ident });
            }
        } else if f.optional {
            if let Some(def) = &f.default {
                let def_ts = coerce_default_expr(ty, def);
                init_stmts.push(quote! { #ident: ::core::option::Option::Some(#def_ts) });
            } else {
                init_stmts.push(quote! { #ident: ::core::option::Option::None });
            }
        } else if let Some(def) = &f.default {
            let def_ts = coerce_default_expr(ty, def);
            init_stmts.push(quote! { #ident: #def_ts });
        } else {
            init_stmts.push(quote! { #ident: ::core::default::Default::default() });
        }
    }
    init_stmts.push(quote! { #debug_field: ::core::option::Option::None });
    init_stmts.push(quote! { #timeout_field: ::concord_core::prelude::TimeoutOverride::Inherit });
    // setters: for all optional fields + defaulted non-optional fields
    let mut setters: Vec<TokenStream2> = Vec::new();
    for name in &plan.order {
        let f = plan.fields.get(name).unwrap();
        if f.is_required() {
            continue;
        }
        let ident = f.ident();
        let ty = &f.ty;
        if f.optional {
            if is_string_type(ty) {
                setters.push(quote! {
                     pub fn #ident(mut self, v: impl ::core::convert::Into<::std::string::String>) -> Self {
                         self.#ident = ::core::option::Option::Some(v.into());
                         self
                     }
                 });
            } else {
                setters.push(quote! {
                    pub fn #ident(mut self, v: #ty) -> Self {
                        self.#ident = ::core::option::Option::Some(v);
                        self
                    }
                });
            }
        } else if f.default.is_some() {
            if is_string_type(ty) {
                setters.push(quote! {
                     pub fn #ident(mut self, v: impl ::core::convert::Into<::std::string::String>) -> Self {
                         self.#ident = v.into();
                         self
                     }
                 });
            } else {
                setters.push(quote! {
                    pub fn #ident(mut self, v: #ty) -> Self {
                        self.#ident = v;
                        self
                    }
                });
            }
        }
    }

    // in-place setters (to make Controller::on_page(ep_next: &mut E) usable externally)
    let mut mut_setters: Vec<TokenStream2> = Vec::new();
    for name in &plan.order {
        let f = plan.fields.get(name).unwrap();
        let ident = f.ident();
        let ty = &f.ty;
        let set_ident = format_ident!("set_{}", ident);
        if f.optional {
            if is_string_type(ty) {
                mut_setters.push(quote! {
pub fn #set_ident(&mut self, v: ::core::option::Option<impl ::core::convert::Into<::std::string::String>>) {
self.#ident = v.map(::core::convert::Into::into);
}
});
            } else {
                mut_setters.push(quote! {
                pub fn #set_ident(&mut self, v: ::core::option::Option<#ty>) {
                self.#ident = v;
                }
                });
            }
        } else {
            if is_string_type(ty) {
                mut_setters.push(quote! {
                pub fn #set_ident(&mut self, v: impl ::core::convert::Into<::std::string::String>) {
                self.#ident = v.into();
                }
                });
            } else {
                mut_setters.push(quote! {
                pub fn #set_ident(&mut self, v: #ty) {
                self.#ident = v;
                }
                });
            }
        }
    }
    if let Some(body) = &ep.body {
        let ty = &body.ty;
        mut_setters.push(quote! {
        pub fn set_body(&mut self, body: #ty) {
        self.body = body;
        }
        });
    }
    let runtime_mut_setters = quote! {
    pub fn set_debug_level_opt(&mut self, level: ::core::option::Option<::concord_core::prelude::DebugLevel>) {
    self.#debug_field = level;
    }
    pub fn set_timeout_override(&mut self, v: ::concord_core::prelude::TimeoutOverride) {
    self.#timeout_field = v;
    }
    };

    let debug_setter = quote! {
        pub fn with_debug_level(mut self, level: ::concord_core::prelude::DebugLevel) -> Self {
            self.#debug_field = ::core::option::Option::Some(level);
            self
        }
    };

    let timeout_setters = quote! {
        pub fn with_timeout(mut self, timeout: ::core::time::Duration) -> Self {
            self.#timeout_field = ::concord_core::prelude::TimeoutOverride::Set(timeout);
            self
        }
        pub fn without_timeout(mut self) -> Self {
            self.#timeout_field = ::concord_core::prelude::TimeoutOverride::Clear;
        self
        }
    };

    let route_name = format_ident!("Route{}", ep_ident);
    let policy_name = format_ident!("Policy{}", ep_ident);
    let body_name = format_ident!("Body{}", ep_ident);
    let map_name = format_ident!("Map{}", ep_ident);
    let (resp_codec, resp_ty) = (&ep.resp.codec, &ep.resp.ty);
    let response_spec_ty = if ep.map.is_some() {
        quote! {
            ::concord_core::internal::Mapped<
                ::concord_core::internal::Decoded<#resp_codec, #resp_ty>,
                super::__internal::#map_name
            >
        }
    } else {
        quote! { ::concord_core::internal::Decoded<#resp_codec, #resp_ty> }
    };
    let body_part_ty = if ep.body.is_some() {
        quote! { super::__internal::#body_name }
    } else {
        quote! { ::concord_core::internal::NoBody }
    };

    let pagination_ty = if ep.paginate.is_some() {
        let pagination_name = format_ident!("Pagination{}", ep_ident);
        quote!(type Pagination = super::__internal::#pagination_name;)
    } else {
        quote!(
            type Pagination = ::concord_core::internal::NoPagination;
        )
    };
    let method_ident = &ep.method;

    Ok(quote! {
        #[derive(Debug)]
        pub struct #ep_ident {
              #(#fields_ts,)*
        }
        impl #ep_ident {
            pub const ENDPOINT_NAME: &'static str = stringify!(#ep_ident);
            pub fn new(#(#new_args),*) -> Self {
                Self { #(#init_stmts,)* }
            }

            #debug_setter
            #timeout_setters
            #runtime_mut_setters
            #(#mut_setters)*
            #(#setters)*
        }
        impl ::concord_core::prelude::Endpoint<super::#cx_name> for #ep_ident {
            const METHOD: ::http::Method = ::http::Method::#method_ident;
            type Route = super::__internal::#route_name;
            type Policy = super::__internal::#policy_name;
            #pagination_ty
            type Body = #body_part_ty;
            type Response = #response_spec_ty;
            fn name(&self) -> &'static str {
                Self::ENDPOINT_NAME
            }
            fn debug_level(&self) -> ::core::option::Option<::concord_core::prelude::DebugLevel> {
                self.#debug_field
            }
        }
    })
}

// --------------------- base policy ---------------------
fn emit_base_policy(
    headers: &[parse::HeaderRule],
    vars: &[IrVar],
    timeout: Option<&Expr>,
) -> syn::Result<TokenStream2> {
    if headers.is_empty() && timeout.is_none() {
        return Ok(quote! {
            let mut p = ::concord_core::prelude::Policy::new();
            Ok(p)
        });
    }
    let vars_fields = vars_name_set_from_vars(vars);
    let vars_opt: BTreeSet<String> = vars
        .iter()
        .filter_map(|v| {
            if v.optional {
                Some(v.name.to_string().to_snake_case())
            } else {
                None
            }
        })
        .collect();

    let mut stmts: Vec<TokenStream2> = Vec::new();
    for h in headers {
        match h {
            parse::HeaderRule::Remove { name } => {
                let n = lower_header_name(name);
                stmts
                    .push(quote! { p.remove_header(::http::header::HeaderName::from_static(#n)); });
            }
            parse::HeaderRule::Set { name, value } => {
                let n = lower_header_name(name);
                stmts.push(emit_client_header_set_stmt(
                    value,
                    &n,
                    &vars_fields,
                    &vars_opt,
                )?);
            }
        }
    }
    if let Some(t) = timeout {
        stmts.push(quote! {
            p.set_timeout(#t);
        });
    }
    Ok(quote! {
        let mut p = ::concord_core::prelude::Policy::new();
        #(#stmts)*
        Ok(p)
    })
}

// --------------------- pagination emission ---------------------

fn require_ident_expr(e: &Expr) -> Option<Ident> {
    if let Expr::Path(p) = e
        && p.qself.is_none()
        && p.path.segments.len() == 1
    {
        return Some(p.path.segments[0].ident.clone());
    }
    None
}

fn placeholder_from_value_expr(v: &parse::ValueExpr) -> Option<String> {
    match v {
        parse::ValueExpr::Ref(r) => {
            // Only treat as endpoint placeholder when the ref is not scoped to vars.
            if matches!(r.scope, Some(parse::RefScope::Vars)) {
                None
            } else {
                Some(r.name.to_string().to_snake_case())
            }
        }
        parse::ValueExpr::Decl(d) => Some(d.name.to_string().to_snake_case()),
        _ => None,
    }
}

fn infer_query_key_lit(ep: &IrEndpoint, placeholder_snake: &str) -> Option<LitStr> {
    // precedence: endpoint query last-wins, then prefix/path query inner-last-wins
    for q in ep.query.iter().rev() {
        match q {
            parse::QueryEntry::Set { key, value } | parse::QueryEntry::Push { key, value } => {
                if let Some(ph) = placeholder_from_value_expr(value)
                    && ph == placeholder_snake
                {
                    return Some(key.clone());
                }
            }
            parse::QueryEntry::Remove { .. } => {}
        }
    }
    for node in ep.full_policy_prefix.iter().rev() {
        for q in node.query.iter().rev() {
            match q {
                parse::QueryEntry::Set { key, value } | parse::QueryEntry::Push { key, value } => {
                    if let Some(ph) = placeholder_from_value_expr(value)
                        && ph == placeholder_snake
                    {
                        return Some(key.clone());
                    }
                }
                parse::QueryEntry::Remove { .. } => {}
            }
        }
    }
    None
}

fn err_ambiguous_placeholder(span: Span, name_snake: &str) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "ambiguous placeholder `{}` (exists in vars and endpoint fields)\nhelp: disambiguate with `vars.{0}` or `ep.{0}`",
            name_snake
        ),
    )
}

fn err_unknown_placeholder(
    span: Span,
    name_snake: &str,
    vars: &BTreeSet<String>,
    eps: &BTreeSet<String>,
    forced: Option<parse::RefScope>,
) -> syn::Error {
    let mut msg = String::new();
    match forced {
        Some(parse::RefScope::Vars) => {
            msg.push_str(&format!("unknown client var `vars.{}`", name_snake))
        }
        Some(parse::RefScope::Ep) => {
            msg.push_str(&format!("unknown endpoint field `ep.{}`", name_snake))
        }
        None => msg.push_str(&format!("unknown placeholder `{}`", name_snake)),
    }
    let mut sugg = Vec::new();
    sugg.extend(
        suggest_similar(name_snake, vars)
            .into_iter()
            .map(|s| format!("vars.{}", s)),
    );
    sugg.extend(
        suggest_similar(name_snake, eps)
            .into_iter()
            .map(|s| format!("ep.{}", s)),
    );
    if !sugg.is_empty() {
        msg.push_str("\nhelp: did you mean one of: ");
        msg.push_str(&sugg.join(", "));
    }
    syn::Error::new(span, msg)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        prev.clone_from_slice(&cur);
    }
    prev[b.len()]
}

fn suggest_similar(needle: &str, set: &BTreeSet<String>) -> Vec<String> {
    let mut scored: Vec<(usize, &String)> =
        set.iter().map(|s| (levenshtein(needle, s), s)).collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(3).map(|(_, s)| s.clone()).collect()
}

fn emit_pagination_impl(
    ir: &Ir,
    ep: &IrEndpoint,
    plan: &EndpointPlan,
    cx_name: &Ident,
) -> syn::Result<TokenStream2> {
    let Some(spec) = &ep.paginate else {
        return Ok(quote! {});
    };

    let ep_ident = &ep.name;
    let pagination_name = format_ident!("Pagination{}", ep_ident);

    let vars_set = vars_name_set(ir);
    let _vars_opt_set = vars_optional_set(ir);
    let ep_set: BTreeSet<String> = plan.fields.keys().cloned().collect();
    let ep_opt_set: BTreeSet<String> = plan
        .fields
        .iter()
        .filter_map(|(k, f)| if f.optional { Some(k.clone()) } else { None })
        .collect();

    // Generic controller construction:
    // - All identifier bindings are resolved to (vars|endpoint) and cloned (non-optional only).
    // - Codegen stays opaque to controller internals: it can only pass optional key hints via Controller::hint_param_key().
    // Controller type must implement Default so we can start from defaults and patch fields.
    let ctrl_ty = &spec.paginator;

    let mut sets: Vec<TokenStream2> = Vec::new();
    for a in &spec.args {
        let k = &a.key;
        let bound_id: Ident = match &a.value {
            None => k.clone(),
            Some(v) => require_ident_expr(v).ok_or_else(|| {
                syn::Error::new(
                    v.span(),
                    "paginate: value must be a single identifier bound to an endpoint field",
                )
            })?,
        };
        let bound_snake = bound_id.to_string().to_snake_case();
        if vars_set.contains(&bound_snake) {
            return Err(syn::Error::new(
                bound_id.span(),
                format!(
                    "paginate: `{}` resolves to a client var; paginate bindings must refer to endpoint fields",
                    bound_snake
                ),
            ));
        }
        if !ep_set.contains(&bound_snake) {
            return Err(syn::Error::new(
                bound_id.span(),
                format!(
                    "paginate: unknown endpoint field `{}` (declare it as a placeholder on the endpoint)",
                    bound_snake
                ),
            ));
        }
        if ep_opt_set.contains(&bound_snake) {
            return Err(syn::Error::new(
                bound_id.span(),
                "paginate: optional endpoint field used implicitly; make it required or bind an explicit expression".to_string(),
            ));
        }
        let bound_ident = Ident::new(&bound_snake, bound_id.span());
        let expr_ts = quote!(::core::clone::Clone::clone(&ep.#bound_ident));
        sets.push(quote! {
            ctrl.#k = #expr_ts;
        });
    }

    // Optional key hints: for each paginate arg whose binding is a single identifier,
    // infer the effective query key from the endpoint query blocks (including prefixes).
    let mut hints: Vec<TokenStream2> = Vec::new();
    for a in &spec.args {
        let param_lit = LitStr::new(&a.key.to_string(), a.key.span());
        let bound_id: Ident = match &a.value {
            None => a.key.clone(),
            Some(v) => match require_ident_expr(v) {
                Some(id) => id,
                None => continue,
            },
        };
        let ph_snake = bound_id.to_string().to_snake_case();
        if !ep_set.contains(&ph_snake) {
            continue;
        }
        let key_lit = infer_query_key_lit(ep, &ph_snake)
            .unwrap_or_else(|| LitStr::new(&bound_id.to_string(), bound_id.span()));
        hints.push(quote! {
        <#ctrl_ty as ::concord_core::internal::Controller<super::#cx_name, super::endpoints::#ep_ident>>::hint_param_key(
            &mut ctrl,
            #param_lit,
            #key_lit,
        );
    });
    }

    Ok(quote! {
        pub struct #pagination_name;
        impl ::concord_core::internal::PaginationPart<super::#cx_name, super::endpoints::#ep_ident> for #pagination_name {
            type Ctrl = #ctrl_ty;
                  fn controller(
                    client: &::concord_core::prelude::ApiClient<super::#cx_name>,
                    ep: &super::endpoints::#ep_ident
                  ) -> ::core::result::Result<Self::Ctrl, ::concord_core::prelude::ApiClientError> {
                    let _ = client;
                    const _: fn() = || {
                        fn assert_bounds<T: ::core::default::Default>() {}
                            assert_bounds::<#ctrl_ty>();
                    };
                    let mut ctrl: #ctrl_ty = ::core::default::Default::default();
                    #(#sets)*
                    #(#hints)*
                    Ok(ctrl)
            }
        }
    })
}

fn emit_client_header_set_stmt(
    value: &parse::ValueExpr,
    header_name: &LitStr,
    vars_fields: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
) -> syn::Result<TokenStream2> {
    match value {
        parse::ValueExpr::Lit(lit) => {
            let v = lit.value();
            Ok(quote! {
                {
                  let __hv = ::http::header::HeaderValue::from_str(#v)
                    .map_err(|e| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                  p.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                }
            })
        }
        parse::ValueExpr::Ref(r) => {
            let name = r.name.to_string().to_snake_case();
            if !vars_fields.contains(&name) {
                return Err(syn::Error::new(
                    r.name.span(),
                    format!(
                        "unknown client var `{}`\nhelp: did you mean one of: {}",
                        name,
                        suggest_similar(&name, vars_fields)
                            .into_iter()
                            .map(|s| format!("vars.{}", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            let ident = Ident::new(&name, r.name.span());
            let is_opt = vars_opt_set.contains(&name);
            if is_opt {
                Ok(quote! {
                    if let ::core::option::Option::Some(v) = &vars.#ident {
                        let __s = ::std::string::ToString::to_string(v);
                        let __hv = ::http::header::HeaderValue::from_str(&__s)
                            .map_err(|e| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                        p.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                    }
                })
            } else {
                Ok(quote! {
                    {
                        let __s = ::std::string::ToString::to_string(&vars.#ident);
                        let __hv = ::http::header::HeaderValue::from_str(&__s)
                            .map_err(|e| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                        p.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                    }
                })
            }
        }
        parse::ValueExpr::Decl(decl) => Err(syn::Error::new(
            decl.name.span(),
            "client headers cannot declare endpoint placeholders",
        )),
        parse::ValueExpr::Format { atoms, mode: _ } => {
            // client header placeholders must refer to vars only
            let gates =
                emit_atoms_optional_gates_client(atoms, vars_fields, vars_opt_set, quote!(vars))?;
            let build = emit_atoms_build_client(atoms, vars_fields, vars_opt_set, quote!(vars))?;
            if gates.is_empty() {
                Ok(quote! {
                    {
                        let mut __s = ::std::string::String::new();
                        #build
                        let __hv = ::http::header::HeaderValue::from_str(&__s)
                            .map_err(|e| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                        p.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                    }
                })
            } else {
                Ok(quote! {
                    if #(#gates)&&* {
                        let mut __s = ::std::string::String::new();
                        #build
                       let __hv = ::http::header::HeaderValue::from_str(&__s)
                            .map_err(|e| ::concord_core::prelude::ApiClientError::InvalidParam(concat!("header:", #header_name)))?;
                        p.insert_header(::http::header::HeaderName::from_static(#header_name), __hv);
                    }
                })
            }
        }
    }
}

// --------------------- misc helpers ---------------------
fn scheme_to_http_scheme(s: &Ident) -> syn::Result<TokenStream2> {
    let v = s.to_string();
    match v.as_str() {
        "https" => Ok(quote!(::http::uri::Scheme::HTTPS)),
        "http" => Ok(quote!(::http::uri::Scheme::HTTP)),
        _ => Err(syn::Error::new_spanned(s, "scheme must be http or https")),
    }
}
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
fn vars_optional_set(ir: &Ir) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for v in &ir.vars {
        if v.optional {
            s.insert(v.name.to_string().to_snake_case());
        }
    }
    s
}

fn collect_decl_fields_from_route_expr(
    r: &parse::RouteExpr,
    ctx: RouteCtx,
    fields: &mut BTreeMap<String, FieldDef>,
    order: &mut Vec<String>,
    route_decl_order: &mut Vec<String>,
) -> syn::Result<()> {
    match r {
        parse::RouteExpr::Static(_) => Ok(()),
        parse::RouteExpr::Segments(segs) => {
            for seg in segs {
                for a in &seg.atoms {
                    if let parse::Atom::Param(ph) = a
                        && ph.is_decl()
                    {
                        let eff = ph.alias.as_ref().unwrap_or(&ph.name);
                        let name_snake = eff.to_string().to_snake_case();
                        let orig = eff.to_string();
                        add_field(
                            fields,
                            order,
                            &name_snake,
                            ph.ty.clone(),
                            ph.optional,
                            ph.default.clone(),
                            ph.name.span(),
                            orig,
                        )?;
                        if matches!(ctx, RouteCtx::Host | RouteCtx::Path)
                            && !route_decl_order.contains(&name_snake)
                        {
                            route_decl_order.push(name_snake);
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

fn collect_decl_fields_from_atoms(
    atoms: &[parse::Atom],
    _ctx: RouteCtx,
    fields: &mut BTreeMap<String, FieldDef>,
    order: &mut Vec<String>,
) -> syn::Result<()> {
    for a in atoms {
        if let parse::Atom::Param(ph) = a
            && ph.is_decl()
        {
            let eff = ph.alias.as_ref().unwrap_or(&ph.name);
            let name_snake = eff.to_string().to_snake_case();
            let orig = eff.to_string();
            add_field(
                fields,
                order,
                &name_snake,
                ph.ty.clone(),
                ph.optional,
                ph.default.clone(),
                ph.name.span(),
                orig,
            )?;
        }
    }
    Ok(())
}

fn validate_route_expr_refs(
    r: &parse::RouteExpr,
    ctx: RouteCtx,
    vars: &BTreeSet<String>,
    eps: &BTreeSet<String>,
) -> syn::Result<()> {
    match r {
        parse::RouteExpr::Static(_) => Ok(()),
        parse::RouteExpr::Segments(segs) => {
            for seg in segs {
                validate_atoms_refs(&seg.atoms, ctx, vars, eps)?;
            }
            Ok(())
        }
    }
}

fn validate_atoms_refs(
    atoms: &[parse::Atom],
    _ctx: RouteCtx,
    vars: &BTreeSet<String>,
    eps: &BTreeSet<String>,
) -> syn::Result<()> {
    for a in atoms {
        match a {
            parse::Atom::Lit(_) => {}
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                match r.scope {
                    Some(parse::RefScope::Vars) => {
                        if !vars.contains(&name) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars,
                                eps,
                                Some(parse::RefScope::Vars),
                            ));
                        }
                    }
                    Some(parse::RefScope::Ep) => {
                        if !eps.contains(&name) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars,
                                eps,
                                Some(parse::RefScope::Ep),
                            ));
                        }
                    }
                    None => {
                        let in_vars = vars.contains(&name);
                        let in_eps = eps.contains(&name);
                        if in_vars && in_eps {
                            return Err(err_ambiguous_placeholder(r.name.span(), &name));
                        }
                        if !(in_vars || in_eps) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars,
                                eps,
                                None,
                            ));
                        }
                    }
                }
            }
            parse::Atom::Param(ph) => {
                let name_snake = ph.name.to_string().to_snake_case();
                if !ph.is_decl() {
                    let in_vars = vars.contains(&name_snake);
                    let in_eps = eps.contains(&name_snake);
                    if in_vars && in_eps {
                        return Err(err_ambiguous_placeholder(ph.name.span(), &name_snake));
                    }
                    if !(in_vars || in_eps) {
                        return Err(err_unknown_placeholder(
                            ph.name.span(),
                            &name_snake,
                            vars,
                            eps,
                            None,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn atoms_static_string(atoms: &[parse::Atom]) -> Option<LitStr> {
    let mut acc = String::new();
    let mut span = None;
    for a in atoms {
        match a {
            parse::Atom::Lit(l) => {
                span.get_or_insert(l.span());
                acc.push_str(&l.value());
            }
            parse::Atom::Param(_) | parse::Atom::Ref(_) => return None,
        }
    }
    Some(LitStr::new(&acc, span.unwrap_or_else(Span::call_site)))
}

#[allow(clippy::too_many_arguments)]
fn emit_atoms_build_expr(
    atoms: &[parse::Atom],
    ctx: RouteCtx,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
    ep_expr: TokenStream2,
) -> syn::Result<TokenStream2> {
    let mut stmts: Vec<TokenStream2> = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(l) => {
                let s = l.value();
                if !s.is_empty() {
                    stmts.push(quote! { __s.push_str(#s); });
                }
            }
            parse::Atom::Ref(r) => {
                let (src, is_opt) = resolve_ref_source(
                    r,
                    vars_set,
                    vars_opt_set,
                    ep_set,
                    ep_opt_set,
                    vars_expr.clone(),
                    ep_expr.clone(),
                )?;
                if is_opt {
                    if matches!(ctx, RouteCtx::Host | RouteCtx::Path) {
                        return Err(syn::Error::new(
                            r.name.span(),
                            "v1.5: optional values cannot be used in routes",
                        ));
                    }
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = &#src {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                } else {
                    let _ = r;
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", #src);
                    });
                }
            }
            parse::Atom::Param(ph) => {
                let name = ph.name.to_string().to_snake_case();
                let ident = Ident::new(&name, ph.name.span());
                let (src, is_opt) = if ph.is_decl() {
                    (quote!(#ep_expr.#ident), ep_opt_set.contains(&name))
                } else {
                    resolve_name_source(
                        &name,
                        ph.name.span(),
                        vars_set,
                        vars_opt_set,
                        ep_set,
                        ep_opt_set,
                        vars_expr.clone(),
                        ep_expr.clone(),
                    )?
                };

                if is_opt {
                    if matches!(ctx, RouteCtx::Host | RouteCtx::Path) {
                        return Err(syn::Error::new(
                            ph.name.span(),
                            "v1.5: optional values cannot be used in routes",
                        ));
                    }
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = &#src {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                } else {
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", #src);
                    });
                }
            }
        }
    }
    Ok(quote! { #(#stmts)* })
}

#[allow(clippy::too_many_arguments)]
fn resolve_name_source(
    name_snake: &str,
    span: Span,
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
    ep_expr: TokenStream2,
) -> syn::Result<(TokenStream2, bool)> {
    let in_vars = vars_set.contains(name_snake);
    let in_eps = ep_set.contains(name_snake);
    if in_vars && in_eps {
        return Err(err_ambiguous_placeholder(span, name_snake));
    }
    let ident = Ident::new(name_snake, span);
    if in_vars {
        Ok((quote!(#vars_expr.#ident), vars_opt_set.contains(name_snake)))
    } else if in_eps {
        Ok((quote!(#ep_expr.#ident), ep_opt_set.contains(name_snake)))
    } else {
        Err(err_unknown_placeholder(
            span, name_snake, vars_set, ep_set, None,
        ))
    }
}

fn emit_atoms_optional_gates(
    atoms: &[parse::Atom],
    vars_set: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    ep_set: &BTreeSet<String>,
    ep_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
    ep_expr: TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    let mut gates = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(_) => {}
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                match r.scope {
                    Some(parse::RefScope::Vars) => {
                        if !vars_set.contains(&name) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars_set,
                                ep_set,
                                Some(parse::RefScope::Vars),
                            ));
                        }
                        if vars_opt_set.contains(&name) {
                            let ident = Ident::new(&name, r.name.span());
                            gates.push(quote!(#vars_expr.#ident.is_some()));
                        }
                    }
                    Some(parse::RefScope::Ep) => {
                        if !ep_set.contains(&name) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars_set,
                                ep_set,
                                Some(parse::RefScope::Ep),
                            ));
                        }
                        if ep_opt_set.contains(&name) {
                            let ident = Ident::new(&name, r.name.span());
                            gates.push(quote!(#ep_expr.#ident.is_some()));
                        }
                    }
                    None => {
                        let in_vars = vars_set.contains(&name);
                        let in_eps = ep_set.contains(&name);
                        if in_vars && in_eps {
                            return Err(err_ambiguous_placeholder(r.name.span(), &name));
                        }
                        if in_vars && vars_opt_set.contains(&name) {
                            let ident = Ident::new(&name, r.name.span());
                            gates.push(quote!(#vars_expr.#ident.is_some()));
                        } else if in_eps && ep_opt_set.contains(&name) {
                            let ident = Ident::new(&name, r.name.span());
                            gates.push(quote!(#ep_expr.#ident.is_some()));
                        } else if !(in_vars || in_eps) {
                            return Err(err_unknown_placeholder(
                                r.name.span(),
                                &name,
                                vars_set,
                                ep_set,
                                None,
                            ));
                        }
                    }
                }
            }
            parse::Atom::Param(ph) => {
                let name = ph.name.to_string().to_snake_case();
                let ident = Ident::new(&name, ph.name.span());
                if ph.is_decl() {
                    if ep_opt_set.contains(&name) {
                        gates.push(quote!(#ep_expr.#ident.is_some()));
                    }
                } else {
                    let in_vars = vars_set.contains(&name);
                    let in_eps = ep_set.contains(&name);
                    if in_vars && in_eps {
                        return Err(syn::Error::new(
                            ph.name.span(),
                            format!(
                                "ambiguous placeholder `{}` (exists in vars and endpoint fields)",
                                name
                            ),
                        ));
                    }
                    if in_vars && vars_opt_set.contains(&name) {
                        gates.push(quote!(#vars_expr.#ident.is_some()));
                    } else if in_eps && ep_opt_set.contains(&name) {
                        gates.push(quote!(#ep_expr.#ident.is_some()));
                    } else if !(in_vars || in_eps) {
                        return Err(syn::Error::new(
                            ph.name.span(),
                            format!("unknown placeholder `{}`", name),
                        ));
                    }
                }
            }
        }
    }
    Ok(gates)
}

fn emit_atoms_build_client(
    atoms: &[parse::Atom],
    vars_fields: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
) -> syn::Result<TokenStream2> {
    let mut stmts = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(l) => {
                let s = l.value();
                if !s.is_empty() {
                    stmts.push(quote! { __s.push_str(#s); });
                }
            }
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                if !vars_fields.contains(&name) {
                    return Err(syn::Error::new(
                        r.name.span(),
                        format!("unknown client var `{}`", name),
                    ));
                }
                let ident = Ident::new(&name, r.name.span());
                if vars_opt_set.contains(&name) {
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = &#vars_expr.#ident {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                } else {
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", #vars_expr.#ident);
                    });
                }
            }
            parse::Atom::Param(ph) => {
                if ph.is_decl() {
                    return Err(syn::Error::new(
                        ph.name.span(),
                        "v1.5: client header placeholders must be references (no decls)",
                    ));
                }
                let name = ph.name.to_string().to_snake_case();
                if !vars_fields.contains(&name) {
                    return Err(syn::Error::new(
                        ph.name.span(),
                        format!("unknown client var `{}`", name),
                    ));
                }
                let ident = Ident::new(&name, ph.name.span());
                if vars_opt_set.contains(&name) {
                    stmts.push(quote! {
                        if let ::core::option::Option::Some(v) = &#vars_expr.#ident {
                            use ::core::fmt::Write as _;
                            let _ = write!(__s, "{}", v);
                        }
                    });
                } else {
                    stmts.push(quote! {
                        use ::core::fmt::Write as _;
                        let _ = write!(__s, "{}", #vars_expr.#ident);
                    });
                }
            }
        }
    }
    Ok(quote! { #(#stmts)* })
}

fn emit_atoms_optional_gates_client(
    atoms: &[parse::Atom],
    vars_fields: &BTreeSet<String>,
    vars_opt_set: &BTreeSet<String>,
    vars_expr: TokenStream2,
) -> syn::Result<Vec<TokenStream2>> {
    let mut gates = Vec::new();
    for a in atoms {
        match a {
            parse::Atom::Lit(_) => {}
            parse::Atom::Ref(r) => {
                let name = r.name.to_string().to_snake_case();
                if !vars_fields.contains(&name) {
                    return Err(syn::Error::new(
                        r.name.span(),
                        format!("unknown client var `{}`", name),
                    ));
                }
                if vars_opt_set.contains(&name) {
                    let ident = Ident::new(&name, r.name.span());
                    gates.push(quote!(#vars_expr.#ident.is_some()));
                }
            }
            parse::Atom::Param(ph) => {
                if ph.is_decl() {
                    return Err(syn::Error::new(
                        ph.name.span(),
                        "v1.5: client header placeholders must be references (no decls)",
                    ));
                }
                let name = ph.name.to_string().to_snake_case();
                if !vars_fields.contains(&name) {
                    return Err(syn::Error::new(
                        ph.name.span(),
                        format!("unknown client var `{}`", name),
                    ));
                }
                if vars_opt_set.contains(&name) {
                    let ident = Ident::new(&name, ph.name.span());
                    gates.push(quote!(#vars_expr.#ident.is_some()));
                }
            }
        }
    }
    Ok(gates)
}

fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn sanitize_ident_piece(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_';
        let c = if ok { ch } else { '_' };
        if i == 0 && c.is_ascii_digit() {
            out.push('_');
        }
        out.push(c);
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn placeholder_eff_snake(ph: &parse::PlaceholderDecl) -> String {
    let eff = ph.alias.as_ref().unwrap_or(&ph.name);
    eff.to_string().to_snake_case()
}

fn atom_placeholder_snake(a: &parse::Atom) -> Option<String> {
    match a {
        parse::Atom::Ref(r) => Some(r.name.to_string().to_snake_case()),
        parse::Atom::Param(ph) => {
            // decl uses alias; ref cannot use alias by grammar
            if ph.is_decl() {
                Some(placeholder_eff_snake(ph))
            } else {
                Some(ph.name.to_string().to_snake_case())
            }
        }
        parse::Atom::Lit(_) => None,
    }
}

fn is_empty_route_expr(r: &parse::RouteExpr) -> bool {
    match r {
        parse::RouteExpr::Static(lit) => {
            let s = lit.value();
            s.trim().is_empty() || s.trim() == "/"
        }
        parse::RouteExpr::Segments(segs) => segs.is_empty(),
    }
}

fn route_expr_key(
    mode: SharedRouteMode,
    r: &parse::RouteExpr,
    vars_set: &BTreeSet<String>,
) -> String {
    // stable structural key including placeholder origin (vars vs endpoint field)
    let mut out = String::new();
    out.push_str(match mode {
        SharedRouteMode::Host => "H|",
        SharedRouteMode::Path => "P|",
    });
    match r {
        parse::RouteExpr::Static(lit) => {
            out.push_str("S:");
            out.push_str(&lit.value());
        }
        parse::RouteExpr::Segments(segs) => {
            for seg in segs {
                out.push_str("SEG|");
                for a in &seg.atoms {
                    match a {
                        parse::Atom::Lit(l) => {
                            out.push_str("L:");
                            out.push_str(&l.value());
                            out.push('|');
                        }
                        parse::Atom::Ref(id) => {
                            let n = id.name.to_string().to_snake_case();
                            let is_var = match id.scope {
                                Some(parse::RefScope::Vars) => true,
                                Some(parse::RefScope::Ep) => false,
                                None => vars_set.contains(&n),
                            };
                            out.push_str(if is_var { "V:" } else { "E:" });
                            out.push_str(&n);
                            out.push('|');
                        }
                        parse::Atom::Param(ph) => {
                            let n = if ph.is_decl() {
                                placeholder_eff_snake(ph)
                            } else {
                                ph.name.to_string().to_snake_case()
                            };
                            out.push_str(if vars_set.contains(&n) && !ph.is_decl() {
                                "V:"
                            } else {
                                "E:"
                            });
                            out.push_str(&n);
                            out.push('|');
                        }
                    }
                }
            }
        }
    }
    out
}

fn route_expr_name_hint(r: &parse::RouteExpr) -> String {
    let mut pieces: Vec<String> = Vec::new();
    match r {
        parse::RouteExpr::Static(lit) => {
            let v = lit.value();
            let p = sanitize_ident_piece(&v);
            if !p.is_empty() {
                pieces.push(p);
            }
        }
        parse::RouteExpr::Segments(segs) => {
            for seg in segs {
                // atoms -> piece
                let mut seg_pieces: Vec<String> = Vec::new();
                for a in &seg.atoms {
                    match a {
                        parse::Atom::Lit(l) => {
                            let p = sanitize_ident_piece(&l.value());
                            if !p.is_empty() {
                                seg_pieces.push(p);
                            }
                        }
                        parse::Atom::Ref(id) => {
                            let p = sanitize_ident_piece(&id.name.to_string().to_snake_case());
                            if !p.is_empty() {
                                seg_pieces.push(p);
                            }
                        }
                        parse::Atom::Param(ph) => {
                            let n = if ph.is_decl() {
                                placeholder_eff_snake(ph)
                            } else {
                                ph.name.to_string().to_snake_case()
                            };
                            let p = sanitize_ident_piece(&n);
                            if !p.is_empty() {
                                seg_pieces.push(p);
                            }
                        }
                    }
                }
                if seg_pieces.is_empty() {
                    continue;
                }
                pieces.push(seg_pieces.join("__"));
            }
        }
    }
    if pieces.is_empty() {
        "root".to_string()
    } else {
        pieces.join("__")
    }
}

fn collect_route_deps(r: &parse::RouteExpr, vars_set: &BTreeSet<String>) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    match r {
        parse::RouteExpr::Static(_) => {}
        parse::RouteExpr::Segments(segs) => {
            for seg in segs {
                for a in &seg.atoms {
                    if let Some(n) = atom_placeholder_snake(a) {
                        // decl always endpoint
                        let is_decl = matches!(a, parse::Atom::Param(ph) if ph.is_decl());
                        if is_decl || !vars_set.contains(&n) {
                            deps.insert(n);
                        }
                    }
                }
            }
        }
    }
    deps
}

fn value_expr_key(v: &parse::ValueExpr) -> String {
    match v {
        parse::ValueExpr::Lit(l) => format!("L:{}", l.value()),
        parse::ValueExpr::Ref(r) => {
            let pfx = match r.scope {
                Some(parse::RefScope::Vars) => "vars.",
                Some(parse::RefScope::Ep) => "ep.",
                None => "",
            };
            format!("R:{}{}", pfx, r.name)
        }
        parse::ValueExpr::Decl(d) => {
            let eff = d.alias.as_ref().unwrap_or(&d.name);
            format!("D:{}:?{}:={}", eff, d.optional, d.default.is_some())
        }
        parse::ValueExpr::Format { atoms, mode } => {
            let mut s = String::new();
            s.push_str(match mode {
                parse::FormatMode::GateEntry => "F:G|",
                parse::FormatMode::Partial => "F:P|",
            });
            for a in atoms {
                match a {
                    parse::Atom::Lit(l) => {
                        s.push_str("L:");
                        s.push_str(&l.value());
                        s.push('|');
                    }
                    parse::Atom::Ref(r) => {
                        s.push_str("R:");
                        s.push_str(&r.name.to_string());
                        s.push('|');
                    }
                    parse::Atom::Param(ph) => {
                        let n = if ph.is_decl() {
                            placeholder_eff_snake(ph)
                        } else {
                            ph.name.to_string().to_snake_case()
                        };
                        s.push_str("P:");
                        s.push_str(&n);
                        s.push('|');
                    }
                }
            }
            s
        }
    }
}

fn policy_node_key(node: &IrPolicyNode, vars_set: &BTreeSet<String>) -> String {
    // includes node kind, template structure, and policy content
    let mut s = String::new();
    s.push_str(match node.kind {
        IrPolicyNodeKind::Prefix => "K:Prefix|",
        IrPolicyNodeKind::Path => "K:Path|",
    });
    // template key without placeholder origin enforcement (naming uses template anyway)
    s.push_str("T:");
    s.push_str(&route_expr_key(
        match node.kind {
            IrPolicyNodeKind::Prefix => SharedRouteMode::Host,
            IrPolicyNodeKind::Path => SharedRouteMode::Path,
        },
        &node.template,
        vars_set,
    ));
    s.push('|');
    for h in &node.headers {
        match h {
            parse::HeaderRule::Remove { name } => {
                s.push_str("HR:");
                s.push_str(&name.value());
                s.push('|');
            }
            parse::HeaderRule::Set { name, value } => {
                s.push_str("HS:");
                s.push_str(&name.value());
                s.push('=');
                s.push_str(&value_expr_key(value));
                s.push('|');
            }
        }
    }
    for q in &node.query {
        match q {
            parse::QueryEntry::Remove { key } => {
                s.push_str("QR:");
                s.push_str(&key.value());
                s.push('|');
            }
            parse::QueryEntry::Set { key, value } => {
                s.push_str("QS:");
                s.push_str(&key.value());
                s.push('=');
                s.push_str(&value_expr_key(value));
                s.push('|');
            }
            parse::QueryEntry::Push { key, value } => {
                s.push_str("QP:");
                s.push_str(&key.value());
                s.push('=');
                s.push_str(&value_expr_key(value));
                s.push('|');
            }
        }
    }
    if let Some(t) = &node.timeout {
        s.push_str("TO:");
        s.push_str(&norm_expr(t));
        s.push('|');
    }
    s
}

fn collect_policy_deps(node: &IrPolicyNode, vars_set: &BTreeSet<String>) -> BTreeSet<String> {
    fn atoms_deps(atoms: &[parse::Atom], vars_set: &BTreeSet<String>, out: &mut BTreeSet<String>) {
        for a in atoms {
            if let Some(n) = atom_placeholder_snake(a) {
                let is_decl = matches!(a, parse::Atom::Param(ph) if ph.is_decl());
                if is_decl || !vars_set.contains(&n) {
                    out.insert(n);
                }
            }
        }
    }
    fn value_deps(v: &parse::ValueExpr, vars_set: &BTreeSet<String>, out: &mut BTreeSet<String>) {
        match v {
            parse::ValueExpr::Lit(_) => {}
            parse::ValueExpr::Ref(r) => {
                let n = r.name.to_string().to_snake_case();
                let is_var = match r.scope {
                    Some(parse::RefScope::Vars) => true,
                    Some(parse::RefScope::Ep) => false,
                    None => vars_set.contains(&n),
                };
                if !is_var {
                    out.insert(n);
                }
            }
            parse::ValueExpr::Decl(d) => {
                out.insert(
                    d.alias
                        .as_ref()
                        .unwrap_or(&d.name)
                        .to_string()
                        .to_snake_case(),
                );
            }
            parse::ValueExpr::Format { atoms, .. } => atoms_deps(atoms, vars_set, out),
        }
    }
    let mut deps = BTreeSet::new();
    for h in &node.headers {
        if let parse::HeaderRule::Set { value, .. } = h {
            value_deps(value, vars_set, &mut deps);
        }
    }
    for q in &node.query {
        match q {
            parse::QueryEntry::Remove { .. } => {}
            parse::QueryEntry::Set { value, .. } | parse::QueryEntry::Push { value, .. } => {
                value_deps(value, vars_set, &mut deps);
            }
        }
    }
    deps
}

struct SharedRoutePart {
    mode: SharedRouteMode,
    #[allow(unused)]
    key: String,
    ident: Ident,
    route: parse::RouteExpr,
    deps: Vec<String>,
}

struct SharedPolicyPart {
    #[allow(unused)]
    key: String,
    ident: Ident,
    node: IrPolicyNode,
    deps: Vec<String>,
}

struct SharedRegistry {
    route_parts: HashMap<String, SharedRoutePart>,
    policy_parts: HashMap<String, SharedPolicyPart>,
    field_names: BTreeSet<String>,
}

impl SharedRegistry {
    fn build(ir: &Ir) -> syn::Result<Self> {
        let vars_set = vars_name_set(ir);
        let mut used_idents: HashSet<String> = HashSet::new();
        let mut route_parts: HashMap<String, SharedRoutePart> = HashMap::new();
        let mut policy_parts: HashMap<String, SharedPolicyPart> = HashMap::new();
        let mut field_names: BTreeSet<String> = BTreeSet::new();

        let mut alloc_ident = |base: String, key: &str| -> Ident {
            let mut name = base;
            if name.len() > 90 {
                let h = fnv1a64(key);
                name = format!("{}_h{:08x}", &name[..80], (h as u32));
            }
            if used_idents.contains(&name) {
                let h = fnv1a64(key);
                name = format!("{}_h{:08x}", name, (h as u32));
            }
            used_idents.insert(name.clone());
            format_ident!("{}", name)
        };

        for ep in &ir.endpoints {
            // host prefix
            for r in &ep.full_host_prefix {
                let key = route_expr_key(SharedRouteMode::Host, r, &vars_set);
                if !route_parts.contains_key(&key) {
                    let hint = route_expr_name_hint(r);
                    let base = format!("RouteHost__{}", hint);
                    let ident = alloc_ident(base, &key);
                    let deps = collect_route_deps(r, &vars_set)
                        .into_iter()
                        .collect::<Vec<_>>();
                    for d in &deps {
                        field_names.insert(d.clone());
                    }
                    route_parts.insert(
                        key.clone(),
                        SharedRoutePart {
                            mode: SharedRouteMode::Host,
                            key: key.clone(),
                            ident,
                            route: r.clone(),
                            deps,
                        },
                    );
                }
            }
            // path prefix
            for r in &ep.full_path_prefix {
                let key = route_expr_key(SharedRouteMode::Path, r, &vars_set);
                if !route_parts.contains_key(&key) {
                    let hint = route_expr_name_hint(r);
                    let base = format!("RoutePath__{}", hint);
                    let ident = alloc_ident(base, &key);
                    let deps = collect_route_deps(r, &vars_set)
                        .into_iter()
                        .collect::<Vec<_>>();
                    for d in &deps {
                        field_names.insert(d.clone());
                    }
                    route_parts.insert(
                        key.clone(),
                        SharedRoutePart {
                            mode: SharedRouteMode::Path,
                            key: key.clone(),
                            ident,
                            route: r.clone(),
                            deps,
                        },
                    );
                }
            }
            // endpoint path (skip empty)
            if !is_empty_route_expr(&ep.endpoint_path) {
                let key = route_expr_key(SharedRouteMode::Path, &ep.endpoint_path, &vars_set);
                if !route_parts.contains_key(&key) {
                    let hint = route_expr_name_hint(&ep.endpoint_path);
                    let base = format!("RoutePath__{}", hint);
                    let ident = alloc_ident(base, &key);
                    let deps = collect_route_deps(&ep.endpoint_path, &vars_set)
                        .into_iter()
                        .collect::<Vec<_>>();
                    for d in &deps {
                        field_names.insert(d.clone());
                    }
                    route_parts.insert(
                        key.clone(),
                        SharedRoutePart {
                            mode: SharedRouteMode::Path,
                            key: key.clone(),
                            ident,
                            route: ep.endpoint_path.clone(),
                            deps,
                        },
                    );
                }
            }

            // policy nodes (prefix/path) - only if non-empty
            for node in &ep.full_policy_prefix {
                let has_any =
                    !node.headers.is_empty() || !node.query.is_empty() || node.timeout.is_some();
                if !has_any {
                    continue;
                }
                let key = policy_node_key(node, &vars_set);
                if !policy_parts.contains_key(&key) {
                    let hint = route_expr_name_hint(&node.template);
                    let base = match node.kind {
                        IrPolicyNodeKind::Prefix => format!("PolicyHost__{}", hint),
                        IrPolicyNodeKind::Path => format!("PolicyPath__{}", hint),
                    };
                    let ident = alloc_ident(base, &key);
                    let deps = collect_policy_deps(node, &vars_set)
                        .into_iter()
                        .collect::<Vec<_>>();
                    for d in &deps {
                        field_names.insert(d.clone());
                    }
                    policy_parts.insert(
                        key.clone(),
                        SharedPolicyPart {
                            key: key.clone(),
                            ident,
                            node: node.clone(),
                            deps,
                        },
                    );
                }
            }
        }

        Ok(Self {
            route_parts,
            policy_parts,
            field_names,
        })
    }

    fn route_part_ty(
        &self,
        mode: SharedRouteMode,
        r: &parse::RouteExpr,
        vars_set: &BTreeSet<String>,
    ) -> TokenStream2 {
        let key = route_expr_key(mode, r, vars_set);
        let p = self
            .route_parts
            .get(&key)
            .expect("shared route part missing");
        let ident = &p.ident;
        quote!(__shared::#ident)
    }

    fn policy_part_ty(&self, node: &IrPolicyNode, vars_set: &BTreeSet<String>) -> TokenStream2 {
        let key = policy_node_key(node, vars_set);
        let p = self
            .policy_parts
            .get(&key)
            .expect("shared policy part missing");
        let ident = &p.ident;
        quote!(__shared::#ident)
    }

    fn field_trait_ident(name: &str) -> Ident {
        format_ident!("Field_{}", name)
    }

    fn emit_shared_module(&self, ir: &Ir, cx_name: &Ident) -> syn::Result<TokenStream2> {
        let vars_set = vars_name_set(ir);
        let vars_opt_set = vars_optional_set(ir);

        // 1) Field traits
        let mut field_traits: Vec<TokenStream2> = Vec::new();
        for f in &self.field_names {
            let tr = Self::field_trait_ident(f);
            let msg = LitStr::new(&format!("missing field `{}`", f), Span::call_site());
            field_traits.push(quote! {
                pub trait #tr {
                    type Ty: ::core::fmt::Display;
                    fn get_opt(ep: &Self) -> ::core::option::Option<&Self::Ty>;
                    #[inline]
                    fn get(ep: &Self) -> &Self::Ty {
                        Self::get_opt(ep).expect(#msg)
                    }
                }
            });
        }

        // 2) Shared RouteParts
        let mut route_defs: Vec<TokenStream2> = Vec::new();
        for p in self.route_parts.values() {
            let ident = &p.ident;
            let deps: Vec<Ident> = p.deps.iter().map(|d| Self::field_trait_ident(d)).collect();

            let pushes = match p.mode {
                SharedRouteMode::Host => {
                    emit_route_expr_pushes_host_shared(&p.route, &vars_set, &vars_opt_set, cx_name)?
                }
                SharedRouteMode::Path => {
                    emit_route_expr_pushes_path_shared(&p.route, &vars_set, &vars_opt_set, cx_name)?
                }
            };
            route_defs.push(quote! {
                pub struct #ident;
                impl<Cx, E> ::concord_core::internal::RoutePart<Cx, E> for #ident
                    where
                        Cx: ::concord_core::prelude::ClientContext
                        #(, E: #deps)*
                    {
                        fn apply(
                            ep: &E,
                            client: &::concord_core::prelude::ApiClient<Cx>,
                            route: &mut ::concord_core::prelude::RouteParts,
                        ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                            let _ = ep;
                            let _ = client;
                            #(#pushes)*
                            ::core::result::Result::Ok(())
                        }
                    }
            });
        }

        // 3) Shared PolicyParts (prefix/path only)
        let mut policy_defs: Vec<TokenStream2> = Vec::new();
        for p in self.policy_parts.values() {
            let ident = &p.ident;
            let deps: Vec<Ident> = p.deps.iter().map(|d| Self::field_trait_ident(d)).collect();
            let stmts = emit_policy_node_apply_stmts_shared(&p.node, &vars_set, &vars_opt_set)?;
            policy_defs.push(quote! {
                pub struct #ident;
                impl<Cx, E> ::concord_core::internal::PolicyPart<Cx, E> for #ident
                    where
                        Cx: ::concord_core::prelude::ClientContext
                        #(, E: #deps)*
                    {
                    fn apply(
                        ep: &E,
                        client: &::concord_core::prelude::ApiClient<Cx>,
                        policy: &mut ::concord_core::prelude::Policy,
                    ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                        let _ = ep;
                        let _ = client;
                        #(#stmts)*
                        ::core::result::Result::Ok(())
                    }
                }
            });
        }

        Ok(quote! {
            #(#field_traits)*
            #(#route_defs)*
            #(#policy_defs)*
        })
    }
}
