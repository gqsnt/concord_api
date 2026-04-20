fn emit_client_wrapper(
    ir: &Ir,
    vars_ty: &Ident,
    auth_vars_ty: &Ident,
    cx_ty: &Ident,
) -> TokenStream2 {
    use quote::quote;

    let client_ty = &ir.client_name;

    // same "required vars" as Vars::new(...)
    let required: Vec<&VarInfo> = ir
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

    let required_auth: Vec<&VarInfo> = ir
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

    let var_setters = ir.client_vars.iter().map(|v| {
        let f = &v.rust;
        let ty = &v.ty;
        let set_name = emit_helpers::ident(&format!("set_{f}"), f.span());
        if v.optional {
            let clear_name = emit_helpers::ident(&format!("clear_{f}"), f.span());
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

    let (credential_secret_names, has_custom_credentials) = auth_credential_secret_names(ir);
    let configure_rate_limiter = if let Some(policy) = &ir.rate_limit_response_policy {
        quote! {
            __inner.set_rate_limiter(::std::sync::Arc::new(
                ::concord_core::prelude::GovernorRateLimiter::new()
                    .with_response_policy(::std::sync::Arc::new(#policy::default()))
            ));
        }
    } else {
        quote! {}
    };
    let configure_cache_store = if ir.cache_store_enabled {
        let configure_cache_store_body = if let Some(config) = &ir.cache_store_config {
            let config = emit_cache_config(config);
            quote! {
                let __cache_config = #config;
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::prelude::MokaCacheStore::new(
                        ::concord_core::prelude::MokaCacheConfig::from_cache_config(&__cache_config)
                    )
                ));
            }
        } else {
            quote! {
                __inner.set_cache_store(::std::sync::Arc::new(
                    ::concord_core::prelude::MokaCacheStore::default()
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
    let auth_setters = ir.client_auth_vars.iter().map(|v| {
        let f = &v.rust;
        let set_name = emit_helpers::ident(&format!("set_{f}"), f.span());
        let rebuild_auth_state =
            has_custom_credentials || credential_secret_names.contains(&f.to_string());
        if v.optional {
            let clear_name = emit_helpers::ident(&format!("clear_{f}"), f.span());
            if rebuild_auth_state {
                quote! {
            #[inline]
            pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = ::core::option::Option::Some(v.into());
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
            #[inline]
            pub fn #clear_name(&mut self) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = ::core::option::Option::None;
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
        }
            } else {
                quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
                let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = ::core::option::Option::Some(v.into());
                self
            }
            #[inline]
            pub fn #clear_name(&self) -> &Self {
                let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = ::core::option::Option::None;
                self
            }
        }
            }
        } else {
            if rebuild_auth_state {
                quote! {
            #[inline]
            pub fn #set_name(&mut self, v: impl Into<::concord_core::prelude::SecretString>) -> &mut Self {
                {
                    let mut __g = self.inner.auth_vars().write().unwrap();
                    __g.#f = v.into();
                }
                self.inner.set_auth_state(<#cx_ty as ::concord_core::prelude::ClientContext>::init_auth_state(self.inner.vars(), self.inner.auth_vars()));
                self
            }
        }
            } else {
                quote! {
            #[inline]
            pub fn #set_name(&self, v: impl Into<::concord_core::prelude::SecretString>) -> &Self {
               let mut __g = self.inner.auth_vars().write().unwrap();
                __g.#f = v.into();
                self
            }
        }
            }
        }
    });
    let credential_lifecycle_methods = ir.client_auth_credentials.iter().filter_map(|credential| {
        let name = &credential.name;
        let AuthCredentialKindIr::Endpoint {
            endpoint,
            output_ty,
            ..
        } = &credential.kind
        else {
            return None;
        };
        let acquire_name = emit_helpers::ident(&format!("acquire_auth_{name}"), name.span());
        let set_name = emit_helpers::ident(&format!("set_auth_{name}_value"), name.span());
        let clear_name = emit_helpers::ident(&format!("clear_auth_{name}"), name.span());
        let has_name = emit_helpers::ident(&format!("has_auth_{name}"), name.span());
        Some(quote! {
            #[inline]
            pub async fn #acquire_name(
                &self,
                ep: endpoints::#endpoint,
            ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError> {
                let value: #output_ty = self.request(ep).execute().await?;
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.set_manual(value).await;
                Ok(())
            }

            #[inline]
            pub async fn #set_name(&self, value: #output_ty) {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.set_manual(value).await;
            }

            #[inline]
            pub async fn #clear_name(&self) {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.clear_manual().await;
            }

            #[inline]
            pub async fn #has_name(&self) -> bool {
                let __auth_state = self.inner.auth_state();
                __auth_state.#name.has_value().await
            }
        })
    });

    quote! {
        #[derive(Clone)]
        pub struct #client_ty<T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
            inner: ::concord_core::prelude::ApiClient<#cx_ty, T>,
        }
        impl #client_ty<::concord_core::prelude::ReqwestTransport> {
            #[inline]
            pub fn new( #( #ctor_args ),* ) -> Self {
               let vars = #vars_ty::new( #( #new_pass ),* );
                let auth_vars = #auth_vars_ty::new( #( #new_auth_pass ),* );
                let mut __inner = ::concord_core::prelude::ApiClient::<#cx_ty, ::concord_core::prelude::ReqwestTransport>::new(vars, auth_vars);
                #configure_rate_limiter
                #configure_cache_store
                Self { inner: __inner }
            }


            #[inline]
            pub fn new_with_transport<T2: ::concord_core::prelude::Transport>(
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


        }

        impl<T: ::concord_core::prelude::Transport> #client_ty<T> {
            #( #var_setters )*
            #( #auth_setters )*
            #( #credential_lifecycle_methods )*

            #[inline]
            pub fn debug_level(&self) -> ::concord_core::prelude::DebugLevel { self.inner.debug_level() }
            #[inline]
            pub fn set_debug_level(&mut self, level: ::concord_core::prelude::DebugLevel) { self.inner.set_debug_level(level); }
            #[inline]
            pub fn with_debug_level(mut self, level: ::concord_core::prelude::DebugLevel) -> Self { self.inner.set_debug_level(level); self }
            #[inline]
            pub fn debug_sink(&self) -> &::std::sync::Arc<dyn ::concord_core::prelude::DebugSink> { self.inner.debug_sink() }
            #[inline]
            pub fn set_debug_sink(&mut self, sink: ::std::sync::Arc<dyn ::concord_core::prelude::DebugSink>) { self.inner.set_debug_sink(sink); }
            #[inline]
            pub fn with_debug_sink(mut self, sink: ::std::sync::Arc<dyn ::concord_core::prelude::DebugSink>) -> Self { self.inner.set_debug_sink(sink); self }
            #[inline]
            pub fn runtime_hooks(&self) -> &::std::sync::Arc<dyn ::concord_core::prelude::RuntimeHooks> { self.inner.runtime_hooks() }
            #[inline]
            pub fn set_runtime_hooks(&mut self, hooks: ::std::sync::Arc<dyn ::concord_core::prelude::RuntimeHooks>) { self.inner.set_runtime_hooks(hooks); }
            #[inline]
            pub fn with_runtime_hooks(mut self, hooks: ::std::sync::Arc<dyn ::concord_core::prelude::RuntimeHooks>) -> Self { self.inner.set_runtime_hooks(hooks); self }
            #[inline]
            pub fn retry_policy(&self) -> &::std::sync::Arc<dyn ::concord_core::prelude::RetryPolicy> { self.inner.retry_policy() }
            #[inline]
            pub fn set_retry_policy(&mut self, retry_policy: ::std::sync::Arc<dyn ::concord_core::prelude::RetryPolicy>) { self.inner.set_retry_policy(retry_policy); }
            #[inline]
            pub fn with_retry_policy(mut self, retry_policy: ::std::sync::Arc<dyn ::concord_core::prelude::RetryPolicy>) -> Self { self.inner.set_retry_policy(retry_policy); self }
            #[inline]
            pub fn max_auth_retries(&self) -> u32 { self.inner.max_auth_retries() }
            #[inline]
            pub fn set_max_auth_retries(&mut self, max_auth_retries: u32) { self.inner.set_max_auth_retries(max_auth_retries); }
            #[inline]
            pub fn with_max_auth_retries(mut self, max_auth_retries: u32) -> Self { self.inner.set_max_auth_retries(max_auth_retries); self }
            #[inline]
            pub fn pagination_caps(&self) -> ::concord_core::prelude::Caps { self.inner.pagination_caps() }
            #[inline]
            pub fn set_pagination_caps(&mut self, caps: ::concord_core::prelude::Caps) { self.inner.set_pagination_caps(caps); }
            #[inline]
            pub fn with_pagination_caps(mut self, caps: ::concord_core::prelude::Caps) -> Self { self.inner.set_pagination_caps(caps); self }
            #[inline]
            pub fn set_rate_limiter(&mut self, limiter: ::std::sync::Arc<dyn ::concord_core::prelude::RateLimiter>) { self.inner.set_rate_limiter(limiter); }
            #[inline]
            pub fn with_rate_limiter(mut self, limiter: ::std::sync::Arc<dyn ::concord_core::prelude::RateLimiter>) -> Self { self.inner.set_rate_limiter(limiter); self }
            #[inline]
            pub fn set_cache_store(&mut self, store: ::std::sync::Arc<dyn ::concord_core::prelude::CacheStore>) { self.inner.set_cache_store(store); }
            #[inline]
            pub fn with_cache_store(mut self, store: ::std::sync::Arc<dyn ::concord_core::prelude::CacheStore>) -> Self { self.inner.set_cache_store(store); self }
            #[inline]
            pub fn set_inflight_policy(&mut self, policy: ::std::sync::Arc<dyn ::concord_core::prelude::InflightPolicy>) { self.inner.set_inflight_policy(policy); }
            #[inline]
            pub fn with_inflight_policy(mut self, policy: ::std::sync::Arc<dyn ::concord_core::prelude::InflightPolicy>) -> Self { self.inner.set_inflight_policy(policy); self }
            #[inline]
            pub fn request<E>(&self, ep: E) -> ::concord_core::prelude::PendingRequest<'_, #cx_ty, E, T>
            where
                E: ::concord_core::prelude::Endpoint<#cx_ty>,
            {
                self.inner.request(ep)
            }
        }
    }
}

