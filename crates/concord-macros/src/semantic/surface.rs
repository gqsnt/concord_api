use std::collections::HashMap;

use proc_macro2::Span;
use quote::ToTokens;
use syn::{Ident, LitStr, Type};

use super::model::{
    Child, ClientSurface, CloneEligibility, ContractId, Credential, CredentialSource,
    CredentialStorageTerminal, Endpoint, EndpointDocAuthPlacement, EndpointDocBody,
    EndpointDocHost, EndpointDocInput, EndpointDocInputKind, EndpointDocModel,
    EndpointDocNamespace, EndpointDocRateLimit, EndpointDocRateLimitWindow, EndpointDocResponse,
    EndpointDocRouteAtom, EndpointDocSetter, EndpointDocTerminals, EndpointDocTimeout,
    EndpointSetter, EndpointSurface, NormalizedConfig, NormalizedRateLimitDimension, RequestBody,
    RequestTerminals, RequiredArgument, ResolvedAuthPlacement, ResolvedChallengePolicy,
    ResolvedExpr, ResolvedMethod, Scope, ScopeSurface, Value, ValueReference,
};

pub(crate) struct VisibleScopeDoc<'a> {
    pub(crate) id: usize,
    pub(crate) name: &'a Ident,
    pub(crate) values: &'a [Value],
}

pub(crate) struct EndpointDocContext<'a> {
    pub(crate) client_name: &'a Ident,
    pub(crate) client_surface: &'a ClientSurface,
    pub(crate) base_url: &'a LitStr,
    pub(crate) client_values: &'a [Value],
    pub(crate) scopes: &'a [VisibleScopeDoc<'a>],
    pub(crate) method: &'a ResolvedMethod,
    pub(crate) parameters: &'a [Value],
    pub(crate) body: Option<&'a RequestBody>,
    pub(crate) config: &'a NormalizedConfig,
    pub(crate) response: &'a Type,
    pub(crate) timeout: EndpointDocTimeout,
    pub(crate) contract_id: &'a ContractId,
}

pub(crate) const RESERVED_GENERATED_METHODS: &[&str] = &[
    "new",
    "builder",
    "build",
    "configure_http",
    "configure_runtime",
    "retry",
    "credentials",
    "request",
    "response",
    "response_bytes",
    "paginate",
    "timeout",
    "disable_timeout",
    "inherit_timeout",
    "debug",
    "description",
    "into_future",
    "__concord_bind",
    "__concord_new",
];

pub(crate) fn client_surface(name: &Ident) -> ClientSurface {
    let canonical = canonical_ident(name);
    ClientSurface {
        builder_name: Ident::new(&format!("{canonical}Builder"), name.span()),
        support_name: snake_ident(name),
        values_type_name: Ident::new(
            &format!("__Concord{}Values", to_pascal_case(&canonical)),
            Span::mixed_site(),
        ),
    }
}

pub(crate) fn scope_surface(name: &Ident) -> ScopeSurface {
    let method_name = snake_ident(name);
    ScopeSurface {
        module_name: method_name.clone(),
        method_name,
        facade_name: pascal_ident(name),
    }
}

pub(crate) fn endpoint_surface(
    client_name: &Ident,
    scope_path: &[Ident],
    endpoint_name: &Ident,
    parameters: &[Value],
    body_position: Option<usize>,
    paginated: bool,
) -> EndpointSurface {
    let endpoint_type_name = pascal_ident(endpoint_name);
    let mut required_arguments = parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_required())
        .map(|(index, value)| (value.source_position, RequiredArgument::Parameter(index)))
        .collect::<Vec<_>>();
    if let Some(position) = body_position {
        required_arguments.push((position, RequiredArgument::Body));
    }
    required_arguments.sort_by_key(|(position, _)| *position);
    let required_arguments = required_arguments
        .into_iter()
        .map(|(_, argument)| argument)
        .collect();
    let setters = parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(parameter, value)| EndpointSetter {
            parameter,
            setter: value_setter_ident(&value.name),
            clear: value
                .accepts_absence()
                .then(|| helper_ident("clear", &value.name)),
            reset: value
                .has_default()
                .then(|| helper_ident("reset", &value.name)),
        })
        .collect();
    let dsl_path = scope_path
        .iter()
        .chain(std::iter::once(endpoint_name))
        .map(canonical_ident)
        .collect::<Vec<_>>();
    EndpointSurface {
        method_name: snake_ident(endpoint_name),
        request_type_name: request_type_name(&endpoint_type_name),
        endpoint_type_name,
        module_path: scope_path.iter().map(snake_ident).collect(),
        required_arguments,
        setters,
        terminals: RequestTerminals {
            response: true,
            pagination: paginated,
            credential_storage: Vec::new(),
        },
        clone_eligibility: if body_position.is_some() {
            CloneEligibility::BodyInputBound
        } else {
            CloneEligibility::Always
        },
        docs_identity: format!("{}::{}", canonical_ident(client_name), dsl_path.join("::")),
    }
}

pub(crate) fn endpoint_doc_model(
    surface: &EndpointSurface,
    context: EndpointDocContext<'_>,
) -> EndpointDocModel {
    let EndpointDocContext {
        client_name,
        client_surface,
        base_url,
        client_values,
        scopes,
        method,
        parameters,
        body,
        config,
        response,
        timeout,
        contract_id,
    } = context;
    let mut endpoint_type_path = canonical_ident(&client_surface.support_name);
    for module in &surface.module_path {
        endpoint_type_path.push_str("::");
        endpoint_type_path.push_str(&canonical_ident(module));
    }
    let mut request_type_path = endpoint_type_path.clone();
    endpoint_type_path.push_str("::");
    endpoint_type_path.push_str(&canonical_ident(&surface.endpoint_type_name));
    request_type_path.push_str("::");
    request_type_path.push_str(&canonical_ident(&surface.request_type_name));

    let body = body.map(|body| {
        let specification = type_name(&body.ty);
        EndpointDocBody {
            argument: canonical_ident(&body.name),
            input_type: format!("<{specification} as RequestBodySpec>::Input"),
            specification,
        }
    });
    let required_endpoint_inputs = surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => doc_input(None, &parameters[*index]),
            RequiredArgument::Body => {
                let body = body.as_ref().expect("body documentation");
                EndpointDocInput {
                    owner: None,
                    name: body.argument.clone(),
                    type_name: body.input_type.clone(),
                    kind: EndpointDocInputKind::Required,
                }
            }
        })
        .collect();
    let endpoint_setters = surface
        .setters
        .iter()
        .map(|setter| {
            let value = &parameters[setter.parameter];
            EndpointDocSetter {
                name: canonical_ident(&value.name),
                setter: canonical_ident(&setter.setter),
                clear: setter.clear.as_ref().map(canonical_ident),
                reset: setter.reset.as_ref().map(canonical_ident),
                type_name: type_name(&value.ty),
            }
        })
        .collect();
    let scope_inputs = scopes
        .iter()
        .flat_map(|scope| {
            let owner = canonical_ident(scope.name);
            scope
                .values
                .iter()
                .map(move |value| doc_input(Some(owner.clone()), value))
        })
        .collect();
    let route_atoms = config
        .path
        .iter()
        .map(|atom| route_atom(atom, client_values, scopes, parameters))
        .collect();
    let host = if config.host.is_empty() {
        None
    } else {
        let base = url::Url::parse(&base_url.value())
            .expect("validated base URL for endpoint documentation");
        Some(EndpointDocHost {
            label_atoms: config
                .host
                .iter()
                .map(|atom| route_atom(atom, client_values, scopes, parameters))
                .collect(),
            base_host: base
                .host_str()
                .expect("validated base host for endpoint documentation")
                .to_owned(),
            port: base.port(),
        })
    };
    let authentication = config
        .auth
        .iter()
        .map(|auth| EndpointDocAuthPlacement {
            placement: match &auth.kind {
                ResolvedAuthPlacement::Bearer => "Bearer authorization".to_owned(),
                ResolvedAuthPlacement::Basic => "Basic authorization".to_owned(),
                ResolvedAuthPlacement::Header(name) => {
                    format!("private header {}", name.value())
                }
                ResolvedAuthPlacement::Query(name) => {
                    format!("private query {}", name.value())
                }
            },
            challenge: match auth.challenge {
                ResolvedChallengePolicy::Unauthorized => "recover once after 401".to_owned(),
                ResolvedChallengePolicy::UnauthorizedOrForbidden => {
                    "recover once after 401 or 403".to_owned()
                }
                ResolvedChallengePolicy::Never => "no automatic recovery".to_owned(),
            },
        })
        .collect();
    let rate_limits = config
        .rate_limits
        .iter()
        .map(|rule| EndpointDocRateLimit {
            id: rule.id.clone(),
            origin: rule.origin.clone(),
            dimensions: rule
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    NormalizedRateLimitDimension::Host { .. } => "host".to_owned(),
                    NormalizedRateLimitDimension::Endpoint { .. } => "endpoint".to_owned(),
                    NormalizedRateLimitDimension::Value { public_name, .. } => public_name.clone(),
                })
                .collect(),
            cost: rule.cost,
            windows: rule
                .windows
                .iter()
                .map(|window| EndpointDocRateLimitWindow {
                    capacity: window.capacity,
                    duration: window.duration,
                })
                .collect(),
        })
        .collect();
    let response_specification = type_name(response);

    EndpointDocModel {
        client_name: canonical_ident(client_name),
        scope_chain: scopes
            .iter()
            .map(|scope| canonical_ident(scope.name))
            .collect(),
        direct_method_name: canonical_ident(&surface.method_name),
        endpoint_type_path,
        request_type_path,
        http_method: resolved_method_name(method),
        base_url: base_url.value(),
        host,
        route_atoms,
        scope_inputs,
        required_endpoint_inputs,
        endpoint_setters,
        safe_query_names: config.query.iter().map(|entry| entry.key.value()).collect(),
        safe_header_names: config
            .headers
            .iter()
            .map(|entry| entry.name.value())
            .collect(),
        body,
        response: EndpointDocResponse {
            output_type: format!("<{response_specification} as ResponseBodySpec>::Output"),
            specification: response_specification,
        },
        authentication,
        rate_limits,
        timeout,
        terminals: EndpointDocTerminals {
            direct_await: true,
            response: surface.terminals.response,
            pagination: surface.terminals.pagination,
            credential_storage: Vec::new(),
        },
        contract_id: contract_id.value.clone(),
        explicit_contract_id: contract_id.explicit,
    }
}

fn route_atom(
    value: &ResolvedExpr,
    client_values: &[Value],
    scopes: &[VisibleScopeDoc<'_>],
    endpoint_values: &[Value],
) -> EndpointDocRouteAtom {
    match value {
        ResolvedExpr::String(value) => EndpointDocRouteAtom::Literal(value.value()),
        ResolvedExpr::Integer(value) => EndpointDocRouteAtom::Literal(value.to_string()),
        ResolvedExpr::Value(value) => {
            let (namespace, name) = match value.reference {
                ValueReference::Client(index) => (
                    EndpointDocNamespace::Vars,
                    &client_values
                        .get(index)
                        .expect("resolved client documentation value")
                        .name,
                ),
                ValueReference::Scope {
                    scope_id,
                    parameter,
                } => (
                    EndpointDocNamespace::Scope,
                    &scopes
                        .iter()
                        .find(|scope| scope.id == scope_id)
                        .expect("resolved scope documentation frame")
                        .values
                        .get(parameter)
                        .expect("resolved scope documentation value")
                        .name,
                ),
                ValueReference::Endpoint(index) => (
                    EndpointDocNamespace::Endpoint,
                    &endpoint_values
                        .get(index)
                        .expect("resolved endpoint documentation value")
                        .name,
                ),
            };
            EndpointDocRouteAtom::Value {
                namespace,
                name: canonical_ident(name),
            }
        }
        ResolvedExpr::Format(values) => EndpointDocRouteAtom::Format(
            values
                .iter()
                .map(|value| route_atom(value, client_values, scopes, endpoint_values))
                .collect(),
        ),
    }
}

fn doc_input(owner: Option<String>, value: &Value) -> EndpointDocInput {
    EndpointDocInput {
        owner,
        name: canonical_ident(&value.name),
        type_name: type_name(&value.ty),
        kind: input_kind(value),
    }
}

fn input_kind(value: &Value) -> EndpointDocInputKind {
    if value.is_required() {
        EndpointDocInputKind::Required
    } else if value.is_optional() {
        EndpointDocInputKind::Optional
    } else {
        EndpointDocInputKind::Defaulted
    }
}

fn type_name(value: &Type) -> String {
    value.to_token_stream().to_string()
}

fn resolved_method_name(method: &ResolvedMethod) -> String {
    match method {
        ResolvedMethod::Get => "GET".to_owned(),
        ResolvedMethod::Post => "POST".to_owned(),
        ResolvedMethod::Put => "PUT".to_owned(),
        ResolvedMethod::Patch => "PATCH".to_owned(),
        ResolvedMethod::Delete => "DELETE".to_owned(),
        ResolvedMethod::Head => "HEAD".to_owned(),
        ResolvedMethod::Options => "OPTIONS".to_owned(),
        ResolvedMethod::Trace => "TRACE".to_owned(),
        ResolvedMethod::Connect => "CONNECT".to_owned(),
        ResolvedMethod::Custom(value) => value.value(),
    }
}

pub(crate) fn finalize_endpoint_surfaces(
    children: &mut [Child],
    credentials: &[Credential],
) -> syn::Result<()> {
    for child in children {
        match child {
            Child::Scope(scope) => {
                finalize_endpoint_surfaces(&mut scope.children, credentials)?;
            }
            Child::Endpoint(endpoint) => {
                let credential_storage = credentials
                    .iter()
                    .enumerate()
                    .filter(|(_, credential)| {
                        matches!(
                            credential.source,
                            CredentialSource::Endpoint {
                                endpoint_id: Some(id),
                                ..
                            } if id == endpoint.id
                        )
                    })
                    .map(|(credential, value)| CredentialStorageTerminal {
                        credential,
                        method: storage_terminal_name(&value.name),
                    })
                    .collect::<Vec<_>>();
                endpoint.docs.terminals.credential_storage = credential_storage
                    .iter()
                    .map(|terminal| canonical_ident(&terminal.method))
                    .collect();
                endpoint.surface.terminals.credential_storage = credential_storage;
                validate_request_surface(endpoint)?;
            }
        }
    }
    Ok(())
}

fn validate_request_surface(endpoint: &Endpoint) -> syn::Result<()> {
    let mut names = RESERVED_GENERATED_METHODS
        .iter()
        .map(|name| ((*name).to_owned(), Span::call_site()))
        .collect::<HashMap<_, _>>();
    for setter in &endpoint.surface.setters {
        insert_name(
            &mut names,
            canonical_ident(&setter.setter),
            setter.setter.span(),
            "named request wrapper methods",
        )?;
        if let Some(clear) = &setter.clear {
            insert_name(
                &mut names,
                canonical_ident(clear),
                clear.span(),
                "named request wrapper methods",
            )?;
        }
        if let Some(reset) = &setter.reset {
            insert_name(
                &mut names,
                canonical_ident(reset),
                reset.span(),
                "named request wrapper methods",
            )?;
        }
    }
    for terminal in &endpoint.surface.terminals.credential_storage {
        insert_name(
            &mut names,
            canonical_ident(&terminal.method),
            terminal.method.span(),
            "named request wrapper methods",
        )?;
    }
    Ok(())
}

pub(crate) fn validate_surface_modules(
    children: &[Child],
    facade: Option<&Scope>,
    owner: &str,
) -> syn::Result<()> {
    let mut items = HashMap::new();
    if let Some(scope) = facade {
        insert_name(
            &mut items,
            canonical_ident(&scope.surface.facade_name),
            scope.name.span(),
            owner,
        )?;
    } else {
        for reserved in [
            "Endpoint",
            "credentials",
            "__concord_endpoint_seal",
            "__concord_generated",
        ] {
            items.insert(reserved.to_owned(), Span::call_site());
        }
    }

    for child in children {
        match child {
            Child::Scope(scope) => {
                insert_name(
                    &mut items,
                    canonical_ident(&scope.surface.module_name),
                    scope.name.span(),
                    owner,
                )?;
            }
            Child::Endpoint(endpoint) => {
                insert_name(
                    &mut items,
                    canonical_ident(&endpoint.surface.endpoint_type_name),
                    endpoint.name.span(),
                    owner,
                )?;
                insert_name(
                    &mut items,
                    canonical_ident(&endpoint.surface.request_type_name),
                    endpoint.name.span(),
                    owner,
                )?;
            }
        }
    }

    for child in children {
        if let Child::Scope(scope) = child {
            let nested_owner = format!(
                "generated `{}` scope module item",
                canonical_ident(&scope.surface.module_name)
            );
            validate_surface_modules(&scope.children, Some(scope), &nested_owner)?;
        }
    }
    Ok(())
}

pub(crate) fn insert_name(
    names: &mut HashMap<String, Span>,
    name: String,
    span: Span,
    owner: &str,
) -> syn::Result<()> {
    if let Some(previous) = names.insert(name.clone(), span) {
        let mut error = syn::Error::new(
            span,
            format!("generated {owner} name `{name}` collides after deterministic normalization"),
        );
        error.combine(syn::Error::new(
            previous,
            "the first generated name is here",
        ));
        return Err(error);
    }
    Ok(())
}

pub(crate) fn canonical_ident(name: &Ident) -> String {
    let name = name.to_string();
    name.strip_prefix("r#").unwrap_or(&name).to_owned()
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len());
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            let previous_is_lowercase = index > 0 && characters[index - 1].is_lowercase();
            let next_is_lowercase = characters
                .get(index + 1)
                .is_some_and(|next| next.is_lowercase());
            if index > 0 && (previous_is_lowercase || next_is_lowercase) {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn to_pascal_case(name: &str) -> String {
    let snake = to_snake_case(name);
    snake
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            first.to_uppercase().chain(characters).collect::<String>()
        })
        .collect()
}

pub(crate) fn public_ident(value: &str, span: Span) -> Ident {
    if is_rust_keyword(value) {
        Ident::new_raw(value, span)
    } else {
        Ident::new(value, span)
    }
}

pub(crate) fn snake_ident(name: &Ident) -> Ident {
    public_ident(&to_snake_case(&canonical_ident(name)), name.span())
}

pub(crate) fn pascal_ident(name: &Ident) -> Ident {
    Ident::new(&to_pascal_case(&canonical_ident(name)), name.span())
}

pub(crate) fn request_type_name(endpoint_type: &Ident) -> Ident {
    Ident::new(
        &format!("{}Request", canonical_ident(endpoint_type)),
        endpoint_type.span(),
    )
}

pub(crate) fn storage_terminal_name(credential: &Ident) -> Ident {
    public_ident(
        &format!("store_as_{}", to_snake_case(&canonical_ident(credential))),
        credential.span(),
    )
}

pub(crate) fn helper_ident(prefix: &str, name: &Ident) -> Ident {
    public_ident(&format!("{prefix}_{}", canonical_ident(name)), name.span())
}

pub(crate) fn value_setter_ident(name: &Ident) -> Ident {
    helper_ident("set", name)
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use quote::quote;
    use syn::parse_quote;

    use super::super::test_support;
    use super::*;

    #[test]
    fn deterministic_names_cover_acronyms_keywords_and_request_types() {
        assert_eq!(to_snake_case("HTTPStatus"), "http_status");
        assert_eq!(to_pascal_case("user_resources"), "UserResources");
        assert_eq!(snake_ident(&parse_quote!(r#type)).to_string(), "r#type");
        assert_eq!(
            request_type_name(&pascal_ident(&parse_quote!(get_http_status))),
            "GetHttpStatusRequest"
        );
        assert_eq!(
            storage_terminal_name(&parse_quote!(session_token)),
            "store_as_session_token"
        );
    }

    #[test]
    fn resolved_surface_owns_names_paths_and_exact_capabilities() {
        let api = test_support::resolve_tokens(quote! {
            client SurfaceApi { base "https://example.test"; }
            scope http_status() {
                GET get_http_status(optional: Option<bool>) -> Bytes;
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "get_http_status");

        assert_eq!(endpoint.surface.method_name, "get_http_status");
        assert_eq!(endpoint.surface.endpoint_type_name, "GetHttpStatus");
        assert_eq!(endpoint.surface.request_type_name, "GetHttpStatusRequest");
        assert_eq!(endpoint.surface.module_path[0], "http_status");
        assert_eq!(endpoint.surface.setters.len(), 1);
        assert!(endpoint.surface.terminals.response);
        assert!(!endpoint.surface.terminals.pagination);
        assert_eq!(endpoint.docs.direct_method_name, "get_http_status");
        assert_eq!(
            endpoint.docs.endpoint_type_path,
            "surface_api::http_status::GetHttpStatus"
        );
    }

    #[test]
    fn static_host_prefix() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi { base "https://example.test:8443/root"; }
            GET static_prefix() -> Bytes {
                host ["api"];
            }
        });
        let host = test_support::endpoint(&api.client.children, "static_prefix")
            .docs
            .host
            .as_ref()
            .expect("effective host");

        assert_eq!(
            host.label_atoms,
            [EndpointDocRouteAtom::Literal("api".to_owned())]
        );
        assert_eq!(host.base_host, "example.test");
        assert_eq!(host.port, Some(8443));
    }

    #[test]
    fn client_value_in_host() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi {
                base "https://example.test";
                vars { locale: String; }
            }
            GET localized() -> Bytes {
                host [vars.locale];
            }
        });
        let host = test_support::endpoint(&api.client.children, "localized")
            .docs
            .host
            .as_ref()
            .expect("effective host");

        assert_eq!(
            host.label_atoms,
            [EndpointDocRouteAtom::Value {
                namespace: EndpointDocNamespace::Vars,
                name: "locale".to_owned(),
            }]
        );
    }

    #[test]
    fn scope_value_in_host() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi { base "https://example.test"; }
            scope platform(platform: String) {
                GET scoped() -> Bytes {
                    host [scope.platform];
                }
            }
        });
        let host = test_support::endpoint(&api.client.children, "scoped")
            .docs
            .host
            .as_ref()
            .expect("effective host");

        assert_eq!(
            host.label_atoms,
            [EndpointDocRouteAtom::Value {
                namespace: EndpointDocNamespace::Scope,
                name: "platform".to_owned(),
            }]
        );
    }

    #[test]
    fn formatted_host_atom() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi { base "https://example.test"; }
            GET formatted(value: String) -> Bytes {
                host [fmt["edge-", ep.value]];
            }
        });
        let host = test_support::endpoint(&api.client.children, "formatted")
            .docs
            .host
            .as_ref()
            .expect("effective host");

        assert_eq!(
            host.label_atoms,
            [EndpointDocRouteAtom::Format(vec![
                EndpointDocRouteAtom::Literal("edge-".to_owned()),
                EndpointDocRouteAtom::Value {
                    namespace: EndpointDocNamespace::Endpoint,
                    name: "value".to_owned(),
                },
            ])]
        );
    }

    #[test]
    fn host_inheritance() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi {
                base "https://example.test";
                host ["gateway"];
            }
            scope inherited() {
                GET read() -> Bytes;
            }
        });
        let host = test_support::endpoint(&api.client.children, "read")
            .docs
            .host
            .as_ref()
            .expect("inherited effective host");

        assert_eq!(
            host.label_atoms,
            [EndpointDocRouteAtom::Literal("gateway".to_owned())]
        );
    }

    #[test]
    fn host_off() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi {
                base "https://example.test";
                host ["gateway"];
            }
            scope base_host() {
                host off;
                GET read() -> Bytes;
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert!(endpoint.config.host.is_empty());
        assert!(endpoint.docs.host.is_none());
        assert_eq!(endpoint.docs.base_url, "https://example.test");
    }

    #[test]
    fn sibling_scopes_have_distinct_hosts() {
        let api = test_support::resolve_tokens(quote! {
            client HostApi { base "https://example.test"; }
            scope platform() {
                host ["platform"];
                GET platform_status() -> Bytes;
            }
            scope regional() {
                host ["regional"];
                GET regional_status() -> Bytes;
            }
        });
        let platform = test_support::endpoint(&api.client.children, "platform_status")
            .docs
            .host
            .as_ref()
            .expect("platform host");
        let regional = test_support::endpoint(&api.client.children, "regional_status")
            .docs
            .host
            .as_ref()
            .expect("regional host");

        assert_eq!(
            platform.label_atoms,
            [EndpointDocRouteAtom::Literal("platform".to_owned())]
        );
        assert_eq!(
            regional.label_atoms,
            [EndpointDocRouteAtom::Literal("regional".to_owned())]
        );
        assert_ne!(platform, regional);
    }

    #[test]
    fn endpoint_doc_model_resolves_riot_style_nested_names_and_flat_inputs() {
        let api = test_support::resolve_tokens(quote! {
            client DDragonApi {
                base "https://ddragon.example.test/root";
                vars { locale: String = "en_US"; }
                timeout 20s;
            }
            scope version(version: String) {
                path ["cdn", scope.version];
                scope assets(category: Option<String>) {
                    path ["data", vars.locale];
                    GET image(
                        filename: String,
                        scale: u32 = 2,
                    ) -> Bytes {
                        path [fmt[ep.filename, ".png"]];
                        query { "scale" = ep.scale; }
                        headers { "X-Asset-View" = "public"; }
                    }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "image");
        let docs = &endpoint.docs;

        assert_eq!(docs.client_name, "DDragonApi");
        assert_eq!(docs.scope_chain, ["version", "assets"]);
        assert_eq!(docs.direct_method_name, "image");
        assert_eq!(
            docs.endpoint_type_path,
            "d_dragon_api::version::assets::Image"
        );
        assert_eq!(
            docs.request_type_path,
            "d_dragon_api::version::assets::ImageRequest"
        );
        assert_eq!(docs.http_method, "GET");
        assert_eq!(docs.base_url, "https://ddragon.example.test/root");
        assert_eq!(
            docs.route_atoms,
            [
                EndpointDocRouteAtom::Literal("cdn".to_owned()),
                EndpointDocRouteAtom::Value {
                    namespace: EndpointDocNamespace::Scope,
                    name: "version".to_owned(),
                },
                EndpointDocRouteAtom::Literal("data".to_owned()),
                EndpointDocRouteAtom::Value {
                    namespace: EndpointDocNamespace::Vars,
                    name: "locale".to_owned(),
                },
                EndpointDocRouteAtom::Format(vec![
                    EndpointDocRouteAtom::Value {
                        namespace: EndpointDocNamespace::Endpoint,
                        name: "filename".to_owned(),
                    },
                    EndpointDocRouteAtom::Literal(".png".to_owned()),
                ]),
            ]
        );
        assert_eq!(
            docs.scope_inputs
                .iter()
                .map(|input| (input.owner.as_deref(), input.name.as_str(), input.kind))
                .collect::<Vec<_>>(),
            [
                (Some("version"), "version", EndpointDocInputKind::Required),
                (Some("assets"), "category", EndpointDocInputKind::Optional),
            ]
        );
        assert_eq!(docs.required_endpoint_inputs[0].name, "filename");
        assert_eq!(docs.endpoint_setters[0].name, "scale");
        assert_eq!(docs.endpoint_setters[0].setter, "set_scale");
        assert_eq!(docs.endpoint_setters[0].clear, None);
        assert_eq!(
            docs.endpoint_setters[0].reset.as_deref(),
            Some("reset_scale")
        );
        assert_eq!(docs.safe_query_names, ["scale"]);
        assert_eq!(docs.safe_header_names, ["X-Asset-View"]);
        assert!(docs.body.is_none());
        assert!(docs.response.specification.contains("Bytes"));
        assert!(docs.authentication.is_empty());
        assert!(docs.rate_limits.is_empty());
        assert_eq!(
            docs.timeout,
            EndpointDocTimeout::Inherited(Duration::from_secs(20))
        );
        assert!(docs.terminals.direct_await);
        assert!(docs.terminals.response);
        assert!(!docs.terminals.pagination);
        assert_eq!(docs.contract_id, "d_dragon_api.version.assets.image");
        assert!(!docs.explicit_contract_id);
    }

    #[test]
    fn endpoint_doc_model_tracks_bodies_auth_timeouts_and_actual_terminals() {
        let api = test_support::resolve_tokens(quote! {
            client DocsApi {
                base "https://docs.example.test";
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                    credential session = endpoint login;
                }
                timeout 20s;
            }
            POST login(
                #[body] body: Json<Login>,
                remember: Option<bool>,
                attempts: u32 = 3,
            ) -> Json<Session> {
                id "stable.login";
                auth bearer bearer;
                timeout off;
            }
            GET page(cursor: Option<String>) -> Json<Page> {
                paginate crate::Controller {
                    cursor: bind ep.cursor,
                }
            }
            GET stream() -> Stream<OctetStream>;
            GET discard() -> NoContent {
                timeout 250ms;
            }
        });
        let login = test_support::endpoint(&api.client.children, "login");
        assert!(login.docs.body.as_ref().is_some_and(|body| {
            body.argument == "body"
                && body.specification.contains("Json")
                && body.input_type.contains("RequestBodySpec")
        }));
        assert!(login.docs.response.specification.contains("Session"));
        assert_eq!(
            login
                .docs
                .endpoint_setters
                .iter()
                .map(|setter| {
                    (
                        setter.name.as_str(),
                        setter.clear.as_deref(),
                        setter.reset.as_deref(),
                    )
                })
                .collect::<Vec<_>>(),
            [
                ("remember", Some("clear_remember"), None),
                ("attempts", None, Some("reset_attempts")),
            ]
        );
        assert_eq!(login.docs.authentication.len(), 1);
        assert_eq!(login.docs.timeout, EndpointDocTimeout::Disabled);
        assert_eq!(
            login.docs.terminals.credential_storage,
            ["store_as_session"]
        );
        assert_eq!(login.docs.contract_id, "stable.login");
        assert!(login.docs.explicit_contract_id);

        let page = test_support::endpoint(&api.client.children, "page");
        assert!(page.docs.body.is_none());
        assert!(page.docs.authentication.is_empty());
        assert!(page.docs.safe_query_names.is_empty());
        assert!(page.docs.safe_header_names.is_empty());
        assert!(page.docs.terminals.pagination);
        assert_eq!(
            page.docs.timeout,
            EndpointDocTimeout::Inherited(Duration::from_secs(20))
        );

        let stream = test_support::endpoint(&api.client.children, "stream");
        assert!(stream.docs.response.specification.contains("Stream"));
        let discard = test_support::endpoint(&api.client.children, "discard");
        assert!(discard.docs.response.specification.contains("NoContent"));
        assert_eq!(
            discard.docs.timeout,
            EndpointDocTimeout::Overridden(Duration::from_millis(250))
        );
    }
}
