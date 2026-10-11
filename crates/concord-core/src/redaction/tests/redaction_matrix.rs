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

/// The classification by construction: a substring search of the normalized
/// copy (`_` to `-`, ASCII lowercase, each byte as one `char`).
fn reference_is_sensitive_name(name: &str) -> bool {
    let normalized: String = name
        .bytes()
        .map(|byte| match byte {
            b'_' => b'-',
            byte => byte.to_ascii_lowercase(),
        })
        .map(char::from)
        .collect();
    MANDATORY
        .iter()
        .any(|category| normalized.contains(category))
}

#[test]
fn redaction_matrix_in_place_normalization_equals_the_normalized_copy() {
    let mut names = vec![
        String::new(),
        "_".to_owned(),
        "-".to_owned(),
        "t".to_owned(),
        "TOKE".to_owned(),
        "tokeN".to_owned(),
        "X_API_KEY".to_owned(),
        "x-api-kez".to_owned(),
        "api_key".to_owned(),
        "apikey".to_owned(),
        "api--key".to_owned(),
        "Set_Cookie".to_owned(),
        "sEt-cOoKiE".to_owned(),
        "proxy_authorization".to_owned(),
        "tök-en".to_owned(),
        "Tökén".to_owned(),
        "tok\u{e9}n".to_owned(),
        "ßsession".to_owned(),
        "sess\u{130}on".to_owned(),
        "\u{212a}ey".to_owned(),
        "pass\u{0}wd".to_owned(),
        "passwd\u{80}".to_owned(),
        "\u{ff}credential\u{ff}".to_owned(),
        "clientsecret".to_owned(),
        "client_secre".to_owned(),
        "refresh__token".to_owned(),
    ];
    for category in MANDATORY {
        names.push((*category).to_owned());
        names.push(category.to_ascii_uppercase());
        names.push(category.replace('-', "_"));
        names.push(format!("é{category}é"));
        names.push(category[..category.len() - 1].to_owned());
        names.push(category[1..].to_owned());
    }
    for name in &names {
        assert_eq!(
            is_sensitive_name(name),
            reference_is_sensitive_name(name),
            "{name:?}"
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
