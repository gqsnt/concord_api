use super::super::names::is_sensitive_name;

const MANDATORY: &[&str] = &[
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

fn mixed_ascii_case(value: &str) -> String {
    value
        .bytes()
        .enumerate()
        .map(|(index, byte)| {
            if index % 2 == 0 {
                byte.to_ascii_uppercase()
            } else {
                byte
            }
        })
        .map(char::from)
        .collect()
}

#[test]
fn redaction_matrix_covers_every_mandatory_category_and_variant() {
    for category in MANDATORY {
        let variants = [
            (*category).to_owned(),
            category.to_ascii_uppercase(),
            mixed_ascii_case(category),
            category.replace('-', "_"),
            format!("prefix_{category}_suffix"),
            format!("prefix-{category}-suffix"),
            format!("before{category}after"),
        ];

        for variant in variants {
            assert!(
                is_sensitive_name(&variant),
                "mandatory category {category:?} was exposed by variant {variant:?}"
            );
        }
    }
}

#[test]
fn redaction_matrix_normalizes_hyphens_and_underscores_equivalently() {
    for (hyphenated, underscored) in [
        ("proxy-authorization", "proxy_authorization"),
        ("set-cookie", "set_cookie"),
        ("access-token", "access_token"),
        ("refresh-token", "refresh_token"),
        ("api-key", "api_key"),
        ("client-secret", "client_secret"),
    ] {
        assert!(is_sensitive_name(hyphenated));
        assert!(is_sensitive_name(underscored));
        assert_eq!(
            is_sensitive_name(hyphenated),
            is_sensitive_name(underscored)
        );
    }
}

#[test]
fn redaction_matrix_retains_useful_public_near_misses() {
    for public_name in [
        "accept",
        "content-type",
        "traceparent",
        "x-request-id",
        "sessionless-mode",
        "tok-en",
        "authorizationless",
    ] {
        // Categories are substring-sensitive, so the two intentionally
        // containing examples are conservatively classified.
        let expected = matches!(public_name, "sessionless-mode" | "authorizationless");
        assert_eq!(is_sensitive_name(public_name), expected, "{public_name}");
    }
}
