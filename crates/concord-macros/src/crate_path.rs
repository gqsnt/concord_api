use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

pub(crate) fn resolve_runtime_path() -> syn::Result<TokenStream> {
    match crate_name("concord") {
        Ok(FoundCrate::Itself) => Ok(quote!(crate)),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            Ok(quote!(::#ident))
        }
        Err(_) => Err(syn::Error::new(
            Span::call_site(),
            "Concord `api!` requires the `concord` package; add it to `[dependencies]` (renaming the dependency is supported)",
        )),
    }
}
