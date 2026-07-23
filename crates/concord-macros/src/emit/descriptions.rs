use super::*;

pub(super) fn generate_endpoint_description(
    endpoint: &Endpoint,
    response: &TokenStream,
    core_path: &TokenStream,
) -> (Ident, TokenStream) {
    let description_static = private_ident("endpoint_description", endpoint.id, &endpoint.name);
    let contract_id = &endpoint.contract_id.value;
    let source_path = syn::LitStr::new(&endpoint.surface.docs_identity, endpoint.name.span());
    let route = syn::LitStr::new(&route_shape(&endpoint.config.path), endpoint.name.span());
    let method = method_token(&endpoint.method, core_path);
    let timeout = endpoint.config.timeout.map_or_else(
        || quote!(#core_path::advanced::TimeoutSelection::Disabled),
        |duration| {
            let seconds = duration.as_secs();
            let nanos = duration.subsec_nanos();
            quote! {
                #core_path::advanced::TimeoutSelection::Explicit(
                    ::std::time::Duration::new(#seconds, #nanos)
                )
            }
        },
    );
    let body_category = endpoint.body.as_ref().map_or_else(
        || quote!(#core_path::advanced::RequestBodyCategory::Empty),
        |_| quote!(#core_path::advanced::RequestBodyCategory::Specification),
    );
    let body_specification = endpoint.body.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |body| {
            let identity = syn::LitStr::new(&body.ty.to_token_stream().to_string(), body.ty.span());
            quote!(::std::option::Option::Some(#identity))
        },
    );
    let pagination = endpoint.pagination.is_some();
    let credential_storage = !endpoint.surface.terminals.credential_storage.is_empty();
    let authentication = !endpoint.config.auth.is_empty();
    let rate_limit = !endpoint.config.rate_limits.is_empty();
    let tokens = quote! {
        static #description_static:
            ::std::sync::LazyLock<#core_path::advanced::EndpointDescription> =
            ::std::sync::LazyLock::new(|| {
                let __concord_response_acquisition =
                    <<#response as #core_path::advanced::ResponseBodySpec>::Mode
                    as #core_path::advanced::ResponseMode>::ACQUISITION;
                #core_path::advanced::EndpointDescription::generated(
                    #core_path::advanced::EndpointId::from_static(#contract_id),
                    #method,
                    #source_path,
                    #route,
                    #body_category,
                    #body_specification,
                    __concord_response_acquisition,
                    #core_path::advanced::EndpointCapabilities::generated(
                        #pagination,
                        #credential_storage,
                        __concord_response_acquisition.raw_response_eligible(),
                        #authentication,
                        #rate_limit,
                    ),
                    #timeout,
                )
            });
    };
    (description_static, tokens)
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_only_safe_semantic_description_inputs() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let (_, tokens) = generate_endpoint_description(
            endpoint,
            &quote!(crate::ResponseSpec),
            &test_support::core_path(),
        );
        let output = tokens.to_string();

        assert!(output.contains("EndpointDescription :: generated"));
        assert!(output.contains("EndpointId :: from_static"));
        assert!(output.contains("EndpointCapabilities :: generated"));
        assert!(!output.contains("bearer_value"));
    }
}
