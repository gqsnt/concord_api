use std::collections::HashMap;

use proc_macro2::Span;
use syn::Ident;

use super::model::{
    Child, ClientSurface, CloneEligibility, Credential, CredentialSource,
    CredentialStorageTerminal, Endpoint, EndpointSetter, EndpointSurface, RequestTerminals,
    RequiredArgument, Scope, ScopeSurface, Value,
};

pub(crate) const RESERVED_GENERATED_METHODS: &[&str] = &[
    "new",
    "builder",
    "build",
    "configure_http",
    "configure_runtime",
    "retry",
    "credentials",
    "contract",
    "unbound",
    "request",
    "plan",
    "response",
    "accepted_bytes",
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
    accepted_bytes: bool,
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
            with: value
                .accepts_absence()
                .then(|| helper_ident("with", &value.name)),
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
            accepted_bytes,
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
        if let Some(with) = &setter.with {
            insert_name(
                &mut names,
                canonical_ident(with),
                with.span(),
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
            client SurfaceApi {
                timeout off; base "https://example.test";
            scope http_status() {
                GET get_http_status(optional?: bool) -> Bytes using Bytes;
            }
        }});
        let endpoint = test_support::endpoint(&api.client.children, "get_http_status");

        assert_eq!(endpoint.surface.method_name, "get_http_status");
        assert_eq!(endpoint.surface.endpoint_type_name, "GetHttpStatus");
        assert_eq!(endpoint.surface.request_type_name, "GetHttpStatusRequest");
        assert_eq!(endpoint.surface.module_path[0], "http_status");
        assert_eq!(endpoint.surface.setters.len(), 1);
        assert!(endpoint.surface.terminals.response);
        assert!(!endpoint.surface.terminals.pagination);
        assert_eq!(
            endpoint.surface.docs_identity,
            "SurfaceApi::http_status::get_http_status"
        );
    }
}
