use super::*;

pub(super) struct RequestBridge<'a> {
    pub(super) support_path: &'a TokenStream,
    pub(super) generated_core: &'a TokenStream,
}

pub(super) fn generate_request_wrapper(
    client: &Client,
    endpoint: &Endpoint,
    bridge: RequestBridge<'_>,
    core_path: &TokenStream,
    where_clause: &TokenStream,
) -> TokenStream {
    let RequestBridge {
        support_path,
        generated_core,
    } = bridge;
    let endpoint_name = &endpoint.surface.endpoint_type_name;
    let request_name = &endpoint.surface.request_type_name;
    let setter_methods = endpoint
        .surface
        .setters
        .iter()
        .map(|setter| {
            let value = &endpoint.parameters[setter.parameter];
            request_wrapper_setters(
                value,
                &setter.setter,
                setter.clear.as_ref(),
                setter.reset.as_ref(),
                generated_core,
            )
        })
        .collect::<Vec<_>>();
    let storage_methods = endpoint
        .surface
        .terminals
        .credential_storage
        .iter()
        .map(|terminal| {
            let credential = &client.credentials[terminal.credential];
            let method = &terminal.method;
            let field = credential_store_field(terminal.credential, &credential.name);
            let identity = endpoint_identity(endpoint);
            let native_method = method_token(&endpoint.method, core_path);
            quote! {
                /// Executes this endpoint and atomically stores its decoded output as the
                /// endpoint-backed credential selected by the declaration.
                pub async fn #method(
                    self,
                ) -> ::std::result::Result<(), #core_path::ApiError> {
                    let __concord_credential = #generated_core::__private::inspect_endpoint(
                        &self.__concord_inner,
                        |endpoint| {
                            endpoint
                                .__concord_credentials
                                .as_ref()
                                .expect("generated endpoint is bound by the sealed client request path")
                                .#field
                                .clone()
                        },
                    );
                    let value = self.__concord_inner.await?;
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
    let pagination_method = endpoint.surface.terminals.pagination.then(|| {
        quote! {
            /// Consumes this request and creates one directly awaitable bounded sequential pagination operation.
            #[must_use]
            pub fn paginate<L>(
                self,
                limit: L,
            ) -> #core_path::pagination::PaginatedRequest<#endpoint_name>
            where
                L: ::std::convert::Into<#core_path::PageLimit>,
            {
                #generated_core::__private::paginate_request(self.__concord_inner, limit)
            }
        }
    });
    let response_method = endpoint.surface.terminals.response.then(|| {
        quote! {
            /// Selects universal successful response metadata while retaining the declared output.
            #[must_use]
            pub fn response(self) -> #core_path::endpoint::ResponseRequest<#endpoint_name> {
                #generated_core::__private::response_request(self.__concord_inner)
            }
        }
    });
    let response_bytes_method = quote! {
        /// Selects complete bounded bytes while preserving declared status
        /// acceptance and `Accept` and skipping validation and decoding.
        #[must_use]
        pub fn response_bytes<Mode>(
            self,
        ) -> #core_path::endpoint::ResponseBytesRequest<#endpoint_name>
        where
            <#endpoint_name as #support_path::Endpoint>::Response:
                #generated_core::response::ResponseBodySpec<Mode = Mode>,
            Mode: #generated_core::response::ResponseBytesMode,
        {
            #generated_core::__private::response_bytes_request(self.__concord_inner)
        }
    };
    let storage_impl = (!storage_methods.is_empty()).then(|| {
        quote! {
            impl #request_name
            #where_clause
            {
                #(#storage_methods)*
            }
        }
    });
    let request_doc = request_wrapper_doc(endpoint);
    let debug_name = canonical_ident(request_name);

    quote! {
        #[doc = #request_doc]
        #[must_use = "request construction is inert; await it or select a terminal"]
        pub struct #request_name {
            __concord_inner: #core_path::endpoint::Request<#endpoint_name>,
        }

        impl #request_name {
            /// Replaces the generated operation deadline with one explicit duration.
            #[must_use]
            pub fn timeout(mut self, duration: ::std::time::Duration) -> Self {
                self.__concord_inner = self.__concord_inner.timeout(duration);
                self
            }

            /// Disables the generated operation deadline for this request.
            #[must_use]
            pub fn disable_timeout(mut self) -> Self {
                self.__concord_inner = self.__concord_inner.disable_timeout();
                self
            }

            /// Restores the endpoint's inherited deadline selection.
            #[must_use]
            pub fn inherit_timeout(mut self) -> Self {
                self.__concord_inner = self.__concord_inner.inherit_timeout();
                self
            }

            /// Replaces the built-in body-free debug level for this request.
            #[must_use]
            pub fn debug(
                mut self,
                level: #generated_core::__private::DebugLevel,
            ) -> Self {
                self.__concord_inner = self.__concord_inner.debug(level);
                self
            }

            #(#setter_methods)*
            #pagination_method
            #response_method
            #response_bytes_method
        }

        #storage_impl

        #[doc(hidden)]
        impl ::std::future::IntoFuture for #request_name
        #where_clause
        {
            type Output =
                <#core_path::endpoint::Request<#endpoint_name> as ::std::future::IntoFuture>::Output;
            type IntoFuture =
                <#core_path::endpoint::Request<#endpoint_name> as ::std::future::IntoFuture>::IntoFuture;

            fn into_future(self) -> Self::IntoFuture {
                ::std::future::IntoFuture::into_future(self.__concord_inner)
            }
        }

        impl ::std::fmt::Debug for #request_name {
            fn fmt(
                &self,
                formatter: &mut ::std::fmt::Formatter<'_>,
            ) -> ::std::fmt::Result {
                formatter.debug_struct(#debug_name).finish_non_exhaustive()
            }
        }
    }
}

pub(super) fn request_wrapper_setters(
    value: &Value,
    setter: &Ident,
    clear: Option<&Ident>,
    reset: Option<&Ident>,
    generated_core: &TokenStream,
) -> TokenStream {
    let ty = value.setter_argument_type();
    let setter_doc = if value.accepts_absence() && value.has_default() {
        format!(
            "Consumes this request and selects a present value for defaulted absence-capable endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    } else if value.accepts_absence() {
        format!(
            "Consumes this request and selects a present value for absence-capable endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    } else {
        format!(
            "Consumes this request and overrides defaulted endpoint value `{}`.",
            canonical_ident(&value.name)
        )
    };
    let clear_method = clear.map(|clear| {
        let clear_doc = format!(
            "Consumes this request and explicitly selects absence for absence-capable endpoint value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.__concord_inner = #generated_core::__private::map_endpoint(
                    self.__concord_inner,
                    |endpoint| endpoint.#clear(),
                );
                self
            }
        }
    });
    let reset_method = reset.map(|reset| {
        let reset_doc = format!(
            "Consumes this request and restores the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.__concord_inner = #generated_core::__private::map_endpoint(
                    self.__concord_inner,
                    |endpoint| endpoint.#reset(),
                );
                self
            }
        }
    });
    quote! {
        #[doc = #setter_doc]
        #[must_use]
        pub fn #setter(mut self, value: #ty) -> Self {
            self.__concord_inner = #generated_core::__private::map_endpoint(
                self.__concord_inner,
                |endpoint| endpoint.#setter(value),
            );
            self
        }

        #clear_method
        #reset_method
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_named_request_with_inherent_setters_and_terminal() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let core = test_support::core_path();
        let output = generate_request_wrapper(
            &api.client,
            endpoint,
            RequestBridge {
                support_path: &quote!(super),
                generated_core: &core,
            },
            &core,
            &quote!(),
        )
        .to_string();

        assert!(output.contains("pub struct FetchItemRequest"));
        assert!(output.contains("pub fn set_verbose"));
        assert!(output.contains(
            "Consumes this request and selects a present value for absence-capable endpoint value"
        ));
        assert!(output.contains(
            "Consumes this request and explicitly selects absence for absence-capable endpoint value"
        ));
        assert!(!output.contains("selects `Some(value)`"));
        assert!(output.contains("IntoFuture"));
        assert!(!output.contains("__concord_into_development_request"));
        assert!(!output.contains("emit_generated_development_method"));
        assert!(output.contains("pub fn response_bytes"));
        assert!(output.contains("ResponseBytesRequest"));
        assert!(output.contains("ResponseBytesMode"));
        assert!(output.contains("__private :: response_bytes_request"));
    }
}
