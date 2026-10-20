//! Diagnostic construction and lexical validation shared by the parser and
//! the semantic resolver. Every custom diagnostic carries a stable Concord
//! code (§19.1) as a bracketed prefix, names its owning client, scope or
//! endpoint with an owner label (for example ``endpoint `Api::regional::item` ``)
//! and, where one exists, a correction valid in the v3 grammar. Messages are
//! value-free: they never echo secret or initializer values.

use http::HeaderName;
use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitInt, LitStr};
use url::Url;

use super::ast::Clause;
use crate::semantic::surface::{canonical_ident, to_snake_case};

/// Suggests grammar words without consuming the unexpected token.
pub(crate) fn keyword_suggestion(input: ParseStream<'_>, candidates: &[&str]) -> String {
    input
        .fork()
        .call(Ident::parse_any)
        .map(|written| crate::suggestions::suggestion(&canonical_ident(&written), candidates))
        .unwrap_or_default()
}

/// Retains the parser's original diagnostic and span when adding a keyword hint.
pub(crate) fn parse_with_keywords<T: Parse>(
    input: ParseStream<'_>,
    candidates: &[&str],
) -> syn::Result<T> {
    input.parse().map_err(|error| {
        let hint = keyword_suggestion(input, candidates);
        if hint.is_empty() {
            error
        } else {
            syn::Error::new(error.span(), format!("{error}{hint}"))
        }
    })
}

/// Stable diagnostic families (§19.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Code {
    Name,
    Place,
    Override,
    Atom,
    Path,
    Owner,
    Rep,
    Auth,
    Rate,
    Page,
    Generated,
    Migrate,
}

impl Code {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "C3-NAME",
            Self::Place => "C3-PLACE",
            Self::Override => "C3-OVERRIDE",
            Self::Atom => "C3-ATOM",
            Self::Path => "C3-PATH",
            Self::Owner => "C3-OWNER",
            Self::Rep => "C3-REP",
            Self::Auth => "C3-AUTH",
            Self::Rate => "C3-RATE",
            Self::Page => "C3-PAGE",
            Self::Generated => "C3-GENERATED",
            Self::Migrate => "C3-MIGRATE",
        }
    }
}

/// Owner label of a client: ``client `Api` ``.
pub(crate) fn client_label(name: &str) -> String {
    format!("client `{name}`")
}

/// Owner label of a scope: ``scope `Api::regional` ``.
pub(crate) fn scope_label(path: &str) -> String {
    format!("scope `{path}`")
}

/// Owner label of an endpoint: ``endpoint `Api::regional::item` ``.
pub(crate) fn endpoint_label(path: &str) -> String {
    format!("endpoint `{path}`")
}

/// Builds a coded diagnostic at `span`.
pub(crate) fn coded(span: Span, code: Code, message: impl std::fmt::Display) -> syn::Error {
    syn::Error::new(span, format!("[{}] {message}", code.as_str()))
}

/// Adds a secondary (related) span to an existing diagnostic.
pub(crate) fn related(
    mut error: syn::Error,
    span: Span,
    message: impl std::fmt::Display,
) -> syn::Error {
    error.combine(syn::Error::new(span, format!("note: {message}")));
    error
}

pub(crate) fn insert_clause<T>(
    slot: &mut Option<Clause<T>>,
    clause: Clause<T>,
    name: &str,
    owner: &str,
) -> syn::Result<()> {
    if let Some(first) = slot {
        let error = coded(
            clause.key_span,
            Code::Owner,
            format!(
                "duplicate `{name}` clause for {owner}; singleton clauses are not last-writer-wins, so keep exactly one `{name}` clause"
            ),
        );
        return Err(related(
            error,
            first.key_span,
            format!("the first `{name}` clause is here"),
        ));
    }
    *slot = Some(clause);
    Ok(())
}

/// `what` names the literal's role (`atom`, `initializer`); `owner` is the
/// owning client/scope/endpoint label.
pub(crate) fn validate_integer(literal: &LitInt, what: &str, owner: &str) -> syn::Result<()> {
    let error = || {
        coded(
            literal.span(),
            Code::Atom,
            format!(
                "{what} integer literals in {owner} must be unsuffixed base-10 values; write the number alone (`42`, not `42u32` or `0x2a`)"
            ),
        )
    };
    if !literal.suffix().is_empty() {
        return Err(error());
    }
    let text = literal.to_string().replace('_', "");
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error());
    }
    Ok(())
}

pub(crate) fn validate_base(base: &LitStr, owner: &str) -> syn::Result<()> {
    let parsed = Url::parse(&base.value()).map_err(|_| {
        coded(
            base.span(),
            Code::Owner,
            format!(
                "the fixed base of {owner} must be a valid absolute HTTP or HTTPS URL; write `base \"https://api.example.com/\";`"
            ),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(coded(
            base.span(),
            Code::Owner,
            format!(
                "the fixed base of {owner} must be absolute HTTP(S), contain a host, and contain no user information, query, or fragment; declare credentials in `credentials {{ ... }}` and query pairs in `query {{ ... }}`"
            ),
        ));
    }
    Ok(())
}

/// `what` is `scope` or `endpoint`; `owner` labels the declaring parent.
pub(crate) fn validate_snake_source_identifier(
    name: &Ident,
    what: &str,
    owner: &str,
) -> syn::Result<()> {
    let canonical = canonical_ident(name);
    let expected = to_snake_case(&canonical);
    if canonical != expected {
        return Err(coded(
            name.span(),
            Code::Generated,
            format!(
                "{what} source identifier `{canonical}` must be snake_case (declared in {owner}); write `{expected}`"
            ),
        ));
    }
    Ok(())
}

/// Operational endpoint ID grammar (IDENTITY-01).
pub(crate) fn is_valid_endpoint_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.is_ascii()
        && value.split('.').all(|segment| {
            let bytes = segment.as_bytes();
            (1..=63).contains(&bytes.len())
                && bytes.iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && bytes
                    .first()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && bytes
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

pub(crate) fn validate_endpoint_id(id: &LitStr, owner: &str) -> syn::Result<()> {
    if !is_valid_endpoint_id(&id.value()) {
        return Err(coded(
            id.span(),
            Code::Generated,
            format!(
                "the explicit ID of {owner} is invalid: an endpoint ID must be 1-128 ASCII bytes in dot-separated 1-63 byte segments; each segment begins and ends with a lowercase letter or digit and may also contain `_` or `-` (for example `id \"catalog.items.get\";`)"
            ),
        ));
    }
    Ok(())
}

/// One lowercase-normalized ASCII DNS label (ORIGIN-02).
pub(crate) fn validate_static_host_label(label: &LitStr, owner: &str) -> syn::Result<()> {
    let value = label.value();
    let bytes = value.as_bytes();
    if !(1..=63).contains(&bytes.len())
        || !bytes.is_ascii()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        || bytes.first() == Some(&b'-')
        || bytes.last() == Some(&b'-')
    {
        return Err(coded(
            label.span(),
            Code::Atom,
            format!(
                "a subdomain literal of {owner} must be exactly one 1-63 byte lowercase ASCII DNS label (letters, digits, hyphens; no leading/trailing hyphen, no dots); write one list atom per label, such as `subdomain [\"eu\", \"api\"];`"
            ),
        ));
    }
    Ok(())
}

/// Header names never available as generic public targets (HEAD-01).
pub(crate) const RESERVED_PUBLIC_HEADERS: &[&str] = &[
    "host",
    "authorization",
    "proxy-authorization",
    "proxy-authenticate",
    "accept",
    "accept-encoding",
    "content-type",
    "content-encoding",
    "content-length",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "te",
    "trailer",
    "upgrade",
    "proxy-connection",
    "cookie",
    "set-cookie",
];

/// Singleton request fields that cannot be repeated (HEAD-01).
pub(crate) const SINGLETON_HEADERS: &[&str] = &[
    "user-agent",
    "referer",
    "from",
    "date",
    "if-modified-since",
    "if-unmodified-since",
    "if-range",
    "range",
    "max-forwards",
    "origin",
    "access-control-request-method",
    "access-control-request-headers",
];

pub(crate) fn validate_public_header_name(name: &LitStr, owner: &str) -> syn::Result<HeaderName> {
    let value = name.value();
    if value.len() > 256 {
        return Err(coded(
            name.span(),
            Code::Place,
            format!(
                "a public header name of {owner} permits at most 256 bytes; write a shorter field name"
            ),
        ));
    }
    let parsed = HeaderName::from_bytes(value.as_bytes()).map_err(|_| {
        coded(
            name.span(),
            Code::Place,
            format!(
                "public header name of {owner} is not a valid HTTP field-name token; write a token such as `\"X-Request-Id\"`"
            ),
        )
    })?;
    if RESERVED_PUBLIC_HEADERS.contains(&parsed.as_str()) {
        return Err(coded(
            name.span(),
            Code::Place,
            format!(
                "`{value}` is not a generic public-header target of {owner}; it is owned by transport, authentication, representation negotiation, body media, cookies or proxy handling (place credentials with `auth {{ ... }}`; representations own media and negotiation)"
            ),
        ));
    }
    Ok(parsed)
}

pub(crate) fn validate_query_key(key: &LitStr, owner: &str) -> syn::Result<()> {
    let value = key.value();
    if value.is_empty() || value.len() > 256 {
        return Err(coded(
            key.span(),
            Code::Place,
            format!(
                "a query key of {owner} must be a nonempty static string of at most 256 UTF-8 bytes; write a key such as `\"page\"`"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_auth_header_name(name: &LitStr, owner: &str) -> syn::Result<()> {
    let parsed = HeaderName::from_bytes(name.value().as_bytes()).map_err(|_| {
        coded(
            name.span(),
            Code::Auth,
            format!(
                "authentication header name of {owner} is not a valid HTTP header name; write a token such as `header \"X-Api-Key\" = key;`"
            ),
        )
    })?;
    if RESERVED_PUBLIC_HEADERS.contains(&parsed.as_str()) || parsed.as_str() == "user-agent" {
        return Err(coded(
            name.span(),
            Code::Auth,
            format!(
                "this authentication header name of {owner} is owned by Concord request or transport processing; write `bearer`, `basic` or `authorization` for `Authorization`, or choose a custom header name"
            ),
        ));
    }
    validate_auth_placement_name_bytes(name, "header", owner)
}

pub(crate) fn validate_auth_query_name(name: &LitStr, owner: &str) -> syn::Result<()> {
    if name.value().is_empty() {
        return Err(coded(
            name.span(),
            Code::Auth,
            format!(
                "an authentication query name of {owner} cannot be empty; write a key such as `query \"api_key\" = key;`"
            ),
        ));
    }
    validate_auth_placement_name_bytes(name, "query", owner)
}

pub(crate) fn validate_authorization_scheme(scheme: &LitStr, owner: &str) -> syn::Result<()> {
    let value = scheme.value();
    if value.is_empty()
        || value.len() > crate::MAX_AUTHORIZATION_SCHEME_BYTES
        || !value.bytes().all(is_http_token_byte)
    {
        return Err(coded(
            scheme.span(),
            Code::Auth,
            format!(
                "an Authorization scheme of {owner} must be a nonempty HTTP token of at most {} bytes; write a scheme such as `authorization \"Token\" = key;`",
                crate::MAX_AUTHORIZATION_SCHEME_BYTES
            ),
        ));
    }
    Ok(())
}

pub(crate) fn is_http_token_byte(byte: u8) -> bool {
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

fn validate_auth_placement_name_bytes(name: &LitStr, kind: &str, owner: &str) -> syn::Result<()> {
    if name.value().len() > crate::MAX_AUTH_PLACEMENT_NAME_BYTES {
        return Err(coded(
            name.span(),
            Code::Auth,
            format!(
                "an authentication {kind} name of {owner} permits at most {} UTF-8 bytes; write a shorter name",
                crate::MAX_AUTH_PLACEMENT_NAME_BYTES
            ),
        ));
    }
    Ok(())
}
