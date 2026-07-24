use super::*;

pub(super) fn generate_secret_builder_methods(
    client: &Client,
    core_path: &TokenStream,
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
                    value: #core_path::auth::SecretString,
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
    core_path: &TokenStream,
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
                    P: #core_path::auth::CredentialProvider<
                        Material = <#provider as #core_path::auth::CredentialProvider>::Material
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

pub(super) fn generate_secret_resolution(client: &Client, _core_path: &TokenStream) -> TokenStream {
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
    _core_path: &TokenStream,
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
    core_path: &TokenStream,
) -> TokenStream {
    let fields = client
        .credentials
        .iter()
        .enumerate()
        .map(|(index, credential)| {
            let field = credential_store_field(index, &credential.name);
            let material = credential_material_type(client, credential, core_path);
            quote!(#field: #core_path::__private::GeneratedCredential<#material>,)
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

pub(super) fn generate_credential_store_initializers(
    client: &Client,
    core_path: &TokenStream,
) -> TokenStream {
    let initializers = client
        .credentials
        .iter()
        .enumerate()
        .map(|(index, credential)| {
            let field = credential_store_field(index, &credential.name);
            let value = match &credential.source {
                CredentialSource::ApiKey { secret } => {
                    let secret = resolved_secret(*secret, &client.secrets[*secret].name);
                    quote!(#core_path::__private::GeneratedCredential::static_value(
                        &core,
                        #core_path::auth::ApiKey::new(#secret.clone()),
                    ))
                }
                CredentialSource::Bearer { secret } => {
                    let secret = resolved_secret(*secret, &client.secrets[*secret].name);
                    quote!(#core_path::__private::GeneratedCredential::static_value(
                        &core,
                        #core_path::auth::AccessToken::new(#secret.clone()),
                    ))
                }
                CredentialSource::Basic { username, password } => {
                    let username = resolved_secret(*username, &client.secrets[*username].name);
                    let password = resolved_secret(*password, &client.secrets[*password].name);
                    quote!(#core_path::__private::GeneratedCredential::static_value(
                        &core,
                        #core_path::auth::BasicCredential::new(
                            #username.clone(),
                            #password.clone(),
                        ),
                    ))
                }
                CredentialSource::Endpoint { .. } => {
                    quote!(#core_path::__private::GeneratedCredential::endpoint(&core))
                }
                CredentialSource::Custom { .. } => {
                    let provider = resolved_provider(index, &credential.name);
                    quote!(#core_path::__private::GeneratedCredential::provider_arc(
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

pub(super) fn generate_credentials_module(client: &Client, core_path: &TokenStream) -> TokenStream {
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
            let material = credential_material_type(client, credential, core_path);
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
    let handle_types = generate_credential_handle_types(client, core_path);
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
    core_path: &TokenStream,
) -> TokenStream {
    let api_key = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::ApiKey { .. }))
        .then(|| quote! {
            /// Handle for one static API-key source.
            pub struct ApiKeyCredentialHandle {
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::auth::ApiKey>,
            }
            impl ApiKeyCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::auth::CredentialRef<#core_path::auth::ApiKey> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::Into<#core_path::auth::SecretString>,
                ) -> ::std::result::Result<(), #core_path::auth::AuthError> {
                    self.inner.replace_static(#core_path::auth::ApiKey::new(value)).await
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
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::auth::AccessToken>,
            }
            impl BearerCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::auth::CredentialRef<#core_path::auth::AccessToken> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::Into<#core_path::auth::SecretString>,
                ) -> ::std::result::Result<(), #core_path::auth::AuthError> {
                    self.inner.replace_static(#core_path::auth::AccessToken::new(value)).await
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
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::auth::BasicCredential>,
            }
            impl BasicCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::auth::CredentialRef<#core_path::auth::BasicCredential> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    username: impl ::std::convert::Into<#core_path::auth::SecretString>,
                    password: impl ::std::convert::Into<#core_path::auth::SecretString>,
                ) -> ::std::result::Result<(), #core_path::auth::AuthError> {
                    self.inner.replace_static(
                        #core_path::auth::BasicCredential::new(username, password),
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
                M: #core_path::auth::CredentialMaterial,
            {
                pub(super) inner: #core_path::__private::GeneratedCredential<M>,
            }
            impl<M> EndpointCredentialHandle<M>
            where
                M: #core_path::auth::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #core_path::auth::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn set(&self, value: M) -> ::std::result::Result<(), #core_path::auth::AuthError> {
                    self.inner.set(value).await
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #core_path::auth::AuthError> {
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
                M: #core_path::auth::CredentialMaterial,
            {
                pub(super) inner: #core_path::__private::GeneratedCredential<M>,
            }
            impl<M> CustomProviderCredentialHandle<M>
            where
                M: #core_path::auth::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #core_path::auth::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #core_path::auth::AuthError> {
                    self.inner.clear().await
                }
                pub async fn replace_provider<P>(
                    &self,
                    provider: P,
                ) -> ::std::result::Result<(), #core_path::auth::AuthError>
                where
                    P: #core_path::auth::CredentialProvider<Material = M>,
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
    core_path: &TokenStream,
) -> TokenStream {
    match &credential.source {
        CredentialSource::ApiKey { .. } => quote!(#core_path::auth::ApiKey),
        CredentialSource::Bearer { .. } => quote!(#core_path::auth::AccessToken),
        CredentialSource::Basic { .. } => quote!(#core_path::auth::BasicCredential),
        CredentialSource::Custom { provider } => {
            quote!(<#provider as #core_path::auth::CredentialProvider>::Material)
        }
        CredentialSource::Endpoint {
            endpoint_id: Some(endpoint_id),
            ..
        } => response_output_type(
            &find_endpoint_by_id(&client.children, *endpoint_id)
                .expect("resolved endpoint credential")
                .response,
            core_path,
        ),
        CredentialSource::Endpoint {
            endpoint_id: None, ..
        } => {
            unreachable!("endpoint credentials are resolved before code generation")
        }
    }
}

pub(super) fn response_output_type(response: &Type, core_path: &TokenStream) -> TokenStream {
    quote!(<#response as #core_path::response::ResponseBodySpec>::Output)
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
            generate_credentials_module(&api.client, &test_support::core_path()).to_string();

        assert!(output.contains("pub mod credentials"));
        assert!(output.contains("pub fn bearer"));
        assert!(output.contains("CredentialHandle"));
    }
}
