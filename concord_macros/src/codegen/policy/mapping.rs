fn emit_map_part(ep: &EndpointIr, map_ty: &Ident) -> TokenStream2 {
    let _name = &ep.name;
    let Some(m) = &ep.map else {
        return quote! {};
    };

    let dec_ty = &ep.response.ty;
    let out_ty = &m.out_ty;
    let body = &m.body;

    quote! {
        pub struct #map_ty;

        impl ::concord_core::internal::Transform<#dec_ty> for #map_ty {
            type Out = #out_ty;
            fn map(v: #dec_ty) -> ::core::result::Result<Self::Out, ::concord_core::prelude::FxError> {
                let r: #dec_ty = v;
                let out: #out_ty = (#body);
                ::core::result::Result::Ok(out)
            }
        }
    }
}

