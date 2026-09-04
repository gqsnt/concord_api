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
                let sensitive = resolved_expr_requires_redaction(value, &pagination_values);
                let encode = atom_closure(value, client, scopes, endpoint);
                quote!(__concord_builder.push_host_atoms(#sensitive, #encode)?;)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn pagination_continuation_values(endpoint: &Endpoint) -> Vec<ValueReference> {
    let mut values = Vec::new();
    if let Some(pagination) = &endpoint.pagination {
        for field in &pagination.fields {
            if let PaginationInitializer::Bound { value, .. } = &field.initializer {
                if !values.contains(&value.reference) {
                    values.push(value.reference);
                }
            }
        }
    }
    values
}

pub(super) fn resolved_expr_requires_redaction(
    expression: &ResolvedExpr,
    pagination_values: &[ValueReference],
) -> bool {
    match expression {
        ResolvedExpr::Value(value) => {
            value.observation == ObservationClass::RedactedPublic
                || pagination_values.contains(&value.reference)
        }
        ResolvedExpr::Concat(parts) => parts
            .iter()
            .any(|part| resolved_expr_requires_redaction(part, pagination_values)),
        ResolvedExpr::Adapted { value, .. } => {
            resolved_expr_requires_redaction(value, pagination_values)
        }
        ResolvedExpr::String(_)
        | ResolvedExpr::Character(_)
        | ResolvedExpr::Boolean(_)
        | ResolvedExpr::Integer { .. } => false,
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
                let sensitive = resolved_expr_requires_redaction(value, &pagination_values);
                let encode = atom_closure(value, client, scopes, endpoint);
                quote!(__concord_builder.push_path_atoms(#sensitive, #encode)?;)
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
            let sensitive = resolved_expr_requires_redaction(&entry.value, &pagination_values);
            if let Some((value, adapter)) = value_and_adapter(&entry.value) {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                let remove = matches!(entry.operation, AttachmentOperation::WhenPresent)
                    .then(|| quote!(__concord_builder.remove_query_values(#key)?;));
                match (entry.cardinality, value.presence, adapter) {
                    (QueryCardinality::One, ValuePresence::Required, None) => {
                        let encode = bound_atom_closure(&access, None);
                        quote! {
                            #remove
                            __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                        }
                    }
                    (QueryCardinality::One, ValuePresence::Required, Some(adapter)) => {
                        let encode = bound_atom_closure(&access, Some(adapter));
                        quote! {
                            #remove
                            __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                        }
                    }
                    (QueryCardinality::One, ValuePresence::Optional, adapter) => {
                        let encode = bound_atom_closure(&quote!(value), adapter);
                        quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #remove
                            __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                        }
                        }
                    }
                    (QueryCardinality::Each, ValuePresence::Required, None) => quote! {
                        #remove
                        __concord_builder.push_query_each(#key, #access, #sensitive)?;
                    },
                    (QueryCardinality::Each, ValuePresence::Required, Some(adapter)) => quote! {
                        #remove
                        __concord_builder.push_query_each_with::<#adapter, _>(
                            #key,
                            #access,
                            #sensitive,
                        )?;
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, None) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            #remove
                            __concord_builder.push_query_each(#key, values, #sensitive)?;
                        }
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, Some(adapter)) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            #remove
                            __concord_builder.push_query_each_with::<#adapter, _>(
                                #key,
                                values,
                                #sensitive,
                            )?;
                        }
                    },
                }
            } else {
                let remove = matches!(entry.operation, AttachmentOperation::WhenPresent)
                    .then(|| quote!(__concord_builder.remove_query_values(#key)?;));
                let encode = atom_closure(&entry.value, client, scopes, endpoint);
                quote! {
                    #remove
                    __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                }
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
            let sensitive = resolved_expr_requires_redaction(&entry.value, &pagination_values);
            let append = matches!(entry.operation, AttachmentOperation::Append);
            let remove_when_present = matches!(entry.operation, AttachmentOperation::WhenPresent)
                .then(|| quote!(__concord_builder.remove_header_values(#name)?;));
            if let Some((value, adapter)) = value_and_adapter(&entry.value) {
                let access = resolved_value_access(value.reference, client, scopes, endpoint);
                match (entry.cardinality, value.presence, adapter, append) {
                    (QueryCardinality::One, ValuePresence::Optional, adapter, append) => {
                        let encode = bound_atom_closure(&quote!(value), adapter);
                        let emit = if append {
                            quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
                        } else {
                            quote!(__concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;)
                        };
                        quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #remove_when_present
                            #emit
                        }
                        }
                    }
                    (QueryCardinality::One, ValuePresence::Required, adapter, append) => {
                        let encode = bound_atom_closure(&access, adapter);
                        let emit = if append {
                            quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
                        } else {
                            quote!(__concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;)
                        };
                        quote! { #remove_when_present #emit }
                    }
                    (QueryCardinality::Each, ValuePresence::Required, None, false) => quote! {
                        #remove_when_present
                        __concord_builder.insert_header_each(#name, #access, #sensitive)?;
                    },
                    (QueryCardinality::Each, ValuePresence::Required, Some(adapter), false) => quote! {
                        #remove_when_present
                        __concord_builder.insert_header_each_with::<#adapter, _>(
                            #name, #access, #sensitive,
                        )?;
                    },
                    (QueryCardinality::Each, ValuePresence::Required, None, true) => quote! {
                        __concord_builder.append_header_each(#name, #access, #sensitive)?;
                    },
                    (QueryCardinality::Each, ValuePresence::Required, Some(adapter), true) => quote! {
                        __concord_builder.append_header_each_with::<#adapter, _>(
                            #name, #access, #sensitive,
                        )?;
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, None, false) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            #remove_when_present
                            __concord_builder.insert_header_each(#name, values, #sensitive)?;
                        }
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, Some(adapter), false) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            #remove_when_present
                            __concord_builder.insert_header_each_with::<#adapter, _>(
                                #name, values, #sensitive,
                            )?;
                        }
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, None, true) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            __concord_builder.append_header_each(#name, values, #sensitive)?;
                        }
                    },
                    (QueryCardinality::Each, ValuePresence::Optional, Some(adapter), true) => quote! {
                        if let ::std::option::Option::Some(values) = #access.as_ref() {
                            __concord_builder.append_header_each_with::<#adapter, _>(
                                #name, values, #sensitive,
                            )?;
                        }
                    },
                }
            } else {
                let encode = atom_closure(&entry.value, client, scopes, endpoint);
                if append {
                    quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
                } else {
                    quote! {
                        #remove_when_present
                        __concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) struct RuntimeScopeHeadStatements {
    pub(super) host: TokenStream,
    pub(super) path: TokenStream,
    pub(super) query: TokenStream,
    pub(super) headers: TokenStream,
    pub(super) auth: TokenStream,
    pub(super) rate_limits: TokenStream,
}

pub(super) fn runtime_scope_head_statements(
    client: &Client,
    scopes: &[&Scope],
    facade_path: &TokenStream,
    identity_path: &TokenStream,
    runtime_endpoint_id: &Ident,
) -> RuntimeScopeHeadStatements {
    let scope = scopes.last().expect("runtime scope binding has an owner");
    let config = &scope.config;
    let host = config.host.iter().map(|value| match value {
        ResolvedExpr::String(literal) => {
            quote!(__concord_builder.push_host_static(#literal)?;)
        }
        _ => {
            let sensitive = resolved_expr_requires_redaction(value, &[]);
            let encode = runtime_scope_atom_closure(value, client, scopes);
            quote!(__concord_builder.push_host_atoms(#sensitive, #encode)?;)
        }
    });
    let path = config.path.iter().map(|value| match value {
        ResolvedExpr::String(literal) => quote!(__concord_builder.push_static(#literal)?;),
        _ => {
            let sensitive = resolved_expr_requires_redaction(value, &[]);
            let encode = runtime_scope_atom_closure(value, client, scopes);
            quote!(__concord_builder.push_path_atoms(#sensitive, #encode)?;)
        }
    });
    let query = config.query.iter().map(|entry| {
        let key = &entry.key;
        let sensitive = resolved_expr_requires_redaction(&entry.value, &[]);
        let replace = matches!(entry.operation, AttachmentOperation::Replace)
            .then(|| quote!(__concord_builder.remove_query_values(#key)?;));
        let clear_on_absence = matches!(entry.operation, AttachmentOperation::Replace)
            .then(|| quote!(__concord_builder.remove_query_values(#key)?;));
        let remove_when_present = matches!(entry.operation, AttachmentOperation::WhenPresent)
            .then(|| quote!(__concord_builder.remove_query_values(#key)?;));
        if let Some((value, adapter)) = value_and_adapter(&entry.value) {
            let access = runtime_scope_value_access(value.reference, client, scopes);
            match (entry.cardinality, value.presence, adapter) {
                (QueryCardinality::One, ValuePresence::Required, adapter) => {
                    let encode = bound_atom_closure(&access, adapter);
                    quote! {
                        #replace
                        #remove_when_present
                        __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                    }
                }
                (QueryCardinality::One, ValuePresence::Optional, adapter) => {
                    let encode = bound_atom_closure(&quote!(value), adapter);
                    quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #replace
                            #remove_when_present
                            __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
                        } else {
                            #clear_on_absence
                        }
                    }
                }
                (QueryCardinality::Each, ValuePresence::Required, None) => quote! {
                    #replace
                    #remove_when_present
                    __concord_builder.push_query_each(#key, #access, #sensitive)?;
                },
                (QueryCardinality::Each, ValuePresence::Required, Some(adapter)) => quote! {
                    #replace
                    #remove_when_present
                    __concord_builder.push_query_each_with::<#adapter, _>(
                        #key, #access, #sensitive,
                    )?;
                },
                (QueryCardinality::Each, ValuePresence::Optional, None) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        #replace
                        #remove_when_present
                        __concord_builder.push_query_each(#key, values, #sensitive)?;
                    }
                },
                (QueryCardinality::Each, ValuePresence::Optional, Some(adapter)) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        #replace
                        #remove_when_present
                        __concord_builder.push_query_each_with::<#adapter, _>(
                            #key, values, #sensitive,
                        )?;
                    }
                },
            }
        } else {
            let encode = runtime_scope_atom_closure(&entry.value, client, scopes);
            quote! {
                #replace
                #remove_when_present
                __concord_builder.push_query_atoms(#key, #sensitive, #encode)?;
            }
        }
    });
    let headers = config.headers.iter().map(|entry| {
        let name = &entry.name;
        let sensitive = resolved_expr_requires_redaction(&entry.value, &[]);
        let append = matches!(entry.operation, AttachmentOperation::Append);
        let remove_when_present = matches!(entry.operation, AttachmentOperation::WhenPresent)
            .then(|| quote!(__concord_builder.remove_header_values(#name)?;));
        if let Some((value, adapter)) = value_and_adapter(&entry.value) {
            let access = runtime_scope_value_access(value.reference, client, scopes);
            match (entry.cardinality, value.presence, adapter, append) {
                (QueryCardinality::One, ValuePresence::Optional, adapter, append) => {
                    let encode = bound_atom_closure(&quote!(value), adapter);
                    let emit = if append {
                        quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
                    } else {
                        quote!(__concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;)
                    };
                    quote! {
                        if let ::std::option::Option::Some(value) = #access.as_ref() {
                            #remove_when_present
                            #emit
                        }
                    }
                }
                (QueryCardinality::One, ValuePresence::Required, adapter, append) => {
                    let encode = bound_atom_closure(&access, adapter);
                    let emit = if append {
                        quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
                    } else {
                        quote!(__concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;)
                    };
                    quote! { #remove_when_present #emit }
                }
                (QueryCardinality::Each, ValuePresence::Required, None, false) => quote! {
                    #remove_when_present
                    __concord_builder.insert_header_each(#name, #access, #sensitive)?;
                },
                (QueryCardinality::Each, ValuePresence::Required, Some(adapter), false) => quote! {
                    #remove_when_present
                    __concord_builder.insert_header_each_with::<#adapter, _>(
                        #name, #access, #sensitive,
                    )?;
                },
                (QueryCardinality::Each, ValuePresence::Required, None, true) => quote! {
                    __concord_builder.append_header_each(#name, #access, #sensitive)?;
                },
                (QueryCardinality::Each, ValuePresence::Required, Some(adapter), true) => quote! {
                    __concord_builder.append_header_each_with::<#adapter, _>(
                        #name, #access, #sensitive,
                    )?;
                },
                (QueryCardinality::Each, ValuePresence::Optional, None, false) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        #remove_when_present
                        __concord_builder.insert_header_each(#name, values, #sensitive)?;
                    }
                },
                (QueryCardinality::Each, ValuePresence::Optional, Some(adapter), false) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        #remove_when_present
                        __concord_builder.insert_header_each_with::<#adapter, _>(
                            #name, values, #sensitive,
                        )?;
                    }
                },
                (QueryCardinality::Each, ValuePresence::Optional, None, true) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        __concord_builder.append_header_each(#name, values, #sensitive)?;
                    }
                },
                (QueryCardinality::Each, ValuePresence::Optional, Some(adapter), true) => quote! {
                    if let ::std::option::Option::Some(values) = #access.as_ref() {
                        __concord_builder.append_header_each_with::<#adapter, _>(
                            #name, values, #sensitive,
                        )?;
                    }
                },
            }
        } else {
            let encode = runtime_scope_atom_closure(&entry.value, client, scopes);
            if append {
                quote!(__concord_builder.append_header_atoms(#name, #sensitive, #encode)?;)
            } else {
                quote! {
                    #remove_when_present
                    __concord_builder.insert_header_atoms(#name, #sensitive, #encode)?;
                }
            }
        }
    });
    let auth = auth_config_statements(client, config, facade_path);
    let rate_limits = runtime_scope_rate_limit_statements(
        client,
        scopes,
        config,
        facade_path,
        identity_path,
        runtime_endpoint_id,
    );
    RuntimeScopeHeadStatements {
        host: quote!(#(#host)*),
        path: quote!(#(#path)*),
        query: quote!(#(#query)*),
        headers: quote!(#(#headers)*),
        auth,
        rate_limits,
    }
}

pub(super) fn accept_statements(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let Some(accept) = &endpoint.accept else {
        return quote!();
    };
    match accept {
        ResolvedAccept::Off => quote!(__concord_builder.suppress_accept();),
        ResolvedAccept::Value(expression) => {
            let pagination_values = pagination_continuation_values(endpoint);
            let redacted = resolved_expr_requires_redaction(expression, &pagination_values);
            let encode = atom_closure(expression, client, scopes, endpoint);
            quote!(__concord_builder.set_accept_atoms(#redacted, #encode)?;)
        }
    }
}

pub(super) fn auth_statements(
    client: &Client,
    endpoint: &Endpoint,
    facade_path: &TokenStream,
) -> TokenStream {
    auth_config_statements(client, &endpoint.config, facade_path)
}

fn auth_config_statements(
    client: &Client,
    config: &NormalizedConfig,
    facade_path: &TokenStream,
) -> TokenStream {
    if config.auth.is_empty() {
        return quote!();
    }
    let statements = config
        .auth
        .iter()
        .map(|placement| {
            let credential = &client.credentials[placement.credential];
            let field = credential_store_field(placement.credential, &credential.name);
            let reference = quote!(__concord_credentials.#field.credential());
            let material = credential_material_type(client, credential, facade_path);
            let source_span = match &credential.source {
                CredentialSource::Custom { provider } => provider.span(),
                _ => credential.name.span(),
            };
            let (constructor, fallible) = match &placement.kind {
                ResolvedAuthPlacement::Bearer => (
                    quote_spanned!(source_span=>
                        #facade_path::__private::Auth::bearer::<#material>(#reference)
                    ),
                    false,
                ),
                ResolvedAuthPlacement::Basic => (
                    quote_spanned!(source_span=>
                        #facade_path::__private::Auth::basic(#reference)
                    ),
                    false,
                ),
                ResolvedAuthPlacement::Authorization(None) => (
                    quote_spanned!(source_span=>
                        #facade_path::__private::Auth::authorization::<#material>(#reference)
                    ),
                    false,
                ),
                ResolvedAuthPlacement::Authorization(Some(scheme)) => (
                    quote_spanned!(source_span=>
                        #facade_path::__private::Auth::authorization_with_scheme::<#material>(
                            #scheme,
                            #reference,
                        )
                    ),
                    true,
                ),
                ResolvedAuthPlacement::Header(name) => {
                    let canonical =
                        syn::LitStr::new(&name.value().to_ascii_lowercase(), name.span());
                    (
                        quote_spanned!(source_span=>
                            #facade_path::__private::Auth::header::<#material>(
                                #facade_path::__private::HeaderName::from_static(#canonical),
                                #reference,
                            )
                        ),
                        true,
                    )
                }
                ResolvedAuthPlacement::Query(name) => (
                    quote_spanned!(source_span=>
                        #facade_path::__private::Auth::query::<#material>(#name, #reference)
                    ),
                    true,
                ),
            };
            let policy = match placement.challenge {
                ResolvedChallengePolicy::Unauthorized => quote!(
                    #facade_path::auth::ChallengePolicy::Unauthorized
                ),
                ResolvedChallengePolicy::UnauthorizedOrForbidden => quote!(
                    #facade_path::auth::ChallengePolicy::UnauthorizedOrForbidden
                ),
                ResolvedChallengePolicy::Never => {
                    quote!(#facade_path::auth::ChallengePolicy::Never)
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
    scopes: &[&Scope],
    endpoint: &Endpoint,
    facade_path: &TokenStream,
    identity_path: &TokenStream,
) -> TokenStream {
    let rate_limit_ids = rate_limit_identity_ids(client);
    let statements = endpoint
        .config
        .rate_limits
        .iter()
        .enumerate()
        .map(|(ordinal, rule)| {
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
            if rule.dimensions.is_empty() {
                return quote! {
                    __concord_builder.push_generated_rate_limit_policy(
                        #identity_path::#identity,
                        #cost,
                        [#(#windows,)*],
                    )?;
                };
            }

            let origin_binding = Ident::new(
                &format!("__concord_rate_limit_origin_{ordinal}"),
                Span::mixed_site(),
            );
            let capture_origin = rule
                .dimensions
                .iter()
                .any(|dimension| matches!(dimension, NormalizedRateLimitDimension::Origin { .. }))
                .then(|| {
                    quote! {
                        let #origin_binding = __concord_builder.rate_limit_origin();
                    }
                });
            let contract_id =
                syn::LitStr::new(&endpoint.contract_id.value, endpoint.contract_id.span);
            let parts = rule
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    NormalizedRateLimitDimension::Origin { .. } => {
                        quote!(("origin", &#origin_binding as &dyn #facade_path::rate_limit::RateKeyPart))
                    }
                    NormalizedRateLimitDimension::Endpoint { span } => {
                        quote_spanned!(*span=>
                            ("endpoint", &#contract_id as &dyn #facade_path::rate_limit::RateKeyPart)
                        )
                    }
                    NormalizedRateLimitDimension::Value {
                        public_name,
                        reference,
                        span,
                    } => {
                        let name = syn::LitStr::new(public_name, *span);
                        let access = resolved_value_access(*reference, client, scopes, endpoint);
                        quote_spanned!(*span=>
                            (#name, #access as &dyn #facade_path::rate_limit::RateKeyPart)
                        )
                    }
                })
                .collect::<Vec<_>>();
            quote! {
                #capture_origin
                __concord_builder.push_rate_limit_policy(
                    #identity_path::#identity,
                    [#(#parts,)*],
                    #cost,
                    [#(#windows,)*],
                )?;
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

fn runtime_scope_rate_limit_statements(
    client: &Client,
    scopes: &[&Scope],
    config: &NormalizedConfig,
    facade_path: &TokenStream,
    identity_path: &TokenStream,
    runtime_endpoint_id: &Ident,
) -> TokenStream {
    let rate_limit_ids = rate_limit_identity_ids(client);
    let statements = config
        .rate_limits
        .iter()
        .enumerate()
        .map(|(ordinal, rule)| {
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
            if rule.dimensions.is_empty() {
                return quote! {
                    __concord_builder.push_generated_rate_limit_policy(
                        #identity_path::#identity,
                        #cost,
                        [#(#windows,)*],
                    )?;
                };
            }
            let origin_binding = Ident::new(
                &format!("__concord_runtime_rate_limit_origin_{ordinal}"),
                Span::mixed_site(),
            );
            let capture_origin = rule
                .dimensions
                .iter()
                .any(|dimension| matches!(dimension, NormalizedRateLimitDimension::Origin { .. }))
                .then(|| quote!(let #origin_binding = __concord_builder.rate_limit_origin();));
            let parts = rule
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    NormalizedRateLimitDimension::Origin { .. } => quote!(
                        ("origin", &#origin_binding as &dyn #facade_path::rate_limit::RateKeyPart)
                    ),
                    NormalizedRateLimitDimension::Endpoint { span } => quote_spanned!(*span=>
                        (
                            "endpoint",
                            &#runtime_endpoint_id
                                as &dyn #facade_path::rate_limit::RateKeyPart,
                        )
                    ),
                    NormalizedRateLimitDimension::Value {
                        public_name,
                        reference,
                        span,
                    } => {
                        let name = syn::LitStr::new(public_name, *span);
                        let access = runtime_scope_value_access(*reference, client, scopes);
                        quote_spanned!(*span=>
                            (#name, #access as &dyn #facade_path::rate_limit::RateKeyPart)
                        )
                    }
                })
                .collect::<Vec<_>>();
            quote! {
                #capture_origin
                __concord_builder.push_rate_limit_policy(
                    #identity_path::#identity,
                    [#(#parts,)*],
                    #cost,
                    [#(#windows,)*],
                )?;
            }
        });
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

fn value_and_adapter(expression: &ResolvedExpr) -> Option<(&ResolvedValue, Option<&Type>)> {
    match expression {
        ResolvedExpr::Value(value) => Some((value, None)),
        ResolvedExpr::Adapted { value, adapter } => match value.as_ref() {
            ResolvedExpr::Value(value) => Some((value, Some(adapter))),
            _ => None,
        },
        _ => None,
    }
}

fn atom_closure(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let statements = atom_write_statements(expression, client, scopes, endpoint);
    quote! {
        |__concord_atom| {
            #statements
            ::std::result::Result::Ok(())
        }
    }
}

fn runtime_scope_atom_closure(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
) -> TokenStream {
    let statements = runtime_scope_atom_write_statements(expression, client, scopes);
    quote! {
        |__concord_atom| {
            #statements
            ::std::result::Result::Ok(())
        }
    }
}

fn runtime_scope_atom_write_statements(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Character(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Boolean(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Integer {
            negative: false,
            literal,
        } => quote!(__concord_atom.write(&#literal)?;),
        ResolvedExpr::Integer {
            negative: true,
            literal,
        } => quote!(__concord_atom.write(&(-#literal))?;),
        ResolvedExpr::Value(value) => {
            let value = runtime_scope_value_access(value.reference, client, scopes);
            quote!(__concord_atom.write(#value)?;)
        }
        ResolvedExpr::Adapted { value, adapter } => {
            let value = runtime_scope_atom_argument(value, client, scopes);
            quote!(__concord_atom.write_with::<#adapter, _>(#value)?;)
        }
        ResolvedExpr::Concat(values) => {
            let statements = values
                .iter()
                .map(|value| runtime_scope_atom_write_statements(value, client, scopes));
            quote!(#(#statements)*)
        }
    }
}

fn runtime_scope_atom_argument(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(&#value),
        ResolvedExpr::Character(value) => quote!(&#value),
        ResolvedExpr::Boolean(value) => quote!(&#value),
        ResolvedExpr::Integer {
            negative: false,
            literal,
        } => quote!(&#literal),
        ResolvedExpr::Integer {
            negative: true,
            literal,
        } => quote!(&(-#literal)),
        ResolvedExpr::Value(value) => runtime_scope_value_access(value.reference, client, scopes),
        ResolvedExpr::Adapted { .. } | ResolvedExpr::Concat(_) => {
            unreachable!("adapters accept one unadapted scalar atom")
        }
    }
}

fn runtime_scope_value_access(
    reference: ValueReference,
    client: &Client,
    scopes: &[&Scope],
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
                .expect("resolved scope identity appears in the runtime owner chain");
            let field = scope_value_field(scope_id, parameter, &scope.parameters[parameter]);
            quote!((&*__concord_scope.#field))
        }
        ValueReference::Endpoint(_) => {
            unreachable!("scope-owned configuration cannot reference an endpoint value")
        }
    }
}

fn bound_atom_closure(value: &TokenStream, adapter: Option<&Type>) -> TokenStream {
    let write = if let Some(adapter) = adapter {
        quote!(__concord_atom.write_with::<#adapter, _>(#value)?;)
    } else {
        quote!(__concord_atom.write(#value)?;)
    };
    quote! {
        |__concord_atom| {
            #write
            ::std::result::Result::Ok(())
        }
    }
}

fn atom_write_statements(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Character(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Boolean(value) => quote!(__concord_atom.write(&#value)?;),
        ResolvedExpr::Integer {
            negative: false,
            literal,
        } => quote!(__concord_atom.write(&#literal)?;),
        ResolvedExpr::Integer {
            negative: true,
            literal,
        } => quote!(__concord_atom.write(&(-#literal))?;),
        ResolvedExpr::Value(value) => {
            let value = resolved_value_access(value.reference, client, scopes, endpoint);
            quote!(__concord_atom.write(#value)?;)
        }
        ResolvedExpr::Adapted { value, adapter } => {
            let value = atom_argument(value, client, scopes, endpoint);
            quote!(__concord_atom.write_with::<#adapter, _>(#value)?;)
        }
        ResolvedExpr::Concat(values) => {
            let statements = values
                .iter()
                .map(|value| atom_write_statements(value, client, scopes, endpoint));
            quote!(#(#statements)*)
        }
    }
}

fn atom_argument(
    expression: &ResolvedExpr,
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    match expression {
        ResolvedExpr::String(value) => quote!(&#value),
        ResolvedExpr::Character(value) => quote!(&#value),
        ResolvedExpr::Boolean(value) => quote!(&#value),
        ResolvedExpr::Integer {
            negative: false,
            literal,
        } => quote!(&#literal),
        ResolvedExpr::Integer {
            negative: true,
            literal,
        } => quote!(&(-#literal)),
        ResolvedExpr::Value(value) => {
            resolved_value_access(value.reference, client, scopes, endpoint)
        }
        ResolvedExpr::Adapted { .. } | ResolvedExpr::Concat(_) => {
            unreachable!("adapters accept one unadapted scalar atom")
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

pub(super) fn endpoint_constructor_binding(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_input", index, &value.name)
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_public_head_shaping_from_resolved_expressions() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let output = path_statements(&api.client, &[], endpoint).to_string();

        assert!(output.contains("push_path_atoms"));
        assert!(output.contains("__concord_client_values"));
    }

    #[test]
    fn nested_concats_emit_one_bounded_atom_writer_without_formatting() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client FormatApi {
                timeout 30s;
                base "https://example.test";

            GET read(first: String, second: String) -> Bytes via Binary {
                path [concat["prefix-", concat[endpoint.first, "-", endpoint.second]]];
            }
        }});
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let output = path_statements(&api.client, &[], endpoint).to_string();

        assert_eq!(output.matches("push_path_atoms").count(), 1);
        assert_eq!(output.matches("__concord_atom . write").count(), 4);
        assert!(!output.contains("format_args"));
        assert!(!output.contains("String :: new"));
    }

    #[test]
    fn origin_key_uses_the_canonical_policy_path() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.shaping.1"; key [origin]; cost 1;  allow 1 per 1s; }

            GET read() -> Bytes via Binary;
        }});
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let output = rate_limit_statements(
            &api.client,
            &[],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
        )
        .to_string();

        assert!(!output.contains("push_generated_rate_limit_policy"));
        assert!(output.contains("push_rate_limit_policy"));
        assert!(output.contains("rate_limit_origin"));
    }

    fn keyed_rule_output() -> String {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }

            scope regional(region: String) {
                subdomain [regional.region, "api"];
                GET read(account: String) -> Bytes via Binary {
                    id "stable.rate.read";
                    rate_limit { id "concord.shaping.2"; cost 1;
                        key [origin, endpoint, client.tenant, regional.region, endpoint.account];
                        allow 1 per 1s;
                    }
                }
            }
        }});
        let scope = api
            .client
            .children
            .iter()
            .find_map(|child| match child {
                Child::Scope(scope) => Some(scope.as_ref()),
                Child::Endpoint(_) => None,
            })
            .expect("regional scope");
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        rate_limit_statements(
            &api.client,
            &[scope],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
        )
        .to_string()
    }

    #[test]
    fn origin_dimension_uses_normalized_logical_origin() {
        let output = keyed_rule_output();
        assert!(output.contains("rate_limit_origin"));
        assert!(output.contains("push_rate_limit_policy"));
    }

    #[test]
    fn endpoint_dimension_uses_contract_id() {
        let output = keyed_rule_output();
        assert!(output.contains("\"stable.rate.read\""));
    }

    #[test]
    fn client_value_dimension_is_generated() {
        let output = keyed_rule_output();
        assert!(output.contains("\"client.tenant\""));
        assert!(output.contains("__concord_client_values"));
    }

    #[test]
    fn scope_value_dimension_is_generated() {
        let output = keyed_rule_output();
        assert!(output.contains("\"regional.region\""));
    }

    #[test]
    fn endpoint_value_dimension_is_generated() {
        let output = keyed_rule_output();
        assert!(output.contains("\"endpoint.account\""));
    }

    #[test]
    fn named_rule_emits_logical_formal_name_with_bound_lexical_access() {
        let api = crate::semantic::test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy account_reads(account) { id "concord.shaping.3"; cost 1;
                        key [account];
                        allow 10 per 1m;
                    }
                }

            scope accounts(account: String) {
                rate_limit account_reads(account = accounts.account);
                GET read() -> Bytes via Binary;
            }
        }});
        let scope = api
            .client
            .children
            .iter()
            .find_map(|child| match child {
                Child::Scope(scope) => Some(scope.as_ref()),
                Child::Endpoint(_) => None,
            })
            .expect("accounts scope");
        let endpoint = crate::semantic::test_support::endpoint(&api.client.children, "read");
        let output = rate_limit_statements(
            &api.client,
            &[scope],
            endpoint,
            &test_support::facade_path(),
            &quote!(super),
        )
        .to_string();

        assert!(output.contains("(\"account\""));
        assert!(!output.contains("\"scope.account\""));
        assert!(output.contains("__concord_scope_0_0_account"));
        assert!(output.contains("push_rate_limit_policy"));
    }

    #[test]
    fn composite_dimension_order_is_preserved() {
        let output = keyed_rule_output();
        let ordered = [
            "\"origin\"",
            "\"endpoint\"",
            "\"client.tenant\"",
            "\"regional.region\"",
            "\"endpoint.account\"",
        ]
        .map(|name| output.find(name).expect("generated key-part name"));
        assert!(
            ordered.windows(2).all(|pair| pair[0] < pair[1]),
            "generated dimensions lost declaration order: {output}"
        );
        assert!(output.contains("as & dyn :: concord :: rate_limit :: RateKeyPart"));
    }
}
