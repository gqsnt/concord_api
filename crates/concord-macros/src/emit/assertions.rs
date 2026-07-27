use super::*;

pub(super) fn generate_specification_aliases(children: &[Child]) -> TokenStream {
    let aliases = children
        .iter()
        .map(|child| match child {
            Child::Endpoint(endpoint) => {
                let response_name = response_specification_name(endpoint);
                let response = &endpoint.response;
                let body = endpoint.body.as_ref().map(|body| {
                    let body_name = body_specification_name(endpoint);
                    let body = &body.ty;
                    quote!(#[doc(hidden)] pub type #body_name = #body;)
                });
                quote! {
                    #body
                    #[doc(hidden)]
                    pub type #response_name = #response;
                }
            }
            Child::Scope(scope) => generate_specification_aliases(&scope.children),
        })
        .collect::<Vec<_>>();
    quote!(#(#aliases)*)
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
    core_path: &TokenStream,
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
            let ty = &value.ty;
            bounds.push(quote!(#ty: ::std::marker::Send + ::std::marker::Sync + 'static));
        }
    }
    for rule in &endpoint.config.rate_limits {
        for dimension in &rule.dimensions {
            if let NormalizedRateLimitDimension::Value {
                reference, span, ..
            } = dimension
            {
                let ty = resolved_value_type(*reference, client, scopes, endpoint);
                bounds.push(quote_spanned!(*span=> #ty: ::std::fmt::Display));
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
    let specification_module = specification_module_name(client);
    let body = endpoint.body.as_ref().map_or_else(
        || quote!(#core_path::__private::EmptyBody),
        |_| {
            let body_specification = body_specification_name(endpoint);
            quote!(#invocation_path::#specification_module::#body_specification)
        },
    );
    if include_execution {
        if endpoint.pagination.is_some() {
            bounds.push(quote!(#body: #core_path::body::PageRequestBodySpec));
        } else {
            bounds.push(quote!(#body: #core_path::body::RequestBodySpec));
        }
    }
    let response_specification = response_specification_name(endpoint);
    let response = quote!(#invocation_path::#specification_module::#response_specification);
    if include_execution {
        bounds.push(quote!(#response: #core_path::response::ResponseBodySpec));
        if let Some(pagination) = &endpoint.pagination {
            let page = quote!(<#response as #core_path::response::ResponseBodySpec>::Output);
            let controller =
                pagination_controller_type(&pagination.controller, invocation_path, false);
            bounds.push(quote!(#response: #core_path::response::PageResponseSpec));
            bounds.push(quote!(#page: ::std::iter::IntoIterator));
            bounds.push(quote! {
                <#page as ::std::iter::IntoIterator>::Item:
                    ::std::marker::Send + 'static
            });
            bounds.push(quote! {
                #controller: #core_path::pagination::PaginationController<#page>
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
        collect_expression_display_types(&entry.value, client, scopes, endpoint, true, output);
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
    collection_aware: bool,
    output: &mut Vec<Type>,
) {
    match expression {
        ResolvedExpr::Value(value) => {
            let declared = resolved_value_type(value.reference, client, scopes, endpoint);
            let ty = if collection_aware {
                match &value.shape {
                    ValueShape::Option { inner } => inner.clone(),
                    ValueShape::Vec { element } | ValueShape::OptionVec { element, .. } => {
                        element.clone()
                    }
                    ValueShape::Scalar => declared.clone(),
                }
            } else {
                declared.clone()
            };
            output.push(ty);
        }
        ResolvedExpr::Format(values) => {
            for value in values {
                collect_expression_display_types(value, client, scopes, endpoint, false, output);
            }
        }
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) => {}
    }
}

fn resolved_value_type<'a>(
    reference: ValueReference,
    client: &'a Client,
    scopes: &[&'a Scope],
    endpoint: &'a Endpoint,
) -> &'a Type {
    match reference {
        ValueReference::Client(index) => &client.values[index].ty,
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            &scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("scope reference")
                .parameters[parameter]
                .ty
        }
        ValueReference::Endpoint(index) => &endpoint.parameters[index].ty,
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
            &test_support::core_path(),
            &quote!(super),
            true,
        )
        .to_string();

        assert!(output.contains("pub type"));
        assert!(output.contains("ResponseSpecification"));
        assert!(bounds.contains("RequestBodySpec"));
        assert!(bounds.contains("ResponseBodySpec"));
    }

    #[test]
    fn paginated_body_emits_the_trusted_repeatability_obligation() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client SearchApi {
                base "https://example.test";
            }
            POST search(
                offset: u64,
                #[body] filter: crate::SearchCodec,
            ) -> crate::PageCodec {
                paginate crate::Controller {
                    offset: bind ep.offset,
                }
            }
        });
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "search");
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::core_path(),
            &quote!(super),
            true,
        )
        .to_string();

        assert!(bounds.contains("PageRequestBodySpec"));
        assert!(!bounds.contains("SearchCodec : :: concord :: body :: RequestBodySpec"));
    }

    #[test]
    fn only_referenced_dimension_types_require_display() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                vars {
                    tenant: crate::TenantKey;
                    unrelated: crate::ClientWithoutDisplay;
                }
            }
            GET read(
                resource: crate::ResourceKey,
                unrelated: crate::EndpointWithoutDisplay,
            ) -> Bytes {
                rate_limit {
                    by [vars.tenant, ep.resource];
                    1 / 1s;
                }
            }
        });
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let bounds = endpoint_where_clause(
            &api.client,
            &[],
            endpoint,
            &test_support::core_path(),
            &quote!(super),
            false,
        )
        .to_string();

        assert!(bounds.contains("crate :: TenantKey : :: std :: fmt :: Display"));
        assert!(bounds.contains("crate :: ResourceKey : :: std :: fmt :: Display"));
        assert!(!bounds.contains("ClientWithoutDisplay : :: std :: fmt :: Display"));
        assert!(!bounds.contains("EndpointWithoutDisplay : :: std :: fmt :: Display"));
    }
}
