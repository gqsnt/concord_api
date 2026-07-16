use std::collections::HashSet;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::Index;

use crate::model::{Api, Parameter, PathAtom};

const GENERATED_CONTRACT_VERSION: u32 = 1;

pub(crate) fn generate(api: &Api, core_path: &TokenStream) -> TokenStream {
    let client = &api.client;
    let scope = &client.scope;
    let endpoint = &scope.endpoint;

    let client_name = &client.name;
    let scope_name = &scope.name;
    let endpoint_name = &endpoint.name;
    let support_name = format_ident!("{}", to_snake_case(&client_name.to_string()));
    let scope_module_name = format_ident!("{}", to_snake_case(&scope_name.to_string()));
    let scope_method_name = &scope_module_name;
    let endpoint_method_name = format_ident!("{}", to_snake_case(&endpoint_name.to_string()));
    let facade_name = format_ident!("{}Facade", to_pascal_case(&scope_name.to_string()));

    let scope_names: Vec<_> = scope
        .parameters
        .iter()
        .map(|parameter| &parameter.name)
        .collect();
    let scope_types: Vec<_> = scope
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect();
    let endpoint_names: Vec<_> = endpoint
        .parameters
        .iter()
        .map(|parameter| &parameter.name)
        .collect();
    let endpoint_types: Vec<_> = endpoint
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect();
    let scope_bindings: Vec<_> = scope
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| parameter_binding("scope", index, &parameter.name))
        .collect();
    let endpoint_bindings: Vec<_> = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| parameter_binding("endpoint", index, &parameter.name))
        .collect();
    let scope_indices: Vec<_> = (0..scope.parameters.len()).map(Index::from).collect();

    let reserved: HashSet<_> = scope
        .parameters
        .iter()
        .chain(endpoint.parameters.iter())
        .map(|parameter| parameter.name.to_string())
        .collect();
    let local_prefix = format!(
        "__concord_{}_{}_{}",
        to_snake_case(&client_name.to_string()),
        to_snake_case(&scope_name.to_string()),
        to_snake_case(&endpoint_name.to_string())
    );
    let url_local = collision_safe_ident(&format!("{local_prefix}_logical_url"), &reserved);
    let segment_local = collision_safe_ident(&format!("{local_prefix}_segment"), &reserved);
    let head_local = collision_safe_ident(&format!("{local_prefix}_head"), &reserved);

    let all_path_atoms: Vec<_> = scope.path.iter().chain(endpoint.path.iter()).collect();
    let path_expressions: Vec<_> = all_path_atoms
        .iter()
        .map(|atom| {
            path_expression(
                atom,
                &scope.parameters,
                &scope_bindings,
                &endpoint.parameters,
                &endpoint_bindings,
            )
        })
        .collect();
    let scope_snapshot =
        scope
            .parameters
            .iter()
            .zip(&scope_bindings)
            .map(|(parameter, binding)| {
                let name_text = parameter.name.to_string();
                quote!((
                    #name_text,
                    ::std::string::ToString::to_string(&*#binding),
                ))
            });
    let base = &client.base;
    let endpoint_identity = format!("{client_name}::{scope_name}::{endpoint_name}");
    let contract_version = GENERATED_CONTRACT_VERSION;

    quote! {
        #[derive(Clone)]
        pub struct #client_name {
            __concord_private: (),
        }

        pub mod #support_name {
            mod __concord_generated {
                pub(super) use #core_path as __core;

                const _: () = __core::__private::assert_generated_contract(#contract_version);
            }

            pub mod facades {
                #[derive(Clone)]
                pub struct #facade_name {
                    pub(super) __concord_scope: (#(::std::sync::Arc<#scope_types>,)*),
                }
            }

            pub mod endpoints {
                pub mod #scope_module_name {
                    pub struct #endpoint_name {
                        __concord_scope: (#(::std::sync::Arc<#scope_types>,)*),
                        __concord_endpoint: (#(#endpoint_types,)*),
                        __concord_head: super::super::__concord_generated::__core::__private::RequestHead,
                    }

                    impl #endpoint_name {
                        pub fn new(#(#scope_bindings: #scope_types,)* #(#endpoint_bindings: #endpoint_types),*) -> Self {
                            Self::__concord_from_shared(
                                #(::std::sync::Arc::new(#scope_bindings),)*
                                #(#endpoint_bindings),*
                            )
                        }

                        pub(in super::super) fn __concord_from_shared(
                            #(#scope_bindings: ::std::sync::Arc<#scope_types>,)*
                            #(#endpoint_bindings: #endpoint_types),*
                        ) -> Self {
                            let mut #url_local = ::std::string::String::from(
                                #base.trim_end_matches('/')
                            );
                            for #segment_local in [#(#path_expressions),*] {
                                #url_local.push('/');
                                #url_local.push_str(&#segment_local);
                            }
                            let #head_local = super::super::__concord_generated::__core::__private::RequestHead::new(
                                #endpoint_identity,
                                "GET",
                                #url_local,
                                ::std::vec::Vec::new(),
                                ::std::vec::Vec::new(),
                                ::std::vec![#(#scope_snapshot),*],
                            );
                            Self {
                                __concord_scope: (#(#scope_bindings,)*),
                                __concord_endpoint: (#(#endpoint_bindings,)*),
                                __concord_head: #head_local,
                            }
                        }

                        #[doc(hidden)]
                        #[must_use]
                        pub fn __concord_request_head(
                            &self,
                        ) -> &super::super::__concord_generated::__core::__private::RequestHead {
                            &self.__concord_head
                        }
                    }
                }
            }

            impl super::#client_name {
                #[must_use]
                pub const fn new() -> Self {
                    Self { __concord_private: () }
                }

                #[must_use]
                pub fn #scope_method_name(
                    &self,
                    #(#scope_names: #scope_types),*
                ) -> facades::#facade_name {
                    facades::#facade_name {
                        __concord_scope: (#(::std::sync::Arc::new(#scope_names),)*),
                    }
                }
            }

            impl facades::#facade_name {
                #[must_use]
                pub fn #endpoint_method_name(
                    &self,
                    #(#endpoint_names: #endpoint_types),*
                ) -> endpoints::#scope_module_name::#endpoint_name {
                    endpoints::#scope_module_name::#endpoint_name::__concord_from_shared(
                        #(::std::sync::Arc::clone(&self.__concord_scope.#scope_indices),)*
                        #(#endpoint_names),*
                    )
                }
            }
        }
    }
}

fn path_expression(
    atom: &PathAtom,
    scope_parameters: &[Parameter],
    scope_bindings: &[Ident],
    endpoint_parameters: &[Parameter],
    endpoint_bindings: &[Ident],
) -> TokenStream {
    match atom {
        PathAtom::Static(value) => quote!(::std::string::String::from(#value)),
        PathAtom::Scope(name) => {
            let index = scope_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated scope path references have a generated binding");
            let binding = &scope_bindings[index];
            quote!(::std::string::ToString::to_string(&*#binding))
        }
        PathAtom::Endpoint(name) => {
            let index = endpoint_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated endpoint path references have a generated binding");
            let binding = &endpoint_bindings[index];
            quote!(::std::string::ToString::to_string(&#binding))
        }
    }
}

fn parameter_binding(namespace: &str, index: usize, name: &Ident) -> Ident {
    let normalized = to_snake_case(&name.to_string());
    Ident::new(
        &format!("__concord_{namespace}_{index}_{normalized}"),
        Span::mixed_site(),
    )
}

fn collision_safe_ident(base: &str, reserved: &HashSet<String>) -> Ident {
    if !reserved.contains(base) {
        return Ident::new(base, Span::mixed_site());
    }
    for suffix in 1_u32.. {
        let candidate = format!("{base}_{suffix}");
        if !reserved.contains(&candidate) {
            return Ident::new(&candidate, Span::mixed_site());
        }
    }
    unreachable!("the finite parameter set cannot exhaust generated suffixes")
}

fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters: Vec<_> = name.chars().collect();
    let mut output = String::with_capacity(name.len());
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            let previous_is_lowercase = index > 0 && characters[index - 1].is_lowercase();
            let next_is_lowercase = characters
                .get(index + 1)
                .is_some_and(|next| next.is_lowercase());
            if index > 0 && (previous_is_lowercase || next_is_lowercase) {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn to_pascal_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            first.to_uppercase().chain(characters).collect::<String>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{to_pascal_case, to_snake_case};

    #[test]
    fn generated_names_are_deterministic() {
        assert_eq!(to_snake_case("ProofApi"), "proof_api");
        assert_eq!(to_snake_case("ById"), "by_id");
        assert_eq!(to_snake_case("HTTPStatus"), "http_status");
        assert_eq!(to_pascal_case("resources"), "Resources");
        assert_eq!(to_pascal_case("user_resources"), "UserResources");
    }
}
