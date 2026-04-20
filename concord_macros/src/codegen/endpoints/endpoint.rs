fn emit_endpoint_def(ep: &EndpointIr, ty_name: &Ident, cx_ty: &Ident) -> TokenStream2 {
    let method = &ep.method;
    let endpoint_name_str = endpoint_qualified_name(ep);
    let endpoint_name = LitStr::new(&endpoint_name_str, ep.name.span());

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

    let route_ident = emit_helpers::ident(&format!("__Route_{ty_name}"), Span::call_site());
    let policy_ident = emit_helpers::ident(&format!("__Policy_{ty_name}"), Span::call_site());
    let route_ty = quote! { super::__internal::#route_ident };
    let policy_ty = quote! { super::__internal::#policy_ident };
    let auth_ty = emit_endpoint_auth_ty(ep);

    // pagination part
    let pagination_ty = if ep.paginate.is_some() {
        let p_ident = emit_helpers::ident(&format!("__Pag_{ty_name}"), Span::call_site());
        quote! { super::__internal::#p_ident }
    } else {
        quote! { ::concord_core::internal::NoPagination }
    };

    // body part
    let body_ty = if ep.body.is_some() {
        let b_ident = emit_helpers::ident(&format!("__Body_{ty_name}"), Span::call_site());
        quote! { #b_ident }
    } else {
        quote! { ::concord_core::internal::NoBody }
    };

    // response spec
    let dec_enc = &ep.response.enc;
    let decoded_ty = &ep.response.ty;
    let response_base = quote! { ::concord_core::internal::Decoded<#dec_enc, #decoded_ty> };

    let response_ty = if ep.map.is_some() {
        let m_ident = emit_helpers::ident(&format!("__Map_{ty_name}"), Span::call_site());
        quote! { ::concord_core::internal::Mapped<#response_base, super::__internal::#m_ident> }
    } else {
        response_base
    };

    // BodyPart impl if needed
    let body_impl = if let Some(body) = &ep.body {
        let enc = &body.enc;
        let ty = &body.ty;
        let b_ident = emit_helpers::ident(&format!("__Body_{ty_name}"), Span::call_site());
        quote! {
            pub struct #b_ident;
            impl ::concord_core::internal::BodyPart<#ty_name> for #b_ident {
                type Body = #ty;
                type Enc = #enc;
                fn body(ep: &#ty_name) -> ::core::option::Option<&Self::Body> {
                    ::core::option::Option::Some(&ep.body)
                }
            }
        }
    } else {
        quote! {}
    };

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

        #body_impl

        impl ::concord_core::prelude::Endpoint<super::#cx_ty> for #ty_name {
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

