fn emit_endpoint_auth_plan(resolved_api: &ResolvedApi, ep: &ResolvedEndpoint) -> TokenStream2 {
    if ep.policy.auth.is_empty() {
        return quote! { ::concord_core::advanced::AuthPlan::default() };
    }
    let requirements = ep
        .policy
        .auth
        .iter()
        .map(|req| emit_auth_requirement(resolved_api, req));
    quote! {
        ::concord_core::advanced::AuthPlan {
            requirements: ::std::vec![ #( #requirements ),* ],
        }
    }
}

fn emit_auth_requirement(
    resolved_api: &ResolvedApi,
    requirement: &AuthRequirementIr,
) -> TokenStream2 {
    let client_ns = LitStr::new(&resolved_api.client_name.to_string(), resolved_api.client_name.span());
    let credential_name = LitStr::new(&requirement.credential.to_string(), requirement.credential.span());
    let usage_id = LitStr::new(&requirement.usage_id, Span::call_site());
    let step_id = LitStr::new(&requirement.step_id, Span::call_site());
    let provenance = LitStr::new(&requirement.provenance.label, Span::call_site());
    let placement = emit_auth_placement(&requirement.placement);
    quote! {
        ::concord_core::advanced::AuthRequirement {
            credential: ::concord_core::advanced::CredentialRef {
                id: ::concord_core::advanced::CredentialId::new(#client_ns, #credential_name),
            },
            placement: #placement,
            usage_id: ::concord_core::advanced::AuthUsageId::new(#usage_id),
            step_id: ::core::option::Option::Some(#step_id),
            provenance: ::concord_core::advanced::AuthProvenance::new(#provenance),
            challenge: ::concord_core::advanced::AuthChallengePolicy::Default,
        }
    }
}

fn emit_auth_placement(placement: &AuthPlacementIr) -> TokenStream2 {
    match placement {
        AuthPlacementIr::Bearer => quote! { ::concord_core::advanced::AuthPlacement::Bearer },
        AuthPlacementIr::Header { name } => {
            quote! { ::concord_core::advanced::AuthPlacement::Header(#name) }
        }
        AuthPlacementIr::Query { key } => {
            quote! { ::concord_core::advanced::AuthPlacement::Query(#key) }
        }
        AuthPlacementIr::Basic => quote! { ::concord_core::advanced::AuthPlacement::Basic },
    }
}
