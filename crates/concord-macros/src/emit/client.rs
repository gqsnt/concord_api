use super::*;

pub(super) fn generate_client(
    client: &Client,
    core_path: &TokenStream,
    capabilities: Capabilities,
) -> TokenStream {
    let client_name = &client.name;
    let builder_name = &client.surface.builder_name;
    let support_name = &client.surface.support_name;
    let values_type = &client.surface.values_type_name;
    let credentials_store = credentials_store_name(client);
    let specification_module = specification_module_name(client);
    let specification_aliases = generate_specification_aliases(&client.children);
    let base = &client.base;
    let contract_assertion = generated_contract_assertion();

    let value_summary = value_category_summary(&client.values, "client");
    let retry_summary = retry_summary(&client.retry);
    let credential_summary = credential_summary(client);
    let rate_summary = if client.requires_limiter {
        "This client statically declares or attaches rate-limit rules and therefore requires functional or explicit no-op limiter selection."
    } else {
        "This client declares no rate-limit rule."
    };
    let client_doc = format!(
        "Generated Concord client `{client_name}` with one fixed immutable public-value snapshot. {value_summary} {credential_summary} {retry_summary} {rate_summary}"
    );
    let builder_doc = format!(
        "Canonical fallible builder for [`{client_name}`]. {value_summary} {credential_summary} Selected client defaults are evaluated once by `build`; overridden defaults are not evaluated. {retry_summary} {rate_summary}"
    );
    let support_doc = format!(
        "Predictable generated scopes, endpoints, and named request types for [`{client_name}`]."
    );
    let constructor_doc = format!(
        "Builds zero-input client `{client_name}` through its canonical fallible builder. {value_summary} {credential_summary}"
    );
    let capability_doc = format!(
        "Sealed endpoint contract for [`{client_name}`]. Its associated [`Request`](Endpoint::Request) is the named bound request type returned by this client's direct and explicit binding paths. Downstream types cannot implement this trait."
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
            quote!(#field: ::std::option::Option<#core_path::auth::SecretString>,)
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
                        dyn #core_path::auth::CredentialProvider<
                            Material = <#provider as #core_path::auth::CredentialProvider>::Material
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
    let client_constructor = (!client.has_required_construction_inputs()).then(|| {
        quote! {
            #[doc = #constructor_doc]
            pub fn new() -> ::std::result::Result<Self, #core_path::ClientBuildError> {
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
        ResolvedRetry::Disabled => {
            quote!(__concord_generated::__core::__private::default_http_config())
        }
        ResolvedRetry::ReqwestDefault => quote!(
            __concord_generated::__core::__private::default_http_config().retry_reqwest_default()
        ),
    };
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

    let scope_tree = generate_scope_tree(client, &client.children, &[], core_path, capabilities);
    let client_child_methods = generate_client_child_methods(client, core_path);
    let rate_limit_identity_constants = rate_limit_identity_constants(client);

    quote! {
        #rate_limit_identity_constants

        #[doc(hidden)]
        pub mod #specification_module {
            #[allow(unused_imports)]
            use super::*;
            #[allow(unused_imports)]
            use #core_path::prelude::*;

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
            __concord_core: #core_path::__private::ClientCore,
            __concord_values: ::std::sync::Arc<#values_type>,
            __concord_credentials: ::std::sync::Arc<#credentials_store>,
        }

        #[doc = #builder_doc]
        pub struct #builder_name {
            #(#builder_value_fields)*
            #(#secret_builder_fields)*
            #(#provider_builder_fields)*
            __concord_http: ::std::option::Option<#core_path::http::HttpConfigBuilder>,
            __concord_runtime: ::std::option::Option<#core_path::runtime::RuntimeConfigBuilder>,
        }

        #[doc = #support_doc]
        pub mod #support_name {
            #[allow(unused_imports)]
            use super::*;

            mod __concord_generated {
                pub(super) use #core_path as __core;

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

                /// Returns the safe static or bounded runtime endpoint description.
                fn description(
                    &self,
                ) -> &#core_path::endpoint::EndpointDescription;

                #[doc(hidden)]
                fn __concord_bind(self, client: &super::#client_name) -> Self::Request;

                #[doc(hidden)]
                fn __concord_plan(
                    &self,
                    client: &super::#client_name,
                ) -> ::std::result::Result<
                    #core_path::endpoint::SafeRequestPlan,
                    #core_path::ApiError,
                >;
            }

            #[doc(hidden)]
            impl<R> __concord_endpoint_seal::Sealed
                for #core_path::endpoint::RuntimeEndpoint<R>
            where
                R: #core_path::response::ResponseBodySpec,
            {}

            #[doc(hidden)]
            impl<R> Endpoint for #core_path::endpoint::RuntimeEndpoint<R>
            where
                R: #core_path::response::ResponseBodySpec,
            {
                type Request = #core_path::endpoint::Request<Self>;
                type Response = R;

                fn description(
                    &self,
                ) -> &#core_path::endpoint::EndpointDescription {
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
                    #core_path::endpoint::SafeRequestPlan,
                    #core_path::ApiError,
                > {
                    __concord_generated::__core::__private::safe_request_plan(
                        &client.__concord_core,
                        self.description(),
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

            #scope_tree

            impl super::#client_name {
                #client_constructor

                #[must_use]
                /// Creates the canonical generated client builder.
                pub fn builder() -> super::#builder_name {
                    super::#builder_name::new()
                }

                #client_child_methods
                #credentials_method

                #[must_use]
                /// Creates a side-effect-free pending request from an explicit endpoint generated
                /// for this client or a stable runtime endpoint.
                pub fn request<E>(&self, endpoint: E) -> E::Request
                where
                    E: Endpoint,
                {
                    <E as Endpoint>::__concord_bind(endpoint, self)
                }

                /// Builds one sanitized no-effect plan by borrowing an endpoint value.
                pub fn plan<E>(
                    &self,
                    endpoint: &E,
                ) -> ::std::result::Result<
                    #core_path::endpoint::SafeRequestPlan,
                    #core_path::ApiError,
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
                        __concord_http: ::std::option::Option::Some(#retry_baseline),
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
                        #core_path::http::HttpConfigBuilder,
                    ) -> #core_path::http::HttpConfigBuilder,
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
                        #core_path::runtime::RuntimeConfigBuilder,
                    ) -> #core_path::runtime::RuntimeConfigBuilder,
                {
                    let Some(runtime) = self.__concord_runtime.take() else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    self.__concord_runtime = ::std::option::Option::Some(configure(runtime));
                    self
                }

                /// Resolves selected defaults once and builds the immutable generated client snapshot.
                pub fn build(
                    self,
                ) -> ::std::result::Result<super::#client_name, #core_path::ClientBuildError> {
                    #(#build_value_statements)*
                    #build_secret_statements
                    #build_provider_statements
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

pub(super) fn generate_builder_value_methods(index: usize, value: &Value) -> TokenStream {
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
        ResolvedValueKind::Required => format!(
            "Sets the required client value `{}`; a later call replaces this selection.",
            canonical_ident(&value.name)
        ),
        ResolvedValueKind::Optional => format!(
            "Selects `Some(value)` for optional client value `{}`; a later call replaces this selection.",
            canonical_ident(&value.name)
        ),
        ResolvedValueKind::Defaulted(_) => format!(
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

pub(super) fn generate_client_value_resolution(index: usize, value: &Value) -> TokenStream {
    let builder_field = builder_value_field(index, value);
    let resolved = resolved_client_value(index, value);
    let method = &value.name;
    let ty = &value.ty;
    match &value.kind {
        ResolvedValueKind::Required => quote! {
            let #resolved: #ty = match self.#builder_field {
                ::std::option::Option::Some(value) => value,
                ::std::option::Option::None => {
                    return ::std::result::Result::Err(
                        __concord_generated::__core::__private::missing_input_error(
                            ::std::stringify!(#method),
                            __concord_generated::__core::__private::ClientInputKind::Value,
                        ),
                    );
                }
            };
        },
        ResolvedValueKind::Optional => quote! {
            let #resolved: #ty = self.#builder_field;
        },
        ResolvedValueKind::Defaulted(default) => {
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

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_client_builder_and_sealed_endpoint_contract() {
        let api = test_support::api();
        let output = generate_client(
            &api.client,
            &test_support::core_path(),
            Capabilities::default(),
        )
        .to_string();

        assert!(output.contains("pub struct ArtifactApi"));
        assert!(output.contains("pub struct ArtifactApiBuilder"));
        assert!(output.contains("pub fn builder"));
        assert!(output.contains("pub trait Endpoint"));
    }
}
