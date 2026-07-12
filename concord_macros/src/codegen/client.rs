fn emit_scheme(s: crate::model::Scheme) -> TokenStream2 {
    match s {
        crate::model::Scheme::Http => quote! { ::http::uri::Scheme::HTTP },
        crate::model::Scheme::Https => quote! { ::http::uri::Scheme::HTTPS },
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
            match &v.default {
                Some(d) => quote! { #name: #d },
                None => {
                    let err = emit_helpers::compile_error_expr(
                        "required client variable default was missing in resolved IR",
                        name.span(),
                    );
                    quote! { #name: #err }
                }
            }
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

fn emit_client_auth_state(resolved_api: &ResolvedApi, auth_state_ty: &Ident, cx_ty: &Ident) -> TokenStream2 {
    if resolved_api.client_auth_credentials.is_empty() {
        return quote! {};
    }

    let fields = resolved_api.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let provider_ty = emit_auth_provider_ty(&c.kind);
        quote! {
            pub(crate) #name: ::std::sync::Arc<::concord_core::__private::CredentialSlot<#cx_ty, #provider_ty>>
        }
    });

    quote! {
        #[derive(Clone)]
        pub struct #auth_state_ty {
            #( #fields, )*
        }
    }
}

fn emit_client_auth_state_init(resolved_api: &ResolvedApi, auth_state_ty: &Ident) -> (TokenStream2, TokenStream2) {
    if resolved_api.client_auth_credentials.is_empty() {
        return (
            quote! { ::concord_core::__private::NoAuthState },
            quote! {
                let _ = vars;
                let _ = auth;
                ::concord_core::__private::NoAuthState
            },
        );
    }

    let client_ns = LitStr::new(&resolved_api.client_name.to_string(), resolved_api.client_name.span());
    let init_fields = resolved_api.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let name_lit = LitStr::new(&name.to_string(), name.span());
        let provider = emit_auth_provider_init(&client_ns, c);
        match &c.kind {
            AuthCredentialKindIr::OAuth2ClientCredentials { .. } => quote! {
                #name: ::std::sync::Arc::new(::concord_core::__private::CredentialSlot::new_result(
                    ::concord_core::advanced::CredentialId::new(#client_ns, #name_lit),
                    #provider,
                ))
            },
            _ => quote! {
                #name: ::std::sync::Arc::new(::concord_core::__private::CredentialSlot::new(#provider))
            },
        }
    });
    let auth_bind = if resolved_api.client_auth_vars.is_empty() {
        quote! { let _ = auth; }
    } else {
        quote! {
            let auth = match auth.read() {
                ::core::result::Result::Ok(__guard) => __guard,
                ::core::result::Result::Err(__poisoned) => __poisoned.into_inner(),
            };
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
            quote! { ::concord_core::advanced::StaticApiKeyProvider }
        }
        AuthCredentialKindIr::StaticBearer { .. } => {
            quote! { ::concord_core::advanced::StaticBearerProvider }
        }
        AuthCredentialKindIr::Basic { .. } => {
            quote! { ::concord_core::advanced::StaticBasicProvider }
        }
        AuthCredentialKindIr::OAuth2ClientCredentials { .. } => {
            quote! { ::concord_core::advanced::OAuth2ClientCredentialsProvider }
        }
        AuthCredentialKindIr::Endpoint { output_ty, .. } => {
            quote! { ::concord_core::advanced::ManualCredentialProvider<#output_ty> }
        }
    }
}

fn emit_auth_provider_init(client_ns: &LitStr, credential: &AuthCredentialIr) -> TokenStream2 {
    let name = &credential.name;
    let name_lit = LitStr::new(&name.to_string(), name.span());
    let credential_id =
        quote! { ::concord_core::advanced::CredentialId::new(#client_ns, #name_lit) };

    match &credential.kind {
        AuthCredentialKindIr::ApiKey { secret } => quote! {
            ::concord_core::advanced::StaticApiKeyProvider::new(
                #credential_id,
                ::concord_core::prelude::ApiKey::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::StaticBearer { secret } => quote! {
            ::concord_core::advanced::StaticBearerProvider::new(
                #credential_id,
                ::concord_core::prelude::AccessToken::new(auth.#secret.clone()),
            )
        },
        AuthCredentialKindIr::Basic { username, password } => quote! {
            ::concord_core::advanced::StaticBasicProvider::new(
                #credential_id,
                ::concord_core::prelude::BasicCredential::new(
                    auth.#username.clone(),
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
                {
                    let provider = ::concord_core::advanced::OAuth2ClientCredentialsProvider::from_validated_token_url(
                        #credential_id,
                        #token_url,
                        auth.#client_id.clone(),
                        auth.#client_secret.clone(),
                    );
                    provider
                }
            };
            if let Some(scope) = scope {
                quote! {{
                    let provider = #provider;
                    provider.map(|provider| provider.scope(#scope))
                }}
            } else {
                provider
            }
        }
        AuthCredentialKindIr::Endpoint { .. } => {
            let acquire_name = emit_helpers::ident(&format!("acquire_auth_{name}"), name.span());
            let hint = LitStr::new(&format!("client.{acquire_name}(...)"), Span::call_site());
            quote! {
                ::concord_core::advanced::ManualCredentialProvider::new(#credential_id)
                    .with_missing_hint(#hint)
            }
        }
    }
}

fn auth_credential_secret_names(resolved_api: &ResolvedApi) -> (std::collections::BTreeSet<String>, bool) {
    let mut out = std::collections::BTreeSet::new();
    for c in &resolved_api.client_auth_credentials {
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

fn emit_client_auth_prepare_fn(resolved_api: &ResolvedApi) -> TokenStream2 {
    let client_ns = LitStr::new(&resolved_api.client_name.to_string(), resolved_api.client_name.span());
    let arms = resolved_api.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let name_lit = LitStr::new(&name.to_string(), name.span());
        let (apply, reuse) = match &c.kind {
            AuthCredentialKindIr::Basic { .. } => (
                quote! { ::concord_core::advanced::apply_basic_credential(request, requirement, &lease.value)? },
                quote! { ::concord_core::advanced::AuthPreparationReuse::RequestLocal },
            ),
            AuthCredentialKindIr::Endpoint { material_shape, .. } => match material_shape {
                AuthMaterialShapeIr::Basic => {
                    (
                        quote! { ::concord_core::advanced::apply_basic_credential(request, requirement, &lease.value)? },
                        quote! { ::concord_core::advanced::AuthPreparationReuse::Never },
                    )
                }
                AuthMaterialShapeIr::AccessToken
                | AuthMaterialShapeIr::SecretValue
                | AuthMaterialShapeIr::Unknown => {
                    (
                        quote! { ::concord_core::advanced::apply_secret_credential(request, requirement, &lease.value)? },
                        quote! { ::concord_core::advanced::AuthPreparationReuse::Never },
                    )
                }
            },
            AuthCredentialKindIr::ApiKey { .. }
            | AuthCredentialKindIr::StaticBearer { .. } => (
                quote! { ::concord_core::advanced::apply_secret_credential(request, requirement, &lease.value)? },
                quote! { ::concord_core::advanced::AuthPreparationReuse::RequestLocal },
            ),
            AuthCredentialKindIr::OAuth2ClientCredentials { .. } => (
                quote! { ::concord_core::advanced::apply_secret_credential(request, requirement, &lease.value)? },
                quote! { ::concord_core::advanced::AuthPreparationReuse::Never },
            ),
        };
        quote! {
            (#client_ns, #name_lit) => {
                let credential_ctx = ::concord_core::advanced::CredentialContext {
                    vars,
                    auth,
                    auth_state,
                    executor,
                    credential_id: requirement.credential.id.clone(),
                    reason: ::concord_core::advanced::CredentialRefreshReason::Missing,
                };
                let lease = auth_state.#name
                    .get_or_refresh(credential_ctx, ::concord_core::advanced::AuthStepPolicy::default())
                    .await?;
                let application = #apply;
                let applied = ::concord_core::advanced::AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: ::core::option::Option::Some(lease.generation),
                    provenance: requirement.provenance.clone(),
                };
                return ::core::result::Result::Ok(
                    ::concord_core::advanced::PreparedAuthCredential::new(applied, application)
                        .with_reuse(#reuse)
                );
            }
        }
    });
    quote! {
        fn prepare_auth_requirement<'a>(
            requirement: &'a ::concord_core::advanced::AuthRequirement,
            request: &'a mut ::concord_core::advanced::AuthApplicationRequest<'_>,
            vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            auth_state: &'a Self::AuthState,
            executor: &'a dyn ::concord_core::advanced::AuthHttpExecutor,
            _meta: &'a ::concord_core::advanced::RequestMeta,
        ) -> ::concord_core::advanced::AuthFuture<'a, ::core::result::Result<::concord_core::advanced::PreparedAuthCredential, ::concord_core::advanced::AuthError>> {
            ::std::boxed::Box::pin(async move {
                match (requirement.credential.id.namespace(), requirement.credential.id.name()) {
                    #( #arms, )*
                    _ => ::core::result::Result::Err(::concord_core::advanced::AuthError::new(
                        ::concord_core::advanced::AuthErrorKind::UnsupportedScheme,
                        format!("unknown auth credential `{}`", requirement.credential.id),
                    )),
                }
            })
        }
    }
}

fn emit_client_auth_response_fn(resolved_api: &ResolvedApi) -> TokenStream2 {
    let client_ns = LitStr::new(&resolved_api.client_name.to_string(), resolved_api.client_name.span());
    let arms = resolved_api.client_auth_credentials.iter().map(|c| {
        let name = &c.name;
        let name_lit = LitStr::new(&name.to_string(), name.span());
        let retry_after_refresh = match &c.kind {
            AuthCredentialKindIr::Endpoint { .. } => quote! {},
            AuthCredentialKindIr::ApiKey { .. }
            | AuthCredentialKindIr::StaticBearer { .. }
            | AuthCredentialKindIr::Basic { .. }
            | AuthCredentialKindIr::OAuth2ClientCredentials { .. } => quote! {
                if let ::core::option::Option::Some(retry_reason) = decision.retry_reason {
                    return ::core::result::Result::Ok(::concord_core::advanced::AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: retry_reason,
                    });
                }
            },
        };
        quote! {
            (#client_ns, #name_lit) => {
                if let ::core::option::Option::Some(decision) =
                    ::concord_core::advanced::auth_decision_for_status(
                        status,
                        requirement,
                        applied,
                        ::concord_core::advanced::AuthStepPolicy::default(),
                    )
                {
                    if let ::core::option::Option::Some(invalidate_reason) = decision.invalidate_reason {
                        ::concord_core::advanced::invalidate_rejected_credential(
                            auth_state.#name.as_ref(),
                            vars,
                            auth,
                            auth_state,
                            executor,
                            applied,
                            invalidate_reason,
                        ).await?;
                    }
                    #retry_after_refresh
                }
                return ::core::result::Result::Ok(::concord_core::advanced::AuthDecision::Continue);
            }
        }
    });
    quote! {
        fn handle_auth_response<'a>(
            requirement: &'a ::concord_core::advanced::AuthRequirement,
            applied: &'a ::concord_core::advanced::AuthAppliedCredential,
            vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            auth_state: &'a Self::AuthState,
            executor: &'a dyn ::concord_core::advanced::AuthHttpExecutor,
            _meta: &'a ::concord_core::advanced::RequestMeta,
            status: ::http::StatusCode,
            _headers: &'a ::http::HeaderMap,
        ) -> ::concord_core::advanced::AuthFuture<'a, ::core::result::Result<::concord_core::advanced::AuthDecision, ::concord_core::advanced::AuthError>> {
            ::std::boxed::Box::pin(async move {
                match (requirement.credential.id.namespace(), requirement.credential.id.name()) {
                    #( #arms, )*
                    _ => ::core::result::Result::Ok(::concord_core::advanced::AuthDecision::Continue),
                }
            })
        }
    }
}

struct ClientContextEmit<'a> {
    scheme: &'a TokenStream2,
    domain: &'a LitStr,
    resolved_api: &'a ResolvedApi,
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
        resolved_api,
        policy,
        vars_ty,
        auth_vars_ty,
        auth_state_ty,
        cx_ty,
    } = ctx;
    let base_policy = emit_policy_fn_base(policy);
    let (auth_state_assoc_ty, init_auth_state) = emit_client_auth_state_init(resolved_api, auth_state_ty);
    let prepare_auth_requirement = emit_client_auth_prepare_fn(resolved_api);
    let handle_auth_response = emit_client_auth_response_fn(resolved_api);

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
                __concord_auth_vars: &Self::AuthVars,
                ctx: &::concord_core::error::ErrorContext,
            ) -> ::core::result::Result<::concord_core::__private::Policy, ::concord_core::prelude::ApiClientError> {
                let _ = __concord_auth_vars;
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
    if let Some(retry) = emit_retry_op(&policy.retry) {
        ops.push(retry);
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, PolicyEmitCtx::ClientBase) {
        ops.push(rate_limit);
    }

    quote! {
        let mut policy = ::concord_core::__private::Policy::new();
        let ctx = ctx.clone();
        #[allow(unused_variables)]
        let cx = vars;
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
            match &v.default {
                Some(d) => quote! { #name: ::concord_core::prelude::SecretString::new(#d) },
                None => {
                    let err = emit_helpers::compile_error_expr(
                        "required auth variable default was missing in resolved IR",
                        name.span(),
                    );
                    quote! { #name: #err }
                }
            }
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




