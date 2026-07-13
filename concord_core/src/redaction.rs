use http::{HeaderMap, HeaderValue};

pub(crate) fn is_sensitive_name(name: &str) -> bool {
    matches_ignore_ascii_case(
        name,
        &[
            "authorization",
            "proxy-authorization",
            "cookie",
            "set-cookie",
            "www-authenticate",
            "x-api-key",
            "x-api-token",
            "x-auth-token",
            "x-access-token",
            "x-refresh-token",
            "x-session-token",
            "access_token",
            "refresh_token",
            "api_key",
            "apikey",
            "key",
            "token",
            "secret",
            "password",
            "auth",
        ],
    ) || contains_ignore_ascii_case(name, "token")
        || contains_ignore_ascii_case(name, "secret")
        || contains_ignore_ascii_case(name, "api-key")
        || contains_ignore_ascii_case(name, "apikey")
        || contains_ignore_ascii_case(name, "session")
        || contains_ignore_ascii_case(name, "credential")
        || contains_ignore_ascii_case(name, "authorization")
        || ends_with_ignore_ascii_case(name, "_key")
        || ends_with_ignore_ascii_case(name, "-key")
}

/// Names whose public request values must remain outside of client-owned
/// credential flow.
pub(crate) fn is_credential_bearing_header_name(name: &str) -> bool {
    if crate::header_ownership::is_request_identity_header_name_str(name) {
        return false;
    }

    if matches_ignore_ascii_case(
        name,
        &["set-cookie", "www-authenticate", "proxy-authenticate"],
    ) {
        return false;
    }

    matches_ignore_ascii_case(
        name,
        &[
            "authorization",
            "proxy-authorization",
            "api-key",
            "access-token",
            "refresh-token",
            "session-token",
            "api-token",
            "x-api-key",
            "x-client-key",
            "x-subscription-key",
            "ocp-apim-subscription-key",
            "cookie",
        ],
    ) || contains_ignore_ascii_case(name, "authorization")
        || contains_ignore_ascii_case(name, "credential")
        || contains_ignore_ascii_case(name, "secret")
        || contains_ignore_ascii_case(name, "password")
        || contains_ignore_ascii_case(name, "token")
        || contains_ignore_ascii_case(name, "auth")
        || contains_ignore_ascii_case(name, "api")
            && (contains_ignore_ascii_case(name, "key")
                || contains_ignore_ascii_case(name, "token"))
        || ends_with_ignore_ascii_case(name, "_key")
        || ends_with_ignore_ascii_case(name, "-key")
}

fn matches_ignore_ascii_case(name: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn ends_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    haystack.len() >= needle.len()
        && haystack[haystack.len() - needle.len()..]
            .iter()
            .zip(needle.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

pub(crate) fn should_redact_header_name(name: &http::HeaderName) -> bool {
    is_sensitive_name(name.as_str())
}

pub(crate) fn sanitize_header_map(headers: &HeaderMap) -> HeaderMap {
    let mut sanitized = HeaderMap::new();
    for (name, value) in headers {
        if should_redact_header_name(name) {
            sanitized.append(name.clone(), HeaderValue::from_static("<redacted>"));
        } else {
            sanitized.append(name.clone(), value.clone());
        }
    }
    sanitized
}

pub(crate) fn sanitize_url_for_debug<I, S>(url: &url::Url, sensitive_query_keys: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    I::IntoIter: Clone,
{
    if url.query().is_none() {
        return url.to_string();
    }

    let sensitive_query_keys = sensitive_query_keys.into_iter();
    let mut out = url.clone();
    out.set_query(None);
    {
        let mut pairs = out.query_pairs_mut();
        for (key, value) in url.query_pairs() {
            let redacted = sensitive_query_keys
                .clone()
                .any(|candidate| key.eq_ignore_ascii_case(candidate.as_ref()))
                || is_sensitive_name(&key);

            if redacted {
                pairs.append_pair(&key, "<redacted>");
            } else {
                pairs.append_pair(&key, &value);
            }
        }
    }

    out.to_string().replace("%3Credacted%3E", "<redacted>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_name_rules_cover_auth_and_secret_carriers() {
        for name in [
            "authorization",
            "proxy-authorization",
            "cookie",
            "set-cookie",
            "access_token",
            "x-api-key",
            "client_secret",
            "refresh-token",
        ] {
            assert!(is_sensitive_name(name), "{name} should be sensitive");
        }

        assert!(!is_sensitive_name("accept"));
    }

    #[test]
    fn sensitive_name_rules_are_ascii_case_insensitive_and_keep_non_sensitive_visible() {
        for name in [
            "Authorization",
            "X-Api-Key",
            "x-API-TOKEN",
            "X-Auth-Token",
            "x-access-token",
            "X-Refresh-Token",
            "X-Session-Token",
            "client_secret",
            "X-CREDENTIAL-ID",
            "My-Token-Header",
            "apiKEY",
            "prefix_secret_suffix",
            "trailing-key",
        ] {
            assert!(is_sensitive_name(name), "{name} should be sensitive");
        }

        for name in ["accept", "x-visible-id", "x-public-handle"] {
            assert!(!is_sensitive_name(name), "{name} should stay visible");
        }
    }

    #[test]
    fn credential_bearing_name_classifier_targets_authentication_categories() {
        assert!(is_credential_bearing_header_name("authorization"));
        assert!(is_credential_bearing_header_name("x-api-key"));
        assert!(is_credential_bearing_header_name("access_token"));
        assert!(is_credential_bearing_header_name("x-session-token"));
        assert!(is_credential_bearing_header_name("cookie"));
        assert!(is_credential_bearing_header_name("x-client-key"));
        assert!(is_credential_bearing_header_name("x-subscription-key"));
        assert!(is_credential_bearing_header_name(
            "Ocp-Apim-Subscription-Key"
        ));
        assert!(is_credential_bearing_header_name("authorization"));

        assert!(!is_credential_bearing_header_name("idempotency-key"));
        assert!(!is_credential_bearing_header_name("x-request-id"));
        assert!(!is_credential_bearing_header_name("x-correlation-id"));
        assert!(!is_credential_bearing_header_name("set-cookie"));
        assert!(!is_credential_bearing_header_name("www-authenticate"));
        assert!(!is_credential_bearing_header_name("proxy-authenticate"));
        assert!(!is_credential_bearing_header_name("x-client-meta"));
        assert!(!is_credential_bearing_header_name("x-public-scope"));
    }

    #[test]
    fn debug_url_redacts_default_and_explicit_sensitive_query_values() {
        let url: url::Url =
            "https://example.com/items?page=2&api_key=real-secret&x-private-provider-key=custom"
                .parse()
                .expect("valid url");

        let rendered = sanitize_url_for_debug(&url, ["x-private-provider-key"]);

        assert!(rendered.contains("page=2"));
        assert!(rendered.contains("api_key=<redacted>"));
        assert!(rendered.contains("x-private-provider-key=<redacted>"));
        assert!(!rendered.contains("real-secret"));
        assert!(!rendered.contains("custom"));
    }

    #[test]
    fn debug_url_redacts_case_insensitive_duplicate_sensitive_query_values() {
        let url: url::Url =
            "https://example.com/items?API_KEY=one&api_key=two&Token=three&visible=ok"
                .parse()
                .expect("valid url");

        let rendered = sanitize_url_for_debug(&url, ["api_key"]);

        assert!(rendered.contains("API_KEY=<redacted>"));
        assert!(rendered.contains("api_key=<redacted>"));
        assert!(rendered.contains("Token=<redacted>"));
        assert!(rendered.contains("visible=ok"));
        assert!(!rendered.contains("one"));
        assert!(!rendered.contains("two"));
        assert!(!rendered.contains("three"));
    }

    #[test]
    fn debug_url_reencodes_non_sensitive_query_values() {
        let url: url::Url =
            "https://example.com/items?visible=a%26api_key%3Dreal-secret&api_key=actual-secret"
                .parse()
                .expect("valid url");

        let rendered = sanitize_url_for_debug(&url, ["api_key"]);

        assert!(rendered.contains("api_key=<redacted>"));
        assert!(!rendered.contains("actual-secret"));

        // The encoded separator inside the non-sensitive value must not become
        // a fake second query parameter in debug output.
        assert!(!rendered.contains("visible=a&api_key=real-secret"));
        assert!(
            rendered.contains("visible=a%26api_key%3Dreal-secret")
                || rendered.contains("visible=a%26api_key=real-secret")
        );
    }
}
