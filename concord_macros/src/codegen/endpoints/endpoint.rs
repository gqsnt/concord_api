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

    let response_dec_ty;
    let decoded_ty_ty;
    match ep.response_io() {
        ResolvedResponseBodyIo::BufferedCodec(io) => {
            response_dec_ty = io.marker.clone();
            decoded_ty_ty = io.value_ty.clone();
        }
        ResolvedResponseBodyIo::BufferedBytes => {
            response_dec_ty = syn::parse_quote!(::bytes::Bytes);
            decoded_ty_ty = syn::parse_quote!(::bytes::Bytes);
        }
        ResolvedResponseBodyIo::NoContent => {
            response_dec_ty = syn::parse_quote!(());
            decoded_ty_ty = syn::parse_quote!(());
        }
        ResolvedResponseBodyIo::RawStream { media_ty } => {
            response_dec_ty = media_ty.clone();
            decoded_ty_ty = media_ty.clone();
        }
        ResolvedResponseBodyIo::Records { item_ty, .. } => {
            response_dec_ty = item_ty.clone();
            decoded_ty_ty = item_ty.clone();
        }
        ResolvedResponseBodyIo::Multipart { part_ty, .. } => {
            response_dec_ty = part_ty.clone();
            decoded_ty_ty = part_ty.clone();
        }
        ResolvedResponseBodyIo::Sse { event_ty, .. } => {
            response_dec_ty = event_ty.clone();
            decoded_ty_ty = event_ty.clone();
        }
    }
    let response_dec = &response_dec_ty;
    let decoded_ty = &decoded_ty_ty;
    let final_response_ty = endpoint_response_output_ty(ep);
    let decode_fn = emit_helpers::ident(&format!("__decode_{ty_name}"), Span::call_site());
    let response_decode_fn = endpoint_response_decode_fn(ep, ty_name, response_dec, decoded_ty);
    let response_plan_accept = endpoint_response_accept_tokens(ep, response_dec);
    let response_plan_no_content = endpoint_response_no_content_tokens(ep, response_dec);
    let response_plan_format = endpoint_response_format_tokens(ep, response_dec);
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
    let execute_override = endpoint_execute_override(ep, cx_ty);
    let response_marker_impl = endpoint_response_marker_impl(ep, ty_name, cx_ty);

    let pagination_endpoint_state_bindings =
        emit_pagination_endpoint_state_bindings(ep, ty_name, cx_ty);
    let pagination_plan = emit_endpoint_pagination_plan(ep);
    let pagination_marker_impl = if let Some(p) = &ep.paginate {
        if matches!(p.controller, PaginationControllerResolved::OffsetLimit(_))
            && offset_limit_runtime_bindings_present(p)
        {
            let helper_name = emit_helpers::ident(
                &format!("__{}_pagination_bindings", ty_name),
                Span::call_site(),
            );
            quote! {
                impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name {
                    #[inline]
                    fn endpoint_state_pagination(
                        &self,
                    ) -> ::core::option::Option<::std::boxed::Box<dyn ::concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
                    where
                        Self: Sized,
                        Self::Response: ::concord_core::advanced::PageItems,
                    {
                        ::core::option::Option::Some(::std::boxed::Box::new(
                            ::concord_core::internal::EndpointPaginationRuntimeAdapter::new(
                                ::concord_core::advanced::OffsetLimitPagination::default(),
                                Self::#helper_name(),
                            ),
                        ))
                    }
                }
            }
        } else if matches!(p.controller, PaginationControllerResolved::Cursor(_))
            && cursor_runtime_bindings_present(p)
        {
            let helper_name = emit_helpers::ident(
                &format!("__{}_pagination_bindings", ty_name),
                Span::call_site(),
            );
            let PaginationControllerResolved::Cursor(ctrl) = &p.controller else {
                return quote! {};
            };
            let send_cursor_on_first = ctrl.send_cursor_on_first;
            let stop_when_cursor_missing = ctrl.stop_when_cursor_missing;
            quote! {
                impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name
                where
                    <#ty_name as ::concord_core::prelude::Endpoint<super::#cx_ty>>::Response: ::concord_core::advanced::PageItems + ::concord_core::advanced::HasNextCursor,
                {
                    #[inline]
                    fn endpoint_state_pagination(
                        &self,
                    ) -> ::core::option::Option<::std::boxed::Box<dyn ::concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
                    where
                        Self: Sized,
                        Self::Response: ::concord_core::advanced::PageItems,
                    {
                        let ctrl = ::concord_core::advanced::CursorPagination {
                            send_cursor_on_first: #send_cursor_on_first,
                            stop_when_cursor_missing: #stop_when_cursor_missing,
                            ..::core::default::Default::default()
                        };
                        ::core::option::Option::Some(::std::boxed::Box::new(
                            ::concord_core::internal::EndpointPaginationRuntimeAdapter::new(
                                ctrl,
                                Self::#helper_name(),
                            ),
                        ))
                    }
                }
            }
        } else if matches!(p.controller, PaginationControllerResolved::Paged(_))
            && paged_runtime_bindings_present(p)
        {
            let helper_name = emit_helpers::ident(
                &format!("__{}_pagination_bindings", ty_name),
                Span::call_site(),
            );
            quote! {
                impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name {
                    #[inline]
                    fn endpoint_state_pagination(
                        &self,
                    ) -> ::core::option::Option<::std::boxed::Box<dyn ::concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
                    where
                        Self: Sized,
                        Self::Response: ::concord_core::advanced::PageItems,
                    {
                        ::core::option::Option::Some(::std::boxed::Box::new(
                            ::concord_core::internal::EndpointPaginationRuntimeAdapter::new(
                                ::concord_core::advanced::PagedPagination::default(),
                                Self::#helper_name(),
                            ),
                        ))
                    }
                }
            }
        } else if let PaginationControllerResolved::CustomEndpointState { ctrl_ty, bindings_ty } = &p.controller {
            let helper_name = emit_helpers::ident(
                &format!("__{}_pagination_bindings", ty_name),
                Span::call_site(),
            );
            quote! {
                impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name
                where
                    #ctrl_ty: ::core::default::Default
                        + ::concord_core::advanced::EndpointPaginationController<
                            #ty_name,
                            <#ty_name as ::concord_core::prelude::Endpoint<super::#cx_ty>>::Response,
                            Bindings = #bindings_ty::<#ty_name>,
                        >,
                {
                    #[inline]
                    fn endpoint_state_pagination(
                        &self,
                    ) -> ::core::option::Option<::std::boxed::Box<dyn ::concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
                    where
                        Self: Sized,
                        Self::Response: ::concord_core::advanced::PageItems,
                    {
                        ::core::option::Option::Some(::std::boxed::Box::new(
                            ::concord_core::internal::EndpointPaginationRuntimeAdapter::new(
                                <#ctrl_ty as ::core::default::Default>::default(),
                                Self::#helper_name(),
                            ),
                        ))
                    }
                }
            }
        } else {
            quote! {
                impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name {}
            }
        }
    } else {
        quote! {}
    };
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
            #pagination_endpoint_state_bindings
        }

        #response_decode_fn

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
                        response: ::concord_core::internal::ResponsePlan {
                            accept: #response_plan_accept,
                            no_content: #response_plan_no_content,
                            format: #response_plan_format,
                            decode: #decode_fn,
                        },
                        pagination: __pagination_plan,
                    },
                    args: __request_args,
                    overrides: ::concord_core::internal::RequestOverrides::default(),
                })
            }
        }

        #response_marker_impl

        #pagination_marker_impl

        #pending_request_ext
    }
}

fn emit_pagination_endpoint_state_bindings(
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let Some(p) = &ep.paginate else {
        return quote! {};
    };
    if p.bindings.is_empty() {
        return quote! {};
    }

    let helper_name = emit_helpers::ident(
        &format!("__{}_pagination_bindings", ty_name),
        Span::call_site(),
    );
    if matches!(p.controller, PaginationControllerResolved::Cursor(_))
        && cursor_runtime_bindings_present(p)
    {
        let Some(cursor) = pagination_binding_for_controller_field(p, "cursor") else {
            return quote! {};
        };
        let Some(per_page) = pagination_binding_for_controller_field(p, "per_page") else {
            return quote! {};
        };
        let cursor_field = &cursor.endpoint_rust_field;
        let per_page_field = &per_page.endpoint_rust_field;
        return quote! {
            #[allow(dead_code)]
            fn #helper_name() -> ::concord_core::advanced::CursorBindings<
                Self,
                <<Self as ::concord_core::prelude::Endpoint<super::#cx_ty>>::Response as ::concord_core::advanced::HasNextCursor>::Cursor
            > {
                ::concord_core::advanced::CursorBindings {
                    cursor: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#cursor_field.clone(),
                        |ep: &mut Self, value| ep.#cursor_field = value,
                    ),
                    per_page: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#per_page_field.clone(),
                        |ep: &mut Self, value| ep.#per_page_field = value,
                    ),
                }
            }
        };
    }
    if matches!(p.controller, PaginationControllerResolved::OffsetLimit(_))
        && offset_limit_runtime_bindings_present(p)
    {
        let Some(offset) = pagination_binding_for_controller_field(p, "offset") else {
            return quote! {};
        };
        let Some(limit) = pagination_binding_for_controller_field(p, "limit") else {
            return quote! {};
        };
        let offset_field = &offset.endpoint_rust_field;
        let limit_field = &limit.endpoint_rust_field;
        return quote! {
            #[allow(dead_code)]
            fn #helper_name() -> ::concord_core::advanced::OffsetLimitBindings<Self> {
                ::concord_core::advanced::OffsetLimitBindings {
                    offset: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#offset_field.clone(),
                        |ep: &mut Self, value| ep.#offset_field = value,
                    ),
                    limit: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#limit_field.clone(),
                        |ep: &mut Self, value| ep.#limit_field = value,
                    ),
                }
            }
        };
    }
    if matches!(p.controller, PaginationControllerResolved::Paged(_))
        && paged_runtime_bindings_present(p)
    {
        let Some(page) = pagination_binding_for_controller_field(p, "page") else {
            return quote! {};
        };
        let Some(per_page) = pagination_binding_for_controller_field(p, "per_page") else {
            return quote! {};
        };
        let page_field = &page.endpoint_rust_field;
        let per_page_field = &per_page.endpoint_rust_field;
        return quote! {
            #[allow(dead_code)]
            fn #helper_name() -> ::concord_core::advanced::PagedBindings<Self> {
                ::concord_core::advanced::PagedBindings {
                    page: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#page_field.clone(),
                        |ep: &mut Self, value| ep.#page_field = value,
                    ),
                    per_page: ::concord_core::advanced::EndpointField::new(
                        |ep: &Self| ep.#per_page_field.clone(),
                        |ep: &mut Self, value| ep.#per_page_field = value,
                    ),
                }
            }
        };
    }
    if let PaginationControllerResolved::CustomEndpointState { bindings_ty, .. } = &p.controller {
        let inits = p.bindings.iter().map(|binding| {
            let field = &binding.controller_field;
            let endpoint_field = &binding.endpoint_rust_field;
            quote! {
                #field: ::concord_core::advanced::EndpointField::new(
                    |ep: &Self| ep.#endpoint_field.clone(),
                    |ep: &mut Self, value| ep.#endpoint_field = value,
                )
            }
        });
        return quote! {
            #[allow(dead_code)]
            fn #helper_name() -> #bindings_ty::<Self> {
                #bindings_ty::<Self> {
                    #( #inits, )*
                }
            }
        };
    }
    let struct_name = emit_helpers::ident(
        &format!("__{}_PaginationBindings", ty_name),
        Span::call_site(),
    );
    let fields = p.bindings.iter().map(|binding| {
        let field = &binding.controller_field;
        let field_ty = &binding.endpoint_field_ty;
        quote! {
            pub(crate) #field: ::concord_core::advanced::EndpointField<#ty_name, #field_ty>
        }
    });
    let inits = p.bindings.iter().map(|binding| {
        let field = &binding.controller_field;
        let endpoint_field = &binding.endpoint_rust_field;
        quote! {
            #field: ::concord_core::advanced::EndpointField::new(
                |ep: &#ty_name| ep.#endpoint_field.clone(),
                |ep: &mut #ty_name, value| ep.#endpoint_field = value,
            )
        }
    });

    quote! {
        #[allow(dead_code)]
        struct #struct_name {
            #( #fields, )*
        }

        #[allow(dead_code)]
        fn #helper_name() -> #struct_name {
            #struct_name {
                #( #inits, )*
            }
        }
    }
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

fn offset_limit_runtime_bindings_present(p: &PaginateResolved) -> bool {
    pagination_binding_for_controller_field(p, "offset").is_some()
        && pagination_binding_for_controller_field(p, "limit").is_some()
}

fn paged_runtime_bindings_present(p: &PaginateResolved) -> bool {
    pagination_binding_for_controller_field(p, "page").is_some()
        && pagination_binding_for_controller_field(p, "per_page").is_some()
}

fn cursor_runtime_bindings_present(p: &PaginateResolved) -> bool {
    pagination_binding_for_controller_field(p, "cursor").is_some()
        && pagination_binding_for_controller_field(p, "per_page").is_some()
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
    let Some(p) = &ep.paginate else {
        return quote! {
            let __pagination_plan = ::core::option::Option::None;
        };
    };
    let page_ty = ep
        .map
        .as_ref()
        .map(|m| m.out_ty.clone())
        .unwrap_or_else(|| resolved_response_output_ty(ep.response_io(), None));
    match &p.controller {
        PaginationControllerResolved::Custom { ctrl_ty } => quote! {
            let __pagination_plan = ::core::option::Option::Some(
                ::concord_core::internal::PaginationPlan::custom::<#ctrl_ty, #page_ty>()
            );
        },
        PaginationControllerResolved::CustomEndpointState { .. } => quote! {
            let __pagination_plan = ::core::option::Option::None;
        },
        PaginationControllerResolved::OffsetLimit(ctrl) => {
            let auto_key_assigns = [
                ("offset_key", &ctrl.offset_key_from_query),
                ("limit_key", &ctrl.limit_key_from_query),
            ]
            .into_iter()
            .filter_map(|(field, key)| {
                let key = key.as_ref()?;
                let (_, _, key_ts) = emit_key_string(key, PolicyKeyKind::Query);
                let field = syn::Ident::new(field, Span::call_site());
                Some(quote! {
                    ctrl.#field = ::std::borrow::Cow::from(#key_ts);
                })
            });
            let assigns = ctrl.assigns.iter().map(|assign| emit_pagination_assign(assign));
            quote! {
                #[allow(unused_variables)]
                let cx = vars;
                let mut ctrl: ::concord_core::internal::OffsetLimitPagination = ::core::default::Default::default();
                #( #auto_key_assigns )*
                #( #assigns )*
                let __pagination_plan = ::core::option::Option::Some(::concord_core::internal::PaginationPlan::from(ctrl));
            }
        }
        PaginationControllerResolved::Cursor(ctrl) => {
            let auto_key_assigns = [
                ("cursor_key", &ctrl.cursor_key_from_query),
                ("per_page_key", &ctrl.per_page_key_from_query),
            ]
            .into_iter()
            .filter_map(|(field, key)| {
                let key = key.as_ref()?;
                let (_, _, key_ts) = emit_key_string(key, PolicyKeyKind::Query);
                let field = syn::Ident::new(field, Span::call_site());
                Some(quote! {
                    ctrl.#field = ::std::borrow::Cow::from(#key_ts);
                })
            });
            let assigns = ctrl.assigns.iter().map(|assign| emit_pagination_assign(assign));
            quote! {
                #[allow(unused_variables)]
                let cx = vars;
                let mut ctrl: ::concord_core::internal::CursorPagination = ::core::default::Default::default();
                #( #auto_key_assigns )*
                #( #assigns )*
                let __pagination_plan = ::core::option::Option::Some(::concord_core::internal::PaginationPlan::cursor::<#page_ty>(ctrl));
            }
        }
        PaginationControllerResolved::Paged(ctrl) => {
            let auto_key_assigns = [
                ("page_key", &ctrl.page_key_from_query),
                ("per_page_key", &ctrl.per_page_key_from_query),
            ]
            .into_iter()
            .filter_map(|(field, key)| {
                let key = key.as_ref()?;
                let (_, _, key_ts) = emit_key_string(key, PolicyKeyKind::Query);
                let field = syn::Ident::new(field, Span::call_site());
                Some(quote! {
                    ctrl.#field = ::std::borrow::Cow::from(#key_ts);
                })
            });
            let assigns = ctrl.assigns.iter().map(|assign| emit_pagination_assign(assign));
            quote! {
                #[allow(unused_variables)]
                let cx = vars;
                let mut ctrl: ::concord_core::internal::PagedPagination = ::core::default::Default::default();
                #( #auto_key_assigns )*
                #( #assigns )*
                let __pagination_plan = ::core::option::Option::Some(::concord_core::internal::PaginationPlan::from(ctrl));
            }
        }
    }
}

fn emit_pagination_assign(assign: &PaginationAssignmentResolved) -> TokenStream2 {
    let field = &assign.field;
    let value = match &assign.value {
        PaginationValueKind::EpField(f) => quote! { ep.#f.clone() },
        PaginationValueKind::LitStr(s) => quote! { ::std::borrow::Cow::from(#s) },
        PaginationValueKind::OtherExpr(e) => quote! { (#e) },
        PaginationValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
        }
    };
    quote! { ctrl.#field = #value; }
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
    match ep.request_io() {
        ResolvedRequestBodyIo::None => Ok(None),
        ResolvedRequestBodyIo::BufferedCodec(io) => {
            let ty = &io.value_ty;
            Ok(Some(quote! { #ty }))
        }
        ResolvedRequestBodyIo::RawStream { .. } => Ok(Some(quote! { ::concord_core::advanced::StreamBody })),
        ResolvedRequestBodyIo::Records { item_ty, .. } => {
            Ok(Some(quote! { ::concord_core::advanced::RecordBody<#item_ty> }))
        }
        ResolvedRequestBodyIo::Multipart { .. } => Ok(Some(quote! { ::concord_core::advanced::MultipartBody })),
    }
}

fn endpoint_request_body_plan(ep: &ResolvedEndpoint) -> Result<TokenStream2, TokenStream2> {
    match ep.request_io() {
        ResolvedRequestBodyIo::None => Ok(quote! {
            let __body_plan = ::concord_core::internal::BodyPlan::None;
            let __request_args = ::concord_core::internal::RequestArgs::default();
        }),
        ResolvedRequestBodyIo::BufferedCodec(io) => {
            let enc = &io.marker;
            Ok(quote! {
                let __body_value = self
                    .body
                    .lock()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                    .take()
                    .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                let __encoded_body = <#enc as ::concord_core::advanced::BodyCodec>::encode(
                    __body_value,
                    ::concord_core::advanced::EncodeContext::new(ctx_err.endpoint, &ctx_err.method),
                )
                    .map_err(|e| ::concord_core::prelude::ApiClientError::codec_error(ctx_err.clone(), e))?;
                let (__body_bytes, __body_format) = __encoded_body.into_parts();
                let __body_plan = ::concord_core::internal::BodyPlan::Encoded {
                    content_type: <#enc as ::concord_core::advanced::BodyCodec>::try_content_type()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?,
                    format: __body_format,
                };
                let __request_args = ::concord_core::internal::RequestArgs::with_body_bytes(__body_bytes);
            })
        }
        ResolvedRequestBodyIo::RawStream { media_ty } => {
            Ok(quote! {
                let __body_value = self
                    .body
                    .lock()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                    .take()
                    .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                let __body_plan = ::concord_core::internal::BodyPlan::RawStream {
                    content_type: <#media_ty as ::concord_core::advanced::ContentType>::try_header_value()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?,
                };
                let __request_args = ::concord_core::internal::RequestArgs::with_stream_body(__body_value);
            })
        }
        ResolvedRequestBodyIo::Records { item_ty, format_ty } => {
            Ok(quote! {
                let __body_value = self
                    .body
                    .lock()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                    .take()
                    .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                let __body_plan = ::concord_core::internal::BodyPlan::Records {
                    content_type: <#format_ty as ::concord_core::advanced::ContentType>::try_header_value()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?,
                    format: ::concord_core::internal::Format::Text,
                };
                let __request_args = ::concord_core::internal::RequestArgs::with_record_body::<#item_ty, #format_ty>(__body_value);
            })
        }
        ResolvedRequestBodyIo::Multipart { format_ty, .. } => {
            Ok(quote! {
                let __body_value = self
                    .body
                    .lock()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                    .take()
                    .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
                let __body_plan = ::concord_core::internal::BodyPlan::Multipart {
                    content_type: __body_value
                        .try_content_type::<#format_ty>()
                        .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?,
                    format: ::concord_core::internal::Format::Text,
                };
                let __request_args = ::concord_core::internal::RequestArgs::with_multipart_body::<#format_ty>(__body_value)
                    .map_err(|source| ::concord_core::prelude::ApiClientError::codec_error(
                        ctx_err.clone(),
                        ::concord_core::advanced::CodecError::new(source.to_string()),
                    ))?;
            })
        }
    }
}

fn resolved_response_output_ty(
    response_io: &ResolvedResponseBodyIo,
    map: Option<&MapResolved>,
) -> syn::Type {
    if let Some(map) = map {
        return map.out_ty.clone();
    }

    match response_io {
        ResolvedResponseBodyIo::BufferedCodec(io) => io.value_ty.clone(),
        ResolvedResponseBodyIo::BufferedBytes => syn::parse_quote!(::bytes::Bytes),
        ResolvedResponseBodyIo::NoContent => syn::parse_quote!(()),
        ResolvedResponseBodyIo::RawStream { media_ty } => {
            syn::parse_quote!(::concord_core::advanced::StreamResponse<#media_ty>)
        }
        ResolvedResponseBodyIo::Records { item_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::RecordStream<#item_ty>)
        }
        ResolvedResponseBodyIo::Multipart { part_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::MultipartStream<#part_ty>)
        }
        ResolvedResponseBodyIo::Sse { event_ty, .. } => {
            syn::parse_quote!(::concord_core::advanced::SseStream<#event_ty>)
        }
    }
}

fn endpoint_response_output_ty(ep: &ResolvedEndpoint) -> TokenStream2 {
    let response_ty = resolved_response_output_ty(ep.response_io(), ep.map.as_ref());
    quote! { #response_ty }
}

fn endpoint_response_accept_tokens(ep: &ResolvedEndpoint, response_dec: &syn::Type) -> TokenStream2 {
    match ep.response_io() {
        ResolvedResponseBodyIo::NoContent => quote! { ::core::option::Option::None },
        ResolvedResponseBodyIo::BufferedBytes => quote! { ::core::option::Option::None },
        ResolvedResponseBodyIo::Multipart { format_ty, .. } => quote! {
            ::core::option::Option::Some(
                <#format_ty as ::concord_core::advanced::ContentType>::try_header_value()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?
            )
        },
        ResolvedResponseBodyIo::Sse { .. } => quote! {
            ::core::option::Option::Some(
                <::concord_core::advanced::EventStream as ::concord_core::advanced::ContentType>::try_header_value()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?
            )
        },
        ResolvedResponseBodyIo::Records { format_ty, .. } => quote! {
            ::core::option::Option::Some(
                <#format_ty as ::concord_core::advanced::ContentType>::try_header_value()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?
            )
        },
        ResolvedResponseBodyIo::RawStream { media_ty } => quote! {
            ::core::option::Option::Some(
                <#media_ty as ::concord_core::advanced::ContentType>::try_header_value()
                    .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?
            )
        },
        _ => quote! {
            <#response_dec as ::concord_core::advanced::ResponseCodec>::try_accept()
                .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "content_type"))?
        },
    }
}

fn endpoint_response_no_content_tokens(
    ep: &ResolvedEndpoint,
    response_dec: &syn::Type,
) -> TokenStream2 {
    match ep.response_io() {
        ResolvedResponseBodyIo::NoContent => quote! { true },
        ResolvedResponseBodyIo::BufferedBytes => quote! { false },
        ResolvedResponseBodyIo::RawStream { .. }
        | ResolvedResponseBodyIo::Records { .. }
        | ResolvedResponseBodyIo::Multipart { .. }
        | ResolvedResponseBodyIo::Sse { .. } => {
            quote! { false }
        }
        _ => quote! { <#response_dec as ::concord_core::advanced::ResponseCodec>::is_no_content() },
    }
}

fn endpoint_response_format_tokens(
    ep: &ResolvedEndpoint,
    response_dec: &syn::Type,
) -> TokenStream2 {
    match ep.response_io() {
        ResolvedResponseBodyIo::NoContent => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::BufferedBytes => quote! { ::concord_core::internal::Format::Binary },
        ResolvedResponseBodyIo::Multipart { .. } => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::RawStream { .. } => quote! { ::concord_core::internal::Format::Binary },
        ResolvedResponseBodyIo::Records { .. } => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::Sse { .. } => quote! { ::concord_core::internal::Format::Text },
        _ => quote! { <#response_dec as ::concord_core::advanced::ResponseCodec>::format() },
    }
}

fn endpoint_response_decode_fn(
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    response_dec: &syn::Type,
    decoded_ty: &syn::Type,
) -> TokenStream2 {
    let decode_fn = emit_helpers::ident(&format!("__decode_{ty_name}"), Span::call_site());
    match ep.response_io() {
        ResolvedResponseBodyIo::NoContent => quote! {
                fn #decode_fn(
                    resp: ::concord_core::transport::BuiltResponse,
                    _ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    let out = ::concord_core::transport::DecodedResponse {
                        meta: resp.meta,
                        url: resp.url,
                        status: resp.status,
                        headers: resp.headers,
                        value: (),
                    };
                    ::core::result::Result::Ok(::std::boxed::Box::new(out))
                }
            },
        ResolvedResponseBodyIo::BufferedBytes => {
            let decode_body = if let Some(map) = &ep.map {
                let out_ty = &map.out_ty;
                let body = &map.body;
                quote! {
                    let r: ::bytes::Bytes = decoded;
                    let value: #out_ty = (#body);
                }
            } else {
                quote! { let value: ::bytes::Bytes = decoded; }
            };

            quote! {
                fn #decode_fn(
                    resp: ::concord_core::transport::BuiltResponse,
                    _ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    let ::concord_core::transport::BuiltResponse { meta, url, status, headers, body, .. } = resp;
                    let decoded: ::bytes::Bytes = body;
                    #decode_body
                    let out = ::concord_core::transport::DecodedResponse {
                        meta,
                        url,
                        status,
                        headers,
                        value,
                    };
                    ::core::result::Result::Ok(::std::boxed::Box::new(out))
                }
            }
        }
        ResolvedResponseBodyIo::Multipart { .. } => quote! {
                fn #decode_fn(
                    _resp: ::concord_core::transport::BuiltResponse,
                    ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    Err(::concord_core::prelude::ApiClientError::PolicyViolation {
                        ctx,
                        msg: "multipart responses must use multipart execution".into(),
                    })
                }
            },
        ResolvedResponseBodyIo::RawStream { .. } => quote! {
                fn #decode_fn(
                    _resp: ::concord_core::transport::BuiltResponse,
                    ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    Err(::concord_core::prelude::ApiClientError::PolicyViolation {
                        ctx,
                        msg: "stream responses must use stream execution".into(),
                    })
                }
            },
        ResolvedResponseBodyIo::Records { .. } => quote! {
                fn #decode_fn(
                    _resp: ::concord_core::transport::BuiltResponse,
                    ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    Err(::concord_core::prelude::ApiClientError::PolicyViolation {
                        ctx,
                        msg: "record responses must use record execution".into(),
                    })
                }
            },
        ResolvedResponseBodyIo::Sse { .. } => quote! {
                fn #decode_fn(
                    _resp: ::concord_core::transport::BuiltResponse,
                    ctx: ::concord_core::error::ErrorContext,
                ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                    Err(::concord_core::prelude::ApiClientError::PolicyViolation {
                        ctx,
                        msg: "sse responses must use sse execution".into(),
                    })
                }
            },
        _ => {
                let decode_body = if let Some(map) = &ep.map {
                    let out_ty = &map.out_ty;
                    let body = &map.body;
                    quote! {
                        let r: #decoded_ty = decoded;
                        let value: #out_ty = (#body);
                    }
                } else {
                    quote! { let value: #decoded_ty = decoded; }
                };

                quote! {
                    fn #decode_fn(
                        resp: ::concord_core::transport::BuiltResponse,
                        ctx: ::concord_core::error::ErrorContext,
                    ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                        let __content_type = resp
                            .headers
                            .get(::http::header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok());
                        let decoded: #decoded_ty = <#response_dec as ::concord_core::advanced::ResponseCodec>::decode(
                            resp.body.clone(),
                            ::concord_core::advanced::DecodeContext::new(
                                ctx.endpoint,
                                &ctx.method,
                                resp.status,
                                __content_type,
                            ),
                        )
                            .map_err(|e| {
                                let content_type = resp
                                    .headers
                                    .get(::http::header::CONTENT_TYPE)
                                    .and_then(|value| value.to_str().ok());
                                ::concord_core::prelude::ApiClientError::decode_error(ctx.clone(), resp.status, content_type, e)
                            })?;
                        #decode_body
                        let out = ::concord_core::transport::DecodedResponse {
                            meta: resp.meta,
                            url: resp.url,
                            status: resp.status,
                            headers: resp.headers,
                            value,
                        };
                        ::core::result::Result::Ok(::std::boxed::Box::new(out))
                    }
                }
            }
    }
}

fn endpoint_execute_override(ep: &ResolvedEndpoint, cx_ty: &Ident) -> TokenStream2 {
    match ep.response_io() {
        ResolvedResponseBodyIo::Multipart { part_ty, format_ty } => quote! {
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
                    client.execute_plan_multipart::<#part_ty, #format_ty>(plan).await
                })
            }
        },
        ResolvedResponseBodyIo::Sse { event_ty, codec_ty } => quote! {
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
                    client.execute_plan_sse::<#event_ty, #codec_ty>(plan).await
                })
            }
        },
        ResolvedResponseBodyIo::RawStream { media_ty } => quote! {
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
                    client.execute_plan_stream::<#media_ty>(plan).await
                })
            }
        },
        ResolvedResponseBodyIo::Records { item_ty, format_ty } => quote! {
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
                    client.execute_plan_records::<#item_ty, #format_ty>(plan).await
                })
            }
        },
        _ => quote! {},
    }
}

fn endpoint_response_marker_impl(
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    match ep.response_io() {
        ResolvedResponseBodyIo::Multipart { part_ty, format_ty } => quote! {
            impl ::concord_core::prelude::MultipartResponseEndpoint<super::#cx_ty> for #ty_name {
                type Part = #part_ty;
                type Format = #format_ty;
            }
        },
        ResolvedResponseBodyIo::Sse { event_ty, codec_ty } => quote! {
            impl ::concord_core::prelude::SseResponseEndpoint<super::#cx_ty> for #ty_name {
                type Event = #event_ty;
                type Codec = #codec_ty;
            }
        },
        ResolvedResponseBodyIo::RawStream { media_ty } => quote! {
            impl ::concord_core::prelude::StreamResponseEndpoint<super::#cx_ty> for #ty_name {
                type Media = #media_ty;
            }
        },
        ResolvedResponseBodyIo::Records { item_ty, format_ty } => quote! {
            impl ::concord_core::prelude::RecordResponseEndpoint<super::#cx_ty> for #ty_name {
                type Item = #item_ty;
                type Format = #format_ty;
            }
        },
        _ => quote! {},
    }
}




