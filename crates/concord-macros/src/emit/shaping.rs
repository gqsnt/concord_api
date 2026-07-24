use super::*;

pub(super) fn host_statements(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let pagination_values = pagination_continuation_values(endpoint);
    let statements = endpoint
        .config
        .host
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(literal) => {
                quote!(__concord_builder.push_host_static(#literal)?;)
            }
            _ => {
                let sensitive = resolved_expr_uses_any_value(value, &pagination_values);
                let value = display_argument(value, client, scopes, endpoint);
                if sensitive {
                    quote!(__concord_builder.push_pagination_host_value(#value)?;)
                } else {
                    quote!(__concord_builder.push_host_value(#value)?;)
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn pagination_continuation_values(endpoint: &Endpoint) -> Vec<ValueReference> {
    let mut values = Vec::new();
    if let Some(pagination) = &endpoint.pagination {
        for field in &pagination.fields {
            if let PaginationInitializer::Bound(value) = &field.initializer {
                if !values.contains(&value.reference) {
                    values.push(value.reference);
                }
            }
        }
    }
    values
}

pub(super) fn resolved_expr_uses_any_value(
    expression: &ResolvedExpr,
    values: &[ValueReference],
) -> bool {
    match expression {
        ResolvedExpr::Value(value) => values.contains(&value.reference),
        ResolvedExpr::Format(parts) => parts
            .iter()
            .any(|part| resolved_expr_uses_any_value(part, values)),
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) => false,
    }
}

pub(super) fn path_statements(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let pagination_values = pagination_continuation_values(endpoint);
    let statements = endpoint
        .config
        .path
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(literal) => quote!(__concord_builder.push_static(#literal)?;),
            _ => {
                let sensitive = resolved_expr_uses_any_value(value, &pagination_values);
                let value = display_argument(value, client, scopes, endpoint);
                if sensitive {
                    quote!(__concord_builder.push_pagination_value(#value)?;)
                } else {
                    quote!(__concord_builder.push_value(#value)?;)
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn query_statements(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let pagination_values = pagination_continuation_values(endpoint);
    let statements = endpoint
        .config
        .query
        .iter()
        .map(|entry| {
            let key = &entry.key;
            let sensitive = resolved_expr_uses_any_value(&entry.value, &pagination_values);
            let push = if sensitive {
                quote!(__concord_builder.push_pagination_query_value)
            } else {
                quote!(__concord_builder.push_query_value)
            };
            if let ResolvedExpr::Value(value) = &entry.value {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                match &value.shape {
                    ValueShape::Scalar => {
                        quote!(#push(#key, #access)?;)
                    }
                    ValueShape::Option { .. } => quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #push(#key, value)?;
                        }
                    },
                    ValueShape::Vec { .. } => quote! {
                        for value in #access.iter() {
                            #push(#key, value)?;
                        }
                    },
                    ValueShape::OptionVec { .. } => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            for value in values.iter() {
                                #push(#key, value)?;
                            }
                        }
                    },
                }
            } else {
                let value = display_argument(&entry.value, client, scopes, endpoint);
                quote!(#push(#key, #value)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn header_statements(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let pagination_values = pagination_continuation_values(endpoint);
    let statements = endpoint
        .config
        .headers
        .iter()
        .map(|entry| {
            let name = &entry.name;
            let sensitive = resolved_expr_uses_any_value(&entry.value, &pagination_values);
            let insert = if sensitive {
                quote!(__concord_builder.insert_pagination_header_value)
            } else {
                quote!(__concord_builder.insert_header_value)
            };
            if let ResolvedExpr::Value(value) = &entry.value {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                match &value.shape {
                    ValueShape::Option { .. } => quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #insert(#name, value)?;
                        }
                    },
                    ValueShape::Scalar => {
                        quote!(#insert(#name, #access)?;)
                    }
                    ValueShape::Vec { .. } | ValueShape::OptionVec { .. } => {
                        unreachable!("resolver rejects vector public headers")
                    }
                }
            } else {
                let value = display_argument(&entry.value, client, scopes, endpoint);
                quote!(#insert(#name, #value)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn auth_statements(
    client: &Client,
    endpoint: &Endpoint,
    core_path: &TokenStream,
) -> TokenStream {
    if endpoint.config.auth.is_empty() {
        return quote!();
    }
    let statements = endpoint
        .config
        .auth
        .iter()
        .map(|placement| {
            let credential = &client.credentials[placement.credential];
            let field = credential_store_field(placement.credential, &credential.name);
            let reference = quote!(__concord_credentials.#field.credential());
            let material = credential_material_type(client, credential, core_path);
            let source_span = match &credential.source {
                CredentialSource::Custom { provider } => provider.span(),
                _ => credential.name.span(),
            };
            let (constructor, fallible) = match &placement.kind {
                ResolvedAuthPlacement::Bearer => (
                    quote_spanned!(source_span=>
                        #core_path::__private::Auth::bearer::<#material>(#reference)
                    ),
                    false,
                ),
                ResolvedAuthPlacement::Basic => (
                    quote_spanned!(source_span=>
                        #core_path::__private::Auth::basic(#reference)
                    ),
                    false,
                ),
                ResolvedAuthPlacement::Header(name) => {
                    let canonical =
                        syn::LitStr::new(&name.value().to_ascii_lowercase(), name.span());
                    (
                        quote_spanned!(source_span=>
                            #core_path::__private::Auth::header::<#material>(
                                #core_path::__private::HeaderName::from_static(#canonical),
                                #reference,
                            )
                        ),
                        true,
                    )
                }
                ResolvedAuthPlacement::Query(name) => (
                    quote_spanned!(source_span=>
                        #core_path::__private::Auth::query::<#material>(#name, #reference)
                    ),
                    true,
                ),
            };
            let policy = match placement.challenge {
                ResolvedChallengePolicy::Unauthorized => quote!(
                    #core_path::auth::ChallengePolicy::Unauthorized
                ),
                ResolvedChallengePolicy::UnauthorizedOrForbidden => quote!(
                    #core_path::auth::ChallengePolicy::UnauthorizedOrForbidden
                ),
                ResolvedChallengePolicy::Never => {
                    quote!(#core_path::auth::ChallengePolicy::Never)
                }
            };
            if fallible {
                quote! {
                    __concord_builder.push_auth_result(
                        (#constructor).map(|auth| auth.challenge(#policy))
                    )?;
                }
            } else {
                quote! {
                    __concord_builder.push_auth((#constructor).challenge(#policy))?;
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn rate_limit_statements(
    client: &Client,
    _scopes: &[&Scope],
    endpoint: &Endpoint,
    _core_path: &TokenStream,
    identity_path: &TokenStream,
) -> TokenStream {
    let rate_limit_ids = rate_limit_identity_ids(client);
    let statements = endpoint
        .config
        .rate_limits
        .iter()
        .map(|rule| {
            let identity =
                rate_limit_identity_const(&rate_limit_ids, &rule.id, &client.name.to_string());
            let cost = rule.cost;
            let windows = rule
                .windows
                .iter()
                .map(|window| {
                    let capacity = window.capacity;
                    let seconds = window.duration.as_secs();
                    let nanos = window.duration.subsec_nanos();
                    quote!((
                        #capacity,
                        ::std::time::Duration::new(#seconds, #nanos),
                    ))
                })
                .collect::<Vec<_>>();
            quote! {
                __concord_builder.push_generated_rate_limit_rule(
                    #identity_path::#identity,
                    #cost,
                    [#(#windows,)*],
                )?;
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn rate_limit_identity_constants(client: &Client) -> TokenStream {
    let ids = rate_limit_identity_ids(client);
    let constants = ids.into_iter().enumerate().map(|(ordinal, id)| {
        let identity = rate_limit_identity_const_for_ordinal(ordinal, &client.name.to_string());
        let id = syn::LitStr::new(&id, Span::call_site());
        quote! {
            #[doc(hidden)]
            const #identity: &str = #id;
        }
    });
    quote!(#(#constants)*)
}

pub(super) fn rate_limit_identity_ids(client: &Client) -> Vec<String> {
    let mut ids = Vec::new();
    collect_rate_limit_ids(&client.config, &mut ids);
    collect_child_rate_limit_ids(&client.children, &mut ids);
    ids
}

pub(super) fn collect_child_rate_limit_ids(children: &[Child], ids: &mut Vec<String>) {
    for child in children {
        match child {
            Child::Scope(scope) => {
                collect_rate_limit_ids(&scope.config, ids);
                collect_child_rate_limit_ids(&scope.children, ids);
            }
            Child::Endpoint(endpoint) => collect_rate_limit_ids(&endpoint.config, ids),
        }
    }
}

pub(super) fn collect_rate_limit_ids(config: &NormalizedConfig, ids: &mut Vec<String>) {
    for rule in &config.rate_limits {
        if !ids.iter().any(|existing| existing == &rule.id) {
            ids.push(rule.id.clone());
        }
    }
}

pub(super) fn rate_limit_identity_const(ids: &[String], id: &str, client_name: &str) -> Ident {
    let ordinal = ids
        .iter()
        .position(|candidate| candidate == id)
        .expect("generated rate-limit identity must have a root constant");
    rate_limit_identity_const_for_ordinal(ordinal, client_name)
}

pub(super) fn rate_limit_identity_const_for_ordinal(ordinal: usize, client_name: &str) -> Ident {
    Ident::new(
        &format!("__CONCORD_RATE_ID_{client_name}_{ordinal}"),
        Span::mixed_site(),
    )
}

pub(super) fn display_argument(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(&#value),
        ResolvedExpr::Integer(value) => quote!(&#value),
        ResolvedExpr::Value(value) => {
            resolved_value_access(value.reference, client, scopes, endpoint)
        }
        ResolvedExpr::Format(values) => {
            let writes = values
                .iter()
                .map(|value| {
                    let value = display_argument(value, client, scopes, endpoint);
                    quote! {
                        ::std::fmt::Write::write_fmt(
                            &mut __concord_formatted,
                            format_args!("{}", #value),
                        ).expect("formatting into a String is infallible");
                    }
                })
                .collect::<Vec<_>>();
            quote!(&{
                let mut __concord_formatted = ::std::string::String::new();
                #(#writes)*
                __concord_formatted
            })
        }
    }
}

pub(super) fn pagination_owned_value_access(
    reference: ValueReference,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let value = pagination_value_access(reference, client, scopes, endpoint);
    quote!(::std::clone::Clone::clone(#value))
}

pub(super) fn pagination_value_access(
    reference: ValueReference,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match reference {
        ValueReference::Client(index) => {
            let value = &client.values[index];
            let field = client_value_field(index, value);
            quote!((&__concord_client_values.#field))
        }
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            let scope = scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("resolved scope identity appears in the endpoint lexical chain");
            let field = scope_value_field(scope_id, parameter, &scope.parameters[parameter]);
            quote!((&*__concord_endpoint.#field))
        }
        ValueReference::Endpoint(index) => {
            let field = endpoint_value_field(index, &endpoint.parameters[index]);
            quote!((&*__concord_endpoint.#field))
        }
    }
}

pub(super) fn resolved_value_access(
    reference: ValueReference,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match reference {
        ValueReference::Client(index) => {
            let value = &client.values[index];
            let field = client_value_field(index, value);
            quote!((&__concord_client_values.#field))
        }
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            let scope = scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("resolved scope identity appears in the endpoint lexical chain");
            let field = scope_value_field(scope_id, parameter, &scope.parameters[parameter]);
            quote!((&*self.#field))
        }
        ValueReference::Endpoint(index) => {
            let field = endpoint_value_field(index, &endpoint.parameters[index]);
            quote!((&*self.#field))
        }
    }
}

pub(super) fn explicit_parameter_bindings(
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> (Vec<Ident>, Vec<Ident>, Option<Ident>) {
    let scope_values = scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let endpoint_values = endpoint
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let body_name = endpoint
        .body
        .as_ref()
        .map(|body| canonical_ident(&body.name));
    let original = scope_values
        .iter()
        .chain(endpoint_values.iter())
        .map(|value| canonical_ident(&value.name))
        .chain(body_name.iter().cloned())
        .collect::<Vec<_>>();
    let reserved = original.iter().cloned().collect::<HashSet<_>>();
    let mut used = HashSet::new();
    let mut allocate = |namespace: &str, name: &Ident| {
        let original_name = canonical_ident(name);
        if original
            .iter()
            .filter(|candidate| **candidate == original_name)
            .count()
            == 1
            && used.insert(original_name.clone())
        {
            return name.clone();
        }
        let base = format!("{namespace}_{}", to_snake_case(&original_name));
        let mut candidate = base.clone();
        let mut suffix = 2;
        while reserved.contains(&candidate) || !used.insert(candidate.clone()) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        public_ident(&candidate, name.span())
    };
    let scope = scope_values
        .iter()
        .map(|value| allocate("scope", &value.name))
        .collect();
    let endpoint_bindings = endpoint_values
        .iter()
        .map(|value| allocate("endpoint", &value.name))
        .collect();
    let body = endpoint
        .body
        .as_ref()
        .map(|body| allocate("request", &body.name));
    (scope, endpoint_bindings, body)
}

pub(super) fn endpoint_constructor_binding(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_input", index, &value.name)
}

pub(super) fn body_constructor(
    body: Option<&Type>,
    binding: Option<&Ident>,
    _generated_core: &TokenStream,
    _core_path: &TokenStream,
) -> TokenStream {
    match (body, binding) {
        (Some(_), Some(binding)) => quote!(#binding),
        (None, None) => quote!(()),
        _ => unreachable!("body and binding are modeled together"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_public_head_shaping_from_resolved_expressions() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let output = path_statements(&api.client, &[], endpoint).to_string();

        assert!(output.contains("push_value"));
        assert!(output.contains("__concord_client_values"));
    }
}
