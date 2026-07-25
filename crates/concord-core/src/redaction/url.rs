use std::collections::HashSet;

use url::Url;

use super::names::is_sensitive_name;

const REDACTED: &str = "<redacted>";

/// Renders a credential-sanitized URL solely from logical URL data and
/// effective authentication keys.
#[allow(dead_code, reason = "used by later production diagnostic surfaces")]
pub(crate) fn render_safe_url(logical_url: &Url, auth_query_keys: &HashSet<String>) -> String {
    let pairs: Vec<_> = logical_url
        .query_pairs()
        .map(|(key, value)| {
            let value = if auth_query_keys.contains(key.as_ref()) || is_sensitive_name(key.as_ref())
            {
                REDACTED.to_owned()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();

    let mut safe_url = logical_url.clone();
    if !pairs.is_empty() {
        safe_url.set_query(None);
        safe_url.query_pairs_mut().extend_pairs(pairs);
    }
    safe_url.to_string()
}
