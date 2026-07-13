//!
//! The request-header ownership boundary.
//!
//! The same source is used by public-header validation and authentication
//! placement preflight so no transport-managed name can slip between them.

use http::HeaderName;
use http::header::{
    ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HOST, PROXY_AUTHORIZATION,
    SET_COOKIE, TRANSFER_ENCODING, USER_AGENT,
};

const REQUEST_IDENTITY_HEADERS: [&str; 7] = [
    "idempotency-key",
    "request-id",
    "request-idempotency-key",
    "x-request-id",
    "x-request-idempotency-key",
    "x-correlation-id",
    "correlation-id",
];

pub(crate) fn is_protocol_or_reqwest_owned(name: &HeaderName) -> bool {
    matches!(
        *name,
        CONTENT_LENGTH
            | TRANSFER_ENCODING
            | ACCEPT_ENCODING
            | HOST
            | CONNECTION
            | PROXY_AUTHORIZATION
            | COOKIE
            | SET_COOKIE
    ) || matches!(
        name.as_str(),
        "keep-alive"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "upgrade"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "www-authenticate"
    )
}

pub(crate) fn is_request_identity_header_name_str(name: &str) -> bool {
    REQUEST_IDENTITY_HEADERS
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

pub(crate) fn is_request_identity_header_name(name: &HeaderName) -> bool {
    is_request_identity_header_name_str(name.as_str())
}

fn is_suffix_key_style_name(name: &str) -> bool {
    name.ends_with("-key") || name.ends_with("_key") || name.eq_ignore_ascii_case("key")
}

fn has_obvious_auth_secret_marker(name: &str) -> bool {
    [
        "authorization",
        "proxy-authorization",
        "www-authenticate",
        "cookie",
        "set-cookie",
        "api",
        "apikey",
        "api_key",
        "api-key",
        "access",
        "refresh",
        "session",
        "secret",
        "password",
        "credential",
        "token",
        "auth",
        "subscription",
        "client",
    ]
    .iter()
    .any(|blocked| name.contains(blocked))
}

pub(crate) fn is_retry_idempotency_exception_allowed(name: &HeaderName) -> bool {
    if is_request_identity_header_name(name) {
        return true;
    }
    if is_forbidden_auth_placement(name) {
        return false;
    }

    let name = name.as_str();
    if !is_suffix_key_style_name(name) {
        return false;
    }
    if name.eq_ignore_ascii_case("key") {
        return false;
    }
    !has_obvious_auth_secret_marker(name)
}

pub(crate) fn is_forbidden_auth_placement(name: &HeaderName) -> bool {
    is_protocol_or_reqwest_owned(name)
        || *name == CONTENT_TYPE
        || *name == USER_AGENT
        || *name == http::header::AUTHORIZATION
        || *name == http::header::CONTENT_LENGTH
        || is_request_identity_header_name(name)
}

pub(crate) fn is_forbidden_public_header(name: &HeaderName) -> bool {
    is_protocol_or_reqwest_owned(name)
        || *name == USER_AGENT
        || crate::redaction::is_credential_bearing_header_name(name.as_str())
}

pub(crate) fn is_forbidden_public_header_with_exceptions(
    name: &HeaderName,
    allowed_request_header_exceptions: &[HeaderName],
) -> bool {
    if !is_forbidden_public_header(name) {
        return false;
    }
    if is_forbidden_auth_placement(name) || is_protocol_or_reqwest_owned(name) {
        return true;
    }
    if !allowed_request_header_exceptions
        .iter()
        .any(|allowed| allowed == name)
    {
        return true;
    }

    !is_retry_idempotency_exception_allowed(name)
}

pub(crate) fn validate_public_headers_with_exceptions(
    headers: &http::HeaderMap,
    allowed_request_header_exceptions: &[HeaderName],
) -> Result<(), HeaderOwnershipError> {
    headers
        .keys()
        .find(|name| {
            is_forbidden_public_header_with_exceptions(name, allowed_request_header_exceptions)
        })
        .map(|name| HeaderOwnershipError { name: name.clone() })
        .map_or(Ok(()), Err)
}

pub(crate) fn validate_public_headers(
    headers: &http::HeaderMap,
) -> Result<(), HeaderOwnershipError> {
    validate_public_headers_with_exceptions(headers, &[])
}

#[derive(Clone, Eq, PartialEq)]
pub struct HeaderOwnershipError {
    name: HeaderName,
}

impl HeaderOwnershipError {
    pub fn header_name(&self) -> &HeaderName {
        &self.name
    }
}

impl std::fmt::Display for HeaderOwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if crate::redaction::is_credential_bearing_header_name(self.name.as_str()) {
            f.write_str("request ownership collision with credential-bearing header")
        } else {
            write!(
                f,
                "header `{}` is reserved by Concord or the HTTP transport",
                self.name
            )
        }
    }
}

impl std::fmt::Debug for HeaderOwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if crate::redaction::is_credential_bearing_header_name(self.name.as_str()) {
            f.debug_struct("HeaderOwnershipError")
                .field("kind", &"header_ownership_violation")
                .finish()
        } else {
            f.debug_struct("HeaderOwnershipError")
                .field("name", &self.name)
                .finish()
        }
    }
}

impl std::error::Error for HeaderOwnershipError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_bearing_public_headers_are_rejected() {
        for name in [
            "authorization",
            "x-api-key",
            "x-api-token",
            "x-auth-token",
            "x-access-token",
            "x-refresh-token",
            "x-session-token",
            "client-secret",
        ] {
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                http::HeaderValue::from_static("not-a-secret"),
            );
            assert!(validate_public_headers(&headers).is_err(), "{name}");
        }
    }

    #[test]
    fn ordinary_public_headers_remain_allowed() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-client-build", http::HeaderValue::from_static("public"));
        headers.insert(
            "x-client-meta",
            http::HeaderValue::from_static("public-metadata"),
        );
        assert!(validate_public_headers(&headers).is_ok());
    }

    #[test]
    fn protocol_owned_headers_are_rejected() {
        let mut headers = http::HeaderMap::new();
        headers.insert("set-cookie", http::HeaderValue::from_static("foo"));
        assert!(validate_public_headers(&headers).is_err());

        let mut headers = http::HeaderMap::new();
        headers.insert("www-authenticate", http::HeaderValue::from_static("basic"));
        assert!(validate_public_headers(&headers).is_err());
    }

    #[test]
    fn idempotency_header_can_be_rejected_when_not_request_identity_without_exception() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::HeaderName::from_static("x-retry-attempt-key"),
            http::HeaderValue::from_static("candidate"),
        );
        assert!(validate_public_headers(&headers).is_err());
    }

    #[test]
    fn idempotency_header_can_be_exempted_when_retry_header_is_configured() {
        for name in [
            "x-retry-attempt-key",
            "idempotency-key",
            "x-request-idempotency-key",
            "request-idempotency-key",
            "x-request-id",
            "request-id",
        ] {
            let mut headers = http::HeaderMap::new();
            let candidate = http::HeaderName::from_static(name);
            headers.append(candidate.clone(), http::HeaderValue::from_static("stable"));
            assert!(
                validate_public_headers_with_exceptions(&headers, std::slice::from_ref(&candidate))
                    .is_ok(),
                "{name}"
            );
            assert!(
                is_retry_idempotency_exception_allowed(&candidate),
                "exception must remain allowed: {name}"
            );
        }
    }

    #[test]
    fn credential_bearing_header_errors_are_sanitized_in_display_and_debug() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-api-key", http::HeaderValue::from_static("not-a-secret"));
        let error = validate_public_headers(&headers).expect_err("credential-bearing header");
        let rendered = format!("{error}");
        let debug = format!("{error:?}");
        assert!(!rendered.contains("x-api-key"));
        assert!(!debug.contains("x-api-key"));
        assert!(rendered.contains("request ownership"));
    }

    #[test]
    fn protocol_name_visible_in_non_credential_debug_and_display() {
        let mut headers = http::HeaderMap::new();
        headers.insert("www-authenticate", http::HeaderValue::from_static("basic"));
        let error = validate_public_headers(&headers).expect_err("protocol-owned headers");
        let rendered = format!("{error}");
        let debug = format!("{error:?}");
        assert!(rendered.contains("www-authenticate"));
        assert!(debug.contains("www-authenticate"));
        assert!(debug.contains("HeaderOwnershipError"));
    }

    #[test]
    fn retry_idempotency_exceptions_do_not_bypass_protocol_or_auth_ownership() {
        for name in [
            "authorization",
            "cookie",
            "set-cookie",
            "www-authenticate",
            "user-agent",
            "host",
            "proxy-authorization",
            "x-api-key",
        ] {
            let mut headers = http::HeaderMap::new();
            headers.insert(name, http::HeaderValue::from_static("sentinel"));
            let exception = [http::HeaderName::from_bytes(name.as_bytes()).expect("name")];
            assert!(
                validate_public_headers_with_exceptions(&headers, &exception).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn request_identity_names_are_forbidden_while_not_credential_owned() {
        for name in [
            "idempotency-key",
            "request-id",
            "request-idempotency-key",
            "x-request-id",
            "x-request-idempotency-key",
            "x-correlation-id",
            "correlation-id",
        ] {
            assert!(
                is_forbidden_auth_placement(&http::HeaderName::from_static(name)),
                "{name}"
            );
        }
    }

    #[test]
    fn configured_retry_idempotency_candidates_are_reserved_for_auth_placement_only_when_safe() {
        for name in [
            "idempotency-key",
            "x-request-id",
            "request-id",
            "request-idempotency-key",
            "x-request-idempotency-key",
            "request-idempotency-key",
            "x-retry-attempt-key",
        ] {
            let candidate = http::HeaderName::from_static(name);
            assert!(
                is_retry_idempotency_exception_allowed(&candidate),
                "must allow safe candidate: {name}"
            );
        }
        for name in [
            "authorization",
            "host",
            "content-length",
            "content-type",
            "user-agent",
            "proxy-authorization",
            "cookie",
            "set-cookie",
            "www-authenticate",
            "x-api-key",
            "x-client-key",
            "key",
        ] {
            let candidate = http::HeaderName::from_static(name);
            assert!(
                !is_retry_idempotency_exception_allowed(&candidate),
                "must reject unsafe candidate: {name}"
            );
        }
    }
}
