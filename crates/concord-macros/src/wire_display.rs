//! `#[derive(WireDisplay)]`: `Display` as the value's scalar serde spelling.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

pub(crate) fn derive(input: syn::DeriveInput) -> TokenStream {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    // The bound names the user's type, so a missing `Serialize` is reported on it.
    let bound: syn::WherePredicate = syn::parse_quote_spanned! {name.span()=>
        #name #ty_generics: ::concord::__macro::Serialize
    };
    generics.make_where_clause().predicates.push(bound);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let wire_text = quote_spanned! {name.span()=> ::concord::__macro::wire_text };
    quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #wire_text(self, f)
            }
        }
    }
}
