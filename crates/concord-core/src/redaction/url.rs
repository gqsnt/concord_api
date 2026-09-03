use std::collections::HashSet;

use url::Url;

use super::names::is_sensitive_name;

const REDACTED: &str = "<redacted>";

/// Renders a credential-sanitized URL solely from logical URL data and
/// effective authentication keys.
pub(crate) fn render_observed_url(logical_url: &Url, auth_query_keys: &HashSet<String>) -> String {
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

    let mut observed_url = logical_url.clone();
    if !pairs.is_empty() {
        observed_url.set_query(None);
        observed_url.query_pairs_mut().extend_pairs(pairs);
    }
    observed_url.to_string()
}
