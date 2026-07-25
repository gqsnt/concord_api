use std::collections::HashSet;

use http::HeaderName;
use proc_macro2::Span;
use syn::{Ident, LitInt, LitStr, Type, TypePath};
use url::Url;

use super::ast::{AuthOperation, Clause, ValueDecl};
use crate::semantic::surface::{canonical_ident, to_snake_case};

pub(super) fn insert_clause<T>(
    slot: &mut Option<Clause<T>>,
    clause: Clause<T>,
    name: &str,
    owner: &str,
) -> syn::Result<()> {
    if let Some(first) = slot {
        let mut error = clause
            .key_span
            .error(format!("duplicate `{name}` clause for {owner}"));
        error.combine(
            first
                .key_span
                .error(format!("the first `{name}` clause is here")),
        );
        return Err(error);
    }
    *slot = Some(clause);
    Ok(())
}

pub(super) fn merge_auth(
    slot: &mut Option<AuthOperation>,
    operation: AuthOperation,
    owner: &str,
) -> syn::Result<()> {
    let Some(existing) = slot else {
        *slot = Some(operation);
        return Ok(());
    };
    match (existing, operation) {
        (
            AuthOperation::Modify {
                replace,
                placements,
                clause_spans,
            },
            AuthOperation::Modify {
                replace: incoming_replace,
                placements: mut incoming,
                clause_spans: incoming_spans,
            },
        ) => {
            if incoming_replace {
                let duplicate_span = incoming_spans[0];
                let mut error = duplicate_span.error(format!(
                    "`auth replace` must be the first authentication entry for {owner}"
                ));
                error.combine(clause_spans[0].error("the first authentication entry is here"));
                return Err(error);
            }
            *replace |= incoming_replace;
            placements.append(&mut incoming);
            clause_spans.extend(incoming_spans);
            Ok(())
        }
        (AuthOperation::Off { key_span }, incoming) => {
            let incoming_span = match incoming {
                AuthOperation::Modify { clause_spans, .. } => clause_spans[0],
                AuthOperation::Off { key_span } => key_span,
            };
            let mut error = incoming_span.error(format!("`auth off` is exclusive within {owner}"));
            error.combine((*key_span).error("the exclusive `auth off` clause is here"));
            Err(error)
        }
        (AuthOperation::Modify { clause_spans, .. }, AuthOperation::Off { key_span }) => {
            let mut error = key_span.error(format!("`auth off` is exclusive within {owner}"));
            error.combine(clause_spans[0].error("the first authentication entry is here"));
            Err(error)
        }
    }
}

pub(super) fn is_named_marker(ty: &Type, expected: &str) -> bool {
    matches!(ty, Type::Path(path) if is_unqualified(path, expected))
}

fn is_unqualified(path: &TypePath, expected: &str) -> bool {
    path.qself.is_none()
        && path.path.leading_colon.is_none()
        && path.path.segments.len() == 1
        && path.path.segments[0].ident == expected
}

pub(super) fn validate_duration_integer(literal: &LitInt, owner: &str) -> syn::Result<()> {
    let text = literal.to_string().replace('_', "");
    let digits = text.strip_suffix(literal.suffix()).unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(literal
            .span()
            .error(format!("{owner} integer literals must be base-10 values")));
    }
    Ok(())
}

pub(super) fn validate_integer(literal: &LitInt, owner: &str) -> syn::Result<()> {
    if !literal.suffix().is_empty() {
        return Err(literal.span().error(format!(
            "{owner} integer literals must be unsuffixed base-10 values"
        )));
    }
    let text = literal.to_string().replace('_', "");
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(literal.span().error(format!(
            "{owner} integer literals must be unsuffixed base-10 values"
        )));
    }
    Ok(())
}

pub(super) fn validate_base(base: &LitStr) -> syn::Result<()> {
    let parsed = Url::parse(&base.value()).map_err(|_| {
        base.span()
            .error("the fixed base must be a valid absolute HTTP or HTTPS URL")
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(base.span().error(
            "the fixed base must be absolute HTTP(S), contain a host, and contain no user information, query, or fragment",
        ));
    }
    Ok(())
}

pub(super) fn validate_snake_source_identifier(name: &Ident, owner: &str) -> syn::Result<()> {
    let canonical = canonical_ident(name);
    let expected = to_snake_case(&canonical);
    if canonical != expected {
        return Err(name.span().error(format!(
            "{owner} source identifier `{canonical}` must be snake_case; use `{expected}`"
        )));
    }
    Ok(())
}

pub(super) fn validate_contract_id(id: &LitStr) -> syn::Result<()> {
    let value = id.value();
    let valid = (1..=128).contains(&value.len())
        && value.is_ascii()
        && value.split('.').all(|segment| {
            let bytes = segment.as_bytes();
            (1..=63).contains(&bytes.len())
                && bytes.iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
                && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        });
    if !valid {
        return Err(id.span().error(
            "an endpoint ID must be 1-128 ASCII bytes in dot-separated 1-63 byte segments; each segment begins and ends with a lowercase letter or digit and may also contain `_` or `-`",
        ));
    }
    Ok(())
}

pub(super) fn validate_static_host_label(label: &LitStr) -> syn::Result<()> {
    let value = label.value();
    let bytes = value.as_bytes();
    if !(1..=63).contains(&bytes.len())
        || !bytes.is_ascii()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(label.span().error(
            "a static host atom must be one 1-63 byte ASCII DNS label beginning and ending with an alphanumeric character",
        ));
    }
    Ok(())
}

pub(super) fn validate_static_segment(segment: &LitStr) -> syn::Result<()> {
    let value = segment.value();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(segment.span().error(
            "a static path atom must be one non-empty, non-dot segment without `/` or `\\`",
        ));
    }
    Ok(())
}

pub(super) fn validate_public_header_name(name: &LitStr) -> syn::Result<()> {
    let parsed = HeaderName::from_bytes(name.value().as_bytes()).map_err(|_| {
        name.span()
            .error("public header name is not a valid HTTP header name")
    })?;
    if matches!(
        parsed.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "upgrade"
            | "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
    ) {
        return Err(name
            .span()
            .error("this public header name is reserved by transport, authentication, cookies, or proxy authentication"));
    }
    Ok(())
}

pub(super) fn validate_auth_header_name(name: &LitStr) -> syn::Result<()> {
    validate_public_header_name(name)?;
    validate_auth_placement_name_bytes(name, "header")
}

pub(super) fn validate_auth_query_name(name: &LitStr) -> syn::Result<()> {
    if name.value().is_empty() {
        return Err(name
            .span()
            .error("an authentication query name cannot be empty"));
    }
    validate_auth_placement_name_bytes(name, "query")
}

fn validate_auth_placement_name_bytes(name: &LitStr, kind: &str) -> syn::Result<()> {
    if name.value().len() > crate::MAX_AUTH_PLACEMENT_NAME_BYTES {
        return Err(name.span().error(format!(
            "an authentication {kind} name permits at most {} UTF-8 bytes",
            crate::MAX_AUTH_PLACEMENT_NAME_BYTES
        )));
    }
    Ok(())
}

pub(super) fn insert_unique_value(
    names: &mut HashSet<String>,
    value: &ValueDecl,
    owner: &str,
) -> syn::Result<()> {
    let name = canonical_ident(&value.name);
    if !names.insert(name.clone()) {
        return Err(value
            .name
            .span()
            .error(format!("duplicate {owner} `{name}`")));
    }
    Ok(())
}

pub(super) trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    #[test]
    fn duplicate_clauses_retain_both_diagnostic_spans() {
        let error = super::super::parse(quote! {
            client DuplicateApi { base "https://example.test"; }
            GET read() -> Bytes {
                path ["first"];
                path ["second"];
            }
        })
        .err()
        .expect("duplicate clause");
        let diagnostic = error.into_compile_error().to_string();

        assert_eq!(diagnostic.matches("compile_error").count(), 2);
        assert!(diagnostic.contains("duplicate `path` clause"));
        assert!(diagnostic.contains("first `path` clause"));
    }
}
