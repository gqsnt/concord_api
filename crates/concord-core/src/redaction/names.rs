const MANDATORY_SENSITIVE_CATEGORIES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "token",
    "access-token",
    "refresh-token",
    "secret",
    "password",
    "passwd",
    "credential",
    "session",
    "api-key",
    "apikey",
    "client-secret",
];

/// Classifies a header or query name at the central redaction boundary.
pub(super) fn is_sensitive_name(name: &str) -> bool {
    let normalized: String = name
        .bytes()
        .map(|byte| match byte {
            b'_' => b'-',
            byte => byte.to_ascii_lowercase(),
        })
        .map(char::from)
        .collect();

    MANDATORY_SENSITIVE_CATEGORIES
        .iter()
        .any(|category| normalized.contains(category))
}
