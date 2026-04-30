use std::hash::{Hash, Hasher};

pub(crate) fn is_sensitive_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "apikey"
            | "key"
            | "token"
            | "secret"
            | "password"
    ) || n.contains("token")
        || n.contains("secret")
        || n.contains("api-key")
        || n.contains("apikey")
        || n.ends_with("_key")
        || n.ends_with("-key")
}

pub(crate) fn hash_value(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(crate) fn hashed_sensitive_value(value: &str) -> String {
    format!("<sensitive:{}>", hash_value(value))
}

pub(crate) fn redacted_display_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) {
        "<redacted>".to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn sanitized_url_for_key(url: &url::Url) -> String {
    if url.query().is_none() {
        return url.to_string();
    }
    let mut out = url.clone();
    out.query_pairs_mut().clear();
    {
        let mut pairs = out.query_pairs_mut();
        for (key, value) in url.query_pairs() {
            if is_sensitive_name(&key) {
                pairs.append_pair(&key, &hashed_sensitive_value(&value));
            } else {
                pairs.append_pair(&key, &value);
            }
        }
    }
    out.to_string()
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
    fn sanitized_url_hashes_sensitive_query_values() {
        let url: url::Url = "https://example.com/items?api_key=secret&visible=plain"
            .parse()
            .expect("valid url");

        let rendered = sanitized_url_for_key(&url);

        assert!(rendered.contains("visible=plain"));
        assert!(rendered.contains("api_key=%3Csensitive%3A"));
        assert!(!rendered.contains("secret"));
    }
}
