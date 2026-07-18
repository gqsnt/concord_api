use proc_macro2::Span;
use syn::Ident;

pub(crate) const RESERVED_GENERATED_METHODS: &[&str] = &[
    "new",
    "builder",
    "build",
    "http",
    "runtime",
    "credentials",
    "request",
    "response",
    "raw_response",
    "paginate",
    "timeout",
    "disable_timeout",
    "inherit_timeout",
    "debug",
    "__concord_request_extension_seal",
];

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

pub(crate) fn request_ext_name(endpoint: &Ident) -> Ident {
    Ident::new(
        &format!("{}RequestExt", to_pascal_case(&canonical_ident(endpoint))),
        endpoint.span(),
    )
}

pub(crate) fn prelude_alias(
    client: &Ident,
    scope_path: &[Ident],
    endpoint_public_name: &Ident,
) -> Ident {
    let mut value = to_pascal_case(&canonical_ident(client));
    for scope in scope_path {
        value.push_str(&to_pascal_case(&canonical_ident(scope)));
    }
    value.push_str(&to_pascal_case(&canonical_ident(endpoint_public_name)));
    value.push_str("RequestExt");
    Ident::new(&value, endpoint_public_name.span())
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
    use syn::parse_quote;

    use super::*;

    #[test]
    fn deterministic_names_cover_acronyms_keywords_and_traits() {
        assert_eq!(to_snake_case("HTTPStatus"), "http_status");
        assert_eq!(to_pascal_case("user_resources"), "UserResources");
        assert_eq!(snake_ident(&parse_quote!(r#type)).to_string(), "r#type");
        assert_eq!(
            prelude_alias(
                &parse_quote!(ExampleApi),
                &[parse_quote!(regions), parse_quote!(users)],
                &parse_quote!(by_id),
            ),
            "ExampleApiRegionsUsersByIdRequestExt"
        );
    }
}
