use super::*;

pub(super) fn generate_secret_builder_methods(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let methods = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let method = &secret.name;
            let field = secret_builder_field(index, method);
            quote! {
                #[must_use]
                #[doc = "Sets one required protected client secret; a later call replaces it."]
                pub fn #method(
                    mut self,
                    value: #facade_path::auth::SecretString,
                ) -> Self {
                    self.#field = ::std::option::Option::Some(value);
                    self
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

pub(super) fn generate_provider_builder_methods(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let methods = client
        .credentials
        .iter()
        .enumerate()
        .filter_map(|(index, credential)| {
            let CredentialSource::Custom { provider } = &credential.source else {
                return None;
            };
            let method = provider_argument_name(&credential.name);
            let field = provider_builder_field(index, &credential.name);
            Some(quote! {
                #[must_use]
                #[doc = "Sets one required custom credential provider; a later call replaces it."]
                pub fn #method<P>(mut self, provider: P) -> Self
                where
                    P: #facade_path::auth::CredentialProvider<
                        Material = <#provider as #facade_path::auth::CredentialProvider>::Material
                    >,
                {
                    self.#field = ::std::option::Option::Some(::std::sync::Arc::new(provider));
                    self
                }
            })
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

pub(super) fn generate_secret_resolution(
    client: &Client,
    _facade_path: &TokenStream,
) -> TokenStream {
    let statements = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let field = secret_builder_field(index, &secret.name);
            let resolved = resolved_secret(index, &secret.name);
            let method = &secret.name;
            quote! {
                let #resolved = match self.#field {
                    ::std::option::Option::Some(value) => value,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::missing_input_error(
                                ::std::stringify!(#method),
                                __concord_generated::__core::__private::ClientInputKind::Secret,
                            ),
                        );
                    }
                };
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn generate_provider_resolution(
    client: &Client,
    _facade_path: &TokenStream,
) -> TokenStream {
    let statements = client
        .credentials
        .iter()
        .enumerate()
        .filter(|(_, credential)| matches!(&credential.source, CredentialSource::Custom { .. }))
        .map(|(index, credential)| {
            let field = provider_builder_field(index, &credential.name);
            let resolved = resolved_provider(index, &credential.name);
            let method = provider_argument_name(&credential.name);
            quote! {
                let #resolved = match self.#field {
                    ::std::option::Option::Some(provider) => provider,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::missing_input_error(
                                ::std::stringify!(#method),
                                __concord_generated::__core::__private::ClientInputKind::CredentialProvider,
                            ),
                        );
                        }
                };
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn generate_credential_store_fields(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let fields = client
        .credentials
        .iter()
        .enumerate()
        .map(|(index, credential)| {
            let field = credential_store_field(index, &credential.name);
            let material = credential_material_type(client, credential, facade_path);
            quote!(#field: #facade_path::__private::GeneratedCredential<#material>,)
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

pub(super) fn generate_static_credential_resolution(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let statements = client
        .credentials
        .iter()
        .enumerate()
        .filter_map(|(index, credential)| {
            let resolved = resolved_credential_material(index, &credential.name);
            let statement = match &credential.source {
                CredentialSource::ApiKey { secret } => {
                    let source = resolved_secret(*secret, &client.secrets[*secret].name);
                    let field = &client.secrets[*secret].name;
                    quote! {
                        let #resolved = #facade_path::auth::ApiKey::try_from_secret(#source.clone())
                            .map_err(|error| {
                                __concord_generated::__core::__private::credential_input_error(
                                    ::std::stringify!(#field),
                                    error,
                                )
                            })?;
                    }
                }
                CredentialSource::Bearer { secret } => {
                    let source = resolved_secret(*secret, &client.secrets[*secret].name);
                    let field = &client.secrets[*secret].name;
                    quote! {
                        let #resolved =
                            #facade_path::auth::AccessToken::try_from_secret(#source.clone())
                                .map_err(|error| {
                                    __concord_generated::__core::__private::credential_input_error(
                                        ::std::stringify!(#field),
                                        error,
                                    )
                                })?;
                    }
                }
                CredentialSource::Basic { username, password } => {
                    let username_source =
                        resolved_secret(*username, &client.secrets[*username].name);
                    let password_source =
                        resolved_secret(*password, &client.secrets[*password].name);
                    let username_field = &client.secrets[*username].name;
                    let password_field = &client.secrets[*password].name;
                    quote! {
                        let #resolved = #facade_path::auth::BasicCredential::try_from_secrets(
                            #username_source.clone(),
                            #password_source.clone(),
                        )
                        .map_err(|error| {
                            __concord_generated::__core::__private::basic_credential_input_error(
                                ::std::stringify!(#username_field),
                                ::std::stringify!(#password_field),
                                error,
                            )
                        })?;
                    }
                }
                CredentialSource::Endpoint { .. } | CredentialSource::Custom { .. } => {
                    return None;
                }
            };
            Some(statement)
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn generate_credential_store_initializers(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let initializers = client
        .credentials
        .iter()
        .enumerate()
        .map(|(index, credential)| {
            let field = credential_store_field(index, &credential.name);
            let value = match &credential.source {
                CredentialSource::ApiKey { .. }
                | CredentialSource::Bearer { .. }
                | CredentialSource::Basic { .. } => {
                    let material = resolved_credential_material(index, &credential.name);
                    quote!(#facade_path::__private::GeneratedCredential::static_value(
                        &core,
                        #material,
                    ))
                }
                CredentialSource::Endpoint { .. } => {
                    quote!(#facade_path::__private::GeneratedCredential::endpoint(&core))
                }
                CredentialSource::Custom { .. } => {
                    let provider = resolved_provider(index, &credential.name);
                    quote!(#facade_path::__private::GeneratedCredential::provider_arc(
                        &core,
                        #provider,
                    ))
                }
            };
            quote!(#field: #value,)
        })
        .collect::<Vec<_>>();
    quote!(#(#initializers)*)
}

pub(super) fn generate_credentials_method(client: &Client) -> TokenStream {
    if client.credentials.is_empty() {
        return quote!();
    }
    let facade = credentials_facade_name(client);
    quote! {
        /// Borrows the typed credential-management facade for this client.
        #[must_use]
        pub fn credentials(&self) -> credentials::#facade<'_> {
            credentials::#facade::__concord_new(self)
        }
    }
}

pub(super) fn generate_credentials_module(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    if client.credentials.is_empty() {
        return quote!();
    }
    let client_name = &client.name;
    let facade = credentials_facade_name(client);
    let accessors = client
        .credentials
        .iter()
        .enumerate()
        .map(|(index, credential)| {
            let method = public_ident(
                &to_snake_case(&canonical_ident(&credential.name)),
                credential.name.span(),
            );
            let field = credential_store_field(index, &credential.name);
            let material = credential_material_type(client, credential, facade_path);
            let handle = credential_handle_type(credential, &material);
            let handle_constructor = credential_handle_constructor(credential);
            quote! {
                /// Returns a typed source-specific credential handle.
                #[must_use]
                pub fn #method(&self) -> #handle {
                    #handle_constructor {
                        inner: self.client.__concord_credentials.#field.clone(),
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    let handle_types = generate_credential_handle_types(client, facade_path);
    quote! {
        /// Generated typed credential access and source-specific mutation capabilities.
        pub mod credentials {
            #[allow(unused_imports)]
            use super::super::*;

            /// Borrowed credential facade for the generated client.
            pub struct #facade<'a> {
                client: &'a super::super::#client_name,
            }

            impl<'a> #facade<'a> {
                pub(super) fn __concord_new(client: &'a super::super::#client_name) -> Self {
                    Self { client }
                }

                #(#accessors)*
            }

            #handle_types
        }
    }
}

pub(super) fn generate_credential_handle_types(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let api_key = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::ApiKey { .. }))
        .then(|| quote! {
            /// Handle for one static API-key source.
            pub struct ApiKeyCredentialHandle {
                pub(super) inner: #facade_path::__private::GeneratedCredential<#facade_path::auth::ApiKey>,
            }
            impl ApiKeyCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #facade_path::auth::CredentialRef<#facade_path::auth::ApiKey> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::AsRef<str>,
                ) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.replace_static(#facade_path::auth::ApiKey::try_new(value)).await
                }
            }
        });
    let bearer = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::Bearer { .. }))
        .then(|| quote! {
            /// Handle for one static bearer source.
            pub struct BearerCredentialHandle {
                pub(super) inner: #facade_path::__private::GeneratedCredential<#facade_path::auth::AccessToken>,
            }
            impl BearerCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #facade_path::auth::CredentialRef<#facade_path::auth::AccessToken> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::AsRef<str>,
                ) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.replace_static(#facade_path::auth::AccessToken::try_new(value)).await
                }
            }
        });
    let basic = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::Basic { .. }))
        .then(|| quote! {
            /// Handle for one atomic static Basic source.
            pub struct BasicCredentialHandle {
                pub(super) inner: #facade_path::__private::GeneratedCredential<#facade_path::auth::BasicCredential>,
            }
            impl BasicCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #facade_path::auth::CredentialRef<#facade_path::auth::BasicCredential> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    username: impl ::std::convert::AsRef<str>,
                    password: impl ::std::convert::AsRef<str>,
                ) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.replace_static(
                        #facade_path::auth::BasicCredential::try_new(username, password),
                    ).await
                }
            }
        });
    let endpoint = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::Endpoint { .. }))
        .then(|| quote! {
            /// Handle for one explicitly acquired endpoint-backed source.
            pub struct EndpointCredentialHandle<M>
            where
                M: #facade_path::auth::CredentialMaterial,
            {
                pub(super) inner: #facade_path::__private::GeneratedCredential<M>,
            }
            impl<M> EndpointCredentialHandle<M>
            where
                M: #facade_path::auth::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #facade_path::auth::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn set(&self, value: M) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.set(value).await
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.clear().await
                }
                #[must_use]
                pub fn is_set(&self) -> bool {
                    self.inner.is_set()
                }
            }
        });
    let custom = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::Custom { .. }))
        .then(|| quote! {
            /// Handle for one custom provider-backed source.
            pub struct CustomProviderCredentialHandle<M>
            where
                M: #facade_path::auth::CredentialMaterial,
            {
                pub(super) inner: #facade_path::__private::GeneratedCredential<M>,
            }
            impl<M> CustomProviderCredentialHandle<M>
            where
                M: #facade_path::auth::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #facade_path::auth::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #facade_path::auth::AuthError> {
                    self.inner.clear().await
                }
                pub async fn replace_provider<P>(
                    &self,
                    provider: P,
                ) -> ::std::result::Result<(), #facade_path::auth::AuthError>
                where
                    P: #facade_path::auth::CredentialProvider<Material = M>,
                {
                    self.inner.replace_provider(provider).await
                }
                #[must_use]
                pub fn is_set(&self) -> bool {
                    self.inner.is_set()
                }
            }
        });
    quote!(#api_key #bearer #basic #endpoint #custom)
}

pub(super) fn credential_handle_type(
    credential: &Credential,
    material: &TokenStream,
) -> TokenStream {
    match credential.source {
        CredentialSource::ApiKey { .. } => quote!(ApiKeyCredentialHandle),
        CredentialSource::Bearer { .. } => quote!(BearerCredentialHandle),
        CredentialSource::Basic { .. } => quote!(BasicCredentialHandle),
        CredentialSource::Endpoint { .. } => quote!(EndpointCredentialHandle<#material>),
        CredentialSource::Custom { .. } => quote!(CustomProviderCredentialHandle<#material>),
    }
}

pub(super) fn credential_handle_constructor(credential: &Credential) -> TokenStream {
    match credential.source {
        CredentialSource::ApiKey { .. } => quote!(ApiKeyCredentialHandle),
        CredentialSource::Bearer { .. } => quote!(BearerCredentialHandle),
        CredentialSource::Basic { .. } => quote!(BasicCredentialHandle),
        CredentialSource::Endpoint { .. } => quote!(EndpointCredentialHandle),
        CredentialSource::Custom { .. } => quote!(CustomProviderCredentialHandle),
    }
}

pub(super) fn credential_material_type(
    client: &Client,
    credential: &Credential,
    facade_path: &TokenStream,
) -> TokenStream {
    match &credential.source {
        CredentialSource::ApiKey { .. } => quote!(#facade_path::auth::ApiKey),
        CredentialSource::Bearer { .. } => quote!(#facade_path::auth::AccessToken),
        CredentialSource::Basic { .. } => quote!(#facade_path::auth::BasicCredential),
        CredentialSource::Custom { provider } => {
            quote!(<#provider as #facade_path::auth::CredentialProvider>::Material)
        }
        CredentialSource::Endpoint {
            endpoint_id: Some(endpoint_id),
            ..
        } => response_output_type(
            &find_endpoint_by_id(&client.children, *endpoint_id)
                .expect("resolved endpoint credential")
                .response,
            facade_path,
        ),
        CredentialSource::Endpoint {
            endpoint_id: None, ..
        } => {
            unreachable!("endpoint credentials are resolved before code generation")
        }
    }
}

pub(super) fn response_output_type(response: &Type, facade_path: &TokenStream) -> TokenStream {
    quote!(<#response as #facade_path::response::ResponseBodySpec>::Output)
}

pub(super) fn find_endpoint_by_id(children: &[Child], id: usize) -> Option<&Endpoint> {
    for child in children {
        match child {
            Child::Endpoint(endpoint) if endpoint.id == id => return Some(endpoint),
            Child::Scope(scope) => {
                if let Some(endpoint) = find_endpoint_by_id(&scope.children, id) {
                    return Some(endpoint);
                }
            }
            Child::Endpoint(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_typed_credential_facade_and_handle() {
        let api = test_support::api();
        let output =
            generate_credentials_module(&api.client, &test_support::facade_path()).to_string();

        assert!(output.contains("pub mod credentials"));
        assert!(output.contains("pub fn bearer"));
        assert!(output.contains("CredentialHandle"));
    }
}
