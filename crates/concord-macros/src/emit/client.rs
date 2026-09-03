use super::*;

pub(super) fn generate_client(client: &Client, facade_path: &TokenStream) -> TokenStream {
    let client_name = &client.name;
    let builder_name = &client.surface.builder_name;
    let support_name = &client.surface.support_name;
    let values_type = &client.surface.values_type_name;
    let credentials_store = credentials_store_name(client);
    let specification_module = specification_module_name(client);
    let specification_aliases = generate_specification_aliases(&client.children);
    let specification_assertions =
        generate_specification_assertions(&client.children, facade_path, &specification_module);
    let base = &client.base;
    let contract_assertion = generated_contract_assertion();

    let value_summary = value_category_summary(&client.values, "Client values");
    let credential_summary = credential_summary(client);
    let timeout_summary = client.config.timeout.map_or_else(
        || "Default operation timeout: disabled.".to_owned(),
        |duration| format!("Default operation timeout: {}.", human_duration(duration)),
    );
    let rate_summary = if client.requires_limiter {
        "Rate-limit policies are declared; building requires an explicitly selected limiter, and Concord never supplies one implicitly."
    } else {
        "Rate-limit policies: none."
    };
    let client_doc = format!(
        "Immutable generated client for base URL `{}`. Build it with [`{builder_name}`]. Transport, runtime policy, client values, and credential sources are fixed in the built snapshot.",
        client.base.value()
    );
    let builder_doc = format!(
        "Builder for [`{client_name}`].\n\n# Inputs\n\n{value_summary} {credential_summary}\n\n# Value controls\n\n`set_*` replaces the exact effective type. Concord-optional values also expose `with_*` and `clear_*`; defaulted values expose `reset_*`. A builder reset discards the override so the next successful `build` evaluates the declaration default once.\n\n# Defaults\n\nBase URL: `{}`. {timeout_summary} {rate_summary} `#[default]` invokes application-owned `Default::default()` for the written payload type; explicit stable initializers are evaluated at the same owner boundary. Concord performs no managed I/O during construction.\n\n# Configuration\n\nUse `retry` for a Concord-managed retry policy, `configure_http` for transport policy, and `configure_runtime` for gates, completion observation, finite limits, limiter selection, and runtime policy. Transformations apply immediately in call order.",
        client.base.value()
    );
    let support_doc = format!(
        "Generated scope navigation, unbound endpoint intents, and named requests for [`{client_name}`]."
    );
    let constructor_doc = format!(
        "Builds `{client_name}` with its declared defaults. This constructor exists only when no required client values, secrets, custom providers, limiter selection, or other runtime choices are missing."
    );
    let capability_doc = format!(
        "Sealed endpoint contract for [`{client_name}`]. Its associated [`Request`](Endpoint::Request) is the named bound request type returned by this client's direct and unbound-then-bound paths. Downstream types cannot implement this trait."
    );

    let value_fields = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = client_value_field(index, value);
            let ty = &value.effective_type;
            quote!(#field: #ty,)
        })
        .collect::<Vec<_>>();
    let builder_value_fields = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = builder_value_field(index, value);
            let ty = value.client_builder_storage_type();
            quote!(#field: #facade_path::__private::Override<#ty>,)
        })
        .collect::<Vec<_>>();
    let secret_builder_fields = client
        .secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| {
            let field = secret_builder_field(index, &secret.name);
            quote!(#field: ::std::option::Option<#facade_path::auth::SecretString>,)
        })
        .collect::<Vec<_>>();
    let provider_builder_fields = client
        .credentials
        .iter()
        .enumerate()
        .filter_map(|(index, credential)| match &credential.source {
            CredentialSource::Custom { provider } => {
                let field = provider_builder_field(index, &credential.name);
                Some(quote!(
                    #field: ::std::option::Option<::std::sync::Arc<
                        dyn #facade_path::auth::CredentialProvider<
                            Material = <#provider as #facade_path::auth::CredentialProvider>::Material
                        >
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
            quote!(#field: #facade_path::__private::Override::UseDeclaration,)
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
    let secret_builder_methods = generate_secret_builder_methods(client, facade_path);
    let provider_builder_methods = generate_provider_builder_methods(client, facade_path);
    let client_constructor = (!client.has_required_construction_inputs()).then(|| {
        quote! {
            #[doc = #constructor_doc]
            pub fn new() -> ::std::result::Result<Self, #facade_path::ClientBuildError> {
                Self::builder().build()
            }
        }
    });
    let build_value_statements = client
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| generate_client_value_resolution(index, value))
        .collect::<Vec<_>>();
    let build_secret_statements = generate_secret_resolution(client, facade_path);
    let build_provider_statements = generate_provider_resolution(client, facade_path);
    let build_static_credential_statements =
        generate_static_credential_resolution(client, facade_path);
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
    let credential_store_fields = generate_credential_store_fields(client, facade_path);
    let credential_store_initializers = generate_credential_store_initializers(client, facade_path);
    let credentials_module = generate_credentials_module(client, facade_path);
    let credentials_method = generate_credentials_method(client);

    let runtime_baseline = client.config.timeout.map_or_else(
        || quote!(__concord_generated::__core::__private::default_runtime_config()),
        |duration| {
            let seconds = duration.as_secs();
            let nanos = duration.subsec_nanos();
            quote!(
                __concord_generated::__core::__private::default_runtime_config()
                    .runtime_endpoint_timeout(::std::time::Duration::new(#seconds, #nanos))
            )
        },
    );
    let requires_limiter = client.requires_limiter;
    let prepare_runtime = quote!(
        __concord_generated::__core::__private::prepare_generated_rate_limits(
            runtime,
            #requires_limiter,
        )?
    );

    let scope_tree = generate_scope_tree(client, &client.children, &[], facade_path);
    let unbound_tree = generate_unbound_tree(client);
    let unbound_type_path = Ident::new("Unbound", Span::mixed_site());
    let unbound_value = &unbound_type_path;
    let client_child_methods = generate_client_child_methods(client, facade_path);
    let rate_limit_identity_constants = rate_limit_identity_constants(client);
    let (contract_static, contract_declaration) =
        generate_contract_description(client, facade_path);

    quote! {
        #rate_limit_identity_constants
        #specification_assertions

        #[doc(hidden)]
        pub mod #specification_module {
            #[allow(unused_imports)]
            use super::*;
            #[allow(unused_imports)]
            use #facade_path::prelude::*;

            #specification_aliases
        }

        struct #values_type {
            #(#value_fields)*
        }

        struct #credentials_store {
            #credential_store_fields
        }

        #[doc = #client_doc]
        #[derive(Clone)]
        pub struct #client_name {
            __concord_core: #facade_path::__private::ClientCore,
            __concord_values: ::std::sync::Arc<#values_type>,
            __concord_credentials: ::std::sync::Arc<#credentials_store>,
        }

        #[doc = #builder_doc]
        pub struct #builder_name {
            #(#builder_value_fields)*
            #(#secret_builder_fields)*
            #(#provider_builder_fields)*
            __concord_http: ::std::option::Option<#facade_path::http::HttpConfigBuilder>,
            __concord_runtime: ::std::option::Option<#facade_path::runtime::RuntimeConfigBuilder>,
        }

        #[doc = #support_doc]
        pub mod #support_name {
            #[allow(unused_imports)]
            use super::*;

            #contract_declaration

            mod __concord_generated {
                pub(super) mod __core {
                    pub use #facade_path::*;
                }

                #contract_assertion
            }

            mod __concord_endpoint_seal {
                pub trait Sealed {}
            }

            #[doc = #capability_doc]
            #[allow(private_bounds)]
            pub trait Endpoint: __concord_endpoint_seal::Sealed + ::std::marker::Sized {
                /// Named inert request produced when this endpoint is bound to its client.
                type Request;
                /// Response specification selected by this endpoint declaration.
                type Response;

                /// Returns the value-free static or bounded runtime endpoint description.
                fn description(
                    &self,
                ) -> &#facade_path::endpoint::EndpointDescription;

                #[doc(hidden)]
                fn __concord_bind(self, client: &super::#client_name) -> Self::Request;

                #[doc(hidden)]
                fn __concord_plan(
                    &self,
                    client: &super::#client_name,
                ) -> ::std::result::Result<
                    #facade_path::endpoint::CredentialFreeRequestPlan,
                    #facade_path::ApiError,
                >;
            }

            #[doc(hidden)]
            impl<R> __concord_endpoint_seal::Sealed
                for #facade_path::endpoint::RuntimeEndpoint<R>
            where
                R: #facade_path::response::ResponseSpec,
            {}

            #[doc(hidden)]
            impl<R> Endpoint for #facade_path::endpoint::RuntimeEndpoint<R>
            where
                R: #facade_path::response::ResponseSpec,
            {
                type Request = #facade_path::endpoint::Request<Self>;
                type Response = R;

                fn description(
                    &self,
                ) -> &#facade_path::endpoint::EndpointDescription {
                    self.description()
                }

                fn __concord_bind(self, client: &super::#client_name) -> Self::Request {
                    __concord_generated::__core::__private::request(
                        &client.__concord_core,
                        self,
                    )
                }

                fn __concord_plan(
                    &self,
                    client: &super::#client_name,
                ) -> ::std::result::Result<
                    #facade_path::endpoint::CredentialFreeRequestPlan,
                    #facade_path::ApiError,
                > {
                    __concord_generated::__core::__private::credential_free_request_plan(
                        &client.__concord_core,
                        self.description(),
                        <R as #facade_path::response::ResponseSpec>::accept(),
                        |builder| {
                            <Self as __concord_generated::__core::__private::EndpointSpec>::prepare_head(
                                self,
                                builder,
                            )
                        },
                    )
                }
            }

            #credentials_module

            #unbound_tree
            #scope_tree

            impl super::#client_name {
                #client_constructor

                #[must_use]
                /// Creates the canonical generated client builder.
                pub fn builder() -> super::#builder_name {
                    super::#builder_name::new()
                }

                /// Starts the inert declaration-shaped unbound endpoint tree.
                #[must_use]
                pub const fn unbound() -> #unbound_type_path {
                    #unbound_value
                }

                /// Returns the immutable, value-free v2 contract tree.
                #[must_use]
                pub fn contract() -> &'static #facade_path::endpoint::ContractDescription {
                    ::std::sync::LazyLock::force(&#contract_static)
                }

                #client_child_methods
                #credentials_method

                #[must_use]
                /// Creates a side-effect-free pending request from an unbound endpoint intent
                /// generated for this client or a stable runtime endpoint.
                pub fn request<E>(&self, endpoint: E) -> E::Request
                where
                    E: Endpoint,
                {
                    <E as Endpoint>::__concord_bind(endpoint, self)
                }

                /// Builds one credential-free, redaction-aware no-effect plan by borrowing an endpoint value.
                pub fn plan<E>(
                    &self,
                    endpoint: &E,
                ) -> ::std::result::Result<
                    #facade_path::endpoint::CredentialFreeRequestPlan,
                    #facade_path::ApiError,
                >
                where
                    E: Endpoint,
                {
                    <E as Endpoint>::__concord_plan(endpoint, self)
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
                        __concord_http: ::std::option::Option::Some(
                            __concord_generated::__core::__private::default_http_config()
                        ),
                        __concord_runtime: ::std::option::Option::Some(#runtime_baseline),
                    }
                }

                #(#builder_methods)*
                #secret_builder_methods
                #provider_builder_methods

                #[must_use]
                /// Applies an HTTP configuration transformation immediately and in call order.
                ///
                /// A later selection replaces an earlier selection for the same setting.
                pub fn configure_http<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #facade_path::http::HttpConfigBuilder,
                    ) -> #facade_path::http::HttpConfigBuilder,
                {
                    let Some(http) = self.__concord_http.take() else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    self.__concord_http = ::std::option::Option::Some(configure(http));
                    self
                }

                #[must_use]
                /// Applies a runtime configuration transformation immediately and in call order.
                ///
                /// A later selection replaces an earlier selection for the same setting.
                pub fn configure_runtime<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #facade_path::runtime::RuntimeConfigBuilder,
                    ) -> #facade_path::runtime::RuntimeConfigBuilder,
                {
                    let Some(runtime) = self.__concord_runtime.take() else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    self.__concord_runtime = ::std::option::Option::Some(configure(runtime));
                    self
                }

                #[must_use]
                /// Installs a Concord-managed retry policy immediately and in call order.
                ///
                /// A later `retry` call or runtime configuration transformation replaces it.
                pub fn retry(mut self, policy: #facade_path::retry::RetryPolicy) -> Self {
                    let Some(runtime) = self.__concord_runtime.take() else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    self.__concord_runtime =
                        ::std::option::Option::Some(runtime.retry_policy(policy));
                    self
                }

                /// Resolves selected defaults once and builds the immutable generated client snapshot.
                pub fn build(
                    self,
                ) -> ::std::result::Result<super::#client_name, #facade_path::ClientBuildError> {
                    #(#build_value_statements)*
                    #build_secret_statements
                    #build_provider_statements
                    #build_static_credential_statements
                    let Some(http) = self.__concord_http else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    let Some(runtime) = self.__concord_runtime else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    let runtime = #prepare_runtime;
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

fn generate_contract_description(
    client: &Client,
    facade_path: &TokenStream,
) -> (Ident, TokenStream) {
    let static_name = private_ident("contract_description", 0, &client.name);
    let client_path_text = canonical_ident(&client.name);
    let client_path = syn::LitStr::new(&client_path_text, client.name.span());
    let declaration = declaration_tokens(
        &client.name,
        &client_path_text,
        &client.attributes,
        quote!(#facade_path::endpoint::DeclarationKind::Client),
        facade_path,
    );
    let values = client.values.iter().map(|value| {
        value_tokens(
            value,
            quote!(#facade_path::endpoint::ValueOwnerKind::Client),
            &client_path,
            facade_path,
        )
    });
    let secrets = client.secrets.iter().map(|secret| {
        let name = syn::LitStr::new(&canonical_ident(&secret.name), secret.name.span());
        quote!(#facade_path::endpoint::SecretDescription::generated(#name))
    });
    let credentials = client.credentials.iter().map(|credential| {
        let name = syn::LitStr::new(&canonical_ident(&credential.name), credential.name.span());
        let (source, protected_input_count, endpoint_id, provider_type, behavior_class) =
            match &credential.source {
                CredentialSource::ApiKey { .. } => (
                    quote!(#facade_path::endpoint::CredentialSourceKind::ApiKey),
                    1_usize,
                    quote!(::std::option::Option::None),
                    quote!(::std::option::Option::None),
                    quote!(#facade_path::endpoint::ContractFactClass::StaticDeclaration),
                ),
                CredentialSource::Bearer { .. } => (
                    quote!(#facade_path::endpoint::CredentialSourceKind::Bearer),
                    1_usize,
                    quote!(::std::option::Option::None),
                    quote!(::std::option::Option::None),
                    quote!(#facade_path::endpoint::ContractFactClass::StaticDeclaration),
                ),
                CredentialSource::Basic { .. } => (
                    quote!(#facade_path::endpoint::CredentialSourceKind::Basic),
                    2_usize,
                    quote!(::std::option::Option::None),
                    quote!(::std::option::Option::None),
                    quote!(#facade_path::endpoint::ContractFactClass::StaticDeclaration),
                ),
                CredentialSource::Endpoint { endpoint_id, .. } => {
                    let endpoint_id = endpoint_id
                        .and_then(|id| endpoint_contract_id_by_semantic_id(&client.children, id))
                        .map_or_else(
                            || quote!(::std::option::Option::None),
                            |id| {
                                let id = syn::LitStr::new(id, credential.name.span());
                                quote!(::std::option::Option::Some(#id))
                            },
                        );
                    (
                        quote!(#facade_path::endpoint::CredentialSourceKind::Endpoint),
                        0_usize,
                        endpoint_id,
                        quote!(::std::option::Option::None),
                        quote!(#facade_path::endpoint::ContractFactClass::StaticDeclaration),
                    )
                }
                CredentialSource::Custom { provider } => {
                    let provider = type_literal(provider);
                    (
                        quote!(#facade_path::endpoint::CredentialSourceKind::CustomProvider),
                        0_usize,
                        quote!(::std::option::Option::None),
                        quote!(::std::option::Option::Some(#provider)),
                        quote!(#facade_path::endpoint::ContractFactClass::OpaqueExtension),
                    )
                }
            };
        quote! {
            #facade_path::endpoint::CredentialDescription::generated(
                #name,
                #source,
                #protected_input_count,
                #endpoint_id,
                #provider_type,
                #behavior_class,
            )
        }
    });
    let rate_profiles = client.rate_limit_profiles.iter().map(|profile| {
        let name = syn::LitStr::new(&profile.name, client.name.span());
        let id = syn::LitStr::new(&profile.id, client.name.span());
        let explicit_id = profile.explicit_id;
        let formals = profile
            .formals
            .iter()
            .map(|formal| syn::LitStr::new(formal, client.name.span()));
        let dimensions = profile
            .dimensions
            .iter()
            .map(|dimension| syn::LitStr::new(dimension, client.name.span()));
        let cost = profile.cost;
        let windows = profile.windows.iter().map(|window| {
            let capacity = window.capacity;
            let seconds = window.duration.as_secs();
            let nanos = window.duration.subsec_nanos();
            quote! {
                #facade_path::endpoint::RateLimitWindowDescription::generated(
                    #capacity,
                    ::std::time::Duration::new(#seconds, #nanos),
                )
            }
        });
        quote! {
            #facade_path::endpoint::RateProfileDescription::generated(
                #name,
                #id,
                #explicit_id,
                ::std::vec![#(#formals),*],
                ::std::vec![#(#dimensions),*],
                #cost,
                ::std::vec![#(#windows),*],
            )
        }
    });
    let local = client
        .local_attachments
        .iter()
        .map(|attachment| attachment_tokens(attachment, true, facade_path));
    let effective = effective_config_attachment_tokens(&client.config, facade_path);
    let scopes = client.children.iter().filter_map(|child| match child {
        Child::Scope(scope) => Some(scope_contract_tokens(client, scope, &[], facade_path)),
        Child::Endpoint(_) => None,
    });
    let endpoints = client.children.iter().filter_map(|child| match child {
        Child::Endpoint(endpoint) => Some(endpoint_description_clone(endpoint, &[])),
        Child::Scope(_) => None,
    });
    let timeout = client.config.timeout.map_or_else(
        || quote!(#facade_path::endpoint::TimeoutSelection::Disabled),
        |duration| {
            let seconds = duration.as_secs();
            let nanos = duration.subsec_nanos();
            quote!(#facade_path::endpoint::TimeoutSelection::Explicit(
                ::std::time::Duration::new(#seconds, #nanos)
            ))
        },
    );
    let base = &client.base;
    let tokens = quote! {
        static #static_name: ::std::sync::LazyLock<#facade_path::endpoint::ContractDescription> =
            ::std::sync::LazyLock::new(|| {
                #facade_path::endpoint::ContractDescription::generated(
                    #declaration,
                    #base,
                    #timeout,
                    ::std::vec![#(#values),*],
                    ::std::vec![#(#secrets),*],
                    ::std::vec![#(#credentials),*],
                    ::std::vec![#(#rate_profiles),*],
                    ::std::vec![#(#local),*],
                    ::std::vec![#(#effective),*],
                    ::std::vec![#(#scopes),*],
                    ::std::vec![#(#endpoints),*],
                )
            });
    };
    (static_name, tokens)
}

fn endpoint_contract_id_by_semantic_id(children: &[Child], id: usize) -> Option<&str> {
    for child in children {
        match child {
            Child::Endpoint(endpoint) if endpoint.id == id => {
                return Some(&endpoint.contract_id.value);
            }
            Child::Scope(scope) => {
                if let Some(contract_id) = endpoint_contract_id_by_semantic_id(&scope.children, id)
                {
                    return Some(contract_id);
                }
            }
            Child::Endpoint(_) => {}
        }
    }
    None
}

fn scope_contract_tokens(
    client: &Client,
    scope: &Scope,
    ancestors: &[&Scope],
    facade_path: &TokenStream,
) -> TokenStream {
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let path_text = std::iter::once(canonical_ident(&client.name))
        .chain(visible.iter().map(|scope| canonical_ident(&scope.name)))
        .collect::<Vec<_>>()
        .join("::");
    let path = syn::LitStr::new(&path_text, scope.name.span());
    let declaration = declaration_tokens(
        &scope.name,
        &path_text,
        &scope.attributes,
        quote!(#facade_path::endpoint::DeclarationKind::Scope),
        facade_path,
    );
    let values = scope.parameters.iter().map(|value| {
        value_tokens(
            value,
            quote!(#facade_path::endpoint::ValueOwnerKind::Scope),
            &path,
            facade_path,
        )
    });
    let local = scope
        .local_attachments
        .iter()
        .map(|attachment| attachment_tokens(attachment, true, facade_path));
    let effective = effective_config_attachment_tokens(&scope.config, facade_path);
    let scopes = scope.children.iter().filter_map(|child| match child {
        Child::Scope(child) => Some(scope_contract_tokens(client, child, &visible, facade_path)),
        Child::Endpoint(_) => None,
    });
    let module_path = visible
        .iter()
        .map(|scope| scope.surface.module_name.clone())
        .collect::<Vec<_>>();
    let endpoints = scope.children.iter().filter_map(|child| match child {
        Child::Endpoint(endpoint) => Some(endpoint_description_clone(endpoint, &module_path)),
        Child::Scope(_) => None,
    });
    quote!(#facade_path::endpoint::ScopeDescription::generated(
        #declaration,
        ::std::vec![#(#values),*],
        ::std::vec![#(#local),*],
        ::std::vec![#(#effective),*],
        ::std::vec![#(#scopes),*],
        ::std::vec![#(#endpoints),*],
    ))
}

fn endpoint_description_clone(endpoint: &Endpoint, modules: &[Ident]) -> TokenStream {
    let description = private_ident("endpoint_description", endpoint.id, &endpoint.name);
    if modules.is_empty() {
        quote!(::std::sync::LazyLock::force(&#description).clone())
    } else {
        quote!(::std::sync::LazyLock::force(&#(#modules)::*::#description).clone())
    }
}

pub(super) fn generate_builder_value_methods(index: usize, value: &Value) -> TokenStream {
    let setter = value_setter_ident(&value.name);
    let field = builder_value_field(index, value);
    let setter_type = value.setter_argument_type();
    let selected_value = quote!(__concord_generated::__core::__private::Override::Set(value));
    let with = value.accepts_absence().then(|| {
        let method = helper_ident("with", &value.name);
        let payload_type = value.with_argument_type();
        quote! {
            #[must_use]
            pub fn #method(mut self, value: #payload_type) -> Self {
                self.#field = __concord_generated::__core::__private::Override::Set(
                    ::std::option::Option::Some(value),
                );
                self
            }
        }
    });
    let clear = value.accepts_absence().then(|| {
        let clear = helper_ident("clear", &value.name);
        let clear_doc = format!(
            "Explicitly selects absence for the absence-capable client value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = __concord_generated::__core::__private::Override::Set(
                    ::std::option::Option::None,
                );
                self
            }
        }
    });
    let reset = value.has_default().then(|| {
        let reset = helper_ident("reset", &value.name);
        let reset_doc = format!(
            "Removes the override for client value `{}` so its declaration default is selected during `.build()`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = __concord_generated::__core::__private::Override::UseDeclaration;
                self
            }
        }
    });
    let setter_doc = format!(
        "Replaces client value `{}` with its exact effective type; a later call replaces this override.",
        canonical_ident(&value.name)
    );
    quote! {
        #[doc = #setter_doc]
        #[must_use]
        pub fn #setter(mut self, value: #setter_type) -> Self {
            self.#field = #selected_value;
            self
        }

        #with
        #clear
        #reset
    }
}

pub(super) fn generate_client_value_resolution(index: usize, value: &Value) -> TokenStream {
    let builder_field = builder_value_field(index, value);
    let resolved = resolved_client_value(index, value);
    let method = &value.name;
    let ty = &value.effective_type;
    if value.is_required() {
        return quote! {
            let #resolved: #ty = match self.#builder_field {
                __concord_generated::__core::__private::Override::Set(value) => value,
                __concord_generated::__core::__private::Override::UseDeclaration => {
                    return ::std::result::Result::Err(
                        __concord_generated::__core::__private::missing_input_error(
                            ::std::stringify!(#method),
                            __concord_generated::__core::__private::ClientInputKind::Value,
                        ),
                    );
                }
            };
        };
    }
    if let Some(default) = value.default() {
        let default = default_expr(default);
        let default = if value.is_optional() {
            quote!(::std::option::Option::Some(#default))
        } else {
            default
        };
        quote! {
            let #resolved: #ty = match self.#builder_field {
                __concord_generated::__core::__private::Override::Set(value) => value,
                __concord_generated::__core::__private::Override::UseDeclaration => #default,
            };
        }
    } else {
        debug_assert!(value.is_optional());
        quote! {
            let #resolved: #ty = match self.#builder_field {
                __concord_generated::__core::__private::Override::Set(value) => value,
                __concord_generated::__core::__private::Override::UseDeclaration => {
                    ::std::option::Option::None
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_client_builder_and_sealed_endpoint_contract() {
        let api = test_support::api();
        let output = generate_client(&api.client, &test_support::facade_path()).to_string();

        assert!(output.contains("pub struct ArtifactApi"));
        assert!(output.contains("pub struct ArtifactApiBuilder"));
        assert!(output.contains("pub fn builder"));
        assert!(output.contains("pub fn retry"));
        assert!(output.contains("retry_policy"));
        assert!(output.contains("pub trait Endpoint"));
    }
}
