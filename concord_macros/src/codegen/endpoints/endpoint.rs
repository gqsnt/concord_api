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
    let method = endpoint_http_method_ident(ep);
    let endpoint_name_str = endpoint_qualified_name(ep);
    let endpoint_name = LitStr::new(&endpoint_name_str, ep.name.span());
    let endpoint_docs = facade_endpoint_docs(ep, &resolved_api.client_policy);

    let mut fields_ts = Vec::new();
    let mut setters_ts = Vec::new();
    for v in &ep.vars {
        let f = &v.rust;
        let ty = &v.ty;
        if v.optional {
            fields_ts.push(quote! { pub(crate) #f: ::core::option::Option<#ty> });
            if let Some(setter) = facade_setter_for_var(facade, f) {
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
                if let Some(setter) = facade_setter_for_var(facade, f) {
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
        struct_fields.push(quote! { pub(crate) body: ::std::sync::Mutex<::core::option::Option<#body_ty>> });
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
        init_parts.push(quote! { body: ::std::sync::Mutex::new(::core::option::Option::Some(body)) });
    }

    let final_response_ty = endpoint_response_output_ty(ep);
    let response_transform_impl = endpoint_response_transform_impl(ep, ty_name);
    let response_plan_setup = endpoint_response_plan_tokens(ep, ty_name);
    let response_plan_accept = quote! { __response_accept };
    let response_plan_no_content = quote! { __response_no_content };
    let idempotent = matches!(
        method.to_string().as_str(),
        "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS"
    );

    let route_policy = emit_endpoint_plan_route_policy(
        ep,
        &method,
        &endpoint_name,
        cx_ty,
        &response_plan_accept,
        &response_plan_no_content,
    );
    let auth_plan = emit_endpoint_auth_plan(resolved_api, ep);
    let body_plan = match endpoint_request_body_plan(ep) {
        Ok(body_plan) => body_plan,
        Err(err) => return err,
    };
    let execute_override = endpoint_execute_override(ep, ty_name, cx_ty);

    let paginate_binding_impl = emit_paginate_binding_impl(ep, ty_name);
    let pagination_plan = emit_endpoint_pagination_plan(ep);
    let pagination_marker_impl = emit_paginated_endpoint_impl(ep, ty_name, cx_ty);
    let pending_ext_trait = endpoint_pending_ext_trait_ident(ep);
    let pending_setter_decls: Vec<TokenStream2> = facade
        .setters
        .iter()
        .filter_map(|setter| {
            let v = endpoint_var_for_setter(ep, setter)?;
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
            let v = endpoint_var_for_setter(ep, setter)?;
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

            impl<'a, T> #pending_ext_trait
                for ::concord_core::prelude::PendingRequest<'a, super::#cx_ty, #ty_name, T>
            where
                T: ::concord_core::advanced::Transport,
            {
                #( #pending_setter_impls )*
            }
        }
    };

    quote! {
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

        #response_transform_impl

        #paginate_binding_impl

        impl ::concord_core::prelude::Endpoint<super::#cx_ty> for #ty_name {
            type Response = #final_response_ty;
            #execute_override

            fn plan(
                &self,
                plan_ctx: &::concord_core::internal::ClientPlanContext<'_, super::#cx_ty>,
            ) -> ::core::result::Result<::concord_core::internal::RequestPlan, ::concord_core::prelude::ApiClientError> {
                let vars = plan_ctx.vars;
                let __concord_auth_vars = plan_ctx.auth_vars;
                let ep = self;
                let ctx_err = ::concord_core::error::ErrorContext { endpoint: #endpoint_name, method: ::http::Method::#method };
                let __auth_plan = #auth_plan;
                let ctx = ctx_err.clone();
                #response_plan_setup
                #route_policy
                #body_plan
                #pagination_plan
                ::core::result::Result::Ok(::concord_core::internal::RequestPlan {
                    endpoint: ::concord_core::internal::EndpointPlan {
                        meta: ::concord_core::internal::EndpointMeta {
                            name: #endpoint_name,
                            method: ::http::Method::#method,
                            idempotent: #idempotent,
                            facade_path: &[],
                        },
                        route: __resolved_route,
                        policy: __resolved_policy,
                        body: __body_plan,
                        response: __response_plan,
                        pagination: __pagination_plan,
                    },
                    args: __request_args,
                    overrides: ::concord_core::internal::RequestOverrides::default(),
                })
            }
        }

        #pagination_marker_impl

        #pending_request_ext
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
            let build = emit_fmt_build_string(&fmt);
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
        .find(|binding| binding.controller_field.to_string() == field)
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
                policy.set_layer(::concord_core::internal::PolicyLayer::PrefixPath);
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
        let __resolved_route = ::concord_core::internal::ResolvedRoute {
            scheme: <super::#cx_ty as ::concord_core::prelude::ClientContext>::SCHEME,
            host: route.host().join(<super::#cx_ty as ::concord_core::prelude::ClientContext>::DOMAIN),
            path: route.path().as_str().to_string(),
        };

        let mut policy = <super::#cx_ty as ::concord_core::prelude::ClientContext>::base_policy(vars, __concord_auth_vars, &ctx_err)?;
        #( #scope_policy_ops )*
        {
            let __prev = policy.layer();
            policy.set_layer(::concord_core::internal::PolicyLayer::Endpoint);
            #endpoint_policy_apply
            policy.set_layer(__prev);
        }
        policy.set_layer(::concord_core::internal::PolicyLayer::Runtime);
        if ::http::Method::#method != ::http::Method::HEAD
            && !#response_no_content
            && let ::core::option::Option::Some(__accept) = #response_accept
        {
            policy.ensure_accept(__accept);
        }
        let (headers, query, timeout, retry, mut rate_limit) = policy.into_parts();
        rate_limit.canonicalize();
        let __resolved_policy = ::concord_core::internal::ResolvedPolicy {
            headers,
            query,
            timeout,
            auth: __auth_plan,
            retry,
            rate_limit,
        };
    }
}


fn emit_endpoint_pagination_plan(ep: &ResolvedEndpoint) -> TokenStream2 {
    if ep.paginate.is_some() {
        quote! {
            let __pagination_plan =
                ::core::option::Option::Some(::concord_core::internal::PaginationMarker);
        }
    } else {
        quote! {
            let __pagination_plan = ::core::option::Option::None;
        }
    }
}

fn facade_setter_for_var<'a>(facade: &'a FacadeEndpoint, field: &Ident) -> Option<&'a FacadeSetter> {
    facade
        .setters
        .iter()
        .find(|setter| field == &setter.field)
}

fn endpoint_var_for_setter<'a>(
    ep: &'a ResolvedEndpoint,
    setter: &FacadeSetter,
) -> Option<&'a VarInfo> {
    ep.vars
        .iter()
        .find(|var| var.rust == setter.field && (var.optional || var.default.is_some()))
}

fn endpoint_request_body_inner_ty(ep: &ResolvedEndpoint) -> Result<Option<TokenStream2>, TokenStream2> {
    Ok(ep.io.request_entity.public_input_ty.clone().map(|ty| quote! { #ty }))
}

fn endpoint_request_body_plan(ep: &ResolvedEndpoint) -> Result<TokenStream2, TokenStream2> {
    let request_adapter_ty = &ep.io.request_entity.adapter_ty;
    match ep.request_io() {
        ResolvedRequestBodyIo::None => Ok(quote! {
            let __prepared_request_entity =
                <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                    (),
                    ctx_err.clone(),
                )?;
            let __body_plan = __prepared_request_entity.body_plan;
            let __request_args = __prepared_request_entity.args;
        }),
        ResolvedRequestBodyIo::BufferedCodec(_) => Ok(quote! {
            let __prepared_request_entity = <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                {
                    let __body_value = self
                        .body
                        .lock()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                        .take()
                        .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                    __body_value
                },
                ctx_err.clone(),
            )?;
            let __body_plan = __prepared_request_entity.body_plan;
            let __request_args = __prepared_request_entity.args;
        }),
        ResolvedRequestBodyIo::RawStream { .. } => Ok(quote! {
            let __prepared_request_entity = <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                {
                    let __body_value = self
                        .body
                        .lock()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                        .take()
                        .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                    __body_value
                },
                ctx_err.clone(),
            )?;
            let __body_plan = __prepared_request_entity.body_plan;
            let __request_args = __prepared_request_entity.args;
        }),
        ResolvedRequestBodyIo::Records { .. } => Ok(quote! {
            let __prepared_request_entity = <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                {
                    let __body_value = self
                        .body
                        .lock()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                        .take()
                        .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                    __body_value
                },
                ctx_err.clone(),
            )?;
            let __body_plan = __prepared_request_entity.body_plan;
            let __request_args = __prepared_request_entity.args;
        }),
        ResolvedRequestBodyIo::Multipart { .. } => Ok(quote! {
            let __prepared_request_entity = <#request_adapter_ty as ::concord_core::advanced::RequestEntity>::prepare(
                {
                    let __body_value = self
                        .body
                        .lock()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                        .take()
                        .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                    __body_value
                },
                ctx_err.clone(),
            )?;
            let __body_plan = __prepared_request_entity.body_plan;
            let __request_args = __prepared_request_entity.args;
        }),
    }
}

fn endpoint_response_output_ty(ep: &ResolvedEndpoint) -> TokenStream2 {
    let response_ty = ep.io.response_entity.public_output_ty.clone();
    quote! { #response_ty }
}

fn endpoint_response_adapter_ty(ep: &ResolvedEndpoint, ty_name: &Ident) -> TokenStream2 {
    let base_adapter_ty = &ep.io.response_entity.adapter_ty;
    if ep.io.response_entity.mapped {
        let transform_ty = emit_helpers::ident(&format!("__Map{ty_name}"), Span::call_site());
        quote! {
            ::concord_core::advanced::MappedResponse<#base_adapter_ty, #transform_ty>
        }
    } else {
        quote! { #base_adapter_ty }
    }
}

fn endpoint_response_transform_impl(ep: &ResolvedEndpoint, ty_name: &Ident) -> TokenStream2 {
    let Some(map) = ep.map.as_ref() else {
        return quote! {};
    };

    let transform_ty = emit_helpers::ident(&format!("__Map{ty_name}"), Span::call_site());
    let decoded_ty = match ep.io.response_entity.decoded_value_ty.as_ref() {
        Some(ty) => ty,
        None => {
            return quote! {
                compile_error!("mapped response is missing a decoded value type in resolved IR");
            };
        }
    };
    let out_ty = &ep.io.response_entity.public_output_ty;
    let body = &map.body;

    quote! {
        struct #transform_ty;

        impl ::concord_core::advanced::ResponseTransform<#decoded_ty> for #transform_ty {
            type Output = #out_ty;

            fn transform(
                input: #decoded_ty,
            ) -> ::core::result::Result<Self::Output, ::concord_core::advanced::FxError> {
                let r: #decoded_ty = input;
                let value: #out_ty = (#body);
                ::core::result::Result::Ok(value)
            }
        }
    }
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
        fn execute<'a, T>(
            client: &'a ::concord_core::prelude::ApiClient<super::#cx_ty, T>,
            plan: ::concord_core::internal::RequestPlan,
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
        where
            T: ::concord_core::advanced::Transport + 'a,
        {
            ::std::boxed::Box::pin(async move {
                <#response_entity_adapter_ty as ::concord_core::advanced::ResponseEntity>::execute(
                    client,
                    plan,
                )
                .await
            })
        }
    }
}

