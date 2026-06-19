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
        let v = resolved_api
            .client_vars
            .iter()
            .find(|var| var.rust == setter.field.as_str())
            .expect("FacadeIr client setter must target a resolved client var");
        let f = &v.rust;
        let ty = &v.ty;
        let set_name = emit_helpers::ident(&setter.set_name, f.span());
        if v.optional {
            let clear_name = emit_helpers::ident(&setter.clear_name, f.span());
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
    let configure_cache_store = if resolved_api.cache_store_enabled {
        let configure_cache_store_body = if let Some(config) = &resolved_api.cache_store_config {
            let config = emit_cache_config(config);
            quote! {
                let __cache_config = #config;
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::advanced::MokaCacheStore::new(
                        ::concord_core::advanced::MokaCacheConfig::from_cache_config(&__cache_config)
                    )
                ));
            }
        } else {
            quote! {
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::advanced::MokaCacheStore::default()
                ));
            }
        };
        quote! {
            #[cfg(not(feature = "cache-moka"))]
            compile_error!(
                "cache default backend requires a `cache-moka` crate feature that enables `concord_core/cache-moka`"
            );
            #[cfg(feature = "cache-moka")]
            {
                #configure_cache_store_body
            }
        }
    } else {
        quote! {}
    };
    let auth_setters = facade_ir.auth_setters.iter().map(|setter| {
        let v = resolved_api
            .client_auth_vars
            .iter()
            .find(|var| var.rust == setter.field.as_str())
            .expect("FacadeIr auth setter must target a resolved auth var");
        let f = &v.rust;
        let set_name = emit_helpers::ident(&setter.set_name, f.span());
        let rebuild_auth_state =
            has_custom_credentials || credential_secret_names.contains(&f.to_string());
        if v.optional {
            let clear_name = emit_helpers::ident(&setter.clear_name, f.span());
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
                        self.inner.try_set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()))?;
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
                        self.inner.try_set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()))?;
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
                        self.inner.try_set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()))?;
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
            endpoint,
            output_ty,
            ..
        } = &credential.kind
        else {
            return None;
        };
        let methods = facade_credential_methods_for(facade_ir, name);
        let acquire_name = emit_helpers::ident(&methods.acquire_name, name.span());
        let set_name = emit_helpers::ident(&methods.set_name, name.span());
        let clear_name = emit_helpers::ident(&methods.clear_name, name.span());
        let has_name = emit_helpers::ident(&methods.has_name, name.span());
        Some(quote! {
            #[inline]
            pub async fn #acquire_name(
                &self,
                ep: endpoints::#endpoint,
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
                __auth_state.#name.clear_manual().await;
                ::core::result::Result::Ok(())
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
        let AuthCredentialKindIr::Endpoint { endpoint, .. } = &credential.kind else {
            return None;
        };
        let methods = facade_credential_methods_for(facade_ir, name);
        let method = emit_helpers::ident(&methods.pending_method, name.span());
        let trait_name = acquire_as_trait_ident(client_ty, name);
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
                for ::concord_core::prelude::PendingRequest<'a, #cx_ty, endpoints::#endpoint, T>
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
                #configure_cache_store
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
                #configure_cache_store
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
            #[doc = "Return the pagination caps."]
            #[inline]
            pub fn pagination_caps(&self) -> ::concord_core::advanced::Caps { self.inner.pagination_caps() }
            #[doc = "Set pagination caps in place."]
            #[inline]
            pub fn set_pagination_caps(&mut self, caps: ::concord_core::advanced::Caps) { self.inner.set_pagination_caps(caps); }
            #[doc = "Return this client with changed pagination caps."]
            #[inline]
            pub fn with_pagination_caps(mut self, caps: ::concord_core::advanced::Caps) -> Self { self.inner.set_pagination_caps(caps); self }
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

fn emit_auth_facade(resolved_api: &ResolvedApi, client_ty: &Ident) -> (TokenStream2, TokenStream2) {
    let root_auth_scope_exists = resolved_api
        .endpoints
        .iter()
        .any(|ep| ep.scope_modules.first().is_some_and(|scope| scope == "auth"));
    let auth_ty = emit_helpers::ident(&format!("{}Auth", client_ty), client_ty.span());
    let handle_items = resolved_api.client_auth_credentials.iter().filter_map(|credential| {
        let AuthCredentialKindIr::Endpoint {
            endpoint,
            output_ty,
            ..
        } = &credential.kind
        else {
            return None;
        };
        let name = &credential.name;
        let handle_ty = emit_helpers::ident(
            &format!("{}{}Auth", client_ty, pascalize(&name.to_string())),
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
                                endpoint: stringify!(#endpoint),
                                method: ::http::Method::GET,
                            },
                            source,
                        }
                    })?;
                    __auth_state.#name.set_manual(value).await.map_err(|source| {
                        ::concord_core::prelude::ApiClientError::Auth {
                            ctx: ::concord_core::advanced::ErrorContext {
                                endpoint: stringify!(#endpoint),
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
                    __auth_state.#name.clear_manual().await;
                    ::core::result::Result::Ok(())
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
            &format!("{}{}Auth", client_ty, pascalize(&name.to_string())),
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
            let facade = facade_ir_for_endpoint(facade_ir, ep);
            emit_facade_endpoint_method(ep, facade, client_ty, cx_ty, &[], true)
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
    let span = Span::call_site();
    let method = emit_helpers::ident(method_name, span);
    let struct_name = emit_helpers::ident(&scope_ir.rust_type_name, span);
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
            let default = v.default.as_ref().unwrap();
            quote! { #name: #default }
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
    let span = Span::call_site();
    let struct_name = emit_helpers::ident(&scope_ir.rust_type_name, span);
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
            .find(|var| var.rust == setter.field.as_str())?;
        Some(emit_scope_setter(setter, var))
    });
    let child_methods = scope_ir.methods.iter().map(|method| {
        let child_ir = facade_scope_ir_for_path(facade_ir, &method.target_scope_path);
        emit_scope_ctor_method(child_ir, Some(scope_ir), Some(method))
    });
    let endpoint_methods = resolved_api
        .endpoints
        .iter()
        .filter_map(|ep| {
            let facade = facade_ir_for_endpoint(facade_ir, ep);
            if facade.scope_path != scope_ir.path {
                return None;
            }
            Some(
            emit_facade_endpoint_method(
                ep,
                facade,
                client_ty,
                cx_ty,
                &scope_ir.decls,
                false,
            )
            )
        });
    let auth_accessor_methods = if scope_ir.path.len() == 1 && scope_ir.path[0] == "auth" {
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
    ep: &ResolvedEndpoint,
    facade: &FacadeEndpoint,
    _client_ty: &Ident,
    cx_ty: &Ident,
    captured: &[VarInfo],
    root: bool,
) -> TokenStream2 {
    let endpoint_ty = ep.scope_modules.iter().fold(quote! { endpoints }, |acc, scope| {
        quote! { #acc::#scope }
    });
    let endpoint_name = &ep.name;
    let endpoint_path = quote! { #endpoint_ty::#endpoint_name };
    let method = emit_helpers::ident(&facade.public_method, ep.name.span());
    let captured_names = captured
        .iter()
        .map(|v| v.rust.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let body_arg = facade
        .required_args
        .iter()
        .find(|arg| arg.name == "body")
        .and(ep.body.as_ref())
        .map(|body| {
            let ty = &body.ty;
            quote! { body: #ty }
        });
    let call_args: Vec<TokenStream2> = ep
        .vars
        .iter()
        .filter(|v| facade.required_args.iter().any(|arg| v.rust == arg.name))
        .map(|v| {
            let name = &v.rust;
            let ty = &v.ty;
            if captured_names.contains(&name.to_string()) {
                quote! {}
            } else {
                quote! { #name: #ty }
            }
        })
        .filter(|tokens| !tokens.is_empty())
        .collect();
    let new_args: Vec<TokenStream2> = ep
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .map(|v| {
            let name = &v.rust;
            if captured_names.contains(&name.to_string()) {
                quote! { self.#name }
            } else {
                quote! { #name }
            }
        })
        .chain(body_arg.as_ref().map(|_| quote! { body }))
        .collect();
    let captured_setters: Vec<TokenStream2> = captured.iter().filter(|v| v.optional || v.default.is_some()).map(|v| {
        let name = &v.rust;
        if v.optional {
            quote! {
                if let ::core::option::Option::Some(value) = self.#name {
                    __ep.#name = ::core::option::Option::Some(value);
                }
            }
        } else {
            quote! { __ep.#name = self.#name; }
        }
    }).collect();
    let self_arg = if root {
        quote! { &self }
    } else {
        quote! { self }
    };
    let client_expr = if root { quote! { self } } else { quote! { __client } };
    let lifetime = if root { quote! { '_ } } else { quote! { 'a } };
    let bind_client = if root { quote! {} } else { quote! { let __client = self.client; } };
    let args: Vec<TokenStream2> = call_args.into_iter().chain(body_arg).collect();
    let docs = facade_ir_endpoint_docs(facade, ep.name.span());

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

fn emit_scope_setter(setter: &FacadeSetter, var: &VarInfo) -> TokenStream2 {
    let span = var.rust.span();
    let name = emit_helpers::ident(&setter.set_name, span);
    let ty = &var.ty;
    if var.optional {
        let clear = emit_helpers::ident(&setter.clear_name, span);
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

fn facade_ir_for_endpoint<'a>(facade_ir: &'a FacadeIr, ep: &ResolvedEndpoint) -> &'a FacadeEndpoint {
    let target = endpoint_qualified_name(ep);
    facade_ir
        .endpoints
        .iter()
        .find(|candidate| candidate.target_endpoint == target)
        .expect("FacadeIr must contain one public endpoint entry per resolved endpoint")
}

fn facade_credential_methods_for<'a>(
    facade_ir: &'a FacadeIr,
    name: &Ident,
) -> &'a FacadeCredentialMethods {
    facade_ir
        .credential_methods
        .iter()
        .find(|methods| name == methods.credential.as_str())
        .expect("FacadeIr must contain one public method set per endpoint-backed credential")
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

fn facade_scope_ir_for_path<'a>(facade_ir: &'a FacadeIr, path: &[String]) -> &'a FacadeScope {
    facade_ir
        .scopes
        .iter()
        .find(|scope| scope.path == path)
        .expect("FacadeIr must contain one scope entry per resolved facade scope")
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
            let AuthUsePlanIr::Use(auth) = auth;
            docs.push(LitStr::new(
                &format!("- {}", doc_auth_use(auth.as_ref())),
                ep.name.span(),
            ));
        }
    }
    if endpoint_has_cache(ep, client_policy) {
        docs.push(LitStr::new("Cache: configured", ep.name.span()));
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
        let controller = pagination
            .ctrl_ty
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "configured".to_string());
        docs.push(LitStr::new(
            &format!("Pagination: {controller}"),
            ep.name.span(),
        ));
    }
    if let Some(body) = &ep.body {
        docs.push(LitStr::new(
            &format!("Body: {}", doc_codec(&body.enc, &body.ty)),
            ep.name.span(),
        ));
    }
    docs.push(LitStr::new(
        &format!("Response: {}", doc_codec(&ep.response.enc, &ep.response.ty)),
        ep.name.span(),
    ));
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
fn doc_auth_use(auth: &AuthUseIr) -> String {
    match &auth.kind {
        AuthUseKindIr::Bearer { credential } => format!("bearer `{credential}`"),
        AuthUseKindIr::Header { header, credential } => {
            format!("header `{}` = `{credential}`", header.value())
        }
        AuthUseKindIr::Query { key, credential } => {
            format!("query `{}` = `{credential}`", key.value())
        }
        AuthUseKindIr::Basic { credential } => format!("basic `{credential}`"),
        AuthUseKindIr::Certificate { credential } => format!("certificate `{credential}`"),
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
fn endpoint_has_cache(ep: &ResolvedEndpoint, client_policy: &PolicyBlocksResolved) -> bool {
    client_policy.cache.is_some()
        || ep
            .policy
            .scopes
            .iter()
            .any(|policy| policy.cache.is_some())
        || ep.policy.endpoint.cache.is_some()
}

#[allow(dead_code)]
fn endpoint_has_retry(ep: &ResolvedEndpoint, client_policy: &PolicyBlocksResolved) -> bool {
    client_policy.retry.is_some()
        || ep
            .policy
            .scopes
            .iter()
            .any(|policy| policy.retry.is_some())
        || ep.policy.endpoint.retry.is_some()
}
