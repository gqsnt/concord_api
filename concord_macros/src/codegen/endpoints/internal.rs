fn emit_internal(_ir: &Ir, _vars_ty: &Ident, _auth_vars_ty: &Ident, _cx_ty: &Ident) -> TokenStream2 {
    quote! {
        mod __internal {
            use super::*;
        }
    }
}
