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
                    const _: fn() = || {
                        fn __concord_assert_request_representation<R, Input>()
                        where
                            R: #facade_path::body::RequestRepresentation<Input>,
                            Input: ::std::marker::Send + 'static,
                        {}
                        let _ = __concord_assert_request_representation::<#body_spec, #input>;
                    };
                }
            });
            quote! {
                #body
                const _: fn() = || {
                    fn __concord_assert_response_representation<R, Output>()
                    where
                        R: #facade_path::response::ResponseRepresentation<Output>,
                        Output: ::std::marker::Send + 'static,
                    {}
                    let _ = __concord_assert_response_representation::<#response, #output>;
                };
            }
        }
        Child::Scope(scope) => {
            generate_specification_assertions(&scope.children, facade_path, specification_module)
        }
    });
    quote!(#(#assertions)*)
}

pub(super) fn body_input_name(endpoint: &Endpoint) -> Ident {
    Ident::new(&format!("BodyInput{}", endpoint.id), Span::mixed_site())
}

pub(super) fn response_output_name(endpoint: &Endpoint) -> Ident {
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

pub(super) fn effective_body_representation(
    endpoint: &Endpoint,
    declared: TokenStream,
    facade_path: &TokenStream,
) -> TokenStream {
    endpoint.body.as_ref().map_or_else(
        || quote!(#facade_path::__private::EmptyBody),
        |body| {
            if matches!(body.presence, ValuePresence::Optional) {
                quote!(#facade_path::__private::OptionalRequestRepresentation<#declared>)
            } else {
                declared
            }
        },
    )
}

pub(super) fn effective_body_input(endpoint: &Endpoint, declared: TokenStream) -> TokenStream {
    endpoint.body.as_ref().map_or_else(
        || quote!(()),
        |body| {
            if matches!(body.presence, ValuePresence::Optional) {
                quote!(::std::option::Option<#declared>)
            } else {
                declared
            }
        },
    )
}

pub(super) fn response_specification_name(endpoint: &Endpoint) -> Ident {
    Ident::new(
        &format!("ResponseRepresentation{}", endpoint.id),
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
                bounds.push(quote_spanned!(*span=> #ty: #facade_path::rate_limit::RateKeyPart));
            }
        }
    }
    let mut atom_bounds = Vec::new();
    collect_config_atom_bounds(
        &endpoint.config,
        client,
        scopes,
        endpoint,
        facade_path,
        &mut atom_bounds,
    );
    bounds.extend(atom_bounds);
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
    let declared_body = endpoint.body.as_ref().map_or_else(
        || quote!(#facade_path::__private::EmptyBody),
        |_| {
            let body_specification = body_specification_name(endpoint);
            quote!(#invocation_path::#specification_module::#body_specification)
        },
    );
    let declared_body_input = endpoint.body.as_ref().map_or_else(
        || quote!(()),
        |_| {
            let input = body_input_name(endpoint);
            quote!(#invocation_path::#specification_module::#input)
        },
    );
    let body = effective_body_representation(endpoint, declared_body, facade_path);
    let body_input = effective_body_input(endpoint, declared_body_input);
    if include_execution {
        if endpoint.pagination.is_some() {
            if endpoint.body.is_some() {
                let declared = body_specification_name(endpoint);
                let input = body_input_name(endpoint);
                bounds.push(quote!(
                    #invocation_path::#specification_module::#declared:
                        #facade_path::body::PageRequestRepresentation<
                            #invocation_path::#specification_module::#input
                        >
                ));
            } else {
                bounds.push(
                    quote!(#body: #facade_path::body::PageRequestRepresentation<#body_input>),
                );
            }
        } else {
            if endpoint.body.is_some() {
                let declared = body_specification_name(endpoint);
                let input = body_input_name(endpoint);
                bounds.push(quote!(
                    #invocation_path::#specification_module::#declared:
                        #facade_path::body::RequestRepresentation<
                            #invocation_path::#specification_module::#input
                        >
                ));
            } else {
                bounds.push(quote!(#body: #facade_path::body::RequestRepresentation<#body_input>));
            }
        }
    }
    let response_specification = response_specification_name(endpoint);
    let response = quote!(#invocation_path::#specification_module::#response_specification);
    let response_output = response_output_name(endpoint);
    let output = quote!(#invocation_path::#specification_module::#response_output);
    if include_execution {
        bounds.push(quote!(#response: #facade_path::response::ResponseRepresentation<#output>));
        if endpoint.expose_accepted_bytes {
            bounds.push(quote!(
                <#response as #facade_path::response::ResponseRepresentation<#output>>::Driver:
                    #facade_path::response::AcceptedBytesDriver
            ));
        }
        if let Some(pagination) = &endpoint.pagination {
            let page = quote!(#output);
            let controller =
                pagination_controller_type(&pagination.controller, invocation_path, false);
            bounds
                .push(quote!(#response: #facade_path::response::PageResponseRepresentation<#page>));
            bounds.push(quote!(#page: ::std::marker::Send + 'static));
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

fn collect_config_atom_bounds(
    config: &NormalizedConfig,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    facade_path: &TokenStream,
    output: &mut Vec<TokenStream>,
) {
    for expression in config.host.iter().chain(config.path.iter()) {
        collect_expression_atom_bounds(
            expression,
            QueryCardinality::One,
            client,
            scopes,
            endpoint,
            facade_path,
            output,
        );
    }
    for entry in &config.query {
        collect_expression_atom_bounds(
            &entry.value,
            entry.cardinality,
            client,
            scopes,
            endpoint,
            facade_path,
            output,
        );
    }
    for entry in &config.headers {
        collect_expression_atom_bounds(
            &entry.value,
            entry.cardinality,
            client,
            scopes,
            endpoint,
            facade_path,
            output,
        );
    }
}

fn collect_expression_atom_bounds(
    expression: &ResolvedExpr,
    cardinality: QueryCardinality,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    facade_path: &TokenStream,
    output: &mut Vec<TokenStream>,
) {
    match expression {
        ResolvedExpr::Value(value) => {
            let ty = resolved_value_payload_type(value.reference, client, scopes, endpoint);
            match cardinality {
                QueryCardinality::One => {
                    output.push(quote!(#ty: #facade_path::RequestAtom));
                }
                QueryCardinality::Each => {
                    output.push(quote!(#ty: #facade_path::OrderedValues));
                    output.push(quote!(
                        <#ty as #facade_path::OrderedValues>::Item: #facade_path::RequestAtom
                    ));
                }
            }
        }
        ResolvedExpr::Adapted { value, adapter } => {
            if let ResolvedExpr::Value(value) = value.as_ref() {
                let ty = resolved_value_payload_type(value.reference, client, scopes, endpoint);
                match cardinality {
                    QueryCardinality::One => {
                        output.push(quote!(#adapter: #facade_path::AtomEncoder<#ty>));
                    }
                    QueryCardinality::Each => {
                        output.push(quote!(#ty: #facade_path::OrderedValues));
                        output.push(quote!(
                            #adapter: #facade_path::AtomEncoder<
                                <#ty as #facade_path::OrderedValues>::Item
                            >
                        ));
                    }
                }
            }
        }
        ResolvedExpr::Concat(values) => {
            for value in values {
                collect_expression_atom_bounds(
                    value,
                    QueryCardinality::One,
                    client,
                    scopes,
                    endpoint,
                    facade_path,
                    output,
                );
            }
        }
        ResolvedExpr::String(_)
        | ResolvedExpr::Character(_)
        | ResolvedExpr::Boolean(_)
        | ResolvedExpr::Integer { .. } => {}
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
        assert!(output.contains("ResponseRepresentation"));
        assert!(bounds.contains("RequestRepresentation"));
        assert!(bounds.contains("ResponseRepresentation"));
    }

    #[test]
    fn paginated_body_emits_the_trusted_effect_obligation() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client SearchApi {
                timeout 30s;
                base "https://example.test";

            POST search(
                offset: u64,
                body filter: crate::SearchInput via crate::SearchCodec,
            ) -> Vec<u8> via crate::PageCodec {
                effect safe;
                paginate via crate::Controller {
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

        assert!(bounds.contains("PageRequestRepresentation"));
        assert!(!bounds.contains("SearchCodec : :: concord :: body :: RequestRepresentation"));
    }

    #[test]
    fn only_referenced_dimension_types_require_canonical_rate_keys() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values {
                    tenant: crate::TenantKey;
                    unrelated: crate::ClientWithoutDisplay;
                }

            GET read(
                resource: crate::ResourceKey,
                unrelated: crate::EndpointWithoutDisplay,
            ) -> Bytes via Binary {
                rate_limit { id "concord.assertions.1"; cost 1;
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

        assert!(bounds.contains("crate :: TenantKey : :: concord :: rate_limit :: RateKeyPart"));
        assert!(bounds.contains("crate :: ResourceKey : :: concord :: rate_limit :: RateKeyPart"));
        assert!(!bounds.contains("ClientWithoutDisplay : :: concord :: rate_limit :: RateKeyPart"));
        assert!(
            !bounds.contains("EndpointWithoutDisplay : :: concord :: rate_limit :: RateKeyPart")
        );
        assert!(!bounds.contains("std :: fmt :: Display"));
    }

    #[test]
    fn named_binding_types_receive_only_the_canonical_key_obligation() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { unrelated: crate::ClientWithoutDisplay; }
                rate_limits {
                    policy account_reads(account) { id "concord.assertions.2"; cost 1;
                        key [account];
                        allow 1 per 1s;
                    }
                }

            GET read(
                resource: crate::ResourceKey,
                unrelated: crate::EndpointWithoutDisplay,
            ) -> Bytes via Binary {
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

        assert!(bounds.contains("crate :: ResourceKey : :: concord :: rate_limit :: RateKeyPart"));
        assert!(!bounds.contains("ClientWithoutDisplay : :: concord :: rate_limit :: RateKeyPart"));
        assert!(
            !bounds.contains("EndpointWithoutDisplay : :: concord :: rate_limit :: RateKeyPart")
        );
        assert!(!bounds.contains("std :: fmt :: Display"));
        assert!(!bounds.contains("Clone"));
        assert!(!bounds.contains("Serialize"));
    }
}
