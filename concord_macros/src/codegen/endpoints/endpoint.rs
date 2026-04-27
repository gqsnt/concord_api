fn emit_endpoint_def(
    ir: &Ir,
    ep: &EndpointIr,
    ty_name: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let method = &ep.method;
    let endpoint_name_str = endpoint_qualified_name(ep);
    let endpoint_name = LitStr::new(&endpoint_name_str, ep.name.span());

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

    let required_vars: Vec<&VarInfo> = ep
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
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
    let mut init_parts: Vec<TokenStream2> = init_fields.collect();
    if ep.body.is_some() {
        init_parts.push(quote! { body });
    }

    let response_dec = &ep.response.enc;
    let decoded_ty = &ep.response.ty;
    let final_response_ty = ep
        .map
        .as_ref()
        .map(|m| m.out_ty.clone())
        .unwrap_or_else(|| ep.response.ty.clone());
    let decode_fn = emit_helpers::ident(&format!("__decode_{ty_name}"), Span::call_site());
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
    let response_decode_fn = quote! {
        fn #decode_fn(
            resp: ::concord_core::transport::BuiltResponse,
            ctx: ::concord_core::prelude::ErrorContext,
        ) -> ::core::result::Result<::std::boxed::Box<dyn ::std::any::Any + Send>, ::concord_core::prelude::ApiClientError> {
            let decoded = <#response_dec as ::concord_core::internal::Decodes<#decoded_ty>>::decode(&resp.body)
                .map_err(|e| ::concord_core::prelude::ApiClientError::Decode { ctx: ctx.clone(), source: e.into() })?;
            #decode_body
            let out = ::concord_core::prelude::DecodedResponse {
                meta: resp.meta,
                url: resp.url,
                status: resp.status,
                headers: resp.headers,
                value,
            };
            ::core::result::Result::Ok(::std::boxed::Box::new(out))
        }
    };

    let route_policy =
        emit_endpoint_plan_route_policy(ep, method, &endpoint_name, cx_ty, response_dec);
    let auth_plan = emit_endpoint_auth_plan(ir, ep);
    let body_plan = if let Some(body) = &ep.body {
        let enc = &body.enc;
        let ty = &body.ty;
        quote! {
            let __body_bytes = <#enc as ::concord_core::internal::Encodes<#ty>>::encode(&self.body)
                .map_err(|e| ::concord_core::prelude::ApiClientError::codec_error(ctx_err.clone(), e))?;
            let __body_plan = ::concord_core::internal::BodyPlan::Encoded {
                content_type: <#enc as ::concord_core::internal::ContentType>::CONTENT_TYPE,
                format: <#enc as ::concord_core::internal::FormatType>::FORMAT_TYPE,
            };
            let __request_args = ::concord_core::internal::RequestArgs { body: ::core::option::Option::Some(__body_bytes) };
        }
    } else {
        quote! {
            let __body_plan = ::concord_core::internal::BodyPlan::None;
            let __request_args = ::concord_core::internal::RequestArgs::default();
        }
    };

    let pagination_plan = emit_endpoint_pagination_plan(ep);

    quote! {
        pub struct #ty_name {
            #( #struct_fields, )*
        }

        impl #ty_name {
            #[inline]
            pub fn new( #( #fn_args ),* ) -> Self {
                Self { #( #init_parts, )* }
            }
            #( #setters_ts )*
        }

        #response_decode_fn

        impl ::concord_core::prelude::Endpoint<super::#cx_ty> for #ty_name {
            type Response = #final_response_ty;

            fn plan(
                &self,
                plan_ctx: &::concord_core::internal::ClientPlanContext<'_, super::#cx_ty>,
            ) -> ::core::result::Result<::concord_core::internal::RequestPlan, ::concord_core::prelude::ApiClientError> {
                let vars = plan_ctx.vars;
                let auth = plan_ctx.auth_vars;
                let ep = self;
                let ctx_err = ::concord_core::prelude::ErrorContext { endpoint: #endpoint_name, method: ::http::Method::#method };
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
                            idempotent: matches!(::http::Method::#method, ::http::Method::GET | ::http::Method::HEAD | ::http::Method::PUT | ::http::Method::DELETE | ::http::Method::OPTIONS),
                            facade_path: &[],
                        },
                        route: __resolved_route,
                        policy: __resolved_policy,
                        body: __body_plan,
                        response: ::concord_core::internal::ResponsePlan {
                            accept: <#response_dec as ::concord_core::internal::ContentType>::CONTENT_TYPE,
                            no_content: <#response_dec as ::concord_core::internal::ContentType>::IS_NO_CONTENT,
                            format: <#response_dec as ::concord_core::internal::FormatType>::FORMAT_TYPE,
                            decode: #decode_fn,
                        },
                        pagination: __pagination_plan,
                    },
                    args: __request_args,
                    overrides: ::concord_core::internal::RequestOverrides::default(),
                })
            }
        }
    }
}

fn emit_endpoint_plan_route_policy(
    ep: &EndpointIr,
    method: &Ident,
    _endpoint_name: &LitStr,
    cx_ty: &Ident,
    response_dec: &syn::Path,
) -> TokenStream2 {
    let ep_opt = ep_optionals(ep);
    let prefix_layer_route_ops = emit_prefix_route_apply(&ep.prefix_pieces, Some(&ep_opt));
    let path_layer_route_ops = emit_path_route_apply(&ep.path_layer_pieces, Some(&ep_opt));
    let layer_policy_ops = ep.policy_layers.iter().map(|policy_layer| {
        let layer_policy_apply = emit_policy_apply_fn(policy_layer, PolicyEmitCtx::Layer);
        quote! {
            {
                let __prev = policy.layer();
                policy.set_layer(::concord_core::prelude::PolicyLayer::PrefixPath);
                #layer_policy_apply
                policy.set_layer(__prev);
            }
        }
    });
    let endpoint_route_apply = emit_path_route_apply(&ep.route_pieces, Some(&ep_opt));
    let endpoint_policy_apply = emit_policy_apply_fn(&ep.policy, PolicyEmitCtx::Endpoint);
    quote! {
        let mut route = <super::#cx_ty as ::concord_core::prelude::ClientContext>::base_route(vars, auth);
        #prefix_layer_route_ops
        #path_layer_route_ops
        #endpoint_route_apply
        route.host().validate(ctx_err.clone())?;
        let __resolved_route = ::concord_core::internal::ResolvedRoute {
            scheme: <super::#cx_ty as ::concord_core::prelude::ClientContext>::SCHEME,
            host: route.host().join(<super::#cx_ty as ::concord_core::prelude::ClientContext>::DOMAIN),
            path: route.path().as_str().to_string(),
        };

        let mut policy = <super::#cx_ty as ::concord_core::prelude::ClientContext>::base_policy(vars, auth, &ctx_err)?;
        #( #layer_policy_ops )*
        {
            let __prev = policy.layer();
            policy.set_layer(::concord_core::prelude::PolicyLayer::Endpoint);
            #endpoint_policy_apply
            policy.set_layer(__prev);
        }
        policy.set_layer(::concord_core::prelude::PolicyLayer::Runtime);
        if ::http::Method::#method != ::http::Method::HEAD
            && !<#response_dec as ::concord_core::internal::ContentType>::IS_NO_CONTENT
        {
            policy.ensure_accept(<#response_dec as ::concord_core::internal::ContentType>::CONTENT_TYPE);
        }
        let (headers, query, timeout, cache, retry, mut rate_limit) = policy.into_parts();
        rate_limit.canonicalize();
        let __resolved_policy = ::concord_core::prelude::ResolvedPolicy {
            headers,
            query,
            timeout,
            auth: __auth_plan,
            cache,
            retry,
            rate_limit,
        };
    }
}


fn emit_endpoint_pagination_plan(ep: &EndpointIr) -> TokenStream2 {
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
    let auto_key_assigns = p.assigns.iter().filter_map(|(k, v)| {
        let ValueKind::EpField(f) = v else { return None; };
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
            ValueKind::EpField(f) => quote! { ep.#f.clone() },
            ValueKind::LitStr(s) => quote! { ::std::borrow::Cow::from(#s) },
            ValueKind::CxField(f) => quote! { vars.#f.clone() },
            ValueKind::AuthField(_) => quote! {{ compile_error!("paginate auth vars are not supported in v4 controller construction"); ::core::unreachable!() }},
            ValueKind::OtherExpr(e) => quote! { (#e) },
            ValueKind::Fmt(fmt) => {
                let build = emit_fmt_build_string(fmt);
                quote! { { #build } }
            }
        };
        quote! { ctrl.#k = #val; }
    });
    quote! {
        #[allow(unused_variables)]
        let cx = vars;
        let mut ctrl: #ctrl_ty = ::core::default::Default::default();
        #( #auto_key_assigns )*
        #( #assigns )*
        let __pagination_plan = ::core::option::Option::Some(::concord_core::internal::PaginationPlan::from(ctrl));
    }
}
