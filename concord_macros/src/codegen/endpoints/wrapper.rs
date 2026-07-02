fn emit_client_wrapper(
    resolved_api: &ResolvedApi,
    facade_ir: &FacadeIr,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    use quote::quote;

    let client_ty = &resolved_api.client_name;

    // same "required vars" as Vars::new(...)
    let required: Vec<&VarInfo> = resolved_api
        .client_vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();

    let new_args: Vec<TokenStream2> = required
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();

    let new_pass: Vec<TokenStream2> = required
        .iter()
        .map(|v| {
            let f = &v.rust;
            quote! { #f }
        })
        .collect();
    let new_pass = new_pass.as_slice();

    let required_auth: Vec<&VarInfo> = resolved_api
        .client_auth_vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
    let new_auth_args: Vec<TokenStream2> = required_auth
        .iter()
        .map(|v| {
            let f = &v.rust;
            let ty = &v.ty;
            quote! { #f: #ty }
        })
        .collect();
    let new_auth_pass: Vec<TokenStream2> = required_auth
        .iter()
        .map(|v| {
            let f = &v.rust;
            quote! { #f }
        })
        .collect();

    let mut ctor_args: Vec<TokenStream2> = Vec::new();
    ctor_args.extend(new_args.iter().cloned());
    ctor_args.extend(new_auth_args.iter().cloned());
    let builder_ty = client_prefixed_ident(client_ty, "Builder");
    let builder_var_fields = required.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        quote! { #f: ::core::option::Option<#ty> }
    });
    let builder_auth_fields = required_auth.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        quote! { #f: ::core::option::Option<#ty> }
    });
    let builder_var_defaults = required.iter().map(|v| {
        let f = &v.rust;
        quote! { #f: ::core::option::Option::None }
    });
    let builder_auth_defaults = required_auth.iter().map(|v| {
        let f = &v.rust;
        quote! { #f: ::core::option::Option::None }
    });
    let builder_var_setters = required.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        quote! {
            #[doc = "Set this required client variable."]
            #[inline]
            pub fn #f(mut self, value: #ty) -> Self {
                self.#f = ::core::option::Option::Some(value);
                self
            }
        }
    });
    let builder_auth_setters = required_auth.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        quote! {
            #[doc = "Set this required client secret."]
            #[inline]
            pub fn #f(mut self, value: #ty) -> Self {
                self.#f = ::core::option::Option::Some(value);
                self
            }
        }
    });
    let builder_var_unwraps = required.iter().map(|v| {
        let f = &v.rust;
        let label = LitStr::new(&format!("builder.{f}"), f.span());
        quote! {
            let #f = self.#f.ok_or_else(|| {
                ::concord_core::prelude::ApiClientError::invalid_param(__ctx.clone(), #label)
            })?;
        }
    });
    let builder_auth_unwraps = required_auth.iter().map(|v| {
        let f = &v.rust;
        let label = LitStr::new(&format!("builder.{f}"), f.span());
        quote! {
            let #f = self.#f.ok_or_else(|| {
                ::concord_core::prelude::ApiClientError::invalid_param(__ctx.clone(), #label)
            })?;
        }
    });
    let builder_var_args = required.iter().map(|v| {
        let f = &v.rust;
        quote! { #f }
    });
    let builder_auth_args = required_auth.iter().map(|v| {
        let f = &v.rust;
        quote! { #f }
    });

    let var_setters = facade_ir.client_setters.iter().map(|setter| {
        let Some(v) = resolved_api
            .client_vars
            .iter()
            .find(|var| var.rust == setter.field)
        else {
            return emit_helpers::compile_error_tokens(
                "FacadeIr client setter must target a resolved client var",
                Span::call_site(),
            );
        };
        let f = &v.rust;
        let ty = &v.ty;
        let set_name = setter.set_name.clone();
        if v.optional {
            let clear_name = setter.clear_name.clone();
            quote! {
                #[inline]
                pub fn #set_name(&mut self, v: #ty) -> &mut Self {
                    self.inner.vars_mut().#f = ::core::option::Option::Some(v);
                    self
                }
                #[inline]
                pub fn #clear_name(&mut self) -> &mut Self {
                    self.inner.vars_mut().#f = ::core::option::Option::None;
                    self
                }
            }
        } else {
            quote! {
                #[inline]
                pub fn #set_name(&mut self, v: #ty) -> &mut Self {
                    self.inner.vars_mut().#f = v;
                    self
                }
            }
        }
    });

    let (credential_secret_names, has_custom_credentials) = auth_credential_secret_names(resolved_api);
    let preserve_manual_slots = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        if matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
            let name = &credential.name;
            Some(quote! {
                __new_auth_state.#name = __old_auth_state.#name.clone();
            })
        } else {
            None
        }
    });
    let rebuild_auth_state_method = if resolved_api.client_auth_credentials.is_empty() {
        quote! {}
    } else {
        quote! {
            #[inline]
            fn __concord_rebuild_auth_state_preserving_manual(
                &mut self,
            ) -> ::core::result::Result<(), ::concord_core::advanced::AuthError> {
                let __old_auth_state = self.inner.try_auth_state()?;
                let mut __new_auth_state =
                    <#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(
                        self.inner.vars(),
                        self.inner.auth_vars(),
                    );
                #( #preserve_manual_slots )*
                self.inner.try_set_auth_state(__new_auth_state)
            }
        }
    };
    let configure_rate_limiter = if let Some(policy) = &resolved_api.rate_limit_response_policy {
        quote! {
            __inner.set_rate_limiter(::std::sync::Arc::new(
                ::concord_core::advanced::GovernorRateLimiter::new()
                    .with_response_policy(::std::sync::Arc::new(#policy::default()))
            ));
        }
    } else {
        quote! {}
    };
    let auth_setters = facade_ir.auth_setters.iter().map(|setter| {
        let Some(v) = resolved_api
            .client_auth_vars
            .iter()
            .find(|var| var.rust == setter.field)
        else {
            return emit_helpers::compile_error_tokens(
                "FacadeIr auth setter must target a resolved auth var",
                Span::call_site(),
            );
        };
        let f = &v.rust;
        let set_name = setter.set_name.clone();
        let rebuild_auth_state =
            has_custom_credentials || credential_secret_names.contains(&f.to_string());
        if v.optional {
            let clear_name = setter.clear_name.clone();
            if rebuild_auth_state {
                quote! {
                    #[inline]
                    pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> ::core::result::Result<&mut Self, ::concord_core::advanced::AuthError> {
                        {
                            let mut __g = ::concord_core::advanced::write_auth_lock(
                                self.inner.auth_vars(),
                                "auth vars lock poisoned",
                            )?;
                            __g.#f = ::core::option::Option::Some(v.into());
                        }
                        self.__concord_rebuild_auth_state_preserving_manual()?;
                        ::core::result::Result::Ok(self)
                    }
                    #[inline]
                    pub fn #clear_name(&mut self) -> ::core::result::Result<&mut Self, ::concord_core::advanced::AuthError> {
                        {
                            let mut __g = ::concord_core::advanced::write_auth_lock(
                                self.inner.auth_vars(),
                                "auth vars lock poisoned",
                            )?;
                            __g.#f = ::core::option::Option::None;
                        }
                        self.__concord_rebuild_auth_state_preserving_manual()?;
                        ::core::result::Result::Ok(self)
                    }
                }
            } else {
                quote! {
                    #[inline]
                    pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> ::core::result::Result<&Self, ::concord_core::advanced::AuthError> {
                        let mut __g = ::concord_core::advanced::write_auth_lock(
                            self.inner.auth_vars(),
                            "auth vars lock poisoned",
                        )?;
                        __g.#f = ::core::option::Option::Some(v.into());
                        ::core::result::Result::Ok(self)
                    }
                    #[inline]
                    pub fn #clear_name(&self) -> ::core::result::Result<&Self, ::concord_core::advanced::AuthError> {
                        let mut __g = ::concord_core::advanced::write_auth_lock(
                            self.inner.auth_vars(),
                            "auth vars lock poisoned",
                        )?;
                        __g.#f = ::core::option::Option::None;
                        ::core::result::Result::Ok(self)
                    }
                }
            }
        } else {
            if rebuild_auth_state {
                quote! {
                    #[inline]
                    pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> ::core::result::Result<&mut Self, ::concord_core::advanced::AuthError> {
                        {
                            let mut __g = ::concord_core::advanced::write_auth_lock(
                                self.inner.auth_vars(),
                                "auth vars lock poisoned",
                            )?;
                            __g.#f = v.into();
                        }
                        self.__concord_rebuild_auth_state_preserving_manual()?;
                        ::core::result::Result::Ok(self)
                    }
                }
            } else {
                quote! {
                    #[inline]
                    pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> ::core::result::Result<&Self, ::concord_core::advanced::AuthError> {
                        let mut __g = ::concord_core::advanced::write_auth_lock(
                            self.inner.auth_vars(),
                            "auth vars lock poisoned",
                        )?;
                        __g.#f = v.into();
                        ::core::result::Result::Ok(self)
                    }
                }
            }
        }
    });
    let credential_lifecycle_methods = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        let name = &credential.name;
        let AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            .. 
        } = &credential.kind
        else {
            return None;
        };
        let Some(methods) = facade_credential_methods_for(facade_ir, name) else {
            return Some(emit_helpers::compile_error_tokens(
                "FacadeIr must contain one public method set per endpoint-backed credential",
                name.span(),
            ));
        };
        let endpoint_type_path = endpoint_type_path(target);
        let acquire_name = methods.acquire_name.clone();
        let set_name = methods.set_name.clone();
        let clear_name = methods.clear_name.clone();
        let has_name = methods.has_name.clone();
        Some(quote! {
            #[inline]
            pub async fn #acquire_name(
                &self,
                ep: #endpoint_type_path,
            ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                self.request(ep)
                    .execute_and_store_manual(|__auth_state| __auth_state.#name.as_ref())
                    .await
            }

            #[inline]
            pub async fn #set_name(
                &self,
                value: #output_ty,
            ) -> ::core::result::Result<(), ::concord_core::advanced::AuthError> {
                let __auth_state = self.inner.try_auth_state()?;
                __auth_state.#name.set_manual(value).await
            }

            #[inline]
            pub async fn #clear_name(&self) -> ::core::result::Result<(), ::concord_core::advanced::AuthError> {
                let __auth_state = self.inner.try_auth_state()?;
                __auth_state.#name.clear_manual().await
            }

            #[inline]
            pub async fn #has_name(&self) -> ::core::result::Result<bool, ::concord_core::advanced::AuthError> {
                let __auth_state = self.inner.try_auth_state()?;
                ::core::result::Result::Ok(__auth_state.#name.has_value().await)
            }
        })
    });
    let credential_pending_methods = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        let name = &credential.name;
        let AuthCredentialKindIr::Endpoint { target, .. } = &credential.kind else {
            return None;
        };
        let Some(methods) = facade_credential_methods_for(facade_ir, name) else {
            return Some(emit_helpers::compile_error_tokens(
                "FacadeIr must contain one public method set per endpoint-backed credential",
                name.span(),
            ));
        };
        let method = methods.pending_method.clone();
        let trait_name = acquire_as_trait_ident(client_ty, name);
        let endpoint_type_path = endpoint_type_path(target);
        Some(quote! {
            pub trait #trait_name<'a> {
                #[doc = "Execute this request and store its response as the endpoint-backed credential."]
                fn #method(
                    self,
                ) -> ::core::pin::Pin<::std::boxed::Box<
                    dyn ::core::future::Future<
                            Output = ::core::result::Result<(), ::concord_core::prelude::ApiClientError>,
                        > + Send + 'a,
                >>;
            }

            impl<'a, T> #trait_name<'a>
                for ::concord_core::prelude::PendingRequest<'a, #cx_ty, #endpoint_type_path, T>
            where
                T: ::concord_core::advanced::Transport + 'a,
            {
                #[inline]
                fn #method(
                    self,
                ) -> ::core::pin::Pin<::std::boxed::Box<
                    dyn ::core::future::Future<
                            Output = ::core::result::Result<(), ::concord_core::prelude::ApiClientError>,
                        > + Send + 'a,
                >> {
                    ::std::boxed::Box::pin(async move {
                        self.execute_and_store_manual(|auth_state| auth_state.#name.as_ref()).await
                    })
                }
            }
        })
    });
    let (auth_facade_methods, auth_facade_items) = emit_auth_facade(resolved_api, client_ty);
    let (facade_methods, facade_items) = emit_tree_facade(resolved_api, facade_ir, client_ty, cx_ty);

    quote! {
        #[doc = "Generated API client."]
        #[derive(Clone)]
        pub struct #client_ty<T: ::concord_core::advanced::Transport = ::concord_core::advanced::ReqwestTransport> {
            inner: ::concord_core::prelude::ApiClient<#cx_ty, T>,
        }
        impl #client_ty<::concord_core::advanced::ReqwestTransport> {
            #[doc = "Create a client with the default reqwest transport."]
            #[inline]
            pub fn new( #( #ctor_args ),* ) -> Self {
               let vars = #vars_ty::new( #( #new_pass ),* );
                let auth_vars = #auth_vars_ty::new( #( #new_auth_pass ),* );
                let mut __inner = ::concord_core::prelude::ApiClient::<#cx_ty, ::concord_core::advanced::ReqwestTransport>::new(vars, auth_vars);
                #configure_rate_limiter
                Self { inner: __inner }
            }


            #[doc = "Create a client with a custom transport."]
            #[inline]
            pub fn new_with_transport<T2: ::concord_core::advanced::Transport>(
                #( #ctor_args, )*
                transport: T2
            ) -> #client_ty<T2> {
                let vars = #vars_ty::new( #( #new_pass ),* );
                let auth_vars = #auth_vars_ty::new( #( #new_auth_pass ),* );
                let mut __inner = ::concord_core::prelude::ApiClient::<#cx_ty, T2>::with_transport(vars, auth_vars, transport);
                #configure_rate_limiter
                #client_ty { inner: __inner }
            }

            #[doc = "Create a builder for required client configuration."]
            #[inline]
            pub fn builder() -> #builder_ty {
                #builder_ty::new()
            }


        }

        #[doc = "Builder for required client configuration."]
        pub struct #builder_ty {
            #( #builder_var_fields, )*
            #( #builder_auth_fields, )*
        }

        impl #builder_ty {
            #[doc = "Create an empty client builder."]
            #[inline]
            pub fn new() -> Self {
                Self {
                    #( #builder_var_defaults, )*
                    #( #builder_auth_defaults, )*
                }
            }

            #( #builder_var_setters )*
            #( #builder_auth_setters )*

            #[doc = "Build the generated client."]
            #[inline]
            pub fn build(self) -> ::core::result::Result<#client_ty<::concord_core::advanced::ReqwestTransport>, ::concord_core::prelude::ApiClientError> {
                let __ctx = ::concord_core::error::ErrorContext {
                    endpoint: concat!(stringify!(#client_ty), "::builder"),
                    method: ::http::Method::GET,
                };
                #( #builder_var_unwraps )*
                #( #builder_auth_unwraps )*
                ::core::result::Result::Ok(#client_ty::new( #( #builder_var_args, )* #( #builder_auth_args ),* ))
            }
        }

        impl<T: ::concord_core::advanced::Transport> #client_ty<T> {
            #rebuild_auth_state_method
            #( #var_setters )*
            #( #auth_setters )*
            #( #credential_lifecycle_methods )*
            #auth_facade_methods

            #[doc = "Return the current debug level."]
            #[inline]
            pub fn debug_level(&self) -> ::concord_core::prelude::DebugLevel { self.inner.debug_level() }
            #[doc = "Set the debug level in place."]
            #[inline]
            pub fn set_debug_level(&mut self, level: ::concord_core::prelude::DebugLevel) { self.inner.set_debug_level(level); }
            #[doc = "Return this client with a changed debug level."]
            #[inline]
            pub fn with_debug_level(mut self, level: ::concord_core::prelude::DebugLevel) -> Self { self.inner.set_debug_level(level); self }
            #[doc = "Return whether pagination loop detection is enabled by default."]
            #[inline]
            pub fn pagination_detect_loops(&self) -> bool { self.inner.pagination_detect_loops() }
            #[doc = "Set whether pagination loop detection is enabled by default."]
            #[inline]
            pub fn set_pagination_detect_loops(&mut self, enabled: bool) { self.inner.set_pagination_detect_loops(enabled); }
            #[doc = "Return this client with changed pagination loop detection default."]
            #[inline]
            pub fn with_pagination_detect_loops(mut self, enabled: bool) -> Self { self.inner.set_pagination_detect_loops(enabled); self }
            #[doc = "Mutate advanced runtime configuration and return this client."]
            #[inline]
            pub fn configure(mut self, f: impl FnOnce(&mut ::concord_core::advanced::RuntimeConfig)) -> Self { self.inner.configure(f); self }
            #[doc = "Mutate advanced runtime configuration in place."]
            #[inline]
            pub fn configure_mut(&mut self, f: impl FnOnce(&mut ::concord_core::advanced::RuntimeConfig)) -> &mut Self { self.inner.configure(f); self }
            #[doc = "Create a pending request from an explicit endpoint value."]
            #[inline]
            pub fn request<E>(&self, ep: E) -> ::concord_core::prelude::PendingRequest<'_, #cx_ty, E, T>
            where
                E: ::concord_core::prelude::Endpoint<#cx_ty>,
            {
                self.inner.request(ep)
            }

            #facade_methods
        }

        #auth_facade_items
        #facade_items
        #( #credential_pending_methods )*
    }
}

fn endpoint_type_path(target: &EndpointTargetIr) -> TokenStream2 {
    let scope_modules = &target.scope_modules;
    let endpoint = &target.endpoint;
    quote! {
        endpoints:: #( #scope_modules :: )* #endpoint
    }
}

fn emit_auth_facade(resolved_api: &ResolvedApi, client_ty: &Ident) -> (TokenStream2, TokenStream2) {
    let root_auth_scope_exists = resolved_api
        .endpoints
        .iter()
        .any(|ep| ep.scope_modules.first().is_some_and(|scope| scope == "auth"));
    let auth_ty = emit_helpers::ident(&format!("{}Auth", client_ty), client_ty.span());
    let handle_items = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        let AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            .. 
        } = &credential.kind
        else {
            return None;
        };
        let name = &credential.name;
        let endpoint_name_lit = LitStr::new(&target.display_string(), target.endpoint.span());
        let handle_ty = emit_helpers::ident(
            &crate::model::facade::generated_auth_handle_type_name(client_ty, name),
            name.span(),
        );
        Some(quote! {
            pub struct #handle_ty<'a, T: ::concord_core::advanced::Transport = ::concord_core::advanced::ReqwestTransport> {
                client: &'a #client_ty<T>,
            }

            impl<'a, T: ::concord_core::advanced::Transport> #handle_ty<'a, T> {
                #[inline]
                pub async fn acquire<R>(
                    &self,
                    request: R,
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
                where
                    R: ::core::future::IntoFuture<Output = ::core::result::Result<#output_ty, ::concord_core::prelude::ApiClientError>>,
                {
                    let value: #output_ty = request.await?;
                    let __auth_state = self.client.inner.try_auth_state().map_err(|source| {
                        ::concord_core::prelude::ApiClientError::Auth {
                            ctx: ::concord_core::advanced::ErrorContext {
                                endpoint: #endpoint_name_lit,
                                method: ::http::Method::GET,
                            },
                            source,
                        }
                    })?;
                    __auth_state.#name.set_manual(value).await.map_err(|source| {
                        ::concord_core::prelude::ApiClientError::Auth {
                            ctx: ::concord_core::advanced::ErrorContext {
                                endpoint: #endpoint_name_lit,
                                method: ::http::Method::GET,
                            },
                            source,
                        }
                    })
                }

                #[inline]
                pub async fn set(
                    &self,
                    value: #output_ty,
                ) -> ::core::result::Result<(), ::concord_core::advanced::AuthError> {
                    let __auth_state = self.client.inner.try_auth_state()?;
                    __auth_state.#name.set_manual(value).await
                }

                #[inline]
                pub async fn clear(&self) -> ::core::result::Result<(), ::concord_core::advanced::AuthError> {
                    let __auth_state = self.client.inner.try_auth_state()?;
                    __auth_state.#name.clear_manual().await
                }

                #[inline]
                pub async fn is_set(&self) -> ::core::result::Result<bool, ::concord_core::advanced::AuthError> {
                    let __auth_state = self.client.inner.try_auth_state()?;
                    ::core::result::Result::Ok(__auth_state.#name.has_value().await)
                }
            }
        })
    });
    let auth_methods = emit_auth_accessor_methods(resolved_api, client_ty);

    let auth_method = if root_auth_scope_exists {
        quote! {}
    } else {
        quote! {
        #[inline]
        pub fn auth(&self) -> #auth_ty<'_, T> {
            #auth_ty { client: self }
        }
        }
    };
    let methods = quote! {
        #auth_method
        #[inline]
        pub fn auth_state(&self) -> #auth_ty<'_, T> {
            #auth_ty { client: self }
        }
    };
    let auth_state_item = quote! {
        pub struct #auth_ty<'a, T: ::concord_core::advanced::Transport = ::concord_core::advanced::ReqwestTransport> {
            client: &'a #client_ty<T>,
        }

        impl<'a, T: ::concord_core::advanced::Transport> #auth_ty<'a, T> {
            #auth_methods
        }
    };

    let items = quote! {
        #auth_state_item
        #( #handle_items )*
    };
    (methods, items)
}

fn emit_auth_accessor_methods(resolved_api: &ResolvedApi, client_ty: &Ident) -> TokenStream2 {
    let methods = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        if !matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
            return None;
        }
        let name = &credential.name;
        let handle_ty = emit_helpers::ident(
            &crate::model::facade::generated_auth_handle_type_name(client_ty, name),
            name.span(),
        );
        Some(quote! {
            #[inline]
            pub fn #name(&self) -> #handle_ty<'a, T> {
                #handle_ty { client: self.client }
            }
        })
    });
    quote! { #( #methods )* }
}

fn required_vars(vars: &[VarInfo]) -> Vec<&VarInfo> {
    vars.iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect()
}

fn emit_tree_facade(
    resolved_api: &ResolvedApi,
    facade_ir: &FacadeIr,
    client_ty: &Ident,
    cx_ty: &Ident,
) -> (TokenStream2, TokenStream2) {
    let root_scope_methods = facade_ir
        .scopes
        .iter()
        .filter(|scope| scope.path.len() == 1)
        .map(|scope| emit_scope_ctor_method(scope, None, None));
    let root_endpoint_methods = resolved_api
        .endpoints
        .iter()
        .filter(|ep| ep.scope_modules.is_empty())
        .map(|ep| {
            let Some(facade) = facade_ir_for_endpoint(facade_ir, ep) else {
                return emit_helpers::compile_error_tokens(
                    "FacadeIr must contain one public endpoint entry per resolved endpoint",
                    ep.name.span(),
                );
            };
            emit_facade_endpoint_method(facade, client_ty, cx_ty, true)
        });
    let scope_structs = facade_ir
        .scopes
        .iter()
        .map(|scope| {
            emit_facade_scope_struct(resolved_api, facade_ir, client_ty, cx_ty, scope)
        });

    let methods = quote! {
        #( #root_scope_methods )*
        #( #root_endpoint_methods )*
    };
    let items = quote! {
        #( #scope_structs )*
    };
    (methods, items)
}

fn emit_scope_ctor_method(
    scope_ir: &FacadeScope,
    parent_scope: Option<&FacadeScope>,
    method_ir: Option<&FacadeMethod>,
) -> TokenStream2 {
    let method_name = method_ir.map_or(&scope_ir.public_method, |method| &method.public_name);
    let span = method_name.span();
    let method = method_name.clone();
    let struct_name = scope_ir.rust_type_name.clone();
    let docs = method_ir.map_or(&scope_ir.constructor_docs, |method| &method.docs);
    let docs = facade_docs_to_lit(docs, span);
    let parent_decl_count = parent_scope.map_or(0, |s| s.decls.len());
    let new_decls = &scope_ir.decls[parent_decl_count..];
    let args = required_vars(new_decls).into_iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        quote! { #name: #ty }
    });

    let parent_fields = parent_scope.into_iter().flat_map(|parent| parent.decls.iter()).map(|v| {
        let name = &v.rust;
        quote! { #name: self.#name }
    });
    let new_fields = new_decls.iter().map(|v| {
        let name = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #name }
        } else if v.optional {
            if let Some(default) = &v.default {
                quote! { #name: ::core::option::Option::Some(#default) }
            } else {
                quote! { #name: ::core::option::Option::None }
            }
        } else {
            match &v.default {
                Some(default) => quote! { #name: #default },
                None => {
                    let err = emit_helpers::compile_error_expr(
                        "required scope parameter default was missing in resolved IR",
                        name.span(),
                    );
                    quote! { #name: #err }
                }
            }
        }
    });

    let receiver = if parent_scope.is_some() {
        quote! { self }
    } else {
        quote! { &self }
    };
    let client_expr = if parent_scope.is_some() {
        quote! { self.client }
    } else {
        quote! { self }
    };
    let lifetime = if parent_scope.is_some() {
        quote! { 'a }
    } else {
        quote! { '_ }
    };

    quote! {
        #( #[doc = #docs] )*
        #[inline]
        pub fn #method(#receiver, #( #args ),*) -> #struct_name<#lifetime, T> {
            #struct_name {
                client: #client_expr,
                #( #parent_fields, )*
                #( #new_fields, )*
            }
        }
    }
}

fn emit_facade_scope_struct(
    resolved_api: &ResolvedApi,
    facade_ir: &FacadeIr,
    client_ty: &Ident,
    cx_ty: &Ident,
    scope_ir: &FacadeScope,
) -> TokenStream2 {
    let span = scope_ir.rust_type_name.span();
    let struct_name = scope_ir.rust_type_name.clone();
    let docs = facade_docs_to_lit(&scope_ir.docs, span);
    let fields = scope_ir.decls.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            quote! { #name: ::core::option::Option<#ty> }
        } else {
            quote! { #name: #ty }
        }
    });
    let setters = scope_ir.setters.iter().filter_map(|setter| {
        let var = scope_ir
            .decls
            .iter()
            .find(|var| var.rust == setter.field)?;
        Some(emit_scope_setter(setter, var))
    });
    let child_methods = scope_ir.methods.iter().map(|method| {
        let Some(child_ir) = facade_scope_ir_for_path(facade_ir, &method.target_scope_path) else {
            return emit_helpers::compile_error_tokens(
                "FacadeIr must contain one scope entry per resolved facade scope",
                Span::call_site(),
            );
        };
        emit_scope_ctor_method(child_ir, Some(scope_ir), Some(method))
    });
    let endpoint_methods = resolved_api
        .endpoints
        .iter()
        .filter_map(|ep| {
            let Some(facade) = facade_ir_for_endpoint(facade_ir, ep) else {
                return Some(emit_helpers::compile_error_tokens(
                    "FacadeIr must contain one public endpoint entry per resolved endpoint",
                    ep.name.span(),
                ));
            };
            if facade.scope_path != scope_ir.path {
                return None;
            }
            Some(emit_facade_endpoint_method(facade, client_ty, cx_ty, false))
        });
    let auth_accessor_methods = if scope_ir.path.len() == 1 && scope_ir.path[0].to_string() == "auth" {
        emit_auth_accessor_methods(resolved_api, client_ty)
    } else {
        quote! {}
    };

    quote! {
        #( #[doc = #docs] )*
        pub struct #struct_name<'a, T: ::concord_core::advanced::Transport = ::concord_core::advanced::ReqwestTransport> {
            client: &'a #client_ty<T>,
            #( #fields, )*
        }

        impl<'a, T: ::concord_core::advanced::Transport> #struct_name<'a, T> {
            #( #setters )*
            #( #child_methods )*
            #( #endpoint_methods )*
            #auth_accessor_methods
        }
    }
}

fn emit_facade_endpoint_method(
    facade: &FacadeEndpoint,
    _client_ty: &Ident,
    cx_ty: &Ident,
    root: bool,
) -> TokenStream2 {
    let endpoint_path = facade_endpoint_path(&facade.target);
    let method = facade.public_method.clone();
    let signature_args = facade.required_args.iter().map(|arg| {
        let name = &arg.name;
        let ty = &arg.ty;
        quote! { #name: #ty }
    });
    let new_args: Vec<TokenStream2> = facade
        .constructor
        .args
        .iter()
        .map(|arg| match arg {
            FacadeConstructorArg::PublicArg { name } => quote! { #name },
            FacadeConstructorArg::CapturedScopeField { name } => quote! { self.#name },
        })
        .collect();
    let captured_setters: Vec<TokenStream2> = facade
        .captured_setters
        .iter()
        .map(|captured| {
            let name = &captured.field;
            if captured.optional {
                quote! {
                    if let ::core::option::Option::Some(value) = self.#name {
                        __ep.#name = ::core::option::Option::Some(value);
                    }
                }
            } else {
                quote! { __ep.#name = self.#name; }
            }
        })
        .collect();
    let self_arg = if root {
        quote! { &self }
    } else {
        quote! { self }
    };
    let client_expr = if root { quote! { self } } else { quote! { __client } };
    let lifetime = if root { quote! { '_ } } else { quote! { 'a } };
    let bind_client = if root { quote! {} } else { quote! { let __client = self.client; } };
    let args: Vec<TokenStream2> = signature_args.collect();
    let docs = facade_ir_endpoint_docs(facade, facade.public_method.span());

    quote! {
        #( #[doc = #docs] )*
        #[inline]
        pub fn #method(#self_arg, #( #args ),*) -> ::concord_core::prelude::PendingRequest<#lifetime, #cx_ty, #endpoint_path, T> {
            #bind_client
            let mut __ep = #endpoint_path::new( #( #new_args ),* );
            #( #captured_setters )*
            #client_expr.request(__ep)
        }
    }
}

fn facade_endpoint_path(target: &FacadeEndpointTarget) -> TokenStream2 {
    let endpoint = &target.endpoint;
    let path = target.scope_path.iter().fold(quote! { endpoints }, |acc, scope| {
        quote! { #acc::#scope }
    });
    quote! { #path::#endpoint }
}

fn emit_scope_setter(setter: &FacadeSetter, var: &VarInfo) -> TokenStream2 {
    let name = setter.set_name.clone();
    let ty = &var.ty;
    if var.optional {
        let clear = setter.clear_name.clone();
        quote! {
            #[inline]
            pub fn #name(mut self, value: #ty) -> Self {
                self.#name = ::core::option::Option::Some(value);
                self
            }
            #[inline]
            pub fn #clear(mut self) -> Self {
                self.#name = ::core::option::Option::None;
                self
            }
        }
    } else {
        quote! {
            #[inline]
            pub fn #name(mut self, value: #ty) -> Self {
                self.#name = value;
                self
            }
        }
    }
}

fn facade_ir_for_endpoint<'a>(facade_ir: &'a FacadeIr, ep: &ResolvedEndpoint) -> Option<&'a FacadeEndpoint> {
    facade_ir
        .endpoints
        .iter()
        .find(|candidate| candidate.target.scope_path == ep.scope_modules && candidate.target.endpoint == ep.name)
}

fn facade_credential_methods_for<'a>(
    facade_ir: &'a FacadeIr,
    name: &Ident,
) -> Option<&'a FacadeCredentialMethods> {
    facade_ir
        .credential_methods
        .iter()
        .find(|methods| name == &methods.credential)
}

fn facade_ir_endpoint_docs(facade: &FacadeEndpoint, span: Span) -> Vec<LitStr> {
    facade_docs_to_lit(&facade.docs, span)
}

fn facade_docs_to_lit(docs: &[FacadeDoc], span: Span) -> Vec<LitStr> {
    docs
        .iter()
        .flat_map(|doc| {
            std::iter::once(doc.summary.as_str()).chain(doc.details.iter().map(String::as_str))
        })
        .map(|line| LitStr::new(line, span))
        .collect()
}

fn facade_scope_ir_for_path<'a>(
    facade_ir: &'a FacadeIr,
    path: &[Ident],
) -> Option<&'a FacadeScope> {
    facade_ir
        .scopes
        .iter()
        .find(|scope| scope.path.as_slice() == path)
}

fn behavior_doc_line(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }

    Some(format!(
        "Behavior: {}",
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn facade_endpoint_docs(ep: &ResolvedEndpoint, client_policy: &PolicyBlocksResolved) -> Vec<LitStr> {
    let mut docs = Vec::new();
    docs.push(LitStr::new(
        &format!("{} {}", ep.method, doc_path(ep)),
        ep.name.span(),
    ));
    let required = ep
        .vars
        .iter()
        .filter(|var| !var.optional && var.default.is_none())
        .map(|var| format!("`{}`", var.rust))
        .collect::<Vec<_>>();
    if !required.is_empty() {
        docs.push(LitStr::new(
            &format!("Required params: {}", required.join(", ")),
            ep.name.span(),
        ));
    }
    let query = doc_policy_keys(ep, client_policy, PolicyKeyKind::Query);
    if !query.is_empty() {
        docs.push(LitStr::new(
            &format!("Query params: {}", query.join(", ")),
            ep.name.span(),
        ));
    }
    let headers = doc_policy_keys(ep, client_policy, PolicyKeyKind::Header);
    if !headers.is_empty() {
        docs.push(LitStr::new(
            &format!("Headers: {}", headers.join(", ")),
            ep.name.span(),
        ));
    }
    if !ep.policy.auth.is_empty() {
        docs.push(LitStr::new("", ep.name.span()));
        docs.push(LitStr::new("Auth:", ep.name.span()));
        for auth in &ep.policy.auth {
            docs.push(LitStr::new(
                &format!("- {}", doc_auth_requirement(auth)),
                ep.name.span(),
            ));
        }
    }
    if endpoint_has_retry(ep, client_policy) {
        docs.push(LitStr::new("Retry: configured", ep.name.span()));
    }
    if endpoint_has_rate_limit(ep, client_policy) {
        docs.push(LitStr::new("Rate limit: configured", ep.name.span()));
    }
    if let Some(line) = behavior_doc_line(&ep.behavior_doc.names) {
        docs.push(LitStr::new(&line, ep.name.span()));
    }
    if let Some(pagination) = &ep.paginate {
        let controller_ty = &pagination.controller_ty;
        docs.push(LitStr::new(
            &format!("Pagination: {}", quote::quote!(#controller_ty)),
            ep.name.span(),
        ));
    }
    match ep.request_io() {
        ResolvedRequestBodyIo::BufferedCodec(io) => docs.push(LitStr::new(
            &format!("Body: {}", doc_codec(&io.codec_path, &io.value_ty)),
            ep.name.span(),
        )),
        ResolvedRequestBodyIo::RawStream { media_ty } => docs.push(LitStr::new(
            &format!("Body: Stream<{}>", quote::quote!(#media_ty)),
            ep.name.span(),
        )),
        ResolvedRequestBodyIo::Records { item_ty, format_ty } => docs.push(LitStr::new(
            &format!(
                "Body: Records<{}, {}>",
                quote::quote!(#item_ty),
                quote::quote!(#format_ty)
            ),
            ep.name.span(),
        )),
        ResolvedRequestBodyIo::Multipart { value_ty, format_ty } => docs.push(LitStr::new(
            &format!(
                "Body: Multipart<{}, {}>",
                quote::quote!(#value_ty),
                quote::quote!(#format_ty)
            ),
            ep.name.span(),
        )),
        ResolvedRequestBodyIo::None => {}
    }
    match ep.response_io() {
        ResolvedResponseBodyIo::BufferedCodec(io) => docs.push(LitStr::new(
            &format!("Response: {}", doc_codec(&io.codec_path, &io.value_ty)),
            ep.name.span(),
        )),
        ResolvedResponseBodyIo::BufferedBytes => docs.push(LitStr::new(
            "Response: bytes::Bytes",
            ep.name.span(),
        )),
        ResolvedResponseBodyIo::NoContent => docs.push(LitStr::new("Response: ()", ep.name.span())),
        ResolvedResponseBodyIo::RawStream { media_ty } => docs.push(LitStr::new(
            &format!("Response: Stream<{}>", quote::quote!(#media_ty)),
            ep.name.span(),
        )),
        ResolvedResponseBodyIo::Records { item_ty, format_ty } => docs.push(LitStr::new(
            &format!(
                "Response: Records<{}, {}>",
                quote::quote!(#item_ty),
                quote::quote!(#format_ty)
            ),
            ep.name.span(),
        )),
        ResolvedResponseBodyIo::Multipart { part_ty, format_ty } => docs.push(LitStr::new(
            &format!(
                "Response: Multipart<{}, {}>",
                quote::quote!(#part_ty),
                quote::quote!(#format_ty)
            ),
            ep.name.span(),
        )),
        ResolvedResponseBodyIo::Sse { event_ty, codec_ty } => docs.push(LitStr::new(
            &format!(
                "Response: Sse<{}, {}>",
                quote::quote!(#event_ty),
                quote::quote!(#codec_ty)
            ),
            ep.name.span(),
        )),
    }
    docs
}

#[allow(dead_code)]
fn doc_policy_keys(
    ep: &ResolvedEndpoint,
    client_policy: &PolicyBlocksResolved,
    kind: PolicyKeyKind,
) -> Vec<String> {
    let mut keys = std::collections::BTreeSet::new();
    let policies = std::iter::once(client_policy)
        .chain(ep.policy.scopes.iter())
        .chain(std::iter::once(&ep.policy.endpoint));
    for policy in policies {
        let ops = match kind {
            PolicyKeyKind::Header => &policy.headers,
            PolicyKeyKind::Query => &policy.query,
        };
        for op in ops {
            let key = match op {
                PolicyOp::Set { key, .. } | PolicyOp::Remove { key } => key,
            };
            let (key, _, _) = emit_key_string(key, kind);
            keys.insert(format!("`{key}`"));
        }
    }
    keys.into_iter().collect()
}

fn doc_codec(enc: &syn::Path, ty: &syn::Type) -> String {
    format!("{}<{}>", quote::quote!(#enc), quote::quote!(#ty))
}

fn doc_path(ep: &ResolvedEndpoint) -> String {
    let mut pieces = Vec::new();
    for piece in ep.scope_path_pieces.iter().chain(ep.route_pieces.iter()) {
        match piece {
            PathPiece::Static(value) => pieces.push(value.clone()),
            PathPiece::CxVar { field, .. } | PathPiece::EpVar { field } => {
                pieces.push(format!("{{{field}}}"));
            }
            PathPiece::Fmt(_) => pieces.push("{part}".to_string()),
        }
    }
    if pieces.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", pieces.join("/"))
    }
}

#[allow(dead_code)]
fn doc_auth_requirement(auth: &AuthRequirementIr) -> String {
    match &auth.placement {
        AuthPlacementIr::Bearer => format!("bearer `{}`", auth.credential),
        AuthPlacementIr::Header { name } => {
            format!("header `{}` = `{}`", name.value(), auth.credential)
        }
        AuthPlacementIr::Query { key } => {
            format!("query `{}` = `{}`", key.value(), auth.credential)
        }
        AuthPlacementIr::Basic => format!("basic `{}`", auth.credential),
        AuthPlacementIr::Certificate => format!("certificate `{}`", auth.credential),
    }
}

#[allow(dead_code)]
fn endpoint_has_rate_limit(ep: &ResolvedEndpoint, client_policy: &PolicyBlocksResolved) -> bool {
    client_policy.rate_limit.is_some()
        || ep
            .policy
            .scopes
            .iter()
            .any(|policy| policy.rate_limit.is_some())
        || ep.policy.endpoint.rate_limit.is_some()
}

#[allow(dead_code)]
fn endpoint_has_retry(ep: &ResolvedEndpoint, client_policy: &PolicyBlocksResolved) -> bool {
    let mut enabled = retry_block_enabled(&client_policy.retry);
    for policy in &ep.policy.scopes {
        match policy.retry {
            Some(RetryResolved::Clear) => enabled = false,
            Some(RetryResolved::Set(_)) => enabled = true,
            None => {}
        }
    }
    match ep.policy.endpoint.retry {
        Some(RetryResolved::Clear) => enabled = false,
        Some(RetryResolved::Set(_)) => enabled = true,
        None => {}
    }
    enabled
}

#[allow(dead_code)]
fn retry_block_enabled(block: &Option<RetryResolved>) -> bool {
    matches!(block, Some(RetryResolved::Set(_)))
}
