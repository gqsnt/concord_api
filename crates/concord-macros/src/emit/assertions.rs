use super::*;

pub(super) fn generate_specification_aliases(children: &[Child]) -> TokenStream {
    let aliases = children
        .iter()
        .map(|child| match child {
            Child::Endpoint(endpoint) => {
                let response_name = response_specification_name(endpoint);
                let response_output_name = response_output_name(endpoint);
                let response = &endpoint.response_spec;
                let output = &endpoint.output;
                let body = endpoint.body.as_ref().map(|body| {
                    let body_name = body_specification_name(endpoint);
                    let body_input_name = body_input_name(endpoint);
                    let body_spec = &body.spec_type;
                    let input = &body.input_type;
                    quote! {
                        #[doc(hidden)] pub type #body_name = #body_spec;
                        #[doc(hidden)] pub type #body_input_name = #input;
                    }
                });
                quote! {
                    #body
                    #[doc(hidden)]
                    pub type #response_name = #response;
                    #[doc(hidden)]
                    pub type #response_output_name = #output;
                }
            }
            Child::Scope(scope) => generate_specification_aliases(&scope.children),
        })
        .collect::<Vec<_>>();
    quote!(#(#aliases)*)
}

pub(super) fn generate_specification_assertions(
    children: &[Child],
    facade_path: &TokenStream,
    specification_module: &Ident,
) -> TokenStream {
    let assertions = children.iter().map(|child| match child {
        Child::Endpoint(endpoint) => {
            let response_name = response_specification_name(endpoint);
            let response_output_name = response_output_name(endpoint);
            let response = quote!(#specification_module::#response_name);
            let output = quote!(#specification_module::#response_output_name);
            let body = endpoint.body.as_ref().map(|body| {
                let _ = body;
                let body_name = body_specification_name(endpoint);
                let body_input_name = body_input_name(endpoint);
                let body_spec = quote!(#specification_module::#body_name);
                let input = quote!(#specification_module::#body_input_name);
                quote! {
                    const _: fn(
                        <#body_spec as #facade_path::body::RequestBodySpec>::Input
                    ) -> #input = |value| value;
                    const _: fn(#input) ->
                        <#body_spec as #facade_path::body::RequestBodySpec>::Input =
                        |value| value;
                }
            });
            quote! {
                #body
                const _: fn(
                    <#response as #facade_path::response::ResponseSpec>::Output
                ) -> #output = |value| value;
                const _: fn(#output) ->
                    <#response as #facade_path::response::ResponseSpec>::Output =
                    |value| value;
            }
        }
        Child::Scope(scope) => {
            generate_specification_assertions(&scope.children, facade_path, specification_module)
        }
    });
    quote!(#(#assertions)*)
}

fn body_input_name(endpoint: &Endpoint) -> Ident {
    Ident::new(&format!("BodyInput{}", endpoint.id), Span::mixed_site())
}

fn response_output_name(endpoint: &Endpoint) -> Ident {
    Ident::new(
        &format!("ResponseOutput{}", endpoint.id),
        Span::mixed_site(),
    )
}

pub(super) fn specification_module_name(client: &Client) -> Ident {
    Ident::new(
        &format!(
            "__concord_{}_specifications",
            to_snake_case(&canonical_ident(&client.name))
        ),
        Span::mixed_site(),
    )
}

pub(super) fn body_specification_name(endpoint: &Endpoint) -> Ident {
    Ident::new(
        &format!("BodySpecification{}", endpoint.id),
        Span::mixed_site(),
    )
}

pub(super) fn response_specification_name(endpoint: &Endpoint) -> Ident {
    Ident::new(
        &format!("ResponseSpecification{}", endpoint.id),
        Span::mixed_site(),
    )
}

pub(super) fn endpoint_where_clause(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    facade_path: &TokenStream,
    invocation_path: &TokenStream,
    include_execution: bool,
) -> TokenStream {
    let mut bounds = Vec::new();
    if include_execution {
        for value in client
            .values
            .iter()
            .chain(scopes.iter().flat_map(|scope| scope.parameters.iter()))
            .chain(endpoint.parameters.iter())
        {
            let ty = &value.effective_type;
            bounds.push(quote!(#ty: ::std::marker::Send + ::std::marker::Sync + 'static));
        }
    }
    for rule in &endpoint.config.rate_limits {
        for dimension in &rule.dimensions {
            if let NormalizedRateLimitDimension::Value {
                reference, span, ..
            } = dimension
            {
                let ty = resolved_value_payload_type(*reference, client, scopes, endpoint);
                bounds
                    .push(quote_spanned!(*span=> #ty: #facade_path::rate_limit::RateLimitKeyValue));
            }
        }
    }
    let mut display_types = Vec::new();
    collect_config_display_types(
        &endpoint.config,
        client,
        scopes,
        endpoint,
        &mut display_types,
    );
    for ty in display_types {
        bounds.push(quote!(#ty: ::std::fmt::Display));
    }
    for placement in &endpoint.config.auth {
        if matches!(placement.kind, ResolvedAuthPlacement::Authorization(_)) {
            let credential = &client.credentials[placement.credential];
            let material = credential_material_type(client, credential, facade_path);
            bounds.push(quote_spanned! {
                placement.span=>
                #material: #facade_path::auth::SecretCredential
            });
        }
    }
    let specification_module = specification_module_name(client);
    let body = endpoint.body.as_ref().map_or_else(
        || quote!(#facade_path::__private::EmptyBody),
        |_| {
            let body_specification = body_specification_name(endpoint);
            quote!(#invocation_path::#specification_module::#body_specification)
        },
    );
    if include_execution {
        if endpoint.pagination.is_some() {
            bounds.push(quote!(#body: #facade_path::body::PageRequestBodySpec));
        } else {
            bounds.push(quote!(#body: #facade_path::body::RequestBodySpec));
        }
    }
    let response_specification = response_specification_name(endpoint);
    let response = quote!(#invocation_path::#specification_module::#response_specification);
    if include_execution {
        bounds.push(quote!(#response: #facade_path::response::ResponseSpec));
        if endpoint.expose_accepted_bytes {
            bounds.push(quote!(
                <#response as #facade_path::response::ResponseSpec>::Mode:
                    #facade_path::response::AcceptedBytesMode
            ));
        }
        if let Some(pagination) = &endpoint.pagination {
            let page = quote!(<#response as #facade_path::response::ResponseSpec>::Output);
            let controller =
                pagination_controller_type(&pagination.controller, invocation_path, false);
            bounds.push(quote!(#response: #facade_path::response::PageResponseSpec));
            bounds.push(quote!(#page: ::std::iter::IntoIterator));
            bounds.push(quote! {
                <#page as ::std::iter::IntoIterator>::Item:
                    ::std::marker::Send + 'static
            });
            bounds.push(quote! {
                #controller: #facade_path::pagination::PaginationController<#page>
            });
        }
    }
    let mut seen = HashSet::new();
    bounds.retain(|bound| seen.insert(bound.to_string()));
    if bounds.is_empty() {
        quote!()
    } else {
        quote!(where #(#bounds,)*)
    }
}

fn collect_config_display_types(
    config: &NormalizedConfig,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    output: &mut Vec<Type>,
) {
    for expression in config.host.iter().chain(config.path.iter()) {
        collect_expression_display_types(expression, client, scopes, endpoint, false, output);
    }
    for entry in &config.query {
        if matches!(entry.cardinality, QueryCardinality::One) {
            collect_expression_display_types(&entry.value, client, scopes, endpoint, false, output);
        }
    }
    for entry in &config.headers {
        collect_expression_display_types(&entry.value, client, scopes, endpoint, true, output);
    }
}

fn collect_expression_display_types(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    _collection_aware: bool,
    output: &mut Vec<Type>,
) {
    match expression {
        ResolvedExpr::Value(value) => {
            output.push(
                resolved_value_payload_type(value.reference, client, scopes, endpoint).clone(),
            );
        }
        ResolvedExpr::Concat(values) => {
            for value in values {
                collect_expression_display_types(value, client, scopes, endpoint, false, output);
            }
        }
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) => {}
    }
}

fn resolved_value_payload_type<'a>(
    reference: ValueReference,
    client: &'a Client,
    scopes: &[&'a Scope],
    endpoint: &'a Endpoint,
) -> &'a Type {
    match reference {
        ValueReference::Client(index) => &client.values[index].payload_type,
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            &scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("scope reference")
                .parameters[parameter]
                .payload_type
        }
        ValueReference::Endpoint(index) => &endpoint.parameters[index].payload_type,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_private_specification_aliases_and_trait_assertions() {
        let api = test_support::api();
        let output = generate_specification_aliases(&api.client.children).to_string();
        let endpoint = test_support::endpoint(&api);
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
            true,
        )
        .to_string();

        assert!(output.contains("pub type"));
        assert!(output.contains("ResponseSpecification"));
        assert!(bounds.contains("RequestBodySpec"));
        assert!(bounds.contains("ResponseSpec"));
    }

    #[test]
    fn paginated_body_emits_the_trusted_repeatability_obligation() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client SearchApi {
                timeout off;
                base "https://example.test";

            POST search(
                offset: u64,
                body filter: crate::SearchInput using crate::SearchCodec,
            ) -> Vec<u8> using crate::PageCodec {
                paginate with crate::Controller {
                    offset -> endpoint.offset;
                }
            }
        }});
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "search");
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
            true,
        )
        .to_string();

        assert!(bounds.contains("PageRequestBodySpec"));
        assert!(!bounds.contains("SearchCodec : :: concord :: body :: RequestBodySpec"));
    }

    #[test]
    fn only_referenced_dimension_types_require_canonical_rate_keys() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                values {
                    tenant: crate::TenantKey;
                    unrelated: crate::ClientWithoutDisplay;
                }

            GET read(
                resource: crate::ResourceKey,
                unrelated: crate::EndpointWithoutDisplay,
            ) -> Bytes using Bytes {
                rate_limit {
                    key [client.tenant, endpoint.resource];
                    allow 1 per 1s;
                }
            }
        }});
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
            false,
        )
        .to_string();

        assert!(
            bounds.contains("crate :: TenantKey : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(
            bounds.contains("crate :: ResourceKey : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(
            !bounds
                .contains("ClientWithoutDisplay : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(
            !bounds
                .contains("EndpointWithoutDisplay : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(!bounds.contains("std :: fmt :: Display"));
    }

    #[test]
    fn named_binding_types_receive_only_the_canonical_key_obligation() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                values { unrelated: crate::ClientWithoutDisplay; }
                rate_limits {
                    account_reads(account) {
                        key [account];
                        allow 1 per 1s;
                    }
                }

            GET read(
                resource: crate::ResourceKey,
                unrelated: crate::EndpointWithoutDisplay,
            ) -> Bytes using Bytes {
                rate_limit account_reads(account = endpoint.resource);
            }
        }});
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
            false,
        )
        .to_string();

        assert!(
            bounds.contains("crate :: ResourceKey : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(
            !bounds
                .contains("ClientWithoutDisplay : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(
            !bounds
                .contains("EndpointWithoutDisplay : :: concord :: rate_limit :: RateLimitKeyValue")
        );
        assert!(!bounds.contains("std :: fmt :: Display"));
        assert!(!bounds.contains("Clone"));
        assert!(!bounds.contains("Serialize"));
    }
}
