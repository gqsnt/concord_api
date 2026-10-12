//! The client type, its builder, credential facade and root module items.

use proc_macro2::TokenStream;
use quote::quote;

use super::owner::{binding_methods, initializer_expr};
use super::{Context, contract, contract_assertion, docs, lit, owner};
use crate::semantic::{Credential, CredentialSource};

pub(crate) fn generate(context: &Context<'_>) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let client = &model.client.name;
    let module = &model.client.module;
    let assertion = contract_assertion(context);
    let doc = docs::client(model);
    let deprecated = docs::deprecated_attr(&model.root().meta);
    let display = lit(&model.client.logical);
    let base = lit(&model.client.base.value());
    let root_items = root_items(context);
    let module_tokens = owner::module(context, 0, root_items);
    let allow_deprecated = super::internal_deprecation_allowance(model);
    let _ = f;
    quote! {
        #assertion

        #[doc = #doc]
        #deprecated
        #[derive(Clone)]
        pub struct #client {
            __concord: ::std::sync::Arc<#module::__State>,
        }

        #allow_deprecated
        impl ::std::fmt::Debug for #client {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter
                    .debug_struct(#display)
                    .field("base", &#base)
                    .finish_non_exhaustive()
            }
        }

        #module_tokens
    }
}

/// The credential material type of one source.
pub(crate) fn material_type(context: &Context<'_>, credential: &Credential) -> TokenStream {
    let f = context.f;
    match &credential.source {
        CredentialSource::ApiKey { .. } => quote!(#f::auth::ApiKey),
        CredentialSource::Bearer { .. } => quote!(#f::auth::AccessToken),
        CredentialSource::Basic { .. } => quote!(#f::auth::BasicCredential),
        CredentialSource::Endpoint { endpoint, .. } => {
            let output = context.rust_type(&context.model.endpoints[*endpoint].output);
            quote!(#output)
        }
        CredentialSource::Custom { provider } => {
            let provider = context.rust_type(provider);
            quote!(<#provider as #f::auth::CredentialProvider>::Material)
        }
    }
}

fn root_items(context: &Context<'_>) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let client = &model.client.name;
    let builder = &model.client.builder;
    let root = model.root();
    let ids = model.endpoints.iter().map(|endpoint| lit(&endpoint.id));
    let contract = contract::contract_expr(context);
    let binding = binding_methods(context, 0);
    let has_required = model.client.has_required_construction_inputs(root);
    let constructor = (!has_required).then(|| {
        let doc = format!(
            "Builds `{client}` with its declared defaults. Exists only because no required value, secret, provider or limiter choice remains; it delegates to the same builder validation."
        );
        quote! {
            #[doc = #doc]
            ///
            /// # Errors
            ///
            /// Returns [`ClientBuildError`](#f::ClientBuildError) when transport
            /// or runtime construction fails.
            pub fn new() -> ::std::result::Result<Self, #f::ClientBuildError> {
                Self::builder().build()
            }
        }
    });
    let credentials_method = (!model.client.credentials.is_empty()).then(|| {
        quote! {
            /// Borrows the typed local credential-management facade. Clear and
            /// replace are synchronous local operations on this client's shared
            /// stores; they perform no network work and do not revoke remote
            /// sessions.
            #[must_use]
            pub fn credentials(&self) -> Credentials<'_> {
                Credentials { __client: self }
            }
        }
    });
    let credentials = credentials_facade(context);
    let builder_tokens = builder_items(context);
    let trait_doc = format!(
        "Generated endpoint intents of [`{client}`]. Binding an intent to another client type fails type checking. Implemented only by this client's generated intents."
    );
    quote! {
        #[doc(hidden)]
        pub const __CONCORD_ENDPOINT_IDS: &[&str] = &[#(#ids),*];

        mod __concord_seal {
            pub trait Sealed {}
        }

        #[doc = #trait_doc]
        #[allow(private_bounds)]
        pub trait GeneratedEndpoint: __concord_seal::Sealed + ::std::marker::Sized {
            /// Named inert request produced by binding.
            type Request;

            #[doc(hidden)]
            fn __concord_bind(self, client: &super::#client) -> Self::Request;

            #[doc(hidden)]
            fn __concord_plan(
                &self,
                client: &super::#client,
            ) -> ::std::result::Result<#f::endpoint::RequestPlan, #f::ApiError>;
        }

        impl super::#client {
            /// Creates the generated client builder; declaration defaults are
            /// evaluated once now.
            #[must_use]
            pub fn builder() -> #builder {
                #builder::new()
            }

            #constructor

            /// Starts the inert unbound navigation tree.
            #[must_use]
            pub const fn unbound() -> Unbound {
                Unbound
            }

            /// Returns the immutable, value-free effective contract
            /// (`concord.contract/3` model): owners, values, routes, key
            /// programs, credentials, auth, rate coverage and sharing, statuses,
            /// effects, pagination and terminals. Built once without I/O.
            #[must_use]
            pub fn contract() -> &'static #f::inspection::ContractDescription {
                static CONTRACT: ::std::sync::LazyLock<#f::inspection::ContractDescription> =
                    ::std::sync::LazyLock::new(|| {
                        #contract
                            .build()
                            .expect("concord macro/core contract mismatch: macro-validated contract rejected")
                    });
                &CONTRACT
            }

            #credentials_method

            /// Binds an unbound intent generated for this client. No I/O is
            /// performed; the intent's captured snapshots are kept and retained
            /// defaults are not re-evaluated.
            pub fn request<E>(&self, endpoint: E) -> E::Request
            where
                E: GeneratedEndpoint,
            {
                GeneratedEndpoint::__concord_bind(endpoint, self)
            }

            /// Builds the inert plan of an unbound intent under this client.
            ///
            /// # Errors
            ///
            /// Public shaping, encoding or structural-limit failures.
            pub fn plan<E>(
                &self,
                endpoint: &E,
            ) -> ::std::result::Result<#f::endpoint::RequestPlan, #f::ApiError>
            where
                E: GeneratedEndpoint,
            {
                GeneratedEndpoint::__concord_plan(endpoint, self)
            }

            #binding
        }

        #credentials
        #builder_tokens
    }
}

fn credentials_facade(context: &Context<'_>) -> TokenStream {
    let model = context.model;
    if model.client.credentials.is_empty() {
        return TokenStream::new();
    }
    let f = context.f;
    let methods = model.client.credentials.iter().map(|credential| {
        let field = &credential.field;
        let clear = &credential.clear;
        let replace = &credential.replace;
        let material = material_type(context, credential);
        let clear_doc = format!(
            "Clears credential `{}` locally: advances its installation intent even when empty, removes ready material and supersedes in-flight explicit acquisition. Performs no network work.",
            credential.logical
        );
        let replace_doc = format!(
            "Replaces credential `{}` locally: advances its installation intent and installs `material` as a new version. Performs no network work.",
            credential.logical
        );
        quote! {
            #[doc = #clear_doc]
            ///
            /// # Errors
            ///
            /// A value-free `AuthError` when the store is poisoned or its
            /// checked counters are exhausted (fail closed).
            pub fn #clear(&self) -> ::std::result::Result<(), #f::auth::AuthError> {
                self.__client.__concord.#field.clear()
            }

            #[doc = #replace_doc]
            ///
            /// # Errors
            ///
            /// A value-free `AuthError` when the store is poisoned or its
            /// checked counters are exhausted (fail closed).
            pub fn #replace(
                &self,
                material: #material,
            ) -> ::std::result::Result<#f::auth::CredentialVersion, #f::auth::AuthError> {
                self.__client.__concord.#field.replace(material)
            }
        }
    });
    let client = &model.client.name;
    quote! {
        /// Borrowed local credential-management facade.
        pub struct Credentials<'a> {
            __client: &'a super::#client,
        }

        impl Credentials<'_> {
            #(#methods)*
        }

        impl ::std::fmt::Debug for Credentials<'_> {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.debug_struct("Credentials").finish_non_exhaustive()
            }
        }
    }
}

fn builder_items(context: &Context<'_>) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let p = context.p();
    let client = &model.client.name;
    let builder = &model.client.builder;
    let root = model.root();
    let doc = docs::builder(model);
    let base = lit(&model.client.base.value());
    let requires_limiter = model.client.requires_limiter;

    let mut fields = Vec::new();
    let mut initial_statements = Vec::new();
    let mut initial_fields = Vec::new();
    let mut methods = Vec::new();
    let mut missing = Vec::new();
    let mut resolve = Vec::new();
    let mut state_values = Vec::new();
    for value in &root.values {
        let field = &value.field;
        let initial = &value.initial_field;
        let ty = context.rust_type(&value.ty);
        let deprecated = docs::deprecated_attr(&value.meta);
        let controls = &value.controls;
        if value.is_required_input() {
            fields.push(quote!(#field: ::std::option::Option<::std::sync::Arc<#ty>>,));
            initial_fields.push(quote!(#field: ::std::option::Option::None,));
            let setter = controls.set.as_ref().expect("root set");
            let if_some = controls.set_if_some.as_ref().expect("root set_if_some");
            let setter_name = lit(&setter.to_string());
            missing.push(quote! {
                if self.#field.is_none() {
                    missing.push((#setter_name, #f::error::ClientInputKind::Value));
                }
            });
            resolve.push(quote! {
                let ::std::option::Option::Some(#field) = self.#field else {
                    unreachable!("missing inputs were reported above");
                };
            });
            state_values.push(quote!(#field,));
            let set_doc = docs::control(value, "set", &model.client.logical);
            let if_some_doc = docs::control(value, "set_if_some", &model.client.logical);
            methods.push(quote! {
                #[doc = #set_doc]
                #deprecated
                #[must_use]
                pub fn #setter(mut self, value: impl #f::IntoValue<#ty>) -> Self {
                    self.#field = ::std::option::Option::Some(::std::sync::Arc::new(#f::IntoValue::<#ty>::into_value(value)));
                    self
                }

                #[doc = #if_some_doc]
                #deprecated
                #[must_use]
                pub fn #if_some(mut self, value: ::std::option::Option<#ty>) -> Self {
                    if let ::std::option::Option::Some(value) = value {
                        self.#field = ::std::option::Option::Some(::std::sync::Arc::new(value));
                    }
                    self
                }
            });
            continue;
        }
        let slot = owner::slot_type(context, value);
        fields.push(quote!(#field: #slot,));
        if value.is_initialized() {
            fields.push(quote!(#initial: ::std::sync::Arc<#ty>,));
            let init = initializer_expr(context, value);
            initial_statements.push(quote!(let #initial = #init;));
            if value.optional {
                initial_fields.push(quote!(#field: ::std::option::Option::Some(::std::sync::Arc::clone(&#initial)), #initial,));
            } else {
                initial_fields.push(quote!(#field: ::std::sync::Arc::clone(&#initial), #initial,));
            }
        } else {
            initial_fields.push(quote!(#field: ::std::option::Option::None,));
        }
        state_values.push(quote!(#field: self.#field,));
        let controls_tokens =
            owner::value_controls(context, std::slice::from_ref(value), &model.client.logical);
        methods.push(controls_tokens);
    }
    for secret in &model.client.secrets {
        let field = &secret.field;
        let method = &secret.name;
        let name = lit(&secret.logical);
        fields.push(quote!(#field: ::std::option::Option<#f::SecretString>,));
        initial_fields.push(quote!(#field: ::std::option::Option::None,));
        let doc = format!(
            "Supplies required protected secret `{}`; a later call replaces it. The value is never formatted or exposed in errors.",
            secret.logical
        );
        methods.push(quote! {
            #[doc = #doc]
            #[must_use]
            pub fn #method(mut self, value: #f::SecretString) -> Self {
                self.#field = ::std::option::Option::Some(value);
                self
            }
        });
        missing.push(quote! {
            if self.#field.is_none() {
                missing.push((#name, #f::error::ClientInputKind::Secret));
            }
        });
        resolve.push(quote! {
            let ::std::option::Option::Some(#field) = self.#field else {
                unreachable!("missing inputs were reported above");
            };
        });
    }
    let mut credential_statements = Vec::new();
    let mut credential_fields = Vec::new();
    for credential in &model.client.credentials {
        let field = &credential.field;
        match &credential.source {
            CredentialSource::ApiKey { secret } | CredentialSource::Bearer { secret } => {
                let secret = &model.client.secrets[*secret];
                let secret_field = &secret.field;
                let secret_name = lit(&secret.logical);
                let material = if matches!(credential.source, CredentialSource::ApiKey { .. }) {
                    quote!(#f::auth::ApiKey)
                } else {
                    quote!(#f::auth::AccessToken)
                };
                credential_statements.push(quote! {
                    let #field = #material::try_from_secret(::std::clone::Clone::clone(&#secret_field))
                        .map_err(|error| #p::credential_input_error(#secret_name, error))?;
                    let #field = #p::GeneratedCredential::static_value(&core, #field);
                });
            }
            CredentialSource::Basic { username, password } => {
                let username = &model.client.secrets[*username];
                let password = &model.client.secrets[*password];
                let username_field = &username.field;
                let password_field = &password.field;
                let username_name = lit(&username.logical);
                let password_name = lit(&password.logical);
                credential_statements.push(quote! {
                    let #field = #f::auth::BasicCredential::try_from_secrets(
                        ::std::clone::Clone::clone(&#username_field),
                        ::std::clone::Clone::clone(&#password_field),
                    )
                    .map_err(|error| #p::basic_credential_input_error(#username_name, #password_name, error))?;
                    let #field = #p::GeneratedCredential::static_value(&core, #field);
                });
            }
            CredentialSource::Endpoint { .. } => {
                credential_statements
                    .push(quote!(let #field = #p::GeneratedCredential::endpoint(&core);));
            }
            CredentialSource::Custom { provider } => {
                let provider = context.rust_type(provider);
                let provider_field =
                    syn::Ident::new(&format!("p_{}", credential.logical), credential.name.span());
                let method = &credential.name;
                let name = lit(&credential.logical);
                fields.push(quote!(#provider_field: ::std::option::Option<#provider>,));
                initial_fields.push(quote!(#provider_field: ::std::option::Option::None,));
                let doc = format!(
                    "Supplies the required custom credential provider for `{}`; a later call replaces it. Its declared helper origins are validated at build.",
                    credential.logical
                );
                methods.push(quote! {
                    #[doc = #doc]
                    #[must_use]
                    pub fn #method(mut self, provider: #provider) -> Self {
                        self.#provider_field = ::std::option::Option::Some(provider);
                        self
                    }
                });
                missing.push(quote! {
                    if self.#provider_field.is_none() {
                        missing.push((#name, #f::error::ClientInputKind::CredentialProvider));
                    }
                });
                resolve.push(quote! {
                    let ::std::option::Option::Some(#provider_field) = self.#provider_field else {
                        unreachable!("missing inputs were reported above");
                    };
                });
                credential_statements.push(quote! {
                    let #field = #p::GeneratedCredential::provider(&core, #name, #provider_field)?;
                });
            }
        }
        credential_fields.push(quote!(#field,));
    }
    let runtime_baseline = quote!(#p::default_runtime_config());
    let unused_secrets = model.client.secrets.iter().map(|secret| {
        let field = &secret.field;
        quote!(let _ = &#field;)
    });
    quote! {
        #[doc = #doc]
        pub struct #builder {
            #(#fields)*
            __http: ::std::option::Option<#f::http::HttpConfigBuilder>,
            __runtime: ::std::option::Option<#f::runtime::RuntimeConfigBuilder>,
        }

        impl #builder {
            fn __with(mut self, update: impl ::std::ops::FnOnce(&mut Self)) -> Self {
                update(&mut self);
                self
            }

            /// Creates the builder, evaluating declaration defaults once.
            #[must_use]
            pub fn new() -> Self {
                #(#initial_statements)*
                Self {
                    #(#initial_fields)*
                    __http: ::std::option::Option::Some(#p::default_http_config()),
                    __runtime: ::std::option::Option::Some(#runtime_baseline),
                }
            }

            #(#methods)*

            /// Applies an HTTP configuration transformation immediately and in
            /// call order (TLS roots, proxy routing, connect timeouts,
            /// compression, pooling). Redirects, cookies and upgrades stay
            /// disabled.
            #[must_use]
            pub fn configure_http<C>(mut self, configure: C) -> Self
            where
                C: ::std::ops::FnOnce(#f::http::HttpConfigBuilder) -> #f::http::HttpConfigBuilder,
            {
                let http = self.__http.take().unwrap_or_else(#p::default_http_config);
                self.__http = ::std::option::Option::Some(configure(http));
                self
            }

            /// Applies a runtime configuration transformation immediately and in
            /// call order (limits, retry policy, limiter, observers).
            #[must_use]
            pub fn configure_runtime<C>(mut self, configure: C) -> Self
            where
                C: ::std::ops::FnOnce(#f::runtime::RuntimeConfigBuilder) -> #f::runtime::RuntimeConfigBuilder,
            {
                let runtime = self.__runtime.take().unwrap_or_else(#p::default_runtime_config);
                self.__runtime = ::std::option::Option::Some(configure(runtime));
                self
            }

            /// Installs a Concord-managed retry policy (shorthand for
            /// `configure_runtime(|runtime| runtime.retry_policy(policy))`).
            #[must_use]
            pub fn retry(mut self, policy: #f::retry::RetryPolicy) -> Self {
                let runtime = self.__runtime.take().unwrap_or_else(#p::default_runtime_config);
                self.__runtime = ::std::option::Option::Some(runtime.retry_policy(policy));
                self
            }

            /// Builds the immutable client snapshot. Defaults are not evaluated
            /// again. No network access, provider acquisition or remote schema
            /// registration happens.
            ///
            /// # Errors
            ///
            /// One `ClientBuildError` accumulating every missing required value,
            /// secret and provider (by setter name), or an invalid credential,
            /// limiter or transport configuration.
            pub fn build(self) -> ::std::result::Result<super::#client, #f::ClientBuildError> {
                let mut missing: ::std::vec::Vec<(&'static str, #f::error::ClientInputKind)> = ::std::vec::Vec::new();
                #(#missing)*
                let http = self.__http.unwrap_or_else(#p::default_http_config);
                let runtime = self.__runtime.unwrap_or_else(#p::default_runtime_config);
                #p::preflight_generated_build(#base, &http, &runtime, #requires_limiter, missing)?;
                #(#resolve)*
                let core = #p::build_client_core(#base, http, runtime)?;
                #(#credential_statements)*
                #(#unused_secrets)*
                ::std::result::Result::Ok(super::#client {
                    __concord: ::std::sync::Arc::new(__State {
                        __core: core,
                        #(#credential_fields)*
                        #(#state_values)*
                    }),
                })
            }
        }

        impl ::std::default::Default for #builder {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::fmt::Debug for #builder {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.debug_struct(::std::stringify!(#builder)).finish_non_exhaustive()
            }
        }
    }
}
