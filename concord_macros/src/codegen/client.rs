fn emit_scheme(s: crate::ast::SchemeLit) -> TokenStream2 {
    match s {
        crate::ast::SchemeLit::Http => quote! { ::http::uri::Scheme::HTTP },
        crate::ast::SchemeLit::Https => quote! { ::http::uri::Scheme::HTTPS },
    }
}

fn emit_client_vars(vars: &[VarInfo], vars_ty: &Ident) -> TokenStream2 {
    let fields = vars.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            quote! { pub #name: ::core::option::Option<#ty> }
        } else {
            quote! { pub #name: #ty }
        }
    });

    let required: Vec<&VarInfo> = vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();

    let new_args = required.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        quote! { #name: #ty }
    });

    let init_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #name }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #name: ::core::option::Option::Some(#d) }
            } else {
                quote! { #name: ::core::option::Option::None }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #name: #d }
        }
    });

    let setters = vars.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        if v.optional {
            let clear = emit_helpers::ident(&format!("clear_{name}"), name.span());
            quote! {
                #[inline]
                pub fn #name(mut self, v: #ty) -> Self { self.#name = ::core::option::Option::Some(v); self }
                #[inline]
                pub fn #clear(mut self) -> Self { self.#name = ::core::option::Option::None; self }
            }
        } else {
            quote! {
                #[inline]
                pub fn #name(mut self, v: #ty) -> Self { self.#name = v; self }
            }
        }
    });

    quote! {
        #[derive(Clone)]
       pub struct #vars_ty {
            #( #fields, )*
        }

       impl #vars_ty {
            #[inline]
            pub fn new( #( #new_args ),* ) -> Self {
                Self { #( #init_fields, )* }
            }

            #( #setters )*
       }
    }
}

fn emit_client_auth_state(ir: &Ir, auth_state_ty: &Ident, cx_ty: &Ident) -> TokenStream2 {
    if ir.client_auth_credentials.is_empty() {
        return quote! {};
    }

    let fields = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let provider_ty = emit_auth_provider_ty(&c.kind);
        quote! {
            pub(crate) #name: ::std::sync::Arc<::concord_core::internal::CredentialSlot<#cx_ty, #provider_ty>>
        }
    });

    quote! {
        #[derive(Clone)]
        pub struct #auth_state_ty {
            #( #fields, )*
        }
    }
}

fn emit_client_auth_state_init(ir: &Ir, auth_state_ty: &Ident) -> (TokenStream2, TokenStream2) {
    if ir.client_auth_credentials.is_empty() {
        return (
            quote! { ::concord_core::internal::NoAuthState },
            quote! {
                let _ = vars;
                let _ = auth;
                ::concord_core::internal::NoAuthState
            },
        );
    }

    let client_ns = LitStr::new(&ir.client_name.to_string(), ir.client_name.span());
    let init_fields = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let provider = emit_auth_provider_init(&client_ns, c);
        quote! {
            #name: ::std::sync::Arc::new(::concord_core::internal::CredentialSlot::new(#provider))
        }
    });
    let auth_bind = if ir.client_auth_vars.is_empty() {
        quote! { let _ = auth; }
    } else {
        quote! {
            let auth = auth.read().unwrap();
            #[allow(unused_variables)]
            let secret = &auth;
        }
    };

    (
        quote! { #auth_state_ty },
        quote! {
            let _ = vars;
            #auth_bind
            #auth_state_ty {
                #( #init_fields, )*
            }
        },
    )
}

fn emit_auth_provider_ty(kind: &AuthCredentialKindIr) -> TokenStream2 {
    match kind {
        AuthCredentialKindIr::ApiKey { .. } => {
            quote! { ::concord_core::prelude::StaticApiKeyProvider }
        }
        AuthCredentialKindIr::StaticBearer { .. } => {
            quote! { ::concord_core::prelude::StaticBearerProvider }
        }
        AuthCredentialKindIr::Basic { .. } => {
            quote! { ::concord_core::prelude::StaticBasicProvider }
        }
        AuthCredentialKindIr::OAuth2ClientCredentials { .. } => {
            quote! { ::concord_core::prelude::OAuth2ClientCredentialsProvider }
        }
        AuthCredentialKindIr::Endpoint { output_ty, .. } => {
            quote! { ::concord_core::prelude::ManualCredentialProvider<#output_ty> }
        }
    }
}

fn emit_auth_provider_init(client_ns: &LitStr, credential: &AuthCredentialIr) -> TokenStream2 {
    let name = &credential.name;
    let name_lit = LitStr::new(&name.to_string(), name.span());
    let credential_id =
        quote! { ::concord_core::prelude::CredentialId::new(#client_ns, #name_lit) };

    match &credential.kind {
        AuthCredentialKindIr::ApiKey { secret } => quote! {
            ::concord_core::prelude::StaticApiKeyProvider::new(
                #credential_id,
                ::concord_core::prelude::ApiKey::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::StaticBearer { secret } => quote! {
            ::concord_core::prelude::StaticBearerProvider::new(
                #credential_id,
                ::concord_core::prelude::AccessToken::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::Basic { username, password } => quote! {
            ::concord_core::prelude::StaticBasicProvider::new(
                #credential_id,
                ::concord_core::prelude::BasicCredential::new(
                    auth.#username.expose().to_string(),
                    auth.#password.clone(),
                ),
            )
        },
        AuthCredentialKindIr::OAuth2ClientCredentials {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            let provider = quote! {
                ::concord_core::prelude::OAuth2ClientCredentialsProvider::new(
                    #credential_id,
                    #token_url.parse().expect("valid OAuth2ClientCredentials token_url"),
                    auth.#client_id.clone(),
                    auth.#client_secret.clone(),
                )
            };
            if let Some(scope) = scope {
                quote! { #provider.scope(#scope) }
            } else {
                provider
            }
        }
        AuthCredentialKindIr::Endpoint { .. } => {
            let acquire_name = emit_helpers::ident(&format!("acquire_auth_{name}"), name.span());
            let hint = LitStr::new(&format!("client.{acquire_name}(...)"), Span::call_site());
            quote! {
                ::concord_core::prelude::ManualCredentialProvider::new(#credential_id)
                    .with_missing_hint(#hint)
            }
        }
    }
}

fn auth_credential_secret_names(ir: &Ir) -> (std::collections::BTreeSet<String>, bool) {
    let mut out = std::collections::BTreeSet::new();
    for c in &ir.client_auth_credentials {
        match &c.kind {
            AuthCredentialKindIr::ApiKey { secret }
            | AuthCredentialKindIr::StaticBearer { secret } => {
                out.insert(secret.to_string());
            }
            AuthCredentialKindIr::Basic { username, password } => {
                out.insert(username.to_string());
                out.insert(password.to_string());
            }
            AuthCredentialKindIr::OAuth2ClientCredentials {
                client_id,
                client_secret,
                ..
            } => {
                out.insert(client_id.to_string());
                out.insert(client_secret.to_string());
            }
            AuthCredentialKindIr::Endpoint { .. } => {}
        }
    }
    (out, false)
}

fn emit_client_auth_prepare_fn(ir: &Ir) -> TokenStream2 {
    let client_ns = LitStr::new(&ir.client_name.to_string(), ir.client_name.span());
    let arms = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let name_lit = LitStr::new(&name.to_string(), name.span());
        let apply = match &c.kind {
            AuthCredentialKindIr::Basic { .. } => quote! { ::concord_core::advanced::apply_basic_credential(request, requirement, &lease.value)? },
            AuthCredentialKindIr::Endpoint { .. } => quote! { ::concord_core::advanced::apply_secret_credential(request, requirement, &lease.value)? },
            AuthCredentialKindIr::ApiKey { .. }
            | AuthCredentialKindIr::StaticBearer { .. }
            | AuthCredentialKindIr::OAuth2ClientCredentials { .. } => quote! { ::concord_core::advanced::apply_secret_credential(request, requirement, &lease.value)? },
        };
        quote! {
            if requirement.credential.id == ::concord_core::prelude::CredentialId::new(#client_ns, #name_lit) {
                let credential_ctx = ::concord_core::prelude::CredentialContext {
                    vars,
                    auth,
                    auth_state,
                    executor,
                    credential_id: requirement.credential.id.clone(),
                    reason: ::concord_core::prelude::CredentialRefreshReason::Missing,
                };
                let lease = auth_state.#name
                    .get_or_refresh(credential_ctx, ::concord_core::advanced::AuthStepPolicy::default())
                    .await?;
                let identity = #apply;
                return ::core::result::Result::Ok(::concord_core::prelude::AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: ::core::option::Option::Some(lease.generation),
                    identity,
                    provenance: requirement.provenance.clone(),
                });
            }
        }
    });
    quote! {
        fn prepare_auth_requirement<'a>(
            requirement: &'a ::concord_core::prelude::AuthRequirement,
            request: &'a mut ::concord_core::transport::BuiltRequest,
            vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            auth_state: &'a Self::AuthState,
            executor: &'a dyn ::concord_core::advanced::AuthHttpExecutor,
            _meta: &'a ::concord_core::prelude::RequestMeta,
        ) -> ::concord_core::advanced::AuthFuture<'a, ::core::result::Result<::concord_core::prelude::AuthAppliedCredential, ::concord_core::prelude::AuthError>> {
            ::std::boxed::Box::pin(async move {
                #( #arms )*
                ::core::result::Result::Err(::concord_core::prelude::AuthError::new(
                    ::concord_core::prelude::AuthErrorKind::UnsupportedScheme,
                    format!("unknown auth credential `{}`", requirement.credential.id),
                ))
            })
        }
    }
}

fn emit_client_auth_response_fn(ir: &Ir) -> TokenStream2 {
    let client_ns = LitStr::new(&ir.client_name.to_string(), ir.client_name.span());
    let arms = ir.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let name_lit = LitStr::new(&name.to_string(), name.span());
        quote! {
            if requirement.credential.id == ::concord_core::prelude::CredentialId::new(#client_ns, #name_lit) {
                let signal = if status == ::http::StatusCode::UNAUTHORIZED {
                    ::core::option::Option::Some((::concord_core::prelude::InvalidateReason::Unauthorized, ::concord_core::advanced::AuthRetryReason::Unauthorized))
                } else if status == ::http::StatusCode::FORBIDDEN {
                    ::core::option::Option::Some((::concord_core::prelude::InvalidateReason::Forbidden, ::concord_core::advanced::AuthRetryReason::Forbidden))
                } else {
                    ::core::option::Option::None
                };
                if let ::core::option::Option::Some((invalidate_reason, retry_reason)) = signal {
                    ::concord_core::advanced::invalidate_rejected_credential(
                        auth_state.#name.as_ref(),
                        vars,
                        auth,
                        auth_state,
                        executor,
                        applied,
                        invalidate_reason,
                    ).await?;
                    return ::core::result::Result::Ok(::concord_core::prelude::AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: retry_reason,
                    });
                }
                return ::core::result::Result::Ok(::concord_core::prelude::AuthDecision::Continue);
            }
        }
    });
    quote! {
        fn handle_auth_response<'a>(
            requirement: &'a ::concord_core::prelude::AuthRequirement,
            applied: &'a ::concord_core::prelude::AuthAppliedCredential,
            vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            auth_state: &'a Self::AuthState,
            executor: &'a dyn ::concord_core::advanced::AuthHttpExecutor,
            _meta: &'a ::concord_core::prelude::RequestMeta,
            status: ::http::StatusCode,
            _headers: &'a ::http::HeaderMap,
        ) -> ::concord_core::advanced::AuthFuture<'a, ::core::result::Result<::concord_core::prelude::AuthDecision, ::concord_core::prelude::AuthError>> {
            ::std::boxed::Box::pin(async move {
                #( #arms )*
                ::core::result::Result::Ok(::concord_core::prelude::AuthDecision::Continue)
            })
        }
    }
}

struct ClientContextEmit<'a> {
    scheme: &'a TokenStream2,
    domain: &'a LitStr,
    ir: &'a Ir,
    policy: &'a PolicyBlocksResolved,
    vars_ty: &'a Ident,
    auth_vars_ty: &'a Ident,
    auth_state_ty: &'a Ident,
    cx_ty: &'a Ident,
}

fn emit_client_context(ctx: ClientContextEmit<'_>) -> TokenStream2 {
    let ClientContextEmit {
        scheme,
        domain,
        ir,
        policy,
        vars_ty,
        auth_vars_ty,
        auth_state_ty,
        cx_ty,
    } = ctx;
    let base_policy = emit_policy_fn_base(policy);
    let (auth_state_assoc_ty, init_auth_state) = emit_client_auth_state_init(ir, auth_state_ty);
    let prepare_auth_requirement = emit_client_auth_prepare_fn(ir);
    let handle_auth_response = emit_client_auth_response_fn(ir);

    quote! {
        #[derive(Clone)]
        pub struct #cx_ty;

        impl ::concord_core::prelude::ClientContext for #cx_ty {
            type Vars = #vars_ty;
            type AuthVars = #auth_vars_ty;
            type AuthState = #auth_state_assoc_ty;
            const SCHEME: ::http::uri::Scheme = #scheme;
            const DOMAIN: &'static str = #domain;

            fn init_auth_state(
                vars: &Self::Vars,
                auth: &Self::AuthVars,
            ) -> Self::AuthState {
                #init_auth_state
            }

            #prepare_auth_requirement

            #handle_auth_response

            fn base_policy(
                vars: &Self::Vars,
                auth: &Self::AuthVars,
                ctx: &::concord_core::prelude::ErrorContext,
            ) -> ::core::result::Result<::concord_core::prelude::Policy, ::concord_core::prelude::ApiClientError> {
                #base_policy
            }
        }
    }
}

fn emit_policy_fn_base(policy: &PolicyBlocksResolved) -> TokenStream2 {
    let mut ops = Vec::new();
    ops.extend(emit_policy_ops(
        policy,
        PolicyKeyKind::Header,
        PolicyEmitCtx::ClientBase,
    ));
    ops.extend(emit_policy_ops(
        policy,
        PolicyKeyKind::Query,
        PolicyEmitCtx::ClientBase,
    ));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, PolicyEmitCtx::ClientBase);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    if let Some(cache) = emit_cache_op(&policy.cache) {
        ops.push(cache);
    }
    if let Some(retry) = emit_retry_op(&policy.retry) {
        ops.push(retry);
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, PolicyEmitCtx::ClientBase) {
        ops.push(rate_limit);
    }

    let lock_auth = if policy_uses_auth(policy) {
        quote! { let auth = auth.read().unwrap(); }
    } else {
        quote! {}
    };

    quote! {
        let mut policy = ::concord_core::prelude::Policy::new();
        let ctx = ctx.clone();
        #[allow(unused_variables)]
        let cx = vars;
        #[allow(unused_variables)]
        let auth = auth;
        #lock_auth
        #( #ops )*
        ::core::result::Result::Ok(policy)
    }
}

fn emit_client_auth_vars(
    vars: &[VarInfo],
    auth_inner_ty: &Ident,
    auth_vars_ty: &Ident,
) -> TokenStream2 {
    use quote::quote;

    // empty auth vars => unit struct (no lock)
    if vars.is_empty() {
        return quote! {
            #[derive(Clone, Default)]
            pub struct #auth_vars_ty;
            impl #auth_vars_ty {
                #[inline]
                pub fn new() -> Self { Self }
            }
        };
    }

    let inner_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if v.optional {
            quote! { pub #name: ::core::option::Option<::concord_core::prelude::SecretString> }
        } else {
            quote! { pub #name: ::concord_core::prelude::SecretString }
        }
    });

    let required: Vec<&VarInfo> = vars
        .iter()
        .filter(|v| !v.optional && v.default.is_none())
        .collect();
    let new_args = required.iter().map(|v| {
        let name = &v.rust;
        let ty = &v.ty;
        quote! { #name: #ty }
    });
    let inner_init_fields = vars.iter().map(|v| {
        let name = &v.rust;
        if !v.optional && v.default.is_none() {
            quote! { #name: ::concord_core::prelude::SecretString::new(#name) }
        } else if v.optional {
            if let Some(d) = &v.default {
                quote! { #name: ::core::option::Option::Some(::concord_core::prelude::SecretString::new(#d)) }
            } else {
                quote! { #name: ::core::option::Option::None }
            }
        } else {
            let d = v.default.as_ref().unwrap();
            quote! { #name: ::concord_core::prelude::SecretString::new(#d) }
        }
    });

    // Default if no required args
    let can_default = required.is_empty();
    let default_impl = if can_default {
        quote! {
            impl ::core::default::Default for #auth_vars_ty {
                fn default() -> Self { Self::new() }
            }
        }
    } else {
        quote! {}
    };

    let new_sig = if required.is_empty() {
        quote! { pub fn new() -> Self }
    } else {
        quote! { pub fn new( #( #new_args ),* ) -> Self }
    };
    quote! {
        #[derive(Clone)]
        pub struct #auth_inner_ty {
            #( #inner_fields, )*
        }
        #[derive(Clone)]
        pub struct #auth_vars_ty(pub ::std::sync::Arc<::std::sync::RwLock<#auth_inner_ty>>);

        impl ::core::ops::Deref for #auth_vars_ty {
            type Target = ::std::sync::RwLock<#auth_inner_ty>;
            #[inline]
            fn deref(&self) -> &Self::Target { &self.0 }
        }

        impl #auth_vars_ty {
            #[inline]
            #new_sig {
                let inner = #auth_inner_ty { #( #inner_init_fields, )* };
                Self(::std::sync::Arc::new(::std::sync::RwLock::new(inner)))
            }
        }
        #default_impl
    }
}

