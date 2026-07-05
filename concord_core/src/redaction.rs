use http::{HeaderMap, HeaderValue};

pub(crate) fn is_sensitive_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "www-authenticate"
            | "x-api-key"
            | "x-api-token"
            | "x-auth-token"
            | "x-access-token"
            | "x-refresh-token"
            | "x-session-token"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "apikey"
            | "key"
            | "token"
            | "secret"
            | "password"
            | "auth"
    ) || n.contains("token")
        || n.contains("secret")
        || n.contains("api-key")
        || n.contains("apikey")
        || n.contains("session")
        || n.contains("credential")
        || n.contains("authorization")
        || n.ends_with("_key")
        || n.ends_with("-key")
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
{
    if url.query().is_none() {
        return url.to_string();
    }

    let explicit_sensitive = sensitive_query_keys
        .into_iter()
        .map(|key| key.as_ref().to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();

    let mut out = url.clone();
    out.set_query(None);
    {
        let mut pairs = out.query_pairs_mut();
        for (key, value) in url.query_pairs() {
            let redacted =
                explicit_sensitive.contains(&key.to_ascii_lowercase()) || is_sensitive_name(&key);

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
