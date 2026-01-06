use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = match input.data {
        Data::Struct(s) => match s.fields {
            Fields::Named(n) => n.named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "ControllerBuild requires a struct with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                name,
                "ControllerBuild can only be derived for structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut arms = Vec::new();
    for f in fields {
        let ident = f.ident.unwrap();
        let key = ident.to_string();
        let ty = f.ty;
        arms.push(quote! {
            #key => {
                if let ::core::option::Option::Some(v) = value.into_typed::<#ty>() {
                    self.#ident = v;
                    return ::core::result::Result::Ok(());
                }
                if let ::core::option::Option::Some(v) = value.into_option_field::<#ty>() {
                    // If field is Option<T>, above branch already handled.
                    // If field is T, this branch is ignored by typing.
                    let _ = v;
                }
                ::core::result::Result::Err(::concord_core::prelude::ApiClientError::ControllerConfig {
                    key,
                    expected: stringify!(#ty),
                })
            }
        });
    }

    let expanded = quote! {
        impl ::concord_core::prelude::ControllerBuild for #name {
            fn set_kv(
                &mut self,
                key: &'static str,
                value: ::concord_core::prelude::ControllerValue
            ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                match key {
                    #(#arms,)*
                    _ => ::core::result::Result::Err(::concord_core::prelude::ApiClientError::ControllerConfig {
                        key,
                        expected: "known key",
                    }),
                }
            }
        }
    };
    expanded.into()
}
