fn endpoint_http_method_ident(ep: &ResolvedEndpoint) -> Ident {
    ep.method.clone()
}

fn emit_endpoint_def(
    resolved_api: &ResolvedApi,
    facade: &FacadeEndpoint,
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let endpoint_docs = facade_ir_endpoint_docs(facade, facade.public_method.span());
    let facade_setters_by_field: std::collections::BTreeMap<String, usize> = facade
        .setters
        .iter()
        .enumerate()
        .map(|(idx, setter)| (setter.field.to_string(), idx))
        .collect();
    let endpoint_vars_by_field: std::collections::BTreeMap<String, usize> = ep
        .vars
        .iter()
        .enumerate()
        .map(|(idx, var)| (var.rust.to_string(), idx))
        .collect();

    let mut fields_ts = Vec::new();
    let mut setters_ts = Vec::new();
    for v in &ep.vars {
        let f = &v.rust;
        let ty = &v.ty;
        if v.optional {
            fields_ts.push(quote! { pub(crate) #f: ::core::option::Option<#ty> });
            if let Some(setter) = facade_setter_for_var(facade, &facade_setters_by_field, f) {
                let set = setter.set_name.clone();
                let opt = setter.set_optional_name.clone();
                let clear = setter.clear_name.clone();
                let set_doc = LitStr::new(&setter.set_doc, f.span());
                let opt_doc = LitStr::new(&setter.set_optional_doc, f.span());
                let clear_doc = LitStr::new(&setter.clear_doc, f.span());
                setters_ts.push(quote! {
                    #[doc = #set_doc]
                    #[inline]
                    pub fn #set(mut self, v: #ty) -> Self { self.#f = ::core::option::Option::Some(v); self }
                    #[doc = #opt_doc]
                    #[inline]
                    pub fn #opt(mut self, v: ::core::option::Option<#ty>) -> Self { self.#f = v; self }
                    #[doc = #clear_doc]
                    #[inline]
                    pub fn #clear(mut self) -> Self { self.#f = ::core::option::Option::None; self }
                });
            }
        } else {
            fields_ts.push(quote! { pub(crate) #f: #ty });
            if let Some(default) = &v.default {
                if let Some(setter) = facade_setter_for_var(facade, &facade_setters_by_field, f) {
                    let set = setter.set_name.clone();
                    let opt = setter.set_optional_name.clone();
                    let clear = setter.clear_name.clone();
                    let set_doc = LitStr::new(&setter.set_doc, f.span());
                    let opt_doc = LitStr::new(&setter.set_optional_doc, f.span());
                    let clear_doc = LitStr::new(&setter.clear_doc, f.span());
                    setters_ts.push(quote! {
                        #[doc = #set_doc]
                        #[inline]
                        pub fn #set(mut self, v: #ty) -> Self { self.#f = v; self }
                        #[doc = #opt_doc]
                        #[inline]
                        pub fn #opt(mut self, v: ::core::option::Option<#ty>) -> Self { self.#f = v.unwrap_or_else(|| #default); self }
                        #[doc = #clear_doc]
                        #[inline]
                        pub fn #clear(mut self) -> Self { self.#f = #default; self }
                    });
                }
            } else {
                setters_ts.push(quote! {});
            }
        }
    }

    let required_vars: Vec<&VarInfo> = ep
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
    let body_inner_ty = match endpoint_request_body_inner_ty(ep) {
        Ok(body_ty) => body_ty,
        Err(err) => return err,
    };
    let mut struct_fields: Vec<TokenStream2> = fields_ts;
    if let Some(body_ty) = &body_inner_ty {
        struct_fields.push(quote! { pub(crate) body: #body_ty });
    }
    let mut fn_args: Vec<TokenStream2> = required_vars
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();
    if let Some(body_ty) = &body_inner_ty {
        fn_args.push(quote! { body: #body_ty });
    }
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
            match &v.default {
                Some(d) => quote! { #f: #d },
                None => {
                    let err = emit_helpers::compile_error_expr(
                        "required endpoint variable default was missing in resolved IR",
                        f.span(),
                    );
                    quote! { #f: #err }
                }
            }
        }
    });
    let mut init_parts: Vec<TokenStream2> = init_fields.collect();
    if body_inner_ty.is_some() {
        init_parts.push(quote! { body });
    }

    let final_response_ty = endpoint_response_output_ty(ep);
    let execute_override = endpoint_execute_override(ep, ty_name, cx_ty);
    let response_terminal_impl = endpoint_response_terminal_impl(ep, ty_name, cx_ty);
    let plan_impl = endpoint_plan_impl(
        resolved_api,
        ep,
        ty_name,
        cx_ty,
        body_inner_ty.is_some(),
    );

    let paginate_binding_impl = emit_paginate_binding_impl(ep, ty_name);
    let pagination_marker_impl = emit_paginated_endpoint_impl(ep, ty_name, cx_ty);
    let pending_ext_trait = endpoint_pending_ext_trait_ident(ep);
    let endpoint_descriptor = emit_endpoint_descriptor(resolved_api, ep);
    let pending_setter_decls: Vec<TokenStream2> = facade
        .setters
        .iter()
        .filter_map(|setter| {
            let v = endpoint_var_for_setter(ep, &endpoint_vars_by_field, setter)?;
            let f = &v.rust;
            let ty = &v.ty;
            let set = setter.set_name.clone();
            let opt = setter.set_optional_name.clone();
            let clear = setter.clear_name.clone();
            let set_doc = LitStr::new(&setter.set_doc, f.span());
            let opt_doc = LitStr::new(&setter.set_optional_doc, f.span());
            let clear_doc = LitStr::new(&setter.clear_doc, f.span());
            Some(quote! {
                #[doc = #set_doc]
                fn #set(self, value: #ty) -> Self;
                #[doc = #opt_doc]
                fn #opt(self, value: ::core::option::Option<#ty>) -> Self;
                #[doc = #clear_doc]
                fn #clear(self) -> Self;
            })
        })
        .collect();
    let pending_setter_impls: Vec<TokenStream2> = facade
        .setters
        .iter()
        .filter_map(|setter| {
            let v = endpoint_var_for_setter(ep, &endpoint_vars_by_field, setter)?;
            let ty = &v.ty;
            let set = setter.set_name.clone();
            let opt = setter.set_optional_name.clone();
            let clear = setter.clear_name.clone();
            Some(quote! {
                #[inline]
                fn #set(self, value: #ty) -> Self {
                    self.map_endpoint(|ep| ep.#set(value))
                }

                #[inline]
                fn #opt(self, value: ::core::option::Option<#ty>) -> Self {
                    self.map_endpoint(|ep| ep.#opt(value))
                }

                #[inline]
                fn #clear(self) -> Self {
                    self.map_endpoint(|ep| ep.#clear())
                }
            })
        })
        .collect();
    let pending_request_ext = if pending_setter_decls.is_empty() {
        quote! {}
    } else {
        let pending_ext_trait = pending_ext_trait.clone();
        quote! {
            #[doc = "Request-builder extension methods for this endpoint."]
            pub trait #pending_ext_trait: Sized {
                #( #pending_setter_decls )*
            }

            impl<'a> #pending_ext_trait
                for ::concord_core::prelude::PendingRequest<'a, super::#cx_ty, #ty_name>
            {
                #( #pending_setter_impls )*
            }
        }
    };

    quote! {
        #endpoint_descriptor

        #( #[doc = #endpoint_docs] )*
        #[doc = "Advanced explicit endpoint request. Prefer facade methods for normal use."]
        pub struct #ty_name {
            #( #struct_fields, )*
        }

        impl #ty_name {
            #[doc = "Create this advanced explicit endpoint request."]
            #[inline]
            pub fn new( #( #fn_args ),* ) -> Self {
                Self { #( #init_parts, )* }
            }
            #( #setters_ts )*
        }

        #paginate_binding_impl

        impl ::concord_core::prelude::Endpoint<super::#cx_ty> for #ty_name {
            type Response = #final_response_ty;
            #execute_override
        }

        #response_terminal_impl

        #plan_impl

        #pagination_marker_impl

        #pending_request_ext
    }
}

fn emit_endpoint_descriptor(api: &ResolvedApi, ep: &ResolvedEndpoint) -> TokenStream2 {
    let descriptor = endpoint_descriptor_ident(ep);
    let name = LitStr::new(&endpoint_qualified_name(ep), ep.name.span());
    let api_name = LitStr::new(&api.client_name.to_string(), api.client_name.span());
    let method = match ep.method.to_string().as_str() {
        "GET" => quote! { ::concord_core::__private::v1::HttpMethod::Get },
        "POST" => quote! { ::concord_core::__private::v1::HttpMethod::Post },
        "PUT" => quote! { ::concord_core::__private::v1::HttpMethod::Put },
        "DELETE" => quote! { ::concord_core::__private::v1::HttpMethod::Delete },
        "HEAD" => quote! { ::concord_core::__private::v1::HttpMethod::Head },
        "OPTIONS" => quote! { ::concord_core::__private::v1::HttpMethod::Options },
        "PATCH" => quote! { ::concord_core::__private::v1::HttpMethod::Patch },
        _ => return emit_helpers::compile_error_tokens("unsupported resolved descriptor method", ep.method.span()),
    };
    let origin = match &ep.descriptor.origin {
        EndpointOriginIr::Fixed(origin) => {
            let fixed = emit_fixed_origin(origin);
            quote! { ::concord_core::__private::v1::EndpointOriginDescriptor::Fixed(#fixed) }
        }
        EndpointOriginIr::Dynamic => {
            quote! { ::concord_core::__private::v1::EndpointOriginDescriptor::Dynamic }
        }
    };
    let request_body = match &ep.descriptor.request_body {
        RequestBodyDescriptorIr::None => {
            quote! { ::concord_core::__private::v1::RequestBodyDescriptor::None }
        }
        RequestBodyDescriptorIr::Buffered { codec } => {
            let codec = LitStr::new(codec, ep.name.span());
            quote! { ::concord_core::__private::v1::RequestBodyDescriptor::Buffered { codec: #codec } }
        }
        RequestBodyDescriptorIr::Streaming { media } => {
            let media = LitStr::new(media, ep.name.span());
            quote! { ::concord_core::__private::v1::RequestBodyDescriptor::Streaming { media: #media } }
        }
        RequestBodyDescriptorIr::Multipart => {
            quote! { ::concord_core::__private::v1::RequestBodyDescriptor::Multipart }
        }
    };
    let response_format = match &ep.descriptor.response_format {
        ResponseFormatDescriptorIr::Buffered { codec } => {
            let codec = LitStr::new(codec, ep.name.span());
            quote! { ::concord_core::__private::v1::ResponseFormatDescriptor::Buffered { codec: #codec } }
        }
        ResponseFormatDescriptorIr::Bytes => {
            quote! { ::concord_core::__private::v1::ResponseFormatDescriptor::Bytes }
        }
        ResponseFormatDescriptorIr::NoContent => {
            quote! { ::concord_core::__private::v1::ResponseFormatDescriptor::NoContent }
        }
        ResponseFormatDescriptorIr::Streaming { media } => {
            let media = LitStr::new(media, ep.name.span());
            quote! { ::concord_core::__private::v1::ResponseFormatDescriptor::Streaming { media: #media } }
        }
    };
    let auth_requirements = ep.policy.auth.iter().map(|requirement| {
        let credential = LitStr::new(&requirement.credential.to_string(), requirement.credential.span());
        let usage_id = LitStr::new(&requirement.usage_id, requirement.credential.span());
        quote! {
            ::concord_core::__private::v1::AuthRequirementDescriptor {
                credential: #credential,
                usage_id: #usage_id,
            }
        }
    });
    let pagination = if ep.paginate.is_some() {
        let can_change_origin = ep.descriptor.pagination_can_change_origin;
        quote! {
            ::core::option::Option::Some(::concord_core::__private::v1::PaginationDescriptor {
                can_change_origin: #can_change_origin,
            })
        }
    } else {
        quote! { ::core::option::Option::None }
    };

    quote! {
        #[doc(hidden)]
        pub(super) static #descriptor: ::concord_core::__private::v1::EndpointDescriptor =
            ::concord_core::__private::v1::EndpointDescriptor {
                name: #name,
                api_name: #api_name,
                method: #method,
                origin: #origin,
                request: ::concord_core::__private::v1::RequestDescriptor { body: #request_body },
                response: ::concord_core::__private::v1::ResponseDescriptor { format: #response_format },
                auth: ::concord_core::__private::v1::AuthDescriptor {
                    requirements: &[ #( #auth_requirements ),* ],
                },
                pagination: #pagination,
            };
    }
}

fn emit_paginate_binding_impl(ep: &ResolvedEndpoint, ty_name: &Ident) -> TokenStream2 {
    let Some(p) = &ep.paginate else {
        return quote! {};
    };

    let pagination_ty = &p.controller_ty;
    let load_assignments: Vec<TokenStream2> = p
        .assigns
        .iter()
        .map(|assign| emit_paginate_binding_assignment(assign, p))
        .collect();

    let store_assignments: Vec<TokenStream2> = p.bindings.iter().map(|binding| {
        let field = &binding.controller_field;
        let endpoint_field = &binding.endpoint_rust_field;
        quote! { self.#endpoint_field = pagination.#field.clone(); }
    }).collect();

    quote! {
        impl ::concord_core::advanced::PaginateBinding<#pagination_ty> for #ty_name {
            fn load_pagination(&self) -> #pagination_ty {
                let mut pagination = <#pagination_ty as ::core::default::Default>::default();
                #( #load_assignments )*
                pagination
            }

            fn store_pagination(&mut self, pagination: &#pagination_ty) {
                #( #store_assignments )*
            }
        }
    }
}

fn emit_paginated_endpoint_impl(
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let Some(p) = &ep.paginate else {
        return quote! {};
    };
    let controller_ty = &p.controller_ty;

    quote! {
        impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name
        {
            type Pagination = #controller_ty;
        }
    }
}

fn emit_paginate_binding_assignment(assign: &PaginationAssignmentResolved, p: &PaginateResolved) -> TokenStream2 {
    let field = &assign.field;
    let value = match &assign.value {
        PaginationValueKind::EpField(_) => {
            let field_name = field.to_string();
            let Some(binding) = pagination_binding_for_controller_field(p, &field_name) else {
                let err = emit_helpers::compile_error_expr(
                    "missing pagination binding in resolved IR",
                    assign.field.span(),
                );
                return quote! { #err };
            };
            let endpoint_field = &binding.endpoint_rust_field;
            quote! { self.#endpoint_field.clone() }
        }
        PaginationValueKind::LitStr(s) => quote! { #s },
        PaginationValueKind::OtherExpr(e) => quote! { (#e) },
        PaginationValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
        }
    };
    quote! { pagination.#field = #value; }
}

fn pagination_binding_for_controller_field<'a>(
    p: &'a PaginateResolved,
    field: &str,
) -> Option<&'a PaginationBindingIr> {
    p.bindings
        .iter()
        .rev()
        .find(|binding| binding.controller_field == field)
}

fn emit_endpoint_plan_route_policy(
    ep: &ResolvedEndpoint,
    method: &Ident,
    _endpoint_name: &LitStr,
    cx_ty: &Ident,
    response_accept: &TokenStream2,
    response_no_content: &TokenStream2,
) -> TokenStream2 {
    let ep_opt = ep_optionals(ep);
    let prefix_layer_route_ops = emit_prefix_route_apply(&ep.prefix_pieces, Some(&ep_opt));
    let path_layer_route_ops = emit_path_route_apply(&ep.scope_path_pieces, Some(&ep_opt));
    let scope_policy_ops = ep.policy.scopes.iter().map(|scope_policy| {
        let scope_policy_apply = emit_policy_apply_fn(scope_policy, PolicyEmitCtx::Layer);
        quote! {
            {
                let __prev = policy.layer();
                policy.set_layer(::concord_core::__private::PolicyLayer::PrefixPath);
                #scope_policy_apply
                policy.set_layer(__prev);
            }
        }
    });
    let endpoint_route_apply = emit_path_route_apply(&ep.route_pieces, Some(&ep_opt));
    let endpoint_policy_apply = emit_policy_apply_fn(&ep.policy.endpoint, PolicyEmitCtx::Endpoint);
    quote! {
        let mut route = <super::#cx_ty as ::concord_core::prelude::ClientContext>::base_route(vars, __concord_auth_vars);
        #prefix_layer_route_ops
        #path_layer_route_ops
        #endpoint_route_apply
        route.host().validate(ctx_err.clone())?;
        let __resolved_route = ::concord_core::__private::ResolvedRoute {
            scheme: <super::#cx_ty as ::concord_core::prelude::ClientContext>::SCHEME,
            host: route.host().join(<super::#cx_ty as ::concord_core::prelude::ClientContext>::DOMAIN),
            path: route.path().as_str().to_string(),
        };

        let mut policy = <super::#cx_ty as ::concord_core::prelude::ClientContext>::base_policy(vars, __concord_auth_vars, &ctx_err)?;
        #( #scope_policy_ops )*
        {
            let __prev = policy.layer();
            policy.set_layer(::concord_core::__private::PolicyLayer::Endpoint);
            #endpoint_policy_apply
            policy.set_layer(__prev);
        }
        policy.set_layer(::concord_core::__private::PolicyLayer::Runtime);
        if ::http::Method::#method != ::http::Method::HEAD
            && !#response_no_content
            && let ::core::option::Option::Some(__accept) = #response_accept
        {
            policy.ensure_accept(__accept);
        }
        let (headers, query, timeout, mut rate_limit) = policy.into_parts();
        rate_limit.canonicalize();
        let __resolved_policy = ::concord_core::__private::ResolvedPolicy {
            headers,
            query,
            timeout,
            auth: __auth_plan,
            rate_limit,
        };
    }
}


fn emit_endpoint_pagination_plan(ep: &ResolvedEndpoint) -> TokenStream2 {
    if ep.paginate.is_some() {
        quote! {
            let __pagination_plan =
                ::core::option::Option::Some(::concord_core::__private::PaginationMarker);
        }
    } else {
        quote! {
            let __pagination_plan = ::core::option::Option::None;
        }
    }
}

fn facade_setter_for_var<'a>(
    facade: &'a FacadeEndpoint,
    setters_by_field: &std::collections::BTreeMap<String, usize>,
    field: &Ident,
) -> Option<&'a FacadeSetter> {
    let index = setters_by_field.get(&field.to_string())?;
    facade.setters.get(*index)
}

fn endpoint_var_for_setter<'a>(
    ep: &'a ResolvedEndpoint,
    vars_by_field: &std::collections::BTreeMap<String, usize>,
    setter: &FacadeSetter,
) -> Option<&'a VarInfo> {
    let index = vars_by_field.get(&setter.field.to_string())?;
    let var = &ep.vars[*index];
    if var.optional || var.default.is_some() {
        Some(var)
    } else {
        None
    }
}

fn endpoint_request_body_inner_ty(ep: &ResolvedEndpoint) -> Result<Option<TokenStream2>, TokenStream2> {
    Ok(ep.io.request_entity.body_field_ty.clone().map(|ty| quote! { #ty }))
}

fn endpoint_request_body_plan(ep: &ResolvedEndpoint) -> Result<TokenStream2, TokenStream2> {
    let request_adapter_ty = &ep.io.request_entity.adapter_ty;
    if ep.io.request_entity.capabilities.has_body {
        Ok(quote! {
            let __prepared_request_entity = <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                {
                    let __body_value = ep.body;
                    __body_value
                },
                ctx_err.clone(),
            )?;
            let __prepared_body = __prepared_request_entity.body;
        })
    } else {
        Ok(quote! {
            let __prepared_request_entity =
                <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                    (),
                    ctx_err.clone(),
                )?;
            let __prepared_body = __prepared_request_entity.body;
        })
    }
}

fn endpoint_plan_impl(
    resolved_api: &ResolvedApi,
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
    owned: bool,
) -> TokenStream2 {
    let method = endpoint_http_method_ident(ep);
    let endpoint_name_str = endpoint_qualified_name(ep);
    let endpoint_name = LitStr::new(&endpoint_name_str, ep.name.span());
    let response_plan_setup = endpoint_response_plan_tokens(ep, ty_name);
    let response_plan_accept = quote! { __response_accept };
    let response_plan_no_content = quote! { __response_no_content };
    let route_policy = emit_endpoint_plan_route_policy(
        ep,
        &method,
        &endpoint_name,
        cx_ty,
        &response_plan_accept,
        &response_plan_no_content,
    );
    let auth_plan = emit_endpoint_auth_plan(resolved_api, ep);
    let pagination_plan = emit_endpoint_pagination_plan(ep);
    let body_plan = match endpoint_request_body_plan(ep) {
        Ok(body_plan) => body_plan,
        Err(err) => return err,
    };
    let idempotent = matches!(
        method.to_string().as_str(),
        "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS"
    );
    let plan_body = quote! {
        let vars = plan_ctx.vars;
        let __concord_auth_vars = plan_ctx.auth_vars;
        let ep = self;
        let ctx_err = ::concord_core::error::ErrorContext { endpoint: #endpoint_name, method: ::http::Method::#method };
        let __auth_plan = #auth_plan;
        let ctx = ctx_err.clone();
        #response_plan_setup
        #route_policy
        #pagination_plan
        #body_plan
        ::core::result::Result::Ok(::concord_core::__private::RequestPlan {
            endpoint: ::concord_core::__private::EndpointPlan {
                meta: ::concord_core::__private::EndpointMeta {
                    name: #endpoint_name,
                    method: ::http::Method::#method,
                    idempotent: #idempotent,
                    facade_path: &[],
                },
                route: __resolved_route,
                policy: __resolved_policy,
                response: __response_plan,
                pagination: __pagination_plan,
            },
            body: __prepared_body,
            overrides: ::concord_core::__private::RequestOverrides::default(),
        })
    };
    if owned {
        quote! {
            impl ::concord_core::prelude::IntoEndpointPlan<super::#cx_ty> for #ty_name {
                fn into_plan(
                    self,
                    plan_ctx: &::concord_core::__private::ClientPlanContext<'_, super::#cx_ty>,
                ) -> ::core::result::Result<::concord_core::__private::RequestPlan, ::concord_core::prelude::ApiClientError> {
                    #plan_body
                }
            }
        }
    } else {
        quote! {
            impl ::concord_core::prelude::ReusableEndpoint<super::#cx_ty> for #ty_name {
                fn plan(
                    &self,
                    plan_ctx: &::concord_core::__private::ClientPlanContext<'_, super::#cx_ty>,
                ) -> ::core::result::Result<::concord_core::__private::RequestPlan, ::concord_core::prelude::ApiClientError> {
                    #plan_body
                }
            }
        }
    }
}

fn endpoint_response_output_ty(ep: &ResolvedEndpoint) -> TokenStream2 {
    let response_ty = ep.io.response_entity.public_output_ty.clone();
    quote! { #response_ty }
}

fn endpoint_response_adapter_ty(ep: &ResolvedEndpoint, ty_name: &Ident) -> TokenStream2 {
    let base_adapter_ty = &ep.io.response_entity.adapter_ty;
    let _ = ty_name;
    quote! { #base_adapter_ty }
}

fn endpoint_response_plan_tokens(ep: &ResolvedEndpoint, ty_name: &Ident) -> TokenStream2 {
    let response_entity_adapter_ty = endpoint_response_adapter_ty(ep, ty_name);
    quote! {
        let __response_entity_plan = <#response_entity_adapter_ty as ::concord_core::advanced::ResponseEntity>::plan(ctx_err.clone())?;
        let __response_plan = __response_entity_plan.response_plan.clone();
        let __response_accept = __response_plan.accept.clone();
        let __response_no_content = __response_plan.no_content;
    }
}

fn endpoint_execute_override(ep: &ResolvedEndpoint, ty_name: &Ident, cx_ty: &Ident) -> TokenStream2 {
    let response_entity_adapter_ty = endpoint_response_adapter_ty(ep, ty_name);
    quote! {
        fn execute<'a>(
            client: &'a ::concord_core::prelude::ApiClient<super::#cx_ty>,
            plan: ::concord_core::__private::RequestPlan,
        ) -> ::core::pin::Pin<
            ::std::boxed::Box<
                dyn ::core::future::Future<
                        Output = ::core::result::Result<
                            Self::Response,
                            ::concord_core::prelude::ApiClientError,
                        >,
                    > + Send + 'a,
            >,
        >
        {
            <#response_entity_adapter_ty as ::concord_core::advanced::ResponseEntity>::execute(
                client,
                plan,
            )
        }
    }
}

fn endpoint_response_terminal_impl(
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    if ep.io.response_entity.capabilities.is_streaming {
        return quote! {};
    }

    let response_entity_adapter_ty = endpoint_response_adapter_ty(ep, ty_name);
    quote! {
        impl ::concord_core::__private::ResponseTerminalEndpoint<super::#cx_ty> for #ty_name {
            fn execute_response<'a>(
                client: &'a ::concord_core::prelude::ApiClient<super::#cx_ty>,
                plan: ::concord_core::__private::RequestPlan,
            ) -> ::core::pin::Pin<
                ::std::boxed::Box<
                    dyn ::core::future::Future<
                            Output = ::core::result::Result<
                                ::concord_core::advanced::DecodedResponse<Self::Response>,
                                ::concord_core::prelude::ApiClientError,
                            >,
                        > + Send + 'a,
                >,
            >
            {
                <#response_entity_adapter_ty as ::concord_core::__private::ResponseEntityWithMeta>::execute_with_meta(
                    client,
                    plan,
                )
            }
        }
    }
}
