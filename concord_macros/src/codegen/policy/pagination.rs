fn find_query_key_for_ep_field<'a>(ep: &'a EndpointIr, field: &Ident) -> Option<&'a KeyResolved> {
    // Take the last matching query op (closest to the endpoint) if multiple exist.
    ep.policy.query.iter().rev().find_map(|op| match op {
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
    let endpoint_ty = endpoint_internal_ident(ep);

    let Some(p) = &ep.paginate else {
        return quote! {
            pub struct #paginate_ty;
            impl ::concord_core::internal::PaginationPart<super::#cx_ty, super::__endpoints::#endpoint_ty> for #paginate_ty {
                type Ctrl = ::concord_core::internal::NoController;
                fn controller(
                    _vars: &super::#vars_ty,
                    _ep: &super::__endpoints::#endpoint_ty
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

        impl ::concord_core::internal::PaginationPart<super::#cx_ty, super::__endpoints::#endpoint_ty> for #paginate_ty {
            type Ctrl = #ctrl_ty;

            fn controller(
                vars: &super::#vars_ty,
                ep: &super::__endpoints::#endpoint_ty
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

