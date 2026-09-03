use std::collections::HashSet;

use http::HeaderName;
use proc_macro2::Span;
use syn::{Ident, LitInt, LitStr};
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
    let first_span = match existing {
        AuthOperation::Modify { clause_spans, .. } => clause_spans[0],
        AuthOperation::Off { key_span } => *key_span,
    };
    let incoming_span = match operation {
        AuthOperation::Modify { clause_spans, .. } => clause_spans[0],
        AuthOperation::Off { key_span } => key_span,
    };
    let mut error = incoming_span.error(format!(
        "a {owner} accepts one grouped `auth` attachment; `auth off` is exclusive"
    ));
    error.combine(first_span.error("the first authentication attachment is here"));
    Err(error)
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
    let parsed = parse_header_name(name, "public")?;
    if reserved_public_header(&parsed) {
        return Err(name
            .span()
            .error("this public header name is reserved by transport, authentication, cookies, or proxy authentication"));
    }
    Ok(())
}

pub(super) fn validate_auth_header_name(name: &LitStr) -> syn::Result<()> {
    let parsed = parse_header_name(name, "authentication")?;
    if reserved_public_header(&parsed)
        || matches!(parsed.as_str(), "accept" | "content-type" | "user-agent")
    {
        return Err(name.span().error(
            "this authentication header name is owned by Concord request or transport processing",
        ));
    }
    validate_auth_placement_name_bytes(name, "header")
}

fn parse_header_name(name: &LitStr, owner: &str) -> syn::Result<HeaderName> {
    HeaderName::from_bytes(name.value().as_bytes()).map_err(|_| {
        name.span().error(format!(
            "{owner} header name is not a valid HTTP header name"
        ))
    })
}

fn reserved_public_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
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
    )
}

pub(super) fn validate_auth_query_name(name: &LitStr) -> syn::Result<()> {
    if name.value().is_empty() {
        return Err(name
            .span()
            .error("an authentication query name cannot be empty"));
    }
    validate_auth_placement_name_bytes(name, "query")
}

pub(super) fn validate_authorization_scheme(scheme: &LitStr) -> syn::Result<()> {
    let value = scheme.value();
    if value.is_empty() {
        return Err(scheme
            .span()
            .error("an Authorization scheme cannot be empty"));
    }
    if value.len() > crate::MAX_AUTHORIZATION_SCHEME_BYTES {
        return Err(scheme.span().error(format!(
            "an Authorization scheme permits at most {} ASCII bytes",
            crate::MAX_AUTHORIZATION_SCHEME_BYTES
        )));
    }
    if !value.bytes().all(is_http_token_byte) {
        return Err(scheme
            .span()
            .error("an Authorization scheme must contain only ASCII HTTP token characters"));
    }
    Ok(())
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
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
    use proc_macro2::Span;
    use quote::quote;
    use syn::LitStr;

    use super::validate_auth_header_name;

    #[test]
    fn duplicate_clauses_retain_both_diagnostic_spans() {
        let error = super::super::parse(quote! {
            client DuplicateApi {
                timeout off; base "https://example.test";
            GET read() -> Bytes using Bytes {
                path ["first"];
                path ["second"];
            }
        }})
        .err()
        .expect("duplicate clause");
        let diagnostic = error.into_compile_error().to_string();

        assert_eq!(diagnostic.matches("compile_error").count(), 2);
        assert!(diagnostic.contains("duplicate `path` clause"));
        assert!(diagnostic.contains("first `path` clause"));
    }

    #[test]
    fn static_auth_header_policy_matches_the_complete_runtime_contract() {
        for reserved in [
            "host",
            "content-length",
            "transfer-encoding",
            "connection",
            "keep-alive",
            "te",
            "trailer",
            "upgrade",
            "proxy-authorization",
            "authorization",
            "cookie",
            "set-cookie",
            "accept",
            "content-type",
            "user-agent",
        ] {
            let literal = LitStr::new(reserved, Span::call_site());
            let error = validate_auth_header_name(&literal)
                .expect_err("request-owned authentication header must fail");
            assert!(
                error
                    .to_string()
                    .contains("owned by Concord request or transport processing")
            );
        }

        validate_auth_header_name(&LitStr::new("X-API-Key", Span::call_site()))
            .expect("ordinary custom authentication header");
    }
}
