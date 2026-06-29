fn endpoint_is_websocket(ep: &ResolvedEndpoint) -> bool {
    ep.method == "WS"
}

fn endpoint_http_method_ident(ep: &ResolvedEndpoint) -> Ident {
    if endpoint_is_websocket(ep) {
        emit_helpers::ident("GET", ep.method.span())
    } else {
        ep.method.clone()
    }
}

fn emit_endpoint_def(
    resolved_api: &ResolvedApi,
    facade: &FacadeEndpoint,
    ep: &ResolvedEndpoint,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let method = endpoint_http_method_ident(ep);
    let is_websocket = endpoint_is_websocket(ep);
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
                let set = emit_helpers::ident(&setter.set_name, f.span());
                let opt = emit_helpers::ident(&setter.set_optional_name, f.span());
                let clear = emit_helpers::ident(&setter.clear_name, f.span());
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
                    let set = emit_helpers::ident(&setter.set_name, f.span());
                    let opt = emit_helpers::ident(&setter.set_optional_name, f.span());
                    let clear = emit_helpers::ident(&setter.clear_name, f.span());
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

    let response_dec = &ep.response.marker;
    let decoded_ty = &ep.response.ty;
    let final_response_ty = endpoint_response_output_ty(ep);
    let decode_fn = emit_helpers::ident(&format!("__decode_{ty_name}"), Span::call_site());
    let response_decode_fn = endpoint_response_decode_fn(ep, ty_name, response_dec, decoded_ty);
    let response_plan_accept = endpoint_response_accept_tokens(ep, response_dec);
    let response_plan_no_content = endpoint_response_no_content_tokens(ep, response_dec);
    let response_plan_format = endpoint_response_format_tokens(ep, response_dec);

    let route_policy = emit_endpoint_plan_route_policy(
        ep,
        &method,
        &endpoint_name,
        cx_ty,
        &response_plan_accept,
        &response_plan_no_content,
        is_websocket,
    );
    let auth_plan = emit_endpoint_auth_plan(resolved_api, ep);
    let body_plan = match endpoint_request_body_plan(ep) {
        Ok(body_plan) => body_plan,
        Err(err) => return err,
    };
    let execute_override = endpoint_execute_override(ep, cx_ty);
    let response_marker_impl = endpoint_response_marker_impl(ep, ty_name, cx_ty);

    let pagination_plan = emit_endpoint_pagination_plan(ep);
    let pagination_marker_impl = if ep.paginate.is_some() {
        quote! {
            impl ::concord_core::prelude::PaginatedEndpoint<super::#cx_ty> for #ty_name {}
        }
    } else {
        quote! {}
    };
    let pending_ext_trait = endpoint_pending_ext_trait_ident(ep);
    let pending_setter_decls = facade.setters.iter().filter_map(|setter| {
        let v = endpoint_var_for_setter(ep, setter)?;
        let f = &v.rust;
        let ty = &v.ty;
        let set = emit_helpers::ident(&setter.set_name, f.span());
        let opt = emit_helpers::ident(&setter.set_optional_name, f.span());
        let clear = emit_helpers::ident(&setter.clear_name, f.span());
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
    });
    let pending_setter_impls = facade.setters.iter().filter_map(|setter| {
        let v = endpoint_var_for_setter(ep, setter)?;
        let f = &v.rust;
        let ty = &v.ty;
        let set = emit_helpers::ident(&setter.set_name, f.span());
        let opt = emit_helpers::ident(&setter.set_optional_name, f.span());
        let clear = emit_helpers::ident(&setter.clear_name, f.span());
        Some(
            quote! {
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
            },
        )
    });

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
                            idempotent: if #is_websocket {
                                false
                            } else {
                                matches!(::http::Method::#method, ::http::Method::GET | ::http::Method::HEAD | ::http::Method::PUT | ::http::Method::DELETE | ::http::Method::OPTIONS)
                            },
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
}

fn emit_endpoint_plan_route_policy(
    ep: &ResolvedEndpoint,
    method: &Ident,
    _endpoint_name: &LitStr,
    cx_ty: &Ident,
    response_accept: &TokenStream2,
    response_no_content: &TokenStream2,
    is_websocket: bool,
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
        if !#is_websocket
            && ::http::Method::#method != ::http::Method::HEAD
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
    let ctrl_ty = &p.ctrl_ty;
    let ctrl_last = ctrl_ty.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
    let is_cursor = ctrl_last == "CursorPagination";
    let is_offset_limit = ctrl_last == "OffsetLimitPagination";
    let is_paged = ctrl_last == "PagedPagination";
    if !is_cursor && !is_offset_limit && !is_paged {
        let page_ty = ep
            .map
            .as_ref()
            .map(|m| m.out_ty.clone())
            .unwrap_or_else(|| ep.response.ty.clone());
        return quote! {
            let __pagination_plan = ::core::option::Option::Some(
                ::concord_core::internal::PaginationPlan::custom::<#ctrl_ty, #page_ty>()
            );
        };
    }
    let page_ty = ep
        .map
        .as_ref()
        .map(|m| m.out_ty.clone())
        .unwrap_or_else(|| ep.response.ty.clone());
    let auto_key_assigns = p.assigns.iter().filter_map(|(k, v)| {
        let PaginationValueKind::EpField(f) = v else { return None; };
        let key_res = find_query_key_for_ep_field(ep, f)?;
        let (_ks, _sp, key_ts) = emit_key_string(key_res, PolicyKeyKind::Query);
        let k_str = k.to_string();
        if is_cursor {
            if k_str == "cursor" { return Some(quote! { ctrl.cursor_key = ::std::borrow::Cow::from(#key_ts); }); }
            if k_str == "per_page" { return Some(quote! { ctrl.per_page_key = ::std::borrow::Cow::from(#key_ts); }); }
        }
        if is_offset_limit {
            if k_str == "offset" { return Some(quote! { ctrl.offset_key = ::std::borrow::Cow::from(#key_ts); }); }
            if k_str == "limit" { return Some(quote! { ctrl.limit_key = ::std::borrow::Cow::from(#key_ts); }); }
        }
        if is_paged {
            if k_str == "page" { return Some(quote! { ctrl.page_key = ::std::borrow::Cow::from(#key_ts); }); }
            if k_str == "per_page" { return Some(quote! { ctrl.per_page_key = ::std::borrow::Cow::from(#key_ts); }); }
        }
        None
    });
    let assigns = p.assigns.iter().map(|(k, v)| {
        let val = match v {
            PaginationValueKind::EpField(f) => quote! { ep.#f.clone() },
            PaginationValueKind::LitStr(s) => quote! { ::std::borrow::Cow::from(#s) },
            PaginationValueKind::OtherExpr(e) => quote! { (#e) },
            PaginationValueKind::Fmt(fmt) => {
                let build = emit_fmt_build_string(fmt);
                quote! { { #build } }
            }
        };
        quote! { ctrl.#k = #val; }
    });
    let plan_expr = if is_cursor {
        quote! { ::concord_core::internal::PaginationPlan::cursor::<#page_ty>(ctrl) }
    } else {
        quote! { ::concord_core::internal::PaginationPlan::from(ctrl) }
    };
    quote! {
        #[allow(unused_variables)]
        let cx = vars;
        let mut ctrl: #ctrl_ty = ::core::default::Default::default();
        #( #auto_key_assigns )*
        #( #assigns )*
        let __pagination_plan = ::core::option::Option::Some(#plan_expr);
    }
}

fn facade_setter_for_var<'a>(facade: &'a FacadeEndpoint, field: &Ident) -> Option<&'a FacadeSetter> {
    facade
        .setters
        .iter()
        .find(|setter| field == setter.field.as_str())
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
    match &ep.request_io {
        ResolvedRequestBodyIo::None => {
            if ep.body.as_ref().is_some() {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly present in resolved IR",
                    ep.name.span(),
                ));
            }
            Ok(None)
        }
        ResolvedRequestBodyIo::BufferedCodec(_)
        | ResolvedRequestBodyIo::RawStream { .. }
        | ResolvedRequestBodyIo::Records { .. }
        | ResolvedRequestBodyIo::Multipart { .. } => {
            let Some(body) = ep.body.as_ref() else {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly missing from resolved IR",
                    ep.name.span(),
                ));
            };
            if matches!(ep.request_io, ResolvedRequestBodyIo::RawStream { .. }) {
                Ok(Some(quote! { ::concord_core::advanced::StreamBody }))
            } else if matches!(ep.request_io, ResolvedRequestBodyIo::Records { .. }) {
                let ty = &body.ty;
                Ok(Some(quote! { ::concord_core::advanced::RecordBody<#ty> }))
            } else if matches!(ep.request_io, ResolvedRequestBodyIo::Multipart { .. }) {
                Ok(Some(quote! { ::concord_core::advanced::MultipartBody }))
            } else {
                let ty = &body.ty;
                Ok(Some(quote! { #ty }))
            }
        }
        ResolvedRequestBodyIo::BufferedBytes => Err(emit_helpers::compile_error_tokens(
            "`Bytes` endpoint I/O is reserved but not supported yet",
            ep.name.span(),
        )),
    }
}

fn endpoint_request_body_plan(ep: &ResolvedEndpoint) -> Result<TokenStream2, TokenStream2> {
    match &ep.request_io {
        ResolvedRequestBodyIo::None => {
            if ep.body.as_ref().is_some() {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly present in resolved IR",
                    ep.name.span(),
                ));
            }
            Ok(quote! {
            let __body_plan = ::concord_core::internal::BodyPlan::None;
            let __request_args = ::concord_core::internal::RequestArgs::default();
        })
        }
        ResolvedRequestBodyIo::BufferedCodec(_) => {
            let Some(body) = ep.body.as_ref() else {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly missing from resolved IR",
                    ep.name.span(),
                ));
            };
            let enc = &body.marker;
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
            if ep.body.as_ref().is_none() {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly missing from resolved IR",
                    ep.name.span(),
                ));
            }
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
            if ep.body.as_ref().is_none() {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly missing from resolved IR",
                    ep.name.span(),
                ));
            }
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
            if ep.body.as_ref().is_none() {
                return Err(emit_helpers::compile_error_tokens(
                    "endpoint request body unexpectedly missing from resolved IR",
                    ep.name.span(),
                ));
            }
            Ok(quote! {
            let __body_value = self
                .body
                .lock()
                .map_err(|_| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?
                .take()
                .ok_or_else(|| ::concord_core::prelude::ApiClientError::invalid_param(ctx_err.clone(), "body"))?;
            let __body_plan = ::concord_core::internal::BodyPlan::Multipart {
                content_type: __body_value.content_type::<#format_ty>(),
                format: ::concord_core::internal::Format::Text,
            };
            let __request_args = ::concord_core::internal::RequestArgs::with_multipart_body::<#format_ty>(__body_value)
                .map_err(|source| ::concord_core::prelude::ApiClientError::codec_error(
                    ctx_err.clone(),
                    ::concord_core::advanced::CodecError::new(source.to_string()),
                ))?;
        })
        }
        ResolvedRequestBodyIo::BufferedBytes => Err(emit_helpers::compile_error_tokens(
            "`Bytes` endpoint I/O is reserved but not supported yet",
            ep.name.span(),
        )),
    }
}

fn endpoint_response_output_ty(ep: &ResolvedEndpoint) -> TokenStream2 {
    match &ep.response_io {
        ResolvedResponseBodyIo::Multipart { part_ty, .. } => quote! {
            ::concord_core::advanced::MultipartStream<#part_ty>
        },
        ResolvedResponseBodyIo::Sse { event_ty, .. } => quote! {
            ::concord_core::advanced::SseStream<#event_ty>
        },
        ResolvedResponseBodyIo::Records { item_ty, .. } => quote! {
            ::concord_core::advanced::RecordStream<#item_ty>
        },
        ResolvedResponseBodyIo::WebSocket { out_ty, in_ty, .. } => quote! {
            ::concord_core::advanced::WebSocketClient<#out_ty, #in_ty>
        },
        ResolvedResponseBodyIo::RawStream { media_ty } => quote! {
            ::concord_core::advanced::StreamResponse<#media_ty>
        },
        _ => {
            let final_response_ty = ep
                .map
                .as_ref()
                .map(|m| m.out_ty.clone())
                .unwrap_or_else(|| ep.response.ty.clone());
            quote! { #final_response_ty }
        }
    }
}

fn endpoint_response_accept_tokens(ep: &ResolvedEndpoint, response_dec: &syn::Type) -> TokenStream2 {
    match &ep.response_io {
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
        ResolvedResponseBodyIo::WebSocket { .. } => quote! {
            ::core::option::Option::None
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
    match &ep.response_io {
        ResolvedResponseBodyIo::RawStream { .. }
        | ResolvedResponseBodyIo::Records { .. }
        | ResolvedResponseBodyIo::Multipart { .. }
        | ResolvedResponseBodyIo::Sse { .. } => {
            quote! { false }
        }
        ResolvedResponseBodyIo::WebSocket { .. } => quote! { false },
        _ => quote! { <#response_dec as ::concord_core::advanced::ResponseCodec>::is_no_content() },
    }
}

fn endpoint_response_format_tokens(
    ep: &ResolvedEndpoint,
    response_dec: &syn::Type,
) -> TokenStream2 {
    match &ep.response_io {
        ResolvedResponseBodyIo::Multipart { .. } => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::RawStream { .. } => quote! { ::concord_core::internal::Format::Binary },
        ResolvedResponseBodyIo::Records { .. } => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::Sse { .. } => quote! { ::concord_core::internal::Format::Text },
        ResolvedResponseBodyIo::WebSocket { .. } => quote! { ::concord_core::internal::Format::Text },
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
    match &ep.response_io {
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
        ResolvedResponseBodyIo::WebSocket { .. } => quote! {
            fn #decode_fn(
                _resp: ::concord_core::transport::BuiltResponse,
                ctx: ::concord_core::error::ErrorContext,
            ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
                Err(::concord_core::prelude::ApiClientError::PolicyViolation {
                    ctx,
                    msg: "websocket endpoints must use websocket execution".into(),
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
    match &ep.response_io {
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
        ResolvedResponseBodyIo::WebSocket {
            out_ty,
            in_ty,
            codec_ty,
        } => quote! {
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
                    client.execute_plan_websocket::<#out_ty, #in_ty, #codec_ty>(plan).await
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
    match &ep.response_io {
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
        ResolvedResponseBodyIo::WebSocket {
            out_ty,
            in_ty,
            codec_ty,
        } => quote! {
            impl ::concord_core::prelude::WebSocketEndpoint<super::#cx_ty> for #ty_name {
                type Out = #out_ty;
                type In = #in_ty;
                type Codec = #codec_ty;
            }
        },
        ResolvedResponseBodyIo::RawStream { media_ty } => quote! {
            impl ::concord_core::prelude::StreamResponseEndpoint<super::#cx_ty> for #ty_name {
                type Media = #media_ty;
            }
        }
        ,
        ResolvedResponseBodyIo::Records { item_ty, format_ty } => quote! {
            impl ::concord_core::prelude::RecordResponseEndpoint<super::#cx_ty> for #ty_name {
                type Item = #item_ty;
                type Format = #format_ty;
            }
        },
        _ => quote! {},
    }
}




