use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::Index;

use crate::model::{Api, Parameter, PathAtom};

const GENERATED_CONTRACT_VERSION: u32 = 2;

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

    let all_path_atoms: Vec<_> = scope.path.iter().chain(endpoint.path.iter()).collect();
    let path_statements: Vec<_> = all_path_atoms
        .iter()
        .map(|atom| path_statement(atom, &scope.parameters, &endpoint.parameters))
        .collect();
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
                            Self {
                                __concord_scope: (#(#scope_bindings,)*),
                                __concord_endpoint: (#(#endpoint_bindings,)*),
                            }
                        }
                    }

                    #[doc(hidden)]
                    impl super::super::__concord_generated::__core::__private::EndpointSpec
                        for #endpoint_name
                    where
                        #(#scope_types: ::std::fmt::Display + ::std::marker::Send + ::std::marker::Sync + 'static,)*
                        #(#endpoint_types: ::std::fmt::Display + ::std::marker::Send + 'static,)*
                    {
                        type Body = super::super::__concord_generated::__core::__private::EmptyBody;
                        type Response = super::super::__concord_generated::__core::__private::NoContentResponse;

                        fn request_meta(
                            &self,
                        ) -> super::super::__concord_generated::__core::__private::RequestMeta {
                            super::super::__concord_generated::__core::__private::RequestMeta::new(
                                #endpoint_identity,
                                super::super::__concord_generated::__core::__private::Method::GET,
                                ::std::option::Option::None,
                            )
                        }

                        fn prepare_head_and_body_metadata(
                            &self,
                            mut __concord_builder: super::super::__concord_generated::__core::__private::HeadBuilder,
                        ) -> ::std::result::Result<
                            (
                                super::super::__concord_generated::__core::__private::PreparedHead,
                                super::super::__concord_generated::__core::__private::BodyMetadata,
                            ),
                            super::super::__concord_generated::__core::__private::ApiError,
                        > {
                            #(#path_statements)*
                            ::std::result::Result::Ok((
                                __concord_builder.finish(),
                                super::super::__concord_generated::__core::__private::EmptyBody::metadata(),
                            ))
                        }

                        fn into_body(self) -> Self::Body {
                            let _ = self;
                            super::super::__concord_generated::__core::__private::EmptyBody::new()
                        }
                    }
                }
            }

            impl super::#client_name {
                #[must_use]
                pub const fn new() -> Self {
                    let _ = #base;
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

fn path_statement(
    atom: &PathAtom,
    scope_parameters: &[Parameter],
    endpoint_parameters: &[Parameter],
) -> TokenStream {
    match atom {
        PathAtom::Static(value) => quote!(__concord_builder.push_static(#value)?;),
        PathAtom::Scope(name) => {
            let index = scope_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated scope path references have a generated binding");
            let index = Index::from(index);
            quote!(__concord_builder.push_value(&*self.__concord_scope.#index)?;)
        }
        PathAtom::Endpoint(name) => {
            let index = endpoint_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated endpoint path references have a generated binding");
            let index = Index::from(index);
            quote!(__concord_builder.push_value(&self.__concord_endpoint.#index)?;)
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
