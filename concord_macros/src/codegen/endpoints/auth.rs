fn auth_use_part_ident(ep_name: &Ident, index: usize) -> Ident {
    emit_helpers::ident(&format!("__Auth_{ep_name}_{index}"), Span::call_site())
}

fn auth_one_of_alt_part_ident(ep_name: &Ident, index: usize, alt_index: usize) -> Ident {
    emit_helpers::ident(
        &format!("__Auth_{ep_name}_{index}_Alt_{alt_index}"),
        Span::call_site(),
    )
}

fn emit_endpoint_auth_item_ty(ep: &EndpointIr, index: usize, plan: &AuthUsePlanIr) -> TokenStream2 {
    let endpoint_ty = endpoint_internal_ident(ep);
    match plan {
        AuthUsePlanIr::Use(_) => {
            let ident = auth_use_part_ident(&endpoint_ty, index);
            quote! { super::__internal::#ident }
        }
        AuthUsePlanIr::OneOf(alts) => {
            let mut iter = (0..alts.len()).rev();
            let last = iter
                .next()
                .expect("one_of must contain at least one alternative");
            let last_ident = auth_one_of_alt_part_ident(&endpoint_ty, index, last);
            let mut out = quote! { super::__internal::#last_ident };
            for alt in iter {
                let ident = auth_one_of_alt_part_ident(&endpoint_ty, index, alt);
                out =
                    quote! { ::concord_core::internal::OneOfAuth<super::__internal::#ident, #out> };
            }
            out
        }
    }
}

fn emit_endpoint_auth_ty(ep: &EndpointIr) -> TokenStream2 {
    if ep.auth_uses.is_empty() {
        return quote! { ::concord_core::internal::NoAuth };
    }

    let item_types: Vec<TokenStream2> = ep
        .auth_uses
        .iter()
        .enumerate()
        .map(|(idx, plan)| emit_endpoint_auth_item_ty(ep, idx, plan))
        .collect();

    let mut iter = item_types.into_iter().rev();
    let mut out = iter.next().expect("non-empty auth item types");
    for item in iter {
        out = quote! { ::concord_core::internal::AuthChain<#item, #out> };
    }
    out
}

fn emit_auth_parts(ir: &Ir, ep: &EndpointIr, cx_ty: &Ident) -> TokenStream2 {
    let endpoint_ty = endpoint_internal_ident(ep);
    let mut parts = Vec::new();
    for (idx, plan) in ep.auth_uses.iter().enumerate() {
        match plan {
            AuthUsePlanIr::Use(auth_use) => {
                let part_ty = auth_use_part_ident(&endpoint_ty, idx);
                parts.push(emit_auth_part_for_ident(
                    ir, ep, cx_ty, &part_ty, idx, None, auth_use,
                ));
            }
            AuthUsePlanIr::OneOf(alts) => {
                for (alt_idx, auth_use) in alts.iter().enumerate() {
                    let part_ty = auth_one_of_alt_part_ident(&endpoint_ty, idx, alt_idx);
                    parts.push(emit_auth_part_for_ident(
                        ir,
                        ep,
                        cx_ty,
                        &part_ty,
                        idx,
                        Some(alt_idx),
                        auth_use,
                    ));
                }
            }
        }
    }

    quote! {
        #( #parts )*
    }
}

fn emit_auth_part_for_ident(
    ir: &Ir,
    ep: &EndpointIr,
    cx_ty: &Ident,
    part_ty: &Ident,
    idx: usize,
    alt_idx: Option<usize>,
    auth_use: &AuthUseIr,
) -> TokenStream2 {
    let endpoint_ty = endpoint_internal_ident(ep);
    let endpoint_key = endpoint_qualified_name(ep);
    let credential = auth_use_credential_ident_ir(auth_use);
    let credential_ir = ir
        .client_auth_credentials
        .iter()
        .find(|c| c.name == *credential)
        .expect("auth use was validated by sema");
    let provider_ty = emit_auth_provider_ty(&credential_ir.kind);
    let usage_ty = emit_auth_usage_ty(auth_use);
    let usage_expr = emit_auth_usage_expr(ep, cx_ty, auth_use);
    let step_id = if let Some(alt) = alt_idx {
        LitStr::new(
            &format!("{}:{}:alt{}:{}", endpoint_key, idx, alt, credential),
            Span::call_site(),
        )
    } else {
        LitStr::new(
            &format!("{}:{}:{}", endpoint_key, idx, credential),
            Span::call_site(),
        )
    };
    let provenance_layer = match auth_use.provenance {
        AuthUseProvenanceIr::Client => LitStr::new("client", Span::call_site()),
        AuthUseProvenanceIr::Scope(scope_id) => {
            LitStr::new(&format!("scope:{scope_id}"), Span::call_site())
        }
        AuthUseProvenanceIr::Endpoint => LitStr::new("endpoint", Span::call_site()),
    };
    let manual_policy = match &credential_ir.kind {
        AuthCredentialKindIr::Endpoint { .. } => Some(quote! {
            ::concord_core::prelude::AuthStepPolicy {
                retry_on_unauthorized: false,
                retry_on_forbidden: false,
                retry_on_challenge_rejection: false,
                invalidate_on_unauthorized: true,
                invalidate_on_forbidden: true,
                invalidate_on_challenge_rejection: true,
                ..::concord_core::prelude::AuthStepPolicy::default()
            }
        }),
        _ => None,
    };
    let manual_policy_chain = manual_policy
        .as_ref()
        .map(|policy| quote! { .with_policy(#policy) })
        .unwrap_or_else(|| quote! {});

    quote! {
        pub struct #part_ty;

        impl ::concord_core::internal::AuthPart<super::#cx_ty, super::__endpoints::#endpoint_ty> for #part_ty {
            type Ctrl = ::concord_core::prelude::UseCredential<super::#cx_ty, #provider_ty, #usage_ty>;

            fn controller(
                ctx: ::concord_core::prelude::AuthBuildContext<'_, super::#cx_ty>,
                ep: &super::__endpoints::#endpoint_ty,
            ) -> ::core::result::Result<Self::Ctrl, ::concord_core::prelude::ApiClientError> {
                #usage_expr
                ::core::result::Result::Ok(::concord_core::prelude::UseCredential::new(
                    ctx.auth_state.#credential.clone(),
                    __usage,
                )
                #manual_policy_chain
                .with_step_id(#step_id)
                .with_provenance(::concord_core::prelude::AuthProvenance::new(#provenance_layer)))
            }
        }
    }
}

fn emit_auth_usage_ty(auth_use: &AuthUseIr) -> TokenStream2 {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => quote! { ::concord_core::prelude::BearerAuth },
        AuthUseKindIr::Header { .. } => quote! { ::concord_core::prelude::HeaderAuth },
        AuthUseKindIr::Query { .. } => quote! { ::concord_core::prelude::QueryAuth },
        AuthUseKindIr::Basic { .. } => quote! { ::concord_core::prelude::BasicAuth },
        AuthUseKindIr::Certificate { .. } => quote! { ::concord_core::prelude::CertificateAuth },
        AuthUseKindIr::Custom { usage_ty, .. } => quote! { #usage_ty },
    }
}

fn emit_auth_usage_expr(ep: &EndpointIr, cx_ty: &Ident, auth_use: &AuthUseIr) -> TokenStream2 {
    let endpoint_ty = endpoint_internal_ident(ep);
    match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::BearerAuth::new();
        },
        AuthUseKindIr::Header { header, .. } => {
            let param = LitStr::new(&format!("auth header:{}", header.value()), header.span());
            quote! {
                let __usage = ::concord_core::prelude::HeaderAuth::new(
                    ::http::header::HeaderName::from_bytes(#header.as_bytes())
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                            ctx: ::concord_core::prelude::ErrorContext {
                                endpoint: <super::__endpoints::#endpoint_ty as ::concord_core::prelude::Endpoint<super::#cx_ty>>::name(ep),
                                method: <super::__endpoints::#endpoint_ty as ::concord_core::prelude::Endpoint<super::#cx_ty>>::METHOD,
                            },
                            param: #param.into(),
                        })?
                );
            }
        }
        AuthUseKindIr::Query { key, .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::QueryAuth::new(#key);
        },
        AuthUseKindIr::Basic { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::BasicAuth::new();
        },
        AuthUseKindIr::Certificate { .. } => quote! {
            let _ = ep;
            let __usage = ::concord_core::prelude::CertificateAuth::new();
        },
        AuthUseKindIr::Custom {
            usage_ty, usage, ..
        } => quote! {
            let _ = ep;
            let __usage: #usage_ty = (#usage);
        },
    }
}

fn auth_use_credential_ident_ir(auth_use: &AuthUseIr) -> &Ident {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { credential }
        | AuthUseKindIr::Header { credential, .. }
        | AuthUseKindIr::Query { credential, .. }
        | AuthUseKindIr::Basic { credential }
        | AuthUseKindIr::Certificate { credential }
        | AuthUseKindIr::Custom { credential, .. } => credential,
    }
}

