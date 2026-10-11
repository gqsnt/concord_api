//! Deterministic generated-name conversion (CALL-01).
//!
//! Raw markers are removed before conversion. Snake conversion inserts `_`
//! before an uppercase letter that follows a lowercase letter or digit, and
//! before the last uppercase letter of an acronym that is followed by a
//! lowercase letter; the result is lowercased. Pascal conversion capitalizes
//! each underscore-separated logical word. Collisions are rejected by the
//! resolver; no hidden suffix, truncation or random name is ever used.

use proc_macro2::Span;
use syn::Ident;

pub(crate) fn canonical_ident(name: &Ident) -> String {
    let name = name.to_string();
    name.strip_prefix("r#").unwrap_or(&name).to_owned()
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                let previous = characters[index - 1];
                let previous_is_word_tail = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_end = previous.is_uppercase()
                    && characters
                        .get(index + 1)
                        .is_some_and(|next| next.is_lowercase());
                if previous_is_word_tail || acronym_end {
                    output.push('_');
                }
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

/// `prefix_name` with the raw marker removed from `name`.
pub(crate) fn prefixed_ident(prefix: &str, name: &Ident) -> Ident {
    public_ident(&format!("{prefix}_{}", canonical_ident(name)), name.span())
}

/// `name_suffix` with the raw marker removed from `name`.
pub(crate) fn suffixed_ident(name: &Ident, suffix: &str) -> Ident {
    public_ident(&format!("{}_{suffix}", canonical_ident(name)), name.span())
}

pub(crate) fn is_rust_keyword(value: &str) -> bool {
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
            | "gen"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_conversion_covers_acronyms_and_digit_transitions() {
        assert_eq!(to_snake_case("HTTPStatus"), "http_status");
        assert_eq!(to_snake_case("CatalogApi"), "catalog_api");
        assert_eq!(to_snake_case("Api2Client"), "api2_client");
        assert_eq!(to_snake_case("V2Api"), "v2_api");
        assert_eq!(to_snake_case("OAuth2Token"), "o_auth2_token");
        assert_eq!(to_snake_case("RiotAPI"), "riot_api");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
        assert_eq!(to_snake_case("r#type"), "type");
        assert_eq!(to_snake_case("ABC"), "abc");
    }

    #[test]
    fn pascal_conversion_capitalizes_logical_words() {
        assert_eq!(to_pascal_case("item_summaries"), "ItemSummaries");
        assert_eq!(to_pascal_case("get_http_status"), "GetHttpStatus");
        assert_eq!(to_pascal_case("v2_lookup"), "V2Lookup");
        assert_eq!(to_pascal_case("regional"), "Regional");
    }

    #[test]
    fn keywords_become_raw_identifiers() {
        assert_eq!(
            public_ident("type", Span::call_site()).to_string(),
            "r#type"
        );
        assert_eq!(
            public_ident("types", Span::call_site()).to_string(),
            "types"
        );
    }
}
