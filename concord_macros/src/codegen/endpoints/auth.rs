fn emit_endpoint_auth_plan(ir: &Ir, ep: &EndpointIr) -> TokenStream2 {
    if ep.auth_uses.is_empty() {
        return quote! { ::concord_core::prelude::AuthPlan::default() };
    }
    let mut requirements = Vec::new();
    for (idx, plan) in ep.auth_uses.iter().enumerate() {
        match plan {
            AuthUsePlanIr::Use(auth_use) => {
                requirements.push(emit_auth_requirement(ir, ep, idx, None, auth_use));
            }
        }
    }
    quote! {
        ::concord_core::prelude::AuthPlan {
            requirements: ::std::vec![ #( #requirements ),* ],
        }
    }
}

fn emit_auth_requirement(
    ir: &Ir,
    ep: &EndpointIr,
    idx: usize,
    alt_idx: Option<usize>,
    auth_use: &AuthUseIr,
) -> TokenStream2 {
    let endpoint_key = endpoint_qualified_name(ep);
    let credential = auth_use_credential_ident_ir(auth_use);
    let credential_ir = ir
        .client_auth_credentials
        .iter()
        .find(|c| c.name == *credential)
        .expect("auth use was validated by sema");
    let client_ns = LitStr::new(&ir.client_name.to_string(), ir.client_name.span());
    let credential_name = LitStr::new(&credential.to_string(), credential.span());
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
        AuthUseProvenanceIr::Scope(scope_id) => LitStr::new(&format!("scope:{scope_id}"), Span::call_site()),
        AuthUseProvenanceIr::Endpoint => LitStr::new("endpoint", Span::call_site()),
    };
    let placement = emit_auth_placement(auth_use);
    let usage_id = emit_auth_usage_id(auth_use);
    let _ = credential_ir;
    quote! {
        ::concord_core::prelude::AuthRequirement {
            credential: ::concord_core::prelude::CredentialRef {
                id: ::concord_core::prelude::CredentialId::new(#client_ns, #credential_name),
            },
            placement: #placement,
            usage_id: ::concord_core::advanced::AuthUsageId::new(#usage_id),
            step_id: ::core::option::Option::Some(#step_id),
            provenance: ::concord_core::advanced::AuthProvenance::new(#provenance_layer),
            challenge: ::concord_core::prelude::AuthChallengePolicy::Default,
        }
    }
}

fn emit_auth_placement(auth_use: &AuthUseIr) -> TokenStream2 {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => quote! { ::concord_core::prelude::AuthPlacement::Bearer },
        AuthUseKindIr::Header { header, .. } => quote! { ::concord_core::prelude::AuthPlacement::Header(#header) },
        AuthUseKindIr::Query { key, .. } => quote! { ::concord_core::prelude::AuthPlacement::Query(#key) },
        AuthUseKindIr::Basic { .. } => quote! { ::concord_core::prelude::AuthPlacement::Basic },
        AuthUseKindIr::Certificate { .. } => quote! { ::concord_core::prelude::AuthPlacement::Certificate },
    }
}

fn emit_auth_usage_id(auth_use: &AuthUseIr) -> LitStr {
    let value = match &auth_use.kind {
        AuthUseKindIr::Bearer { .. } => "bearer",
        AuthUseKindIr::Header { .. } => "header",
        AuthUseKindIr::Query { .. } => "query",
        AuthUseKindIr::Basic { .. } => "basic",
        AuthUseKindIr::Certificate { .. } => "certificate",
    };
    LitStr::new(value, Span::call_site())
}

fn auth_use_credential_ident_ir(auth_use: &AuthUseIr) -> &Ident {
    match &auth_use.kind {
        AuthUseKindIr::Bearer { credential }
        | AuthUseKindIr::Header { credential, .. }
        | AuthUseKindIr::Query { credential, .. }
        | AuthUseKindIr::Basic { credential }
        | AuthUseKindIr::Certificate { credential } => credential,
    }
}
