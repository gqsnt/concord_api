use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::Type;

use crate::ast::{
    AuthChallenge, AuthPlacementKind, CodecKind, DefaultExpr, MediaKind, MethodKind,
    RequestBodyKind, ResponseKind, RetrySelection, ValueKind,
};
use crate::model::{
    Api, Child, Client, Credential, CredentialSource, Endpoint, NormalizedConfig, ResolvedExpr,
    Scope, Value, ValueReference, ValueShape,
};
use crate::names::{canonical_ident, public_ident, to_snake_case};

const GENERATED_CONTRACT_VERSION: u32 = 7;

pub(crate) fn generate(api: &Api, core_path: &TokenStream) -> TokenStream {
    let clients = api
        .clients
        .iter()
        .map(|client| generate_client(client, core_path))
        .collect::<Vec<_>>();
    quote!(#(#clients)*)
}

fn generate_client(client: &Client, core_path: &TokenStream) -> TokenStream {
    let client_name = &client.name;
    let builder_name = &client.builder_name;
    let support_name = &client.support_name;
    let capability_name = &client.endpoint_capability_name;
    let values_type = &client.values_type_name;
    let credentials_store = credentials_store_name(client);
    let base = &client.base;
    let contract_version = GENERATED_CONTRACT_VERSION;

    let value_summary = value_category_summary(&client.values, "client");
    let retry_summary = retry_summary(&client.retry);
    let credential_summary = credential_summary(client);
    let client_doc = format!(
        "Generated Concord client `{client_name}` with one fixed immutable public-value snapshot. {value_summary} {credential_summary} {retry_summary}"
    );
    let builder_doc = format!(
        "Canonical fallible builder for [`{client_name}`]. {value_summary} {credential_summary} Selected client defaults are evaluated once by `build`; overridden defaults are not evaluated. {retry_summary}"
    );
    let support_doc = format!("Public generated API modules for [`{client_name}`].");
    let prelude_doc = format!(
        "Re-exports every endpoint-specific request extension trait for [`{client_name}`] under a deterministic client-qualified alias. Import all setters with `use {support_name}::prelude::*;`. Generic timeout and debug setters remain inherent on `Request<E>`."
    );
    let constructor_doc = format!(
        "Builds `{client_name}` through the canonical builder. Constructor arguments are required client values, then protected secrets, then custom credential providers, each in normative declaration order. {value_summary} {credential_summary}"
    );
    let capability_doc = format!(
        "Sealed capability implemented only by explicit endpoints generated for [`{client_name}`]."
    );

    let value_fields = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = client_value_field(index, value);
            let ty = &value.ty;
            quote!(#field: #ty,)
        })
        .collect::<Vec<_>>();
    let builder_value_fields = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = builder_value_field(index, value);
            let ty = value.setter_type();
            quote!(#field: ::std::option::Option<#ty>,)
        })
        .collect::<Vec<_>>();
    let secret_builder_fields = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let field = secret_builder_field(index, &secret.name);
            quote!(#field: ::std::option::Option<#core_path::prelude::SecretString>,)
        })
        .collect::<Vec<_>>();
    let provider_builder_fields = client
        .credentials
        .iter()
        .enumerate()
        .filter_map(|(index, credential)| match &credential.source {
            CredentialSource::Custom { material } => {
                let field = provider_builder_field(index, &credential.name);
                Some(quote!(
                    #field: ::std::option::Option<::std::sync::Arc<
                        dyn #core_path::advanced::CredentialProvider<Material = #material>
                    >>,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let builder_value_initializers = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = builder_value_field(index, value);
            quote!(#field: ::std::option::Option::None,)
        })
        .collect::<Vec<_>>();
    let secret_builder_initializers = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let field = secret_builder_field(index, &secret.name);
            quote!(#field: ::std::option::Option::None,)
        })
        .collect::<Vec<_>>();
    let provider_builder_initializers = client
        .credentials
        .iter()
        .enumerate()
        .filter(|(_, credential)| matches!(&credential.source, CredentialSource::Custom { .. }))
        .map(|(index, credential)| {
            let field = provider_builder_field(index, &credential.name);
            quote!(#field: ::std::option::Option::None,)
        })
        .collect::<Vec<_>>();
    let builder_methods = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| generate_builder_value_methods(index, value))
        .collect::<Vec<_>>();
    let secret_builder_methods = generate_secret_builder_methods(client, core_path);
    let provider_builder_methods = generate_provider_builder_methods(client, core_path);
    let required_client_values = client
        .values
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_required())
        .collect::<Vec<_>>();
    let mut constructor_parameters = required_client_values
        .iter()
        .map(|(_, value)| {
            let name = &value.name;
            let ty = &value.ty;
            quote!(#name: #ty,)
        })
        .collect::<Vec<_>>();
    constructor_parameters.extend(client.secrets.iter().map(|secret| {
        let name = &secret.name;
        quote!(#name: impl ::std::convert::Into<#core_path::prelude::SecretString>,)
    }));
    constructor_parameters.extend(client.credentials.iter().filter_map(|credential| {
        let CredentialSource::Custom { material } = &credential.source else {
            return None;
        };
        let name = provider_argument_name(&credential.name);
        Some(quote!(
            #name: impl #core_path::advanced::CredentialProvider<Material = #material>,
        ))
    }));
    let mut constructor_setters = required_client_values
        .iter()
        .map(|(_, value)| {
            let name = &value.name;
            quote!(.#name(#name))
        })
        .collect::<Vec<_>>();
    constructor_setters.extend(client.secrets.iter().map(|secret| {
        let name = &secret.name;
        quote!(.#name(#name))
    }));
    constructor_setters.extend(
        client
            .credentials
            .iter()
            .filter(|credential| matches!(&credential.source, CredentialSource::Custom { .. }))
            .map(|credential| {
                let method = provider_argument_name(&credential.name);
                quote!(.#method(#method))
            }),
    );
    let build_value_statements = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| generate_client_value_resolution(index, value))
        .collect::<Vec<_>>();
    let build_secret_statements = generate_secret_resolution(client, core_path);
    let build_provider_statements = generate_provider_resolution(client, core_path);
    let snapshot_fields = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let source = resolved_client_value(index, value);
            let field = client_value_field(index, value);
            quote!(#field: #source,)
        })
        .collect::<Vec<_>>();
    let credential_store_fields = generate_credential_store_fields(client, core_path);
    let credential_store_initializers = generate_credential_store_initializers(client, core_path);
    let credentials_module = generate_credentials_module(client, core_path);
    let credentials_method = generate_credentials_method(client);

    let retry_baseline = match &client.retry {
        RetrySelection::Disabled => {
            quote!(__concord_generated::__core::__private::default_http_config())
        }
        RetrySelection::ReqwestDefault => quote!(
            __concord_generated::__core::__private::default_http_config().retry_reqwest_default()
        ),
        RetrySelection::Custom(path) => quote!(
            __concord_generated::__core::__private::default_http_config_with_custom_retry(#path)
        ),
    };
    let custom_retry_assertion = matches!(&client.retry, RetrySelection::Custom(_)).then(|| {
        quote!(
            const _: () = __core::__private::assert_custom_retry_feature();
        )
    });
    let json_assertion = client_uses_json(client).then(|| {
        quote!(
            const _: () = __core::__private::assert_json_feature();
        )
    });
    let multipart_assertion = client_uses_multipart(client).then(|| {
        quote!(
            const _: () = __core::__private::assert_multipart_feature();
        )
    });

    let facade_modules = generate_facade_children(client, &client.children, &[], core_path);
    let endpoint_modules = generate_endpoint_children(client, &client.children, &[], core_path);
    let client_child_methods = generate_client_child_methods(client, core_path);
    let prelude_exports = collect_prelude_exports(client, &client.children, &[]);

    quote! {
        struct #values_type {
            #(#value_fields)*
        }

        struct #credentials_store {
            #credential_store_fields
        }

        #[doc = #client_doc]
        #[derive(Clone)]
        pub struct #client_name {
            __concord_core: #core_path::__private::ClientCore,
            __concord_values: ::std::sync::Arc<#values_type>,
            __concord_credentials: ::std::sync::Arc<#credentials_store>,
        }

        #[doc = #builder_doc]
        pub struct #builder_name {
            #(#builder_value_fields)*
            #(#secret_builder_fields)*
            #(#provider_builder_fields)*
            __concord_http: ::std::option::Option<#core_path::advanced::HttpConfigBuilder>,
            __concord_runtime: ::std::option::Option<#core_path::advanced::RuntimeConfigBuilder>,
            __concord_http_configured: bool,
            __concord_runtime_configured: bool,
            __concord_duplicate_http: bool,
            __concord_duplicate_runtime: bool,
        }

        #[doc = #support_doc]
        pub mod #support_name {
            mod __concord_generated {
                pub(super) use #core_path as __core;

                const _: () = __core::__private::assert_generated_contract(#contract_version);
                #json_assertion
                #multipart_assertion
                #custom_retry_assertion
            }

            mod __concord_endpoint_seal {
                pub trait Sealed {}
            }

            #[doc(hidden)]
            #[doc = #capability_doc]
            #[allow(private_bounds)]
            pub trait #capability_name: __concord_endpoint_seal::Sealed {
                #[doc(hidden)]
                fn __concord_bind(self, _client: &super::#client_name) -> Self
                where
                    Self: ::std::marker::Sized,
                {
                    self
                }
            }

            #[doc = #prelude_doc]
            pub mod prelude {
                #(#prelude_exports)*
            }

            #credentials_module

            /// Generated immutable nested scope facades.
            pub mod facades {
                #facade_modules
            }

            /// Generated explicit endpoint values and sealed request extension traits.
            pub mod endpoints {
                #endpoint_modules
            }

            impl super::#client_name {
                #[doc = #constructor_doc]
                #[allow(
                    clippy::too_many_arguments,
                    reason = "the normative constructor lists required values, secrets, and providers in declaration order"
                )]
                pub fn new(
                    #(#constructor_parameters)*
                ) -> ::std::result::Result<Self, #core_path::prelude::ClientBuildError> {
                    Self::builder()#(#constructor_setters)*.build()
                }

                #[must_use]
                /// Creates the canonical generated client builder.
                pub fn builder() -> super::#builder_name {
                    super::#builder_name::new()
                }

                #client_child_methods
                #credentials_method

                #[must_use]
                /// Creates a side-effect-free pending request from an endpoint generated for this client.
                pub fn request<E>(&self, endpoint: E) -> #core_path::Request<E>
                where
                    E: #capability_name,
                {
                    let endpoint = <E as #capability_name>::__concord_bind(endpoint, self);
                    __concord_generated::__core::__private::request(
                        &self.__concord_core,
                        endpoint,
                    )
                }
            }

            impl super::#builder_name {
                #[must_use]
                /// Creates an empty canonical generated client builder.
                pub fn new() -> Self {
                    Self {
                        #(#builder_value_initializers)*
                        #(#secret_builder_initializers)*
                        #(#provider_builder_initializers)*
                        __concord_http: ::std::option::Option::Some(#retry_baseline),
                        __concord_runtime: ::std::option::Option::Some(
                            __concord_generated::__core::__private::default_runtime_config(),
                        ),
                        __concord_http_configured: false,
                        __concord_runtime_configured: false,
                        __concord_duplicate_http: false,
                        __concord_duplicate_runtime: false,
                    }
                }

                #(#builder_methods)*
                #secret_builder_methods
                #provider_builder_methods

                #[must_use]
                /// Applies the HTTP configuration closure immediately. A later retry selection replaces the DSL selection.
                pub fn http<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #core_path::advanced::HttpConfigBuilder,
                    ) -> #core_path::advanced::HttpConfigBuilder,
                {
                    if self.__concord_http_configured {
                        self.__concord_duplicate_http = true;
                        return self;
                    }
                    self.__concord_http_configured = true;
                    let Some(http) = self.__concord_http.take() else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    self.__concord_http = ::std::option::Option::Some(configure(http));
                    self
                }

                #[must_use]
                /// Applies the runtime configuration closure immediately.
                pub fn runtime<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #core_path::advanced::RuntimeConfigBuilder,
                    ) -> #core_path::advanced::RuntimeConfigBuilder,
                {
                    if self.__concord_runtime_configured {
                        self.__concord_duplicate_runtime = true;
                        return self;
                    }
                    self.__concord_runtime_configured = true;
                    let Some(runtime) = self.__concord_runtime.take() else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    self.__concord_runtime = ::std::option::Option::Some(configure(runtime));
                    self
                }

                /// Resolves selected defaults once and builds the immutable generated client snapshot.
                pub fn build(
                    self,
                ) -> ::std::result::Result<super::#client_name, #core_path::prelude::ClientBuildError> {
                    if self.__concord_duplicate_http {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::duplicate_http_configuration_error(),
                        );
                    }
                    if self.__concord_duplicate_runtime {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::duplicate_runtime_configuration_error(),
                        );
                    }
                    #(#build_value_statements)*
                    #build_secret_statements
                    #build_provider_statements
                    let Some(http) = self.__concord_http else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    let Some(runtime) = self.__concord_runtime else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    let core = __concord_generated::__core::__private::build_client_core(
                        #base,
                        http,
                        runtime,
                    )?;
                    let credentials = ::std::sync::Arc::new(super::#credentials_store {
                        #credential_store_initializers
                    });
                    let values = ::std::sync::Arc::new(super::#values_type {
                        #(#snapshot_fields)*
                    });
                    ::std::result::Result::Ok(super::#client_name {
                        __concord_core: core,
                        __concord_values: values,
                        __concord_credentials: credentials,
                    })
                }
            }

            impl ::std::default::Default for super::#builder_name {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    }
}

fn generate_secret_builder_methods(client: &Client, core_path: &TokenStream) -> TokenStream {
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
                    value: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                ) -> Self {
                    self.#field = ::std::option::Option::Some(value.into());
                    self
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

fn generate_provider_builder_methods(client: &Client, core_path: &TokenStream) -> TokenStream {
    let methods = client
        .credentials
        .iter()
        .enumerate()
        .filter_map(|(index, credential)| {
            let CredentialSource::Custom { material } = &credential.source else {
                return None;
            };
            let method = provider_argument_name(&credential.name);
            let field = provider_builder_field(index, &credential.name);
            Some(quote! {
                #[must_use]
                #[doc = "Sets one required custom credential provider; a later call replaces it."]
                pub fn #method<P>(mut self, provider: P) -> Self
                where
                    P: #core_path::advanced::CredentialProvider<Material = #material>,
                {
                    self.#field = ::std::option::Option::Some(::std::sync::Arc::new(provider));
                    self
                }
            })
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

fn generate_secret_resolution(client: &Client, _core_path: &TokenStream) -> TokenStream {
    let statements = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let field = secret_builder_field(index, &secret.name);
            let resolved = resolved_secret(index, &secret.name);
            quote! {
                let #resolved = match self.#field {
                    ::std::option::Option::Some(value) => value,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::missing_input_error(),
                        );
                    }
                };
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn generate_provider_resolution(client: &Client, _core_path: &TokenStream) -> TokenStream {
    let statements = client
        .credentials
        .iter()
        .enumerate()
        .filter(|(_, credential)| matches!(&credential.source, CredentialSource::Custom { .. }))
        .map(|(index, credential)| {
            let field = provider_builder_field(index, &credential.name);
            let resolved = resolved_provider(index, &credential.name);
            quote! {
                let #resolved = match self.#field {
                    ::std::option::Option::Some(provider) => provider,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::missing_input_error(),
                        );
                        }
                };
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn generate_credential_store_fields(client: &Client, core_path: &TokenStream) -> TokenStream {
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

fn generate_credential_store_initializers(client: &Client, core_path: &TokenStream) -> TokenStream {
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
                        #core_path::prelude::ApiKey::new(#secret.clone()),
                    ))
                }
                CredentialSource::Bearer { secret } => {
                    let secret = resolved_secret(*secret, &client.secrets[*secret].name);
                    quote!(#core_path::__private::GeneratedCredential::static_value(
                        &core,
                        #core_path::prelude::AccessToken::new(#secret.clone()),
                    ))
                }
                CredentialSource::Basic { username, password } => {
                    let username = resolved_secret(*username, &client.secrets[*username].name);
                    let password = resolved_secret(*password, &client.secrets[*password].name);
                    quote!(#core_path::__private::GeneratedCredential::static_value(
                        &core,
                        #core_path::prelude::BasicCredential::new(
                            #username.clone(),
                            #password.clone(),
                        ),
                    ))
                }
                CredentialSource::OAuth2Client {
                    token_url,
                    client_id,
                    client_secret,
                    scope,
                } => {
                    let client_id = resolved_secret(*client_id, &client.secrets[*client_id].name);
                    let client_secret =
                        resolved_secret(*client_secret, &client.secrets[*client_secret].name);
                    let scope = scope.as_ref().map_or_else(
                        || quote!(::std::option::Option::None),
                        |scope| quote!(::std::option::Option::Some(#scope)),
                    );
                    quote!(#core_path::__private::GeneratedCredential::provider(
                        &core,
                        #core_path::__private::oauth2_client_credentials(
                            #token_url,
                            #client_id.clone(),
                            #client_secret.clone(),
                            #scope,
                        )?,
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

fn generate_credentials_method(client: &Client) -> TokenStream {
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

fn generate_credentials_module(client: &Client, core_path: &TokenStream) -> TokenStream {
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
            let extra = match &credential.source {
                CredentialSource::OAuth2Client {
                    token_url, scope, ..
                } => {
                    let scope = scope.as_ref().map_or_else(
                        || quote!(::std::option::Option::None),
                        |scope| quote!(::std::option::Option::Some(#scope)),
                    );
                    quote!(token_url: #token_url, scope: #scope,)
                }
                _ => quote!(),
            };
            quote! {
                /// Returns a typed source-specific credential handle.
                #[must_use]
                pub fn #method(&self) -> #handle {
                    #handle_constructor {
                        inner: self.client.__concord_credentials.#field.clone(),
                        #extra
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    let handle_types = generate_credential_handle_types(client, core_path);
    quote! {
        /// Generated typed credential access and source-specific mutation capabilities.
        pub mod credentials {
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

fn generate_credential_handle_types(client: &Client, core_path: &TokenStream) -> TokenStream {
    let api_key = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::ApiKey { .. }))
        .then(|| quote! {
            /// Handle for one static API-key source.
            pub struct ApiKeyCredentialHandle {
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::prelude::ApiKey>,
            }
            impl ApiKeyCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<#core_path::prelude::ApiKey> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                ) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    self.inner.replace_static(#core_path::prelude::ApiKey::new(value)).await
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
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::prelude::AccessToken>,
            }
            impl BearerCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<#core_path::prelude::AccessToken> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    value: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                ) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    self.inner.replace_static(#core_path::prelude::AccessToken::new(value)).await
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
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::prelude::BasicCredential>,
            }
            impl BasicCredentialHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<#core_path::prelude::BasicCredential> {
                    self.inner.credential()
                }
                pub async fn replace(
                    &self,
                    username: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                    password: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                ) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    self.inner.replace_static(
                        #core_path::prelude::BasicCredential::new(username, password),
                    ).await
                }
            }
        });
    let oauth = client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::OAuth2Client { .. }))
        .then(|| quote! {
            /// Handle for one built-in OAuth2 client-credentials source.
            pub struct OAuthClientCredentialsHandle {
                pub(super) inner: #core_path::__private::GeneratedCredential<#core_path::prelude::AccessToken>,
                pub(super) token_url: &'static str,
                pub(super) scope: ::std::option::Option<&'static str>,
            }
            impl OAuthClientCredentialsHandle {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<#core_path::prelude::AccessToken> {
                    self.inner.credential()
                }
                pub async fn replace_client_credentials(
                    &self,
                    client_id: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                    client_secret: impl ::std::convert::Into<#core_path::prelude::SecretString>,
                ) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    let provider = #core_path::__private::oauth2_client_credentials_for_replace(
                        self.token_url,
                        client_id.into(),
                        client_secret.into(),
                        self.scope,
                    )?;
                    self.inner.replace_provider(provider).await
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
                M: #core_path::advanced::CredentialMaterial,
            {
                pub(super) inner: #core_path::__private::GeneratedCredential<M>,
            }
            impl<M> EndpointCredentialHandle<M>
            where
                M: #core_path::advanced::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn set(&self, value: M) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    self.inner.set(value).await
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
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
                M: #core_path::advanced::CredentialMaterial,
            {
                pub(super) inner: #core_path::__private::GeneratedCredential<M>,
            }
            impl<M> CustomProviderCredentialHandle<M>
            where
                M: #core_path::advanced::CredentialMaterial,
            {
                #[must_use]
                pub fn credential(&self) -> #core_path::advanced::CredentialRef<M> {
                    self.inner.credential()
                }
                pub async fn clear(&self) -> ::std::result::Result<(), #core_path::advanced::AuthError> {
                    self.inner.clear().await
                }
                #[must_use]
                pub fn is_set(&self) -> bool {
                    self.inner.is_set()
                }
            }
        });
    quote!(#api_key #bearer #basic #oauth #endpoint #custom)
}

fn credential_handle_type(credential: &Credential, material: &TokenStream) -> TokenStream {
    match credential.source {
        CredentialSource::ApiKey { .. } => quote!(ApiKeyCredentialHandle),
        CredentialSource::Bearer { .. } => quote!(BearerCredentialHandle),
        CredentialSource::Basic { .. } => quote!(BasicCredentialHandle),
        CredentialSource::OAuth2Client { .. } => quote!(OAuthClientCredentialsHandle),
        CredentialSource::Endpoint { .. } => quote!(EndpointCredentialHandle<#material>),
        CredentialSource::Custom { .. } => quote!(CustomProviderCredentialHandle<#material>),
    }
}

fn credential_handle_constructor(credential: &Credential) -> TokenStream {
    match credential.source {
        CredentialSource::ApiKey { .. } => quote!(ApiKeyCredentialHandle),
        CredentialSource::Bearer { .. } => quote!(BearerCredentialHandle),
        CredentialSource::Basic { .. } => quote!(BasicCredentialHandle),
        CredentialSource::OAuth2Client { .. } => quote!(OAuthClientCredentialsHandle),
        CredentialSource::Endpoint { .. } => quote!(EndpointCredentialHandle),
        CredentialSource::Custom { .. } => quote!(CustomProviderCredentialHandle),
    }
}

fn credential_material_type(
    client: &Client,
    credential: &Credential,
    core_path: &TokenStream,
) -> TokenStream {
    match &credential.source {
        CredentialSource::ApiKey { .. } => quote!(#core_path::prelude::ApiKey),
        CredentialSource::Bearer { .. } | CredentialSource::OAuth2Client { .. } => {
            quote!(#core_path::prelude::AccessToken)
        }
        CredentialSource::Basic { .. } => quote!(#core_path::prelude::BasicCredential),
        CredentialSource::Custom { material } => quote!(#material),
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

fn response_output_type(response: &ResponseKind, core_path: &TokenStream) -> TokenStream {
    match response {
        ResponseKind::Bytes => quote!(#core_path::prelude::Bytes),
        ResponseKind::NoContent => quote!(()),
        ResponseKind::Buffered(CodecKind::Text) => quote!(::std::string::String),
        ResponseKind::Buffered(CodecKind::Json(ty)) => quote!(#ty),
        ResponseKind::Buffered(CodecKind::Custom(ty)) => {
            quote!(<#ty as #core_path::advanced::DecodeBody>::Output)
        }
        ResponseKind::Stream(MediaKind::OctetStream) => {
            quote!(#core_path::prelude::StreamResponse<#core_path::prelude::OctetStream>)
        }
        ResponseKind::Stream(MediaKind::Custom(ty)) => {
            quote!(#core_path::prelude::StreamResponse<#ty>)
        }
    }
}

fn find_endpoint_by_id(children: &[Child], id: usize) -> Option<&Endpoint> {
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

fn generate_builder_value_methods(index: usize, value: &Value) -> TokenStream {
    let method = &value.name;
    let field = builder_value_field(index, value);
    let setter_type = value.setter_type();
    let helper = if value.is_optional() {
        let clear = helper_ident("clear", &value.name);
        let clear_doc = format!(
            "Selects `None` for the optional client value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = ::std::option::Option::None;
                self
            }
        }
    } else if value.default().is_some() {
        let reset = helper_ident("reset", &value.name);
        let reset_doc = format!(
            "Selects the DSL default for client value `{}` without evaluating it until `build`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = ::std::option::Option::None;
                self
            }
        }
    } else {
        quote!()
    };
    let setter_doc = match &value.kind {
        ValueKind::Required => format!(
            "Sets the required client value `{}`; a later call replaces this selection.",
            canonical_ident(&value.name)
        ),
        ValueKind::Optional => format!(
            "Selects `Some(value)` for optional client value `{}`; a later call replaces this selection.",
            canonical_ident(&value.name)
        ),
        ValueKind::Defaulted(_) => format!(
            "Overrides defaulted client value `{}`; while selected, its DSL default is not evaluated.",
            canonical_ident(&value.name)
        ),
    };
    quote! {
        #[doc = #setter_doc]
        #[must_use]
        pub fn #method(mut self, value: #setter_type) -> Self {
            self.#field = ::std::option::Option::Some(value);
            self
        }

        #helper
    }
}

fn generate_client_value_resolution(index: usize, value: &Value) -> TokenStream {
    let builder_field = builder_value_field(index, value);
    let resolved = resolved_client_value(index, value);
    let ty = &value.ty;
    match &value.kind {
        ValueKind::Required => quote! {
            let #resolved: #ty = match self.#builder_field {
                ::std::option::Option::Some(value) => value,
                ::std::option::Option::None => {
                    return ::std::result::Result::Err(
                        __concord_generated::__core::__private::missing_input_error(),
                    );
                }
            };
        },
        ValueKind::Optional => quote! {
            let #resolved: #ty = self.#builder_field;
        },
        ValueKind::Defaulted(default) => {
            let default = default_expr(default);
            quote! {
                let #resolved: #ty = match self.#builder_field {
                    ::std::option::Option::Some(value) => value,
                    ::std::option::Option::None => #default,
                };
            }
        }
    }
}

fn generate_client_child_methods(client: &Client, core_path: &TokenStream) -> TokenStream {
    let methods = client
        .children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => generate_scope_entry(client, &[], scope, true, core_path),
            Child::Endpoint(endpoint) => {
                generate_endpoint_method(client, &[], endpoint, true, core_path)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

fn generate_facade_children(
    client: &Client,
    children: &[Child],
    ancestors: &[&Scope],
    core_path: &TokenStream,
) -> TokenStream {
    let modules = children
        .iter()
        .filter_map(|child| match child {
            Child::Scope(scope) => {
                Some(generate_facade_module(client, scope, ancestors, core_path))
            }
            Child::Endpoint(_) => None,
        })
        .collect::<Vec<_>>();
    quote!(#(#modules)*)
}

fn generate_facade_module(
    client: &Client,
    scope: &Scope,
    ancestors: &[&Scope],
    core_path: &TokenStream,
) -> TokenStream {
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let module_name = &scope.module_name;
    let facade_name = &scope.facade_name;
    let depth = visible.len();
    let invocation_path = super_chain(depth + 2);
    let support_path = super_chain(depth + 1);
    let client_name = &client.name;
    let scope_values = value_category_summary(&scope.parameters, "local scope");
    let facade_doc = format!(
        "Immutable facade snapshot for the `{}` scope. {scope_values} Optional and defaulted local values use consuming setters; defaults are retained for reset without re-evaluation.",
        scope.name
    );

    let fields = facade_scope_fields(&visible);
    let constructor_parameters =
        facade_constructor_parameters(&visible, &invocation_path, client_name);
    let constructor_fields = facade_constructor_fields(&visible);
    let setters = scope
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(index, value)| generate_facade_setters(scope.id, index, value))
        .collect::<Vec<_>>();
    let child_methods = scope
        .children
        .iter()
        .map(|child| match child {
            Child::Scope(child_scope) => {
                generate_scope_entry(client, &visible, child_scope, false, core_path)
            }
            Child::Endpoint(endpoint) => {
                generate_endpoint_method(client, &visible, endpoint, false, core_path)
            }
        })
        .collect::<Vec<_>>();
    let nested = generate_facade_children(client, &scope.children, &visible, core_path);

    quote! {
        pub mod #module_name {
            #[doc = #facade_doc]
            #[derive(Clone)]
            pub struct #facade_name {
                __concord_client: #invocation_path::#client_name,
                #fields
            }

            impl #facade_name {
                #[doc(hidden)]
                pub(in #support_path) fn __concord_new(
                    #constructor_parameters
                ) -> Self {
                    Self {
                        #constructor_fields
                    }
                }

                #(#setters)*
                #(#child_methods)*
            }

            #nested
        }
    }
}

fn generate_scope_entry(
    client: &Client,
    ancestors: &[&Scope],
    scope: &Scope,
    on_client: bool,
    _core_path: &TokenStream,
) -> TokenStream {
    let method = &scope.method_name;
    let facade = &scope.facade_name;
    let module = &scope.module_name;
    let required = scope
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let parameters = required
        .iter()
        .map(|value| {
            let name = &value.name;
            let ty = &value.ty;
            quote!(#name: #ty,)
        })
        .collect::<Vec<_>>();
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let init = initialize_scope_values(scope, &required);
    let local_arguments = scope_constructor_local_arguments(scope);
    let existing_arguments = if on_client {
        quote!()
    } else {
        let arguments = facade_scope_argument_clones(ancestors);
        quote!(#arguments)
    };
    let client_argument = if on_client {
        quote!(self.clone(),)
    } else {
        quote!(self.__concord_client.clone(),)
    };
    let return_path = if on_client {
        quote!(facades::#module::#facade)
    } else {
        quote!(#module::#facade)
    };
    let constructor_path = return_path.clone();
    let doc = format!(
        "Creates an immutable `{}` facade snapshot. {} Selected scope defaults are evaluated once here.",
        scope.name,
        value_category_summary(&scope.parameters, "local scope")
    );
    let _ = client;
    quote! {
        #[must_use]
        #[doc = #doc]
        pub fn #method(&self, #(#parameters)*) -> #return_path {
            #init
            #constructor_path::__concord_new(
                #client_argument
                #existing_arguments
                #local_arguments
            )
        }
    }
}

fn initialize_scope_values(scope: &Scope, required: &[&Value]) -> TokenStream {
    let required_names = required
        .iter()
        .map(|value| (canonical_ident(&value.name), &value.name))
        .collect::<HashMap<_, _>>();
    let statements = scope
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = scope_value_field(scope.id, index, value);
            let ty = &value.ty;
            if value.is_required() {
                let name = required_names
                    .get(&canonical_ident(&value.name))
                    .expect("required scope binding");
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#name);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let default_field = scope_default_field(scope.id, index, value);
                let default = default_expr(value.default().expect("defaulted scope value"));
                quote! {
                    let #default_field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#default_field);
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn facade_scope_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let ty = &value.ty;
                    let mut values = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

fn facade_constructor_parameters(
    scopes: &[&Scope],
    invocation_path: &TokenStream,
    client_name: &Ident,
) -> TokenStream {
    let fields = facade_scope_fields_as_parameters(scopes);
    quote! {
        __concord_client: #invocation_path::#client_name,
        #fields
    }
}

fn facade_scope_fields_as_parameters(scopes: &[&Scope]) -> TokenStream {
    let parameters = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let ty = &value.ty;
                    let mut values = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#parameters)*)
}

fn facade_constructor_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote! {
        __concord_client,
        #(#fields)*
    }
}

fn facade_scope_argument_clones(scopes: &[&Scope]) -> TokenStream {
    let values = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(::std::sync::Arc::clone(&self.#field),)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(::std::sync::Arc::clone(&self.#default),));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#values)*)
}

fn scope_constructor_local_arguments(scope: &Scope) -> TokenStream {
    let values = scope
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = scope_value_field(scope.id, index, value);
            let mut values = vec![quote!(#field,)];
            if value.default().is_some() {
                let default = scope_default_field(scope.id, index, value);
                values.push(quote!(#default,));
            }
            values
        })
        .collect::<Vec<_>>();
    quote!(#(#values)*)
}

fn generate_facade_setters(scope_id: usize, index: usize, value: &Value) -> TokenStream {
    let method = &value.name;
    let field = scope_value_field(scope_id, index, value);
    let setter_type = value.setter_type();
    if value.is_optional() {
        let clear = helper_ident("clear", method);
        let setter_doc = format!(
            "Returns a new facade selecting `Some(value)` for optional scope value `{}`.",
            canonical_ident(&value.name)
        );
        let clear_doc = format!(
            "Returns a new facade selecting `None` for optional scope value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #setter_type) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::Some(value));
                self
            }

            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::None);
                self
            }
        }
    } else {
        let reset = helper_ident("reset", method);
        let default = scope_default_field(scope_id, index, value);
        let setter_doc = format!(
            "Returns a new facade overriding defaulted scope value `{}`.",
            canonical_ident(&value.name)
        );
        let reset_doc = format!(
            "Returns a new facade restoring the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #setter_type) -> Self {
                self.#field = ::std::sync::Arc::new(value);
                self
            }

            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = ::std::sync::Arc::clone(&self.#default);
                self
            }
        }
    }
}

fn generate_endpoint_method(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    on_client: bool,
    core_path: &TokenStream,
) -> TokenStream {
    let method = &endpoint.public_method_name;
    let endpoint_type_path = endpoint_type_path_from_context(scopes, endpoint, on_client);
    let required = endpoint
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let parameters = required
        .iter()
        .map(|value| {
            let name = &value.name;
            let ty = &value.ty;
            quote!(#name: #ty,)
        })
        .collect::<Vec<_>>();
    let body_type = endpoint
        .body
        .as_ref()
        .map(|body| request_body_input_type(body, core_path));
    let body_parameter = body_type.as_ref().map(|ty| quote!(body: #ty,));
    let scope_arguments = if on_client {
        quote!()
    } else {
        facade_scope_argument_clones(scopes)
    };
    let endpoint_arguments = required
        .iter()
        .map(|value| {
            let name = &value.name;
            quote!(#name,)
        })
        .collect::<Vec<_>>();
    let body_argument = body_type.as_ref().map(|_| quote!(body,));
    let client_expression = if on_client {
        quote!(self)
    } else {
        quote!(&self.__concord_client)
    };
    let endpoint_doc = endpoint_method_doc(client, scopes, endpoint);
    quote! {
        #[must_use]
        #[doc = #endpoint_doc]
        pub fn #method(
            &self,
            #(#parameters)*
            #body_parameter
        ) -> #core_path::Request<#endpoint_type_path> {
            let endpoint = #endpoint_type_path::__concord_from_facade(
                #scope_arguments
                #(#endpoint_arguments)*
                #body_argument
            );
            (#client_expression).request(endpoint)
        }
    }
}

fn endpoint_type_path_from_context(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    on_client: bool,
) -> TokenStream {
    let endpoint_name = &endpoint.name;
    let modules = scopes
        .iter()
        .map(|scope| &scope.module_name)
        .collect::<Vec<_>>();
    if on_client {
        quote!(endpoints::#(#modules::)*#endpoint_name)
    } else {
        let support_path = super_chain(scopes.len() + 1);
        quote!(#support_path::endpoints::#(#modules::)*#endpoint_name)
    }
}

fn generate_endpoint_children(
    client: &Client,
    children: &[Child],
    ancestors: &[&Scope],
    core_path: &TokenStream,
) -> TokenStream {
    let items = children
        .iter()
        .map(|child| match child {
            Child::Endpoint(endpoint) => generate_endpoint(client, ancestors, endpoint, core_path),
            Child::Scope(scope) => {
                let module = &scope.module_name;
                let mut visible = ancestors.to_vec();
                visible.push(scope);
                let children =
                    generate_endpoint_children(client, &scope.children, &visible, core_path);
                quote! {
                    pub mod #module {
                        #children
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#items)*)
}

fn generate_endpoint(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    core_path: &TokenStream,
) -> TokenStream {
    let endpoint_name = &endpoint.name;
    let capability_name = &client.endpoint_capability_name;
    let client_name = &client.name;
    let values_type = &client.values_type_name;
    let credentials_store = credentials_store_name(client);
    let depth = scopes.len();
    let support_path = super_chain(depth + 1);
    let invocation_path = super_chain(depth + 2);
    let generated_core = quote!(#support_path::__concord_generated::__core);
    let endpoint_identity = endpoint_identity(client, scopes, endpoint);
    let endpoint_doc = endpoint_type_doc(client, scopes, endpoint);
    let constructor_doc = format!(
        "Creates an explicit `{endpoint_identity}` snapshot. Required enclosing scope values are outer-to-inner; optional/defaulted controls are inherent methods."
    );

    let scope_fields = endpoint_scope_fields(scopes);
    let endpoint_fields = endpoint_value_fields(endpoint);
    let body_type = body_handler(endpoint.body.as_ref(), core_path);
    let body_field = quote!(__concord_body: #body_type,);
    let client_values_field = quote!(
        __concord_client_values: ::std::option::Option<::std::sync::Arc<#invocation_path::#values_type>>,
    );
    let credentials_field = quote!(
        __concord_credentials: ::std::option::Option<::std::sync::Arc<#invocation_path::#credentials_store>>,
    );

    let (scope_bindings, endpoint_bindings, body_binding) =
        explicit_parameter_bindings(scopes, endpoint);
    let explicit_parameters = explicit_constructor_parameters(
        scopes,
        endpoint,
        &scope_bindings,
        &endpoint_bindings,
        body_binding.as_ref(),
        core_path,
    );
    let explicit_initialization = initialize_explicit_endpoint(
        scopes,
        endpoint,
        &scope_bindings,
        &endpoint_bindings,
        body_binding.as_ref(),
        &generated_core,
        core_path,
    );
    let facade_parameters = endpoint_facade_constructor_parameters(scopes, endpoint, core_path);
    let facade_initialization =
        initialize_facade_endpoint(scopes, endpoint, &generated_core, core_path);
    let inherent_setters = generate_explicit_endpoint_setters(scopes, endpoint);

    let default_impl = (scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .all(|value| !value.is_required())
        && endpoint.parameters.iter().all(|value| !value.is_required())
        && endpoint.body.is_none())
    .then(|| {
        quote! {
            impl ::std::default::Default for #endpoint_name {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    });

    let method = method_token(&endpoint.method, core_path);
    let timeout = endpoint.config.timeout.map_or_else(
        || quote!(::std::option::Option::None),
        |duration| {
            let seconds = duration.as_secs();
            let nanos = duration.subsec_nanos();
            quote!(::std::option::Option::Some(::std::time::Duration::new(#seconds, #nanos)))
        },
    );
    let response = response_handler(&endpoint.response, core_path);
    let host_statements = host_statements(client, scopes, endpoint);
    let path_statements = path_statements(client, scopes, endpoint);
    let query_statements = query_statements(client, scopes, endpoint);
    let header_statements = header_statements(client, scopes, endpoint);
    let auth_statements = auth_statements(client, endpoint, core_path);
    let where_clause = endpoint_where_clause(client, scopes, endpoint, core_path);
    let extension_trait = generate_request_extension_trait(
        client,
        scopes,
        endpoint,
        &support_path,
        &generated_core,
        core_path,
    );

    quote! {
        #[doc = #endpoint_doc]
        pub struct #endpoint_name {
            #client_values_field
            #credentials_field
            #scope_fields
            #endpoint_fields
            #body_field
        }

        impl #endpoint_name {
            #[must_use]
            #[doc = #constructor_doc]
            pub fn new(#explicit_parameters) -> Self {
                #explicit_initialization
            }

            #[doc(hidden)]
            pub(in #support_path) fn __concord_from_facade(#facade_parameters) -> Self {
                #facade_initialization
            }

            #inherent_setters
        }

        #default_impl

        #[doc(hidden)]
        impl #support_path::__concord_endpoint_seal::Sealed for #endpoint_name {}

        #[doc(hidden)]
        impl #support_path::#capability_name for #endpoint_name {
            fn __concord_bind(mut self, client: &#invocation_path::#client_name) -> Self {
                self.__concord_client_values = ::std::option::Option::Some(
                    ::std::sync::Arc::clone(&client.__concord_values),
                );
                self.__concord_credentials = ::std::option::Option::Some(
                    ::std::sync::Arc::clone(&client.__concord_credentials),
                );
                self
            }
        }

        #[doc(hidden)]
        impl #generated_core::__private::EndpointSpec for #endpoint_name
        #where_clause
        {
            type Body = #body_type;
            type Response = #response;

            fn request_meta(&self) -> #generated_core::__private::RequestMeta {
                #generated_core::__private::RequestMeta::new(
                    #endpoint_identity,
                    #method,
                    #timeout,
                )
            }

            fn prepare_head_and_body_metadata(
                &self,
                mut __concord_builder: #generated_core::__private::HeadBuilder,
            ) -> ::std::result::Result<
                (
                    #generated_core::__private::PreparedHead,
                    #generated_core::__private::BodyMetadata,
                ),
                #generated_core::__private::ApiError,
            > {
                let __concord_client_values = self
                    .__concord_client_values
                    .as_ref()
                    .expect("generated endpoint is bound by the sealed client request path");
                #host_statements
                #path_statements
                #query_statements
                #header_statements
                #auth_statements
                let __concord_head = __concord_builder.finish();
                let __concord_method = #method;
                let __concord_body_metadata = <Self::Body as #generated_core::__private::BodySpec>::metadata(
                    &self.__concord_body,
                    #endpoint_identity,
                    &__concord_method,
                )?;
                ::std::result::Result::Ok((__concord_head, __concord_body_metadata))
            }

            fn into_body(self) -> Self::Body {
                self.__concord_body
            }
        }

        #extension_trait
    }
}

fn endpoint_scope_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let ty = &value.ty;
                    let mut fields = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        fields.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    fields
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

fn endpoint_value_fields(endpoint: &Endpoint) -> TokenStream {
    let fields = endpoint
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            let mut fields = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
            if value.default().is_some() {
                let default = endpoint_default_field(index, value);
                fields.push(quote!(#default: ::std::sync::Arc<#ty>,));
            }
            fields
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

fn explicit_constructor_parameters(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    scope_bindings: &[Ident],
    endpoint_bindings: &[Ident],
    body_binding: Option<&Ident>,
    core_path: &TokenStream,
) -> TokenStream {
    let mut scope_index = 0;
    let scope_parameters = scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .filter(|value| value.is_required())
        .map(|value| {
            let binding = &scope_bindings[scope_index];
            scope_index += 1;
            let ty = &value.ty;
            quote!(#binding: #ty,)
        })
        .collect::<Vec<_>>();
    let endpoint_parameters = endpoint
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .enumerate()
        .map(|(index, value)| {
            let binding = &endpoint_bindings[index];
            let ty = &value.ty;
            quote!(#binding: #ty,)
        })
        .collect::<Vec<_>>();
    let body = endpoint.body.as_ref().map(|kind| {
        let binding = body_binding.expect("body binding");
        let ty = request_body_input_type(kind, core_path);
        quote!(#binding: #ty,)
    });
    quote!(#(#scope_parameters)* #(#endpoint_parameters)* #body)
}

fn initialize_explicit_endpoint(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    scope_bindings: &[Ident],
    endpoint_bindings: &[Ident],
    body_binding: Option<&Ident>,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let mut scope_binding_index = 0;
    let scope_statements = scopes
        .iter()
        .flat_map(|scope| {
            scope.parameters.iter().enumerate().map(move |(index, value)| {
                (scope.id, index, value)
            })
        })
        .map(|(scope_id, index, value)| {
            let field = scope_value_field(scope_id, index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding = &scope_bindings[scope_binding_index];
                scope_binding_index += 1;
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = scope_default_field(scope_id, index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let endpoint_statements = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding_index = endpoint
                    .parameters
                    .iter()
                    .take(index + 1)
                    .filter(|value| value.is_required())
                    .count()
                    - 1;
                let binding = &endpoint_bindings[binding_index];
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = endpoint_default_field(index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let fields = endpoint_all_snapshot_field_initializers(scopes, endpoint);
    let body = body_constructor(
        endpoint.body.as_ref(),
        body_binding,
        generated_core,
        core_path,
    );
    quote! {
        #(#scope_statements)*
        #(#endpoint_statements)*
        Self {
            __concord_client_values: ::std::option::Option::None,
            __concord_credentials: ::std::option::Option::None,
            #fields
            __concord_body: #body,
        }
    }
}

fn endpoint_facade_constructor_parameters(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    core_path: &TokenStream,
) -> TokenStream {
    let scope_parameters = facade_scope_fields_as_parameters(scopes);
    let endpoint_parameters = endpoint
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_required())
        .map(|(index, value)| {
            let binding = endpoint_constructor_binding(index, value);
            let ty = &value.ty;
            quote!(#binding: #ty,)
        })
        .collect::<Vec<_>>();
    let body = endpoint.body.as_ref().map(|kind| {
        let ty = request_body_input_type(kind, core_path);
        quote!(__concord_body_input: #ty,)
    });
    quote!(#scope_parameters #(#endpoint_parameters)* #body)
}

fn initialize_facade_endpoint(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let statements = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding = endpoint_constructor_binding(index, value);
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = endpoint_default_field(index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let scope_fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    let endpoint_fields = endpoint
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let mut values = vec![quote!(#field,)];
            if value.default().is_some() {
                let default = endpoint_default_field(index, value);
                values.push(quote!(#default,));
            }
            values
        })
        .collect::<Vec<_>>();
    let body_binding = endpoint
        .body
        .as_ref()
        .map(|_| Ident::new("__concord_body_input", Span::call_site()));
    let body = body_constructor(
        endpoint.body.as_ref(),
        body_binding.as_ref(),
        generated_core,
        core_path,
    );
    quote! {
        #(#statements)*
        Self {
            __concord_client_values: ::std::option::Option::None,
            __concord_credentials: ::std::option::Option::None,
            #(#scope_fields)*
            #(#endpoint_fields)*
            __concord_body: #body,
        }
    }
}

fn endpoint_all_snapshot_field_initializers(scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let scope_fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default,));
                    }
                    values
                })
        })
        .chain(
            endpoint
                .parameters
                .iter()
                .enumerate()
                .flat_map(|(index, value)| {
                    let field = endpoint_value_field(index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = endpoint_default_field(index, value);
                        values.push(quote!(#default,));
                    }
                    values
                }),
        )
        .collect::<Vec<_>>();
    quote!(#(#scope_fields)*)
}

fn generate_explicit_endpoint_setters(scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let scope_setters = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, value)| !value.is_required())
                .map(move |(index, value)| {
                    generate_endpoint_snapshot_setters(
                        value,
                        scope_value_field(scope.id, index, value),
                        value
                            .default()
                            .map(|_| scope_default_field(scope.id, index, value)),
                    )
                })
        })
        .collect::<Vec<_>>();
    let endpoint_setters = endpoint
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(index, value)| {
            generate_endpoint_snapshot_setters(
                value,
                endpoint_value_field(index, value),
                value
                    .default()
                    .map(|_| endpoint_default_field(index, value)),
            )
        })
        .collect::<Vec<_>>();
    quote!(#(#scope_setters)* #(#endpoint_setters)*)
}

fn generate_endpoint_snapshot_setters(
    value: &Value,
    field: Ident,
    default: Option<Ident>,
) -> TokenStream {
    let method = &value.name;
    let ty = value.setter_type();
    if value.is_optional() {
        let clear = helper_ident("clear", method);
        let setter_doc = format!(
            "Returns a new explicit endpoint snapshot selecting `Some(value)` for optional value `{}`.",
            canonical_ident(&value.name)
        );
        let clear_doc = format!(
            "Returns a new explicit endpoint snapshot selecting `None` for optional value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #ty) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::Some(value));
                self
            }

            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::None);
                self
            }
        }
    } else {
        let reset = helper_ident("reset", method);
        let default = default.expect("default field");
        let setter_doc = format!(
            "Returns a new explicit endpoint snapshot overriding defaulted value `{}`.",
            canonical_ident(&value.name)
        );
        let reset_doc = format!(
            "Returns a new explicit endpoint snapshot restoring the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #ty) -> Self {
                self.#field = ::std::sync::Arc::new(value);
                self
            }

            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = ::std::sync::Arc::clone(&self.#default);
                self
            }
        }
    }
}

fn generate_request_extension_trait(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    support_path: &TokenStream,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let controls = endpoint
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .collect::<Vec<_>>();
    let acquisitions = client
        .credentials
        .iter()
        .enumerate()
        .filter(|(_, credential)| {
            matches!(
                credential.source,
                CredentialSource::Endpoint {
                    endpoint_id: Some(id),
                    ..
                } if id == endpoint.id
            )
        })
        .collect::<Vec<_>>();
    if controls.is_empty() && acquisitions.is_empty() {
        return quote!();
    }
    let endpoint_name = &endpoint.name;
    let trait_name = &endpoint.request_ext_name;
    let seal_module = Ident::new(
        &format!(
            "__concord_{}_request_ext_seal",
            to_snake_case(&canonical_ident(endpoint_name))
        ),
        Span::mixed_site(),
    );
    let trait_methods = controls
        .iter()
        .flat_map(|(_, value)| request_extension_trait_methods(value))
        .collect::<Vec<_>>();
    let impl_methods = controls
        .iter()
        .flat_map(|(_, value)| request_extension_impl_methods(value, generated_core))
        .collect::<Vec<_>>();
    let acquisition_trait_methods = acquisitions
        .iter()
        .map(|(_, credential)| {
            let method = public_ident(
                &format!(
                    "acquire_as_{}",
                    to_snake_case(&canonical_ident(&credential.name))
                ),
                credential.name.span(),
            );
            quote! {
                /// Executes this endpoint and atomically stores its decoded output as a credential.
                async fn #method(self) -> ::std::result::Result<(), #core_path::prelude::ApiError>;
            }
        })
        .collect::<Vec<_>>();
    let acquisition_impl_methods = acquisitions
        .iter()
        .map(|(credential_index, credential)| {
            let method = public_ident(
                &format!("acquire_as_{}", to_snake_case(&canonical_ident(&credential.name))),
                credential.name.span(),
            );
            let field = credential_store_field(*credential_index, &credential.name);
            let identity = endpoint_identity(client, scopes, endpoint);
            let native_method = method_token(&endpoint.method, core_path);
            quote! {
                async fn #method(self) -> ::std::result::Result<(), #core_path::prelude::ApiError> {
                    let __concord_credential = #generated_core::__private::inspect_endpoint(
                        &self,
                        |endpoint| {
                            endpoint
                                .__concord_credentials
                                .as_ref()
                                .expect("generated endpoint is bound by the sealed client request path")
                                .#field
                                .clone()
                        },
                    );
                    let value = self.await?;
                    __concord_credential
                        .set(value)
                        .await
                        .map_err(|error| {
                            #generated_core::__private::credential_store_error(
                                #identity,
                                #native_method,
                                error,
                            )
                        })
                }
            }
        })
        .collect::<Vec<_>>();
    let doc = format!(
        "Endpoint-specific consuming setters for `Request<{endpoint_name}>`. Import this client's generated `prelude::*`; it re-exports this trait as `{}`. The trait is sealed and implemented only for this exact request type.",
        endpoint.prelude_alias
    );
    let _ = support_path;
    quote! {
        #[doc(hidden)]
        mod #seal_module {
            pub struct Token;
        }

        #[doc = #doc]
        #[allow(private_interfaces, async_fn_in_trait)]
        pub trait #trait_name {
            #[doc(hidden)]
            fn __concord_request_extension_seal(
                &self,
                _: #seal_module::Token,
            );

            #(#trait_methods)*
            #(#acquisition_trait_methods)*
        }

        impl #trait_name for #core_path::Request<#endpoint_name> {
            fn __concord_request_extension_seal(
                &self,
                _: #seal_module::Token,
            ) {}

            #(#impl_methods)*
            #(#acquisition_impl_methods)*
        }
    }
}

fn request_extension_trait_methods(value: &Value) -> Vec<TokenStream> {
    let method = &value.name;
    let ty = value.setter_type();
    let setter_doc = if value.is_optional() {
        format!(
            "Consumes this request and selects `Some(value)` for optional endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    } else {
        format!(
            "Consumes this request and overrides defaulted endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    };
    let mut methods = vec![quote! {
        #[doc = #setter_doc]
        #[must_use]
        fn #method(self, value: #ty) -> Self;
    }];
    let helper = if value.is_optional() {
        helper_ident("clear", method)
    } else {
        helper_ident("reset", method)
    };
    let helper_doc = if value.is_optional() {
        format!(
            "Consumes this request and selects `None` for optional endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    } else {
        format!(
            "Consumes this request and restores the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        )
    };
    methods.push(quote! {
        #[doc = #helper_doc]
        #[must_use]
        fn #helper(self) -> Self;
    });
    methods
}

fn request_extension_impl_methods(value: &Value, generated_core: &TokenStream) -> Vec<TokenStream> {
    let method = &value.name;
    let ty = value.setter_type();
    let mut methods = vec![quote! {
        fn #method(self, value: #ty) -> Self {
            #generated_core::__private::map_endpoint(self, |endpoint| endpoint.#method(value))
        }
    }];
    let helper = if value.is_optional() {
        helper_ident("clear", method)
    } else {
        helper_ident("reset", method)
    };
    methods.push(quote! {
        fn #helper(self) -> Self {
            #generated_core::__private::map_endpoint(self, |endpoint| endpoint.#helper())
        }
    });
    methods
}

fn collect_prelude_exports(
    client: &Client,
    children: &[Child],
    modules: &[Ident],
) -> Vec<TokenStream> {
    let mut exports = Vec::new();
    for child in children {
        match child {
            Child::Scope(scope) => {
                let mut nested = modules.to_vec();
                nested.push(scope.module_name.clone());
                exports.extend(collect_prelude_exports(client, &scope.children, &nested));
            }
            Child::Endpoint(endpoint)
                if endpoint.parameters.iter().any(|value| !value.is_required())
                    || client.credentials.iter().any(|credential| {
                        matches!(
                            credential.source,
                            CredentialSource::Endpoint {
                                endpoint_id: Some(id),
                                ..
                            } if id == endpoint.id
                        )
                    }) =>
            {
                let trait_name = &endpoint.request_ext_name;
                let alias = &endpoint.prelude_alias;
                exports.push(quote! {
                    pub use super::endpoints::#(#modules::)*#trait_name as #alias;
                });
            }
            Child::Endpoint(_) => {}
        }
    }
    exports
}

fn host_statements(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let statements = endpoint
        .config
        .host
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(literal) => {
                quote!(__concord_builder.push_host_static(#literal)?;)
            }
            _ => {
                let value = display_argument(value, client, scopes, endpoint);
                quote!(__concord_builder.push_host_value(#value)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn path_statements(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let statements = endpoint
        .config
        .path
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(literal) => quote!(__concord_builder.push_static(#literal)?;),
            _ => {
                let value = display_argument(value, client, scopes, endpoint);
                quote!(__concord_builder.push_value(#value)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn query_statements(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let statements = endpoint
        .config
        .query
        .iter()
        .map(|entry| {
            let key = &entry.key;
            if let ResolvedExpr::Value(value) = &entry.value {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                match &value.shape {
                    ValueShape::Scalar => {
                        quote!(__concord_builder.push_query_value(#key, #access);)
                    }
                    ValueShape::Option { .. } => quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            __concord_builder.push_query_value(#key, value);
                        }
                    },
                    ValueShape::Vec { .. } => quote! {
                        for value in #access.iter() {
                            __concord_builder.push_query_value(#key, value);
                        }
                    },
                    ValueShape::OptionVec { .. } => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            for value in values.iter() {
                                __concord_builder.push_query_value(#key, value);
                            }
                        }
                    },
                }
            } else {
                let value = display_argument(&entry.value, client, scopes, endpoint);
                quote!(__concord_builder.push_query_value(#key, #value);)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn header_statements(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let statements = endpoint
        .config
        .headers
        .iter()
        .map(|entry| {
            let name = &entry.name;
            if let ResolvedExpr::Value(value) = &entry.value {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                match &value.shape {
                    ValueShape::Option { .. } => quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            __concord_builder.insert_header_value(#name, value)?;
                        }
                    },
                    ValueShape::Scalar => {
                        quote!(__concord_builder.insert_header_value(#name, #access)?;)
                    }
                    ValueShape::Vec { .. } | ValueShape::OptionVec { .. } => {
                        unreachable!("resolver rejects vector public headers")
                    }
                }
            } else {
                let value = display_argument(&entry.value, client, scopes, endpoint);
                quote!(__concord_builder.insert_header_value(#name, #value)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn auth_statements(client: &Client, endpoint: &Endpoint, core_path: &TokenStream) -> TokenStream {
    if endpoint.config.auth.is_empty() {
        return quote!();
    }
    let statements = endpoint
        .config
        .auth
        .iter()
        .map(|placement| {
            let credential = &client.credentials[placement.credential];
            let field = credential_store_field(placement.credential, &credential.name);
            let reference = quote!(__concord_credentials.#field.credential());
            let constructor = match &placement.kind {
                AuthPlacementKind::Bearer => {
                    quote!(#core_path::__private::Auth::bearer(#reference))
                }
                AuthPlacementKind::Basic => {
                    quote!(#core_path::__private::Auth::basic(#reference))
                }
                AuthPlacementKind::Header(name) => {
                    let canonical =
                        syn::LitStr::new(&name.value().to_ascii_lowercase(), name.span());
                    quote!(#core_path::__private::Auth::header(
                        #core_path::__private::HeaderName::from_static(#canonical),
                        #reference,
                    ))
                }
                AuthPlacementKind::Query(name) => {
                    quote!(#core_path::__private::Auth::query(#name, #reference))
                }
            };
            let policy = match placement.challenge {
                AuthChallenge::Unauthorized => quote!(
                    #core_path::advanced::ChallengePolicy::Unauthorized
                ),
                AuthChallenge::UnauthorizedOrForbidden => quote!(
                    #core_path::advanced::ChallengePolicy::UnauthorizedOrForbidden
                ),
                AuthChallenge::Never => quote!(#core_path::advanced::ChallengePolicy::Never),
            };
            quote! {
                __concord_builder.push_auth((#constructor).challenge(#policy))?;
            }
        })
        .collect::<Vec<_>>();
    quote! {
        let __concord_credentials = self
            .__concord_credentials
            .as_ref()
            .expect("generated endpoint is bound by the sealed client request path");
        #(#statements)*
    }
}

fn display_argument(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(&#value),
        ResolvedExpr::Integer(value) => quote!(&#value),
        ResolvedExpr::Value(value) => {
            resolved_value_access(value.reference, client, scopes, endpoint)
        }
        ResolvedExpr::Format(values) => {
            let writes = values
                .iter()
                .map(|value| {
                    let value = display_argument(value, client, scopes, endpoint);
                    quote! {
                        ::std::fmt::Write::write_fmt(
                            &mut __concord_formatted,
                            format_args!("{}", #value),
                        ).expect("formatting into a String is infallible");
                    }
                })
                .collect::<Vec<_>>();
            quote!(&{
                let mut __concord_formatted = ::std::string::String::new();
                #(#writes)*
                __concord_formatted
            })
        }
    }
}

fn resolved_value_access(
    reference: ValueReference,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match reference {
        ValueReference::Client(index) => {
            let value = &client.values[index];
            let field = client_value_field(index, value);
            quote!((&__concord_client_values.#field))
        }
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            let scope = scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("resolved scope identity appears in the endpoint lexical chain");
            let field = scope_value_field(scope_id, parameter, &scope.parameters[parameter]);
            quote!((&*self.#field))
        }
        ValueReference::Endpoint(index) => {
            let field = endpoint_value_field(index, &endpoint.parameters[index]);
            quote!((&*self.#field))
        }
    }
}

fn endpoint_where_clause(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    core_path: &TokenStream,
) -> TokenStream {
    let mut bounds = Vec::new();
    for value in client
        .values
        .iter()
        .chain(scopes.iter().flat_map(|scope| scope.parameters.iter()))
        .chain(endpoint.parameters.iter())
    {
        let ty = &value.ty;
        bounds.push(quote!(#ty: ::std::marker::Send + ::std::marker::Sync + 'static));
    }
    let mut display_types = Vec::new();
    collect_config_display_types(
        &endpoint.config,
        client,
        scopes,
        endpoint,
        &mut display_types,
    );
    for ty in display_types {
        bounds.push(quote!(#ty: ::std::fmt::Display));
    }
    if let Some(body) = &endpoint.body {
        match body {
            RequestBodyKind::Buffered(codec) => {
                let codec = codec_type(codec, core_path);
                bounds.push(quote!(#codec: #core_path::advanced::EncodeBody));
            }
            RequestBodyKind::Stream(media) => {
                let media = media_type(media, core_path);
                bounds.push(quote!(#media: #core_path::advanced::MediaType));
            }
            RequestBodyKind::Multipart => {}
        }
    }
    match &endpoint.response {
        ResponseKind::Buffered(codec) => {
            let codec = codec_type(codec, core_path);
            bounds.push(quote!(#codec: #core_path::advanced::DecodeBody));
        }
        ResponseKind::Stream(media) => {
            let media = media_type(media, core_path);
            bounds.push(quote!(#media: #core_path::advanced::MediaType));
        }
        ResponseKind::Bytes | ResponseKind::NoContent => {}
    }
    let mut seen = HashSet::new();
    bounds.retain(|bound| seen.insert(bound.to_string()));
    if bounds.is_empty() {
        quote!()
    } else {
        quote!(where #(#bounds,)*)
    }
}

fn collect_config_display_types(
    config: &NormalizedConfig,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    output: &mut Vec<Type>,
) {
    for expression in config.host.iter().chain(config.path.iter()) {
        collect_expression_display_types(expression, client, scopes, endpoint, false, output);
    }
    for entry in &config.query {
        collect_expression_display_types(&entry.value, client, scopes, endpoint, true, output);
    }
    for entry in &config.headers {
        collect_expression_display_types(&entry.value, client, scopes, endpoint, true, output);
    }
}

fn collect_expression_display_types(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    collection_aware: bool,
    output: &mut Vec<Type>,
) {
    match expression {
        ResolvedExpr::Value(value) => {
            let declared = resolved_value_type(value.reference, client, scopes, endpoint);
            let ty = if collection_aware {
                match &value.shape {
                    ValueShape::Option { inner } => inner.clone(),
                    ValueShape::Vec { element } | ValueShape::OptionVec { element, .. } => {
                        element.clone()
                    }
                    ValueShape::Scalar => declared.clone(),
                }
            } else {
                declared.clone()
            };
            output.push(ty);
        }
        ResolvedExpr::Format(values) => {
            for value in values {
                collect_expression_display_types(value, client, scopes, endpoint, false, output);
            }
        }
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) => {}
    }
}

fn resolved_value_type<'a>(
    reference: ValueReference,
    client: &'a Client,
    scopes: &[&'a Scope],
    endpoint: &'a Endpoint,
) -> &'a Type {
    match reference {
        ValueReference::Client(index) => &client.values[index].ty,
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            &scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("scope reference")
                .parameters[parameter]
                .ty
        }
        ValueReference::Endpoint(index) => &endpoint.parameters[index].ty,
    }
}

fn explicit_parameter_bindings(
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> (Vec<Ident>, Vec<Ident>, Option<Ident>) {
    let scope_values = scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let endpoint_values = endpoint
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let body_name = endpoint.body.as_ref().map(|_| "body".to_owned());
    let original = scope_values
        .iter()
        .chain(endpoint_values.iter())
        .map(|value| canonical_ident(&value.name))
        .chain(body_name.iter().cloned())
        .collect::<Vec<_>>();
    let reserved = original.iter().cloned().collect::<HashSet<_>>();
    let mut used = HashSet::new();
    let mut allocate = |namespace: &str, name: &Ident| {
        let original_name = canonical_ident(name);
        if original
            .iter()
            .filter(|candidate| **candidate == original_name)
            .count()
            == 1
            && used.insert(original_name.clone())
        {
            return name.clone();
        }
        let base = format!("{namespace}_{}", to_snake_case(&original_name));
        let mut candidate = base.clone();
        let mut suffix = 2;
        while reserved.contains(&candidate) || !used.insert(candidate.clone()) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        public_ident(&candidate, name.span())
    };
    let scope = scope_values
        .iter()
        .map(|value| allocate("scope", &value.name))
        .collect();
    let endpoint_bindings = endpoint_values
        .iter()
        .map(|value| allocate("endpoint", &value.name))
        .collect();
    let body = body_name.map(|_| allocate("request", &Ident::new("body", Span::call_site())));
    (scope, endpoint_bindings, body)
}

fn endpoint_constructor_binding(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_input", index, &value.name)
}

fn body_constructor(
    body: Option<&RequestBodyKind>,
    binding: Option<&Ident>,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    match (body, binding) {
        (Some(RequestBodyKind::Buffered(codec)), Some(binding)) => {
            let codec = codec_type(codec, core_path);
            quote!(#generated_core::__private::CodecBody::<#codec>::new(#binding))
        }
        (Some(RequestBodyKind::Stream(media)), Some(binding)) => {
            let media = media_type(media, core_path);
            quote!(#generated_core::__private::StreamRequestBody::<#media>::new(#binding))
        }
        (Some(RequestBodyKind::Multipart), Some(binding)) => quote!(#binding),
        (None, None) => quote!(#generated_core::__private::EmptyBody::new()),
        _ => unreachable!("body and binding are modeled together"),
    }
}

fn endpoint_method_doc(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    let identity = endpoint_identity(client, scopes, endpoint);
    let method = method_name(&endpoint.method);
    let route = route_shape(&endpoint.config.path);
    let controls = endpoint
        .parameters
        .iter()
        .filter(|value| !value.is_required())
        .map(|value| canonical_ident(&value.name))
        .collect::<Vec<_>>();
    let control_doc = if controls.is_empty() {
        "This endpoint has no endpoint-specific request setters.".to_owned()
    } else {
        format!(
            "Import the generated support `prelude::*` to use consuming request setters for: {}.",
            controls.join(", ")
        )
    };
    let values = value_category_summary(&endpoint.parameters, "endpoint");
    let configuration = endpoint_configuration_summary(client, endpoint);
    let features = endpoint_feature_summary(client, endpoint);
    format!(
        "Creates a side-effect-free pending `{method}` request for `{identity}` at `{route}`. {values} {control_doc} {configuration} {} Direct await uses the `{}` response mode. {} {features}",
        request_body_doc(endpoint.body.as_ref()),
        response_name(&endpoint.response),
        response_doc(&endpoint.response),
    )
}

fn endpoint_type_doc(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    let identity = endpoint_identity(client, scopes, endpoint);
    let method = method_name(&endpoint.method);
    let route = route_shape(&endpoint.config.path);
    let endpoint_values = value_category_summary(&endpoint.parameters, "endpoint");
    let scope_values = if scopes.is_empty() {
        "No enclosing scope values are stored.".to_owned()
    } else {
        scopes
            .iter()
            .map(|scope| value_category_summary(&scope.parameters, "enclosing scope"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let configuration = endpoint_configuration_summary(client, endpoint);
    let features = endpoint_feature_summary(client, endpoint);
    format!(
        "Explicit `{method}` endpoint snapshot for `{identity}` at route `{route}`. It executes through the same client request path as its facade method. {scope_values} {endpoint_values} {configuration} {} Direct await uses the `{}` response mode. {} {features}",
        request_body_doc(endpoint.body.as_ref()),
        response_name(&endpoint.response),
        response_doc(&endpoint.response),
    )
}

fn endpoint_identity(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    std::iter::once(client.name.to_string())
        .chain(scopes.iter().map(|scope| scope.name.to_string()))
        .chain(std::iter::once(endpoint.name.to_string()))
        .collect::<Vec<_>>()
        .join("::")
}

fn route_shape(path: &[ResolvedExpr]) -> String {
    let values = path
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(value) => value.value(),
            ResolvedExpr::Integer(value) => value.to_string(),
            ResolvedExpr::Value(value) => match value.reference {
                ValueReference::Client(_) => "{vars.VALUE}".to_owned(),
                ValueReference::Scope { .. } => "{scope.VALUE}".to_owned(),
                ValueReference::Endpoint(_) => "{ep.VALUE}".to_owned(),
            },
            ResolvedExpr::Format(_) => "{fmt[...]}".to_owned(),
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        "<fixed base path>".to_owned()
    } else {
        format!("/<fixed base>/{}", values.join("/"))
    }
}

fn value_category_summary(values: &[Value], owner: &str) -> String {
    let names = |predicate: fn(&Value) -> bool| {
        values
            .iter()
            .filter(|value| predicate(value))
            .map(|value| format!("`{}`", canonical_ident(&value.name)))
            .collect::<Vec<_>>()
    };
    let required = names(Value::is_required);
    let optional = names(Value::is_optional);
    let defaulted = names(|value| value.default().is_some());
    let mut categories = Vec::new();
    if !required.is_empty() {
        categories.push(format!("Required {owner} values: {}.", required.join(", ")));
    }
    if !optional.is_empty() {
        categories.push(format!("Optional {owner} values: {}.", optional.join(", ")));
    }
    if !defaulted.is_empty() {
        categories.push(format!(
            "Defaulted {owner} values: {}; default expressions are intentionally omitted.",
            defaulted.join(", ")
        ));
    }
    if categories.is_empty() {
        format!("No {owner} values are declared.")
    } else {
        categories.join(" ")
    }
}

fn retry_summary(retry: &RetrySelection) -> &'static str {
    match retry {
        RetrySelection::Disabled => "Application native retry is disabled.",
        RetrySelection::ReqwestDefault => {
            "Reqwest default native retry is explicitly selected; an HTTP builder retry method may replace it."
        }
        RetrySelection::Custom(_) => {
            "A dangerous custom native retry factory is selected and requires `dangerous-custom-retry`; an HTTP builder retry method may replace it before factory evaluation."
        }
    }
}

fn endpoint_configuration_summary(client: &Client, endpoint: &Endpoint) -> String {
    let query_count = endpoint.config.query.len();
    let header_count = endpoint.config.headers.len();
    let timeout = endpoint.config.timeout.map_or_else(
        || "No inherited operation deadline is active.".to_owned(),
        |duration| {
            format!(
                "The inherited operation deadline is {} seconds and {} nanoseconds; inherent request setters may override, disable, or restore it.",
                duration.as_secs(),
                duration.subsec_nanos()
            )
        },
    );
    let authentication = if endpoint.config.auth.is_empty() {
        "The effective authentication plan is empty.".to_owned()
    } else {
        let placements = endpoint
            .config
            .auth
            .iter()
            .map(|placement| {
                let category = match placement.kind {
                    AuthPlacementKind::Bearer => "bearer",
                    AuthPlacementKind::Basic => "basic",
                    AuthPlacementKind::Header(_) => "custom header",
                    AuthPlacementKind::Query(_) => "private native query",
                };
                let challenge = match placement.challenge {
                    AuthChallenge::Unauthorized => "unauthorized",
                    AuthChallenge::UnauthorizedOrForbidden => "unauthorized-or-forbidden",
                    AuthChallenge::Never => "never",
                };
                format!("{category} ({challenge})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The effective authentication plan has {} placement(s), in declaration order: {placements}. Credential values are absent from generated documentation.",
            endpoint.config.auth.len()
        )
    };
    format!(
        "The normalized public head contains {query_count} public query declaration(s) and {header_count} public header declaration(s). {authentication} {timeout} {}",
        retry_summary(&client.retry)
    )
}

fn endpoint_feature_summary(client: &Client, endpoint: &Endpoint) -> String {
    let uses_json = matches!(
        &endpoint.body,
        Some(RequestBodyKind::Buffered(CodecKind::Json(_)))
    ) || matches!(
        &endpoint.response,
        ResponseKind::Buffered(CodecKind::Json(_))
    ) || client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::OAuth2Client { .. }));
    let uses_multipart = matches!(&endpoint.body, Some(RequestBodyKind::Multipart));
    let mut requirements = Vec::new();
    if uses_json {
        requirements.push("`concord_core/json`");
    }
    if uses_multipart {
        requirements.push("`concord_core/multipart`");
    }
    if requirements.is_empty() {
        "No endpoint-specific Cargo feature is required by this body/response mode.".to_owned()
    } else {
        format!("Required Cargo feature(s): {}.", requirements.join(", "))
    }
}

fn credential_summary(client: &Client) -> String {
    if client.credentials.is_empty() {
        return "No credential sources are declared.".to_owned();
    }
    let sources = client
        .credentials
        .iter()
        .map(|credential| match credential.source {
            CredentialSource::ApiKey { .. } => "static API key (replace)",
            CredentialSource::Bearer { .. } => "static bearer (replace)",
            CredentialSource::Basic { .. } => "atomic Basic pair (replace)",
            CredentialSource::OAuth2Client { .. } => {
                "OAuth2 client credentials (atomic client replacement)"
            }
            CredentialSource::Endpoint { .. } => "endpoint-backed (set/clear)",
            CredentialSource::Custom { .. } => "custom provider (clear)",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "It declares {} protected secret input(s) and {} credential source(s), in order: {sources}. Source-specific handles expose only their documented capabilities.",
        client.secrets.len(),
        client.credentials.len()
    )
}

fn method_name(method: &MethodKind) -> String {
    match method {
        MethodKind::Get => "GET".to_owned(),
        MethodKind::Post => "POST".to_owned(),
        MethodKind::Put => "PUT".to_owned(),
        MethodKind::Patch => "PATCH".to_owned(),
        MethodKind::Delete => "DELETE".to_owned(),
        MethodKind::Head => "HEAD".to_owned(),
        MethodKind::Options => "OPTIONS".to_owned(),
        MethodKind::Trace => "TRACE".to_owned(),
        MethodKind::Connect => "CONNECT".to_owned(),
        MethodKind::Custom(value) => value.value(),
    }
}

fn method_token(method: &MethodKind, core_path: &TokenStream) -> TokenStream {
    match method {
        MethodKind::Get => quote!(#core_path::__private::Method::GET),
        MethodKind::Post => quote!(#core_path::__private::Method::POST),
        MethodKind::Put => quote!(#core_path::__private::Method::PUT),
        MethodKind::Patch => quote!(#core_path::__private::Method::PATCH),
        MethodKind::Delete => quote!(#core_path::__private::Method::DELETE),
        MethodKind::Head => quote!(#core_path::__private::Method::HEAD),
        MethodKind::Options => quote!(#core_path::__private::Method::OPTIONS),
        MethodKind::Trace => quote!(#core_path::__private::Method::TRACE),
        MethodKind::Connect => quote!(#core_path::__private::Method::CONNECT),
        MethodKind::Custom(value) => quote!(
            #core_path::__private::Method::from_bytes(#value.as_bytes())
                .expect("the Concord macro validates custom HTTP method tokens")
        ),
    }
}

fn response_handler(response: &ResponseKind, core_path: &TokenStream) -> TokenStream {
    match response {
        ResponseKind::Bytes => quote!(#core_path::__private::BytesResponse),
        ResponseKind::NoContent => quote!(#core_path::__private::NoContentResponse),
        ResponseKind::Buffered(codec) => {
            let codec = codec_type(codec, core_path);
            quote!(#core_path::__private::BufferedResponse<#codec>)
        }
        ResponseKind::Stream(media) => {
            let media = media_type(media, core_path);
            quote!(#core_path::__private::StreamResponseHandler<#media>)
        }
    }
}

fn body_handler(body: Option<&RequestBodyKind>, core_path: &TokenStream) -> TokenStream {
    match body {
        Some(RequestBodyKind::Buffered(codec)) => {
            let codec = codec_type(codec, core_path);
            quote!(#core_path::__private::CodecBody<#codec>)
        }
        Some(RequestBodyKind::Stream(media)) => {
            let media = media_type(media, core_path);
            quote!(#core_path::__private::StreamRequestBody<#media>)
        }
        Some(RequestBodyKind::Multipart) => quote!(#core_path::advanced::MultipartBody),
        None => quote!(#core_path::__private::EmptyBody),
    }
}

fn request_body_input_type(body: &RequestBodyKind, core_path: &TokenStream) -> TokenStream {
    match body {
        RequestBodyKind::Buffered(codec) => codec_input_type(codec, core_path),
        RequestBodyKind::Stream(_) => quote!(#core_path::prelude::StreamBody),
        RequestBodyKind::Multipart => quote!(#core_path::advanced::MultipartBody),
    }
}

fn codec_type(codec: &CodecKind, core_path: &TokenStream) -> TokenStream {
    match codec {
        CodecKind::Text => quote!(#core_path::prelude::Text),
        CodecKind::Json(ty) => quote!(#core_path::prelude::Json<#ty>),
        CodecKind::Custom(ty) => quote!(#ty),
    }
}

fn media_type(media: &MediaKind, core_path: &TokenStream) -> TokenStream {
    match media {
        MediaKind::OctetStream => quote!(#core_path::prelude::OctetStream),
        MediaKind::Custom(ty) => quote!(#ty),
    }
}

fn codec_input_type(codec: &CodecKind, core_path: &TokenStream) -> TokenStream {
    match codec {
        CodecKind::Text => quote!(::std::string::String),
        CodecKind::Json(ty) => quote!(#ty),
        CodecKind::Custom(ty) => quote!(<#ty as #core_path::advanced::EncodeBody>::Input),
    }
}

fn request_body_doc(body: Option<&RequestBodyKind>) -> String {
    match body {
        None => "The request is bodyless.".to_owned(),
        Some(RequestBodyKind::Buffered(codec)) => {
            format!(
                "The final `body` argument uses the `{}` buffered body mode and is encoded only after protected public-head preparation and the before-send hook.",
                codec_name(codec)
            )
        }
        Some(RequestBodyKind::Stream(media)) => {
            format!(
                "The final `body` argument uses `Stream<{}>` with an owned one-shot `StreamBody`; it remains unpolled until managed native execution.",
                media_name(media)
            )
        }
        Some(RequestBodyKind::Multipart) => {
            "The final `body` argument is `MultipartBody`, requires `concord_core/multipart`, and remains subject to aggregate framing limits while the managed HTTP layer owns the top-level boundary header.".to_owned()
        }
    }
}

fn response_doc(response: &ResponseKind) -> &'static str {
    match response {
        ResponseKind::Stream(_) => {
            "Direct await returns `StreamResponse<M>` with a finite stream-response limit; `.response()` is intentionally unavailable."
        }
        _ => {
            "Direct await returns the semantic value and `.response()` selects successful metadata."
        }
    }
}

fn response_name(response: &ResponseKind) -> String {
    match response {
        ResponseKind::Bytes => "Bytes".to_owned(),
        ResponseKind::NoContent => "NoContent".to_owned(),
        ResponseKind::Buffered(codec) => codec_name(codec),
        ResponseKind::Stream(media) => format!("Stream<{}>", media_name(media)),
    }
}

fn codec_name(codec: &CodecKind) -> String {
    match codec {
        CodecKind::Text => "Text".to_owned(),
        CodecKind::Json(ty) => format!("Json<{}>", ty.to_token_stream()),
        CodecKind::Custom(ty) => ty.to_token_stream().to_string(),
    }
}

fn media_name(media: &MediaKind) -> String {
    match media {
        MediaKind::OctetStream => "OctetStream".to_owned(),
        MediaKind::Custom(ty) => ty.to_token_stream().to_string(),
    }
}

fn client_uses_json(client: &Client) -> bool {
    client
        .credentials
        .iter()
        .any(|credential| matches!(credential.source, CredentialSource::OAuth2Client { .. }))
        || endpoints(&client.children).any(|endpoint| {
            endpoint
                .body
                .as_ref()
                .is_some_and(|body| matches!(body, RequestBodyKind::Buffered(CodecKind::Json(_))))
                || matches!(
                    &endpoint.response,
                    ResponseKind::Buffered(CodecKind::Json(_))
                )
        })
}

fn client_uses_multipart(client: &Client) -> bool {
    endpoints(&client.children)
        .any(|endpoint| matches!(&endpoint.body, Some(RequestBodyKind::Multipart)))
}

fn endpoints(children: &[Child]) -> Box<dyn Iterator<Item = &Endpoint> + '_> {
    Box::new(children.iter().flat_map(|child| match child {
        Child::Endpoint(endpoint) => {
            Box::new(std::iter::once(endpoint)) as Box<dyn Iterator<Item = &Endpoint>>
        }
        Child::Scope(scope) => endpoints(&scope.children),
    }))
}

fn default_expr(value: &DefaultExpr) -> TokenStream {
    match value {
        DefaultExpr::String(value) => quote!(#value),
        DefaultExpr::Integer { negative, literal } if *negative => quote!(-#literal),
        DefaultExpr::Integer { literal, .. } => quote!(#literal),
        DefaultExpr::Boolean(value) => quote!(#value),
        DefaultExpr::Path(value) => quote!(#value),
    }
}

fn super_chain(count: usize) -> TokenStream {
    let supers = (0..count).map(|_| quote!(super)).collect::<Vec<_>>();
    quote!(#(#supers)::* )
}

fn helper_ident(prefix: &str, name: &Ident) -> Ident {
    public_ident(&format!("{prefix}_{}", canonical_ident(name)), name.span())
}

fn client_value_field(index: usize, value: &Value) -> Ident {
    private_ident("client_value", index, &value.name)
}

fn builder_value_field(index: usize, value: &Value) -> Ident {
    private_ident("builder_value", index, &value.name)
}

fn resolved_client_value(index: usize, value: &Value) -> Ident {
    private_ident("resolved_client_value", index, &value.name)
}

fn scope_value_field(scope_id: usize, index: usize, value: &Value) -> Ident {
    Ident::new(
        &format!(
            "__concord_scope_{scope_id}_{index}_{}",
            to_snake_case(&canonical_ident(&value.name))
        ),
        Span::mixed_site(),
    )
}

fn scope_default_field(scope_id: usize, index: usize, value: &Value) -> Ident {
    Ident::new(
        &format!(
            "__concord_scope_default_{scope_id}_{index}_{}",
            to_snake_case(&canonical_ident(&value.name))
        ),
        Span::mixed_site(),
    )
}

fn endpoint_value_field(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_value", index, &value.name)
}

fn endpoint_default_field(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_default", index, &value.name)
}

fn credentials_store_name(client: &Client) -> Ident {
    Ident::new(
        &format!(
            "__Concord{}Credentials",
            crate::names::to_pascal_case(&canonical_ident(&client.name))
        ),
        Span::mixed_site(),
    )
}

fn credentials_facade_name(client: &Client) -> Ident {
    Ident::new(
        &format!(
            "{}Credentials",
            crate::names::to_pascal_case(&canonical_ident(&client.name))
        ),
        client.name.span(),
    )
}

fn secret_builder_field(index: usize, name: &Ident) -> Ident {
    private_ident("builder_secret", index, name)
}

fn resolved_secret(index: usize, name: &Ident) -> Ident {
    private_ident("resolved_secret", index, name)
}

fn provider_builder_field(index: usize, name: &Ident) -> Ident {
    private_ident("builder_provider", index, name)
}

fn resolved_provider(index: usize, name: &Ident) -> Ident {
    private_ident("resolved_provider", index, name)
}

fn credential_store_field(index: usize, name: &Ident) -> Ident {
    private_ident("credential", index, name)
}

fn provider_argument_name(name: &Ident) -> Ident {
    public_ident(
        &format!("{}_provider", to_snake_case(&canonical_ident(name))),
        name.span(),
    )
}

fn private_ident(namespace: &str, index: usize, name: &Ident) -> Ident {
    Ident::new(
        &format!(
            "__concord_{namespace}_{index}_{}",
            to_snake_case(&canonical_ident(name))
        ),
        Span::mixed_site(),
    )
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;
    use crate::{parse, resolve};

    #[test]
    fn complete_generation_consumes_normalized_multi_client_ir() {
        let source = parse::parse(quote! {
            client FirstApi {
                base "https://example.test";
                var tenant: String;
                GET Root(id: String, verbose: Option<bool>) -> Bytes {
                    path [vars.tenant, ep.id];
                }
            }
            client SecondApi {
                base "http://example.test";
                scope resources(region: String) {
                    GET List() -> NoContent;
                }
            }
        })
        .expect("AST");
        let api = resolve::resolve(source).expect("IR");
        let output = generate(&api, &quote!(::concord_core)).to_string();
        assert!(!output.contains("FirstApiRegions"));
        assert!(output.contains("FirstApiRootRequestExt"));
        assert!(output.contains("SecondApi"));
        assert!(output.contains("map_endpoint"));
    }
}
