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
///
/// A name is sensitive when a mandatory category occurs in it after every
/// byte is normalized (`_` to `-`, ASCII lowercase). The comparison
/// normalizes each byte in place instead of building a normalized copy, so
/// classification allocates nothing; the categories are ASCII, so no match
/// can involve a non-ASCII byte and the result equals a substring search of
/// the normalized copy.
pub(crate) fn is_sensitive_name(name: &str) -> bool {
    let name = name.as_bytes();
    MANDATORY_SENSITIVE_CATEGORIES
        .iter()
        .any(|category| contains_normalized(name, category.as_bytes()))
}

/// Whether `category` occurs in `name` with every byte of `name` normalized.
fn contains_normalized(name: &[u8], category: &[u8]) -> bool {
    name.windows(category.len()).any(|window| {
        window
            .iter()
            .zip(category)
            .all(|(&byte, &expected)| normalize_name_byte(byte) == expected)
    })
}

const fn normalize_name_byte(byte: u8) -> u8 {
    match byte {
        b'_' => b'-',
        byte => byte.to_ascii_lowercase(),
    }
}
