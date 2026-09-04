use super::*;

pub(super) fn generate_endpoint_description(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    response: &TokenStream,
    output: &TokenStream,
    facade_path: &TokenStream,
) -> (Ident, TokenStream) {
    let description_static = private_ident("endpoint_description", endpoint.id, &endpoint.name);
    let contract_id = &endpoint.contract_id.value;
    let source_path = syn::LitStr::new(&endpoint.surface.docs_identity, endpoint.name.span());
    let route = syn::LitStr::new(
        &endpoint_route(client, scopes, endpoint),
        endpoint.name.span(),
    );
    let method = method_token(&endpoint.method, facade_path);
    let duration = endpoint
        .config
        .timeout
        .expect("semantic inheritance retains one finite endpoint timeout");
    let seconds = duration.as_secs();
    let nanos = duration.subsec_nanos();
    let timeout = quote! {
        #facade_path::endpoint::TimeoutSelection::Explicit(
            ::std::time::Duration::new(#seconds, #nanos)
        )
    };
    let body_category = endpoint.body.as_ref().map_or_else(
        || quote!(#facade_path::endpoint::RequestBodyCategory::Empty),
        |_| quote!(#facade_path::endpoint::RequestBodyCategory::Specification),
    );
    let body_specification = endpoint.body.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |body| {
            let identity = type_literal(&body.spec_type);
            quote!(::std::option::Option::Some(#identity))
        },
    );
    let pagination = endpoint.pagination.is_some();
    let credential_storage = !endpoint.surface.terminals.credential_storage.is_empty();
    let authentication = !endpoint.config.auth.is_empty();
    let rate_limit = !endpoint.config.rate_limits.is_empty();
    let accepted_bytes = endpoint.surface.terminals.accepted_bytes;
    let accepted_status = match &endpoint.accepted_status {
        AcceptedStatus::Success => {
            quote!(#facade_path::endpoint::AcceptedStatus::Success)
        }
        AcceptedStatus::Codes(codes) => {
            let codes = codes.iter().map(|code| {
                quote!(#facade_path::http::StatusCode::from_u16(#code)
                    .expect("macro-validated HTTP status"))
            });
            quote!(#facade_path::endpoint::AcceptedStatus::codes([#(#codes),*])
                .expect("macro-validated nonempty status set"))
        }
    };
    let status_source = if endpoint.status_explicit {
        quote!(#facade_path::endpoint::StatusSource::Explicit)
    } else {
        quote!(#facade_path::endpoint::StatusSource::DefaultSuccess)
    };
    let effect = match endpoint.effect {
        Effect::Safe => quote!(#facade_path::endpoint::Effect::Safe),
        Effect::Idempotent => quote!(#facade_path::endpoint::Effect::Idempotent),
        Effect::NonIdempotent => quote!(#facade_path::endpoint::Effect::NonIdempotent),
    };
    let effect_source = if endpoint.effect_explicit {
        quote!(#facade_path::endpoint::EffectSource::Explicit)
    } else {
        quote!(#facade_path::endpoint::EffectSource::MethodDefault)
    };
    let details = endpoint_contract_tokens(endpoint, response, output, facade_path);
    let tokens = quote! {
        pub(crate) static #description_static:
            ::std::sync::LazyLock<#facade_path::endpoint::EndpointDescription> =
            ::std::sync::LazyLock::new(|| {
                let __concord_response_acquisition =
                    <<#response as #facade_path::response::ResponseRepresentation<#output>>::Driver
                    as #facade_path::response::ResponseDriver>::ACQUISITION;
                #facade_path::endpoint::EndpointDescription::generated(
                    #facade_path::__private::endpoint_id_from_static(#contract_id),
                    #method,
                    #source_path,
                    #route,
                    #body_category,
                    #body_specification,
                    __concord_response_acquisition,
                    #facade_path::endpoint::EndpointCapabilities::generated(
                        #pagination,
                        #accepted_bytes,
                        #credential_storage,
                        #authentication,
                        #rate_limit,
                    ),
                    #timeout,
                    #accepted_status,
                    #status_source,
                    #effect,
                    #effect_source,
                    #details,
                )
            });
    };
    (description_static, tokens)
}

fn endpoint_contract_tokens(
    endpoint: &Endpoint,
    resolved_response: &TokenStream,
    resolved_output: &TokenStream,
    facade_path: &TokenStream,
) -> TokenStream {
    let declaration = declaration_tokens(
        &endpoint.name,
        &endpoint.surface.docs_identity,
        &endpoint.attributes,
        quote!(#facade_path::endpoint::DeclarationKind::Endpoint),
        facade_path,
    );
    let owner_path = syn::LitStr::new(&endpoint.surface.docs_identity, endpoint.name.span());
    let values = endpoint.parameters.iter().map(|value| {
        value_tokens(
            value,
            quote!(#facade_path::endpoint::ValueOwnerKind::Endpoint),
            &owner_path,
            facade_path,
        )
    });
    let local = endpoint
        .local_attachments
        .iter()
        .map(|attachment| attachment_tokens(attachment, true, facade_path));
    let effective = effective_attachment_tokens(endpoint, facade_path);
    let body = endpoint.body.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |body| {
            let name = syn::LitStr::new(&canonical_ident(&body.name), body.name.span());
            let input = type_literal(&body.input_type);
            let specification = type_literal(&body.spec_type);
            let representation_origin =
                syn::LitStr::new(&body.representation_origin, body.name.span());
            let docs = string_slice(&body.docs, body.name.span());
            quote!(::std::option::Option::Some(
                #facade_path::endpoint::BodyDescription::generated(
                    #name, #input, #specification, #representation_origin, #docs
                )
            ))
        },
    );
    let output = type_literal(&endpoint.output);
    let response_spec = type_literal(&endpoint.response_spec);
    let response_origin = syn::LitStr::new(
        &endpoint.response_representation_origin,
        endpoint.name.span(),
    );
    let auth = endpoint.config.auth.iter().map(|auth| {
        let placement = syn::LitStr::new(&auth_placement_name(&auth.kind), auth.span);
        let recovery = syn::LitStr::new(challenge_name(auth.challenge), auth.span);
        let origin = syn::LitStr::new(&auth.origin, auth.span);
        quote!(#facade_path::endpoint::AuthenticationDescription::generated(
            #placement, #recovery, #origin
        ))
    });
    let rates = endpoint.config.rate_limits.iter().map(|policy| {
        let id = syn::LitStr::new(&policy.id, policy.id_span);
        let origin = syn::LitStr::new(&policy.origin, policy.id_span);
        let explicit = policy.explicit_id;
        let cost = policy.cost;
        let dimensions = policy.dimensions.iter().map(|dimension| {
            let name = match dimension {
                NormalizedRateLimitDimension::Origin { .. } => "origin".to_owned(),
                NormalizedRateLimitDimension::Endpoint { .. } => "endpoint".to_owned(),
                NormalizedRateLimitDimension::Value { public_name, .. } => public_name.clone(),
            };
            syn::LitStr::new(&name, policy.id_span)
        });
        let windows = policy.windows.iter().map(|window| {
            let capacity = window.capacity;
            let seconds = window.duration.as_secs();
            let nanos = window.duration.subsec_nanos();
            quote!(#facade_path::endpoint::RateLimitWindowDescription::generated(
                #capacity,
                ::std::time::Duration::new(#seconds, #nanos),
            ))
        });
        quote!(#facade_path::endpoint::RatePolicyDescription::generated(
            #id,
            #explicit,
            #origin,
            ::std::vec![#(#dimensions),*],
            #cost,
            ::std::vec![#(#windows),*],
        ))
    });
    let pagination = endpoint.pagination.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |pagination| {
            let controller = type_literal(&pagination.controller);
            let fields = pagination.fields.iter().map(|field| {
                let name = syn::LitStr::new(&canonical_ident(&field.name), field.name.span());
                match &field.initializer {
                    PaginationInitializer::Bound { value, .. } => {
                        let destination = resolved_value_name(value, endpoint);
                        let destination = syn::LitStr::new(&destination, field.name.span());
                        quote!(#facade_path::endpoint::PaginationFieldDescription::generated(
                            #name,
                            ::std::option::Option::Some(#destination),
                            false,
                        ))
                    }
                    PaginationInitializer::Value(_) => quote!(
                        #facade_path::endpoint::PaginationFieldDescription::generated(
                            #name,
                            ::std::option::Option::None,
                            true,
                        )
                    ),
                }
            });
            quote!(::std::option::Option::Some(
                #facade_path::endpoint::PaginationContractDescription::generated(
                    #controller,
                    ::std::vec![#(#fields),*],
                )
            ))
        },
    );
    let explicit_id = endpoint.contract_id.explicit;

    quote!(#facade_path::endpoint::EndpointContractDescription::generated(
        #declaration,
        ::std::vec![#(#values),*],
        ::std::vec![#(#local),*],
        ::std::vec![#(#effective),*],
        #body,
        #facade_path::endpoint::ResponseDescription::generated(
            #output,
            #response_spec,
            #response_origin,
            <#resolved_response as #facade_path::response::ResponseRepresentation<#resolved_output>>::accept(),
        ),
        ::std::vec![#(#auth),*],
        ::std::vec![#(#rates),*],
        #pagination,
        #explicit_id,
    ))
}

pub(super) fn declaration_tokens(
    name: &Ident,
    path: &str,
    attributes: &crate::semantic::model::DeclarationMetadata,
    kind: TokenStream,
    facade_path: &TokenStream,
) -> TokenStream {
    let name_literal = syn::LitStr::new(&canonical_ident(name), name.span());
    let path_literal = syn::LitStr::new(path, name.span());
    let docs = string_slice(&attributes.docs, name.span());
    let deprecated = attributes.deprecated.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |note| {
            let note = syn::LitStr::new(note, name.span());
            quote!(::std::option::Option::Some(#note))
        },
    );
    quote!(#facade_path::endpoint::DeclarationDescription::generated(
        #kind,
        #name_literal,
        #path_literal,
        #docs,
        #deprecated,
    ))
}

pub(super) fn value_tokens(
    value: &Value,
    owner: TokenStream,
    owner_path: &syn::LitStr,
    facade_path: &TokenStream,
) -> TokenStream {
    let name = syn::LitStr::new(&canonical_ident(&value.name), value.name.span());
    let payload = type_literal(&value.payload_type);
    let presence = match value.presence {
        ValuePresence::Required => quote!(#facade_path::endpoint::ValuePresence::Required),
        ValuePresence::Optional => quote!(#facade_path::endpoint::ValuePresence::Optional),
    };
    let initializer = match value.initializer {
        None => quote!(#facade_path::endpoint::ValueInitializer::None),
        Some(ResolvedDefault::TraitDefault(_)) => {
            quote!(#facade_path::endpoint::ValueInitializer::TraitDefault)
        }
        Some(_) => quote!(#facade_path::endpoint::ValueInitializer::StableValue),
    };
    let observation = match value.observation {
        ObservationClass::OrdinaryPublic => {
            quote!(#facade_path::endpoint::ObservationClass::OrdinaryPublic)
        }
        ObservationClass::RedactedPublic => {
            quote!(#facade_path::endpoint::ObservationClass::RedactedPublic)
        }
    };
    let docs = string_slice(&value.docs, value.name.span());
    quote!(#facade_path::endpoint::ValueDescription::generated(
        #owner,
        #owner_path,
        #name,
        #payload,
        #presence,
        #initializer,
        #observation,
        #docs,
    ))
}

pub(super) fn attachment_tokens(
    attachment: &crate::semantic::model::AttachmentIr,
    local: bool,
    facade_path: &TokenStream,
) -> TokenStream {
    let family = attachment_family_token(attachment.family, facade_path);
    let operation = attachment_operation_token(attachment.operation, facade_path);
    let view = if local {
        quote!(#facade_path::endpoint::AttachmentView::Local)
    } else {
        quote!(#facade_path::endpoint::AttachmentView::Effective)
    };
    let origin = syn::LitStr::new(&attachment.origin, Span::call_site());
    let target = attachment.target.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |target| {
            let target = syn::LitStr::new(target, Span::call_site());
            quote!(::std::option::Option::Some(#target))
        },
    );
    let item_count = attachment.item_count;
    quote!(#facade_path::endpoint::AttachmentDescription::generated(
        #family, #operation, #view, #origin, #target, #item_count
    ))
}

fn effective_attachment_tokens(endpoint: &Endpoint, facade_path: &TokenStream) -> Vec<TokenStream> {
    effective_config_attachment_tokens(&endpoint.config, facade_path)
}

pub(super) fn effective_config_attachment_tokens(
    config: &NormalizedConfig,
    facade_path: &TokenStream,
) -> Vec<TokenStream> {
    let mut output = Vec::new();
    for (family, state) in [
        (
            AttachmentFamily::RequestRepresentation,
            &config.request_representation,
        ),
        (
            AttachmentFamily::ResponseRepresentation,
            &config.response_representation,
        ),
    ] {
        if let Some(origin) = &state.origin {
            output.push(effective_attachment(
                family,
                if state.fenced {
                    AttachmentOperation::Off
                } else {
                    AttachmentOperation::Set
                },
                origin,
                None,
                facade_path,
            ));
        }
    }
    for origin in &config.host_origins {
        output.push(effective_attachment(
            AttachmentFamily::Subdomain,
            AttachmentOperation::Set,
            origin,
            None,
            facade_path,
        ));
    }
    for origin in &config.path_origins {
        output.push(effective_attachment(
            AttachmentFamily::Path,
            AttachmentOperation::Append,
            origin,
            None,
            facade_path,
        ));
    }
    for query in &config.query {
        output.push(effective_attachment(
            AttachmentFamily::Query,
            query.operation,
            &query.origin,
            Some(query.key.value()),
            facade_path,
        ));
    }
    for header in &config.headers {
        output.push(effective_attachment(
            AttachmentFamily::Headers,
            header.operation,
            &header.origin,
            Some(header.name.value()),
            facade_path,
        ));
    }
    for auth in &config.auth {
        output.push(effective_attachment(
            AttachmentFamily::Authentication,
            AttachmentOperation::Append,
            &auth.origin,
            Some(auth_placement_name(&auth.kind)),
            facade_path,
        ));
    }
    if let Some(origin) = &config.timeout_origin {
        output.push(effective_attachment(
            AttachmentFamily::Timeout,
            if config.timeout.is_some() {
                AttachmentOperation::Set
            } else {
                AttachmentOperation::Off
            },
            origin,
            None,
            facade_path,
        ));
    }
    for policy in &config.rate_limits {
        output.push(effective_attachment(
            AttachmentFamily::RateLimit,
            AttachmentOperation::Append,
            &policy.origin,
            Some(policy.id.clone()),
            facade_path,
        ));
    }
    output
}

fn effective_attachment(
    family: AttachmentFamily,
    operation: AttachmentOperation,
    origin: &str,
    target: Option<String>,
    facade_path: &TokenStream,
) -> TokenStream {
    let ir = crate::semantic::model::AttachmentIr {
        family,
        operation,
        origin: origin.to_owned(),
        target,
        item_count: 1,
    };
    attachment_tokens(&ir, false, facade_path)
}

fn attachment_family_token(family: AttachmentFamily, facade_path: &TokenStream) -> TokenStream {
    let variant = match family {
        AttachmentFamily::RequestRepresentation => quote!(RequestRepresentation),
        AttachmentFamily::ResponseRepresentation => quote!(ResponseRepresentation),
        AttachmentFamily::Subdomain => quote!(Subdomain),
        AttachmentFamily::Path => quote!(Path),
        AttachmentFamily::Query => quote!(Query),
        AttachmentFamily::Headers => quote!(Headers),
        AttachmentFamily::Authentication => quote!(Authentication),
        AttachmentFamily::Timeout => quote!(Timeout),
        AttachmentFamily::RateLimit => quote!(RateLimit),
    };
    quote!(#facade_path::endpoint::AttachmentFamily::#variant)
}

fn attachment_operation_token(
    operation: AttachmentOperation,
    facade_path: &TokenStream,
) -> TokenStream {
    let variant = match operation {
        AttachmentOperation::Prepend => quote!(Prepend),
        AttachmentOperation::Append => quote!(Append),
        AttachmentOperation::Replace => quote!(Replace),
        AttachmentOperation::Remove => quote!(Remove),
        AttachmentOperation::Off => quote!(Off),
        AttachmentOperation::Set => quote!(Set),
        AttachmentOperation::WhenPresent => quote!(WhenPresent),
    };
    quote!(#facade_path::endpoint::AttachmentOperation::#variant)
}

fn auth_placement_name(kind: &ResolvedAuthPlacement) -> String {
    match kind {
        ResolvedAuthPlacement::Bearer => "bearer".to_owned(),
        ResolvedAuthPlacement::Basic => "basic".to_owned(),
        ResolvedAuthPlacement::Authorization(None) => "authorization".to_owned(),
        ResolvedAuthPlacement::Authorization(Some(scheme)) => {
            format!("authorization:{}", scheme.value())
        }
        ResolvedAuthPlacement::Header(name) => format!("header:{}", name.value()),
        ResolvedAuthPlacement::Query(name) => format!("query:{}", name.value()),
    }
}

const fn challenge_name(challenge: ResolvedChallengePolicy) -> &'static str {
    match challenge {
        ResolvedChallengePolicy::Unauthorized => "unauthorized",
        ResolvedChallengePolicy::UnauthorizedOrForbidden => "unauthorized_or_forbidden",
        ResolvedChallengePolicy::Never => "none",
    }
}

fn resolved_value_name(value: &ResolvedValue, endpoint: &Endpoint) -> String {
    match value.reference {
        ValueReference::Endpoint(index) => format!(
            "endpoint.{}",
            canonical_ident(&endpoint.parameters[index].name)
        ),
        ValueReference::Client(index) => format!("client.value#{index}"),
        ValueReference::Scope {
            scope_id,
            parameter,
        } => format!("scope#{scope_id}.value#{parameter}"),
    }
}

pub(super) fn type_literal(ty: &Type) -> syn::LitStr {
    syn::LitStr::new(&ty.to_token_stream().to_string(), ty.span())
}

pub(super) fn string_slice(values: &[String], span: Span) -> TokenStream {
    let values = values.iter().map(|value| syn::LitStr::new(value, span));
    quote!(::std::vec![#(#values),*])
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;
    use quote::quote;

    #[test]
    fn emits_only_safe_semantic_description_inputs() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let (_, tokens) = generate_endpoint_description(
            &api.client,
            &[],
            endpoint,
            &quote!(crate::ResponseRepresentation),
            &quote!(crate::Output),
            &test_support::facade_path(),
        );
        let output = tokens.to_string();
        assert!(output.contains("EndpointDescription :: generated"));
        assert!(output.contains("EndpointContractDescription :: generated"));
        assert!(output.contains("ValueDescription :: generated"));
        assert!(output.contains("__private :: endpoint_id_from_static"));
        assert!(output.contains("EndpointCapabilities :: generated"));
        assert!(!output.contains("bearer_value"));
    }
}
