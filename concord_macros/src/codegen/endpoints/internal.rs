fn emit_internal(ir: &Ir, vars_ty: &Ident, auth_vars_ty: &Ident, cx_ty: &Ident) -> TokenStream2 {
    let endpoint_parts = ir
        .endpoints
        .iter()
        .map(|ep| emit_endpoint_parts(ir, ep, vars_ty, auth_vars_ty, cx_ty));

    quote! {
        mod __internal {
            use super::*;
            #( #endpoint_parts )*
        }
    }
}

fn emit_endpoint_parts(
    ir: &Ir,
    ep: &EndpointIr,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    let endpoint_ty = endpoint_internal_ident(ep);
    let method = &ep.method;
    let route_ty = emit_helpers::ident(&format!("__Route_{endpoint_ty}"), Span::call_site());
    let policy_ty = emit_helpers::ident(&format!("__Policy_{endpoint_ty}"), Span::call_site());

    let ep_opt = ep_optionals(ep);

    let mut prefix_layer_route_ops: Vec<TokenStream2> = Vec::new();
    let mut path_layer_route_ops: Vec<TokenStream2> = Vec::new();
    let mut layer_policy_ops: Vec<TokenStream2> = Vec::new();
    for &lid in &ep.ancestry {
        let layer = &ir.layers[lid];
        let layer_route = match layer.kind {
            crate::ast::LayerKind::Prefix => {
                emit_prefix_route_apply(&layer.prefix_pieces, Some(&ep_opt))
            }
            crate::ast::LayerKind::Path => emit_path_route_apply(&layer.path_pieces, Some(&ep_opt)),
        };
        match layer.kind {
            crate::ast::LayerKind::Prefix => prefix_layer_route_ops.push(layer_route),
            crate::ast::LayerKind::Path => path_layer_route_ops.push(layer_route),
        }

        let layer_policy_apply = emit_policy_apply_fn(&layer.policy, PolicyEmitCtx::Layer);
        layer_policy_ops.push(quote! {
            {
                let __prev = policy.layer();
                policy.set_layer(::concord_core::prelude::PolicyLayer::PrefixPath);
                #layer_policy_apply
                policy.set_layer(__prev);
            }
        });
    }

    let endpoint_route_apply = emit_path_route_apply(&ep.route_pieces, Some(&ep_opt));
    let route_apply = quote! {
        #( #prefix_layer_route_ops )*
        #( #path_layer_route_ops )*
        #endpoint_route_apply
    };

    let endpoint_policy_apply = emit_policy_apply_fn(&ep.policy, PolicyEmitCtx::Endpoint);
    let policy_apply = quote! {
        #( #layer_policy_ops )*
        {
            let __prev = policy.layer();
            policy.set_layer(::concord_core::prelude::PolicyLayer::Endpoint);
            #endpoint_policy_apply
            policy.set_layer(__prev);
        }
    };

    let paginate_ty = emit_helpers::ident(&format!("__Pag_{endpoint_ty}"), Span::call_site());
    let paginate_impl = emit_paginate_part(ep, &paginate_ty, cx_ty, vars_ty);

    let map_ty = emit_helpers::ident(&format!("__Map_{endpoint_ty}"), Span::call_site());
    let map_impl = emit_map_part(ep, &map_ty);

    let auth_impl = emit_auth_parts(ir, ep, cx_ty);

    quote! {
        pub struct #route_ty;
        impl ::concord_core::internal::RoutePart<super::#cx_ty, super::__endpoints::#endpoint_ty> for #route_ty {
            fn apply(
                    ep: &super::__endpoints::#endpoint_ty,
                    vars: &super::#vars_ty,
                    auth: &super::#auth_vars_ty,
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
        impl ::concord_core::internal::PolicyPart<super::#cx_ty, super::__endpoints::#endpoint_ty> for #policy_ty {
            fn apply(ep: &super::__endpoints::#endpoint_ty, vars: &super::#vars_ty, auth: &super::#auth_vars_ty, policy: &mut ::concord_core::prelude::Policy)
                -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
            {
                let ctx = ::concord_core::prelude::ErrorContext {
                    endpoint: ep.name(),
                    method: ::http::Method::#method,
                };
                #policy_apply
                ::core::result::Result::Ok(())
            }
        }

        #auth_impl
        #paginate_impl
        #map_impl
    }
}

