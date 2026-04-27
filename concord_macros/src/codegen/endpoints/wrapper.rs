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
    let (auth_facade_methods, auth_facade_items) = emit_auth_facade(ir, client_ty);
    let (facade_methods, facade_items) = emit_tree_facade(ir, client_ty, cx_ty);

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
            #auth_facade_methods

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
            pub fn configure(&mut self, f: impl FnOnce(&mut ::concord_core::prelude::RuntimeConfig)) -> &mut Self { self.inner.configure(f); self }
            #[inline]
            pub fn with_configure(mut self, f: impl FnOnce(&mut ::concord_core::prelude::RuntimeConfig)) -> Self { self.inner.configure(f); self }
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
    }
}

fn emit_auth_facade(ir: &Ir, client_ty: &Ident) -> (TokenStream2, TokenStream2) {
    let root_auth_scope_exists = ir
        .endpoints
        .iter()
        .any(|ep| ep.scope_modules.first().is_some_and(|scope| scope == "auth"));
    let auth_ty = emit_helpers::ident(&format!("__{}Auth", client_ty), client_ty.span());
    let handle_items = ir.client_auth_credentials.iter().filter_map(|credential| {
        let AuthCredentialKindIr::Endpoint {
            endpoint: _,
            output_ty,
            ..
        } = &credential.kind
        else {
            return None;
        };
        let name = &credential.name;
        let handle_ty = emit_helpers::ident(&format!("__{}Auth{}", client_ty, name), name.span());
        Some(quote! {
            pub struct #handle_ty<'a, T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
                client: &'a #client_ty<T>,
            }

            impl<'a, T: ::concord_core::prelude::Transport> #handle_ty<'a, T> {
                #[inline]
                pub async fn acquire<R>(
                    &self,
                    request: R,
                ) -> ::core::result::Result<(), ::concord_core::prelude::ApiClientError>
                where
                    R: ::core::future::IntoFuture<Output = ::core::result::Result<#output_ty, ::concord_core::prelude::ApiClientError>>,
                {
                    let value: #output_ty = request.await?;
                    let __auth_state = self.client.inner.auth_state();
                    __auth_state.#name.set_manual(value).await;
                    Ok(())
                }

                #[inline]
                pub async fn set(&self, value: #output_ty) {
                    let __auth_state = self.client.inner.auth_state();
                    __auth_state.#name.set_manual(value).await;
                }

                #[inline]
                pub async fn clear(&self) {
                    let __auth_state = self.client.inner.auth_state();
                    __auth_state.#name.clear_manual().await;
                }

                #[inline]
                pub async fn is_set(&self) -> bool {
                    let __auth_state = self.client.inner.auth_state();
                    __auth_state.#name.has_value().await
                }
            }
        })
    });
    let auth_methods = emit_auth_accessor_methods(ir, client_ty);

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
        pub struct #auth_ty<'a, T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
            client: &'a #client_ty<T>,
        }

        impl<'a, T: ::concord_core::prelude::Transport> #auth_ty<'a, T> {
            #auth_methods
        }
    };

    let items = quote! {
        #auth_state_item
        #( #handle_items )*
    };
    (methods, items)
}

fn emit_auth_accessor_methods(ir: &Ir, client_ty: &Ident) -> TokenStream2 {
    let methods = ir.client_auth_credentials.iter().filter_map(|credential| {
        if !matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
            return None;
        }
        let name = &credential.name;
        let handle_ty = emit_helpers::ident(&format!("__{}Auth{}", client_ty, name), name.span());
        Some(quote! {
            #[inline]
            pub fn #name(&self) -> #handle_ty<'a, T> {
                #handle_ty { client: self.client }
            }
        })
    });
    quote! { #( #methods )* }
}

#[derive(Clone)]
struct FacadeScopeInfo {
    path: Vec<Ident>,
    decls: Vec<VarInfo>,
}

fn pascal_to_snake(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_lower_or_digit = false;
    for ch in raw.chars() {
        if ch.is_ascii_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

fn facade_scope_struct(path: &[Ident]) -> Ident {
    let name = path
        .iter()
        .map(ToString::to_string)
        .map(|s| {
            let mut chars = s.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .collect::<String>();
    emit_helpers::ident(&format!("__Facade{name}"), path.last().unwrap().span())
}

fn required_vars(vars: &[VarInfo]) -> Vec<&VarInfo> {
    vars.iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect()
}

fn collect_facade_scopes(ir: &Ir) -> Vec<FacadeScopeInfo> {
    let mut scopes: Vec<FacadeScopeInfo> = Vec::new();
    for ep in &ir.endpoints {
        let mut path = Vec::new();
        for &layer_id in &ep.ancestry {
            let layer = &ir.layers[layer_id];
            let Some(scope_name) = &layer.scope_name else {
                continue;
            };
            path.push(scope_name.clone());
            if scopes.iter().any(|scope| scope.path == path) {
                continue;
            }
            let mut decls = Vec::new();
            for &ancestor_id in &ep.ancestry {
                let ancestor = &ir.layers[ancestor_id];
                if ancestor.scope_name.is_some() {
                    decls.extend(ancestor.decls.clone());
                }
                if ancestor_id == layer_id {
                    break;
                }
            }
            scopes.push(FacadeScopeInfo {
                path: path.clone(),
                decls,
            });
        }
    }
    scopes
}

fn emit_tree_facade(ir: &Ir, client_ty: &Ident, cx_ty: &Ident) -> (TokenStream2, TokenStream2) {
    let scopes = collect_facade_scopes(ir);
    let root_scope_methods = scopes
        .iter()
        .filter(|scope| scope.path.len() == 1)
        .map(|scope| emit_scope_ctor_method(scope, client_ty, None));
    let root_endpoint_methods = ir
        .endpoints
        .iter()
        .filter(|ep| ep.scope_modules.is_empty())
        .map(|ep| emit_facade_endpoint_method(ep, client_ty, cx_ty, &[], true));
    let scope_structs = scopes
        .iter()
        .map(|scope| emit_facade_scope_struct(ir, client_ty, cx_ty, scope, &scopes));

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
    scope: &FacadeScopeInfo,
    _client_ty: &Ident,
    parent_scope: Option<&FacadeScopeInfo>,
) -> TokenStream2 {
    let method = scope.path.last().unwrap();
    let struct_name = facade_scope_struct(&scope.path);
    let parent_decl_count = parent_scope.map_or(0, |s| s.decls.len());
    let new_decls = &scope.decls[parent_decl_count..];
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
    ir: &Ir,
    client_ty: &Ident,
    cx_ty: &Ident,
    scope: &FacadeScopeInfo,
    scopes: &[FacadeScopeInfo],
) -> TokenStream2 {
    let struct_name = facade_scope_struct(&scope.path);
    let fields = scope.decls.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            quote! { #name: ::core::option::Option<#ty> }
        } else {
            quote! { #name: #ty }
        }
    });
    let setters = scope.decls.iter().filter(|v| v.optional || v.default.is_some()).map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            let clear = emit_helpers::ident(&format!("clear_{name}"), name.span());
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
    });
    let child_methods = scopes
        .iter()
        .filter(|child| child.path.len() == scope.path.len() + 1 && child.path.starts_with(&scope.path))
        .map(|child| emit_scope_ctor_method(child, client_ty, Some(scope)));
    let endpoint_methods = ir
        .endpoints
        .iter()
        .filter(|ep| ep.scope_modules == scope.path)
        .map(|ep| emit_facade_endpoint_method(ep, client_ty, cx_ty, &scope.decls, false));
    let auth_accessor_methods = if scope.path.len() == 1 && scope.path[0] == "auth" {
        emit_auth_accessor_methods(ir, client_ty)
    } else {
        quote! {}
    };

    quote! {
        #[allow(non_camel_case_types)]
        pub struct #struct_name<'a, T: ::concord_core::prelude::Transport = ::concord_core::prelude::ReqwestTransport> {
            client: &'a #client_ty<T>,
            #( #fields, )*
        }

        impl<'a, T: ::concord_core::prelude::Transport> #struct_name<'a, T> {
            #( #setters )*
            #( #child_methods )*
            #( #endpoint_methods )*
            #auth_accessor_methods
        }
    }
}

fn emit_facade_endpoint_method(
    ep: &EndpointIr,
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
    let method_name_raw = ep.alias.as_ref().unwrap_or(&ep.name).to_string();
    let method = emit_helpers::ident(&pascal_to_snake(&method_name_raw), ep.name.span());
    let captured_names = captured
        .iter()
        .map(|v| v.rust.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let body_arg = ep.body.as_ref().map(|body| {
        let ty = &body.ty;
        quote! { body: #ty }
    });
    let call_args = ep
        .vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .map(|v| {
            let name = &v.rust;
            let ty = &v.ty;
            if captured_names.contains(&name.to_string()) {
                quote! {}
            } else {
                quote! { #name: #ty }
            }
        })
        .filter(|tokens| !tokens.is_empty());
    let new_args = ep
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
        .chain(body_arg.as_ref().map(|_| quote! { body }));
    let captured_setters = captured.iter().filter(|v| v.optional || v.default.is_some()).map(|v| {
        let name = &v.rust;
        if v.optional {
            quote! {
                if let ::core::option::Option::Some(value) = self.#name {
                    __ep = __ep.#name(value);
                }
            }
        } else {
            quote! { __ep = __ep.#name(self.#name); }
        }
    });
    let self_arg = if root {
        quote! { &self }
    } else {
        quote! { self }
    };
    let client_expr = if root { quote! { self } } else { quote! { __client } };
    let lifetime = if root { quote! { '_ } } else { quote! { 'a } };
    let bind_client = if root { quote! {} } else { quote! { let __client = self.client; } };
    let args = call_args.chain(body_arg);

    quote! {
        #[inline]
        pub fn #method(#self_arg, #( #args ),*) -> ::concord_core::prelude::PendingRequest<#lifetime, #cx_ty, #endpoint_path, T> {
            #bind_client
            let mut __ep = #endpoint_path::new( #( #new_args ),* );
            #( #captured_setters )*
            #client_expr.request(__ep)
        }
    }
}

