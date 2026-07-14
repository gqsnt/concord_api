fn emit_endpoint_auth_plan(resolved_api: &ResolvedApi, ep: &ResolvedEndpoint) -> TokenStream2 {
    if ep.policy.auth.is_empty() {
        return quote! { ::concord_core::__private::GeneratedAuthBuilder::new() };
    }
    let requirements = ep
        .policy
        .auth
        .iter()
        .map(|req| emit_auth_requirement(resolved_api, req));
    quote! {{
        let mut __auth = ::concord_core::__private::GeneratedAuthBuilder::new();
        #( #requirements )*
        __auth
    }}
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
    let challenge = match requirement.challenge {
        crate::sema::AuthChallengePolicyIr::Unauthorized => {
            quote! { ::concord_core::__private::GeneratedChallengePolicy::Unauthorized }
        }
        crate::sema::AuthChallengePolicyIr::UnauthorizedOrForbidden => {
            quote! { ::concord_core::__private::GeneratedChallengePolicy::UnauthorizedOrForbidden }
        }
        crate::sema::AuthChallengePolicyIr::NeverRecover => {
            quote! { ::concord_core::__private::GeneratedChallengePolicy::NeverRecover }
        }
    };
    quote! {
        __auth.require(
            #client_ns,
            #credential_name,
            #placement,
            #usage_id,
            #step_id,
            #provenance,
            #challenge,
        );
    }
}

fn emit_auth_placement(placement: &AuthPlacementIr) -> TokenStream2 {
    match placement {
        AuthPlacementIr::Bearer => quote! { ::concord_core::__private::GeneratedAuthPlacement::Bearer },
        AuthPlacementIr::Header { name } => {
            quote! { ::concord_core::__private::GeneratedAuthPlacement::Header(#name) }
        }
        AuthPlacementIr::Query { key } => {
            quote! { ::concord_core::__private::GeneratedAuthPlacement::Query(#key) }
        }
        AuthPlacementIr::Basic => quote! { ::concord_core::__private::GeneratedAuthPlacement::Basic },
    }
}
