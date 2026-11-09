//! `#[derive(WireDisplay)]`: `Display` as the value's scalar serde spelling.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

pub(crate) fn derive(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut krate = None;
    for attr in &input.attrs {
        if attr.path().is_ident("wire_display") {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("crate") {
                    return Err(meta.error("expected `crate = path`"));
                }
                if krate.is_some() {
                    return Err(meta.error("duplicate `crate` path"));
                }
                krate = Some(meta.value()?.call(syn::Path::parse_mod_style)?);
                Ok(())
            })?;
        }
    }
    let krate = krate.unwrap_or_else(|| syn::parse_quote_spanned!(name.span()=> ::concord));
    let mut generics = input.generics.clone();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    // The bound names the user's type, so a missing `Serialize` is reported on it.
    let bound: syn::WherePredicate = syn::parse_quote_spanned! {name.span()=>
        #name #ty_generics: #krate::__macro::Serialize
    };
    generics.make_where_clause().predicates.push(bound);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let wire_text = quote_spanned! {name.span()=> #krate::__macro::wire_text };
    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #wire_text(self, f)
            }
        }
    })
}
