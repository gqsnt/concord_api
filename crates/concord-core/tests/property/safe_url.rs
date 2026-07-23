use std::collections::HashSet;

use url::Url;

use super::url::render_safe_url;

const REDACTED: &str = "<redacted>";

fn query_pairs(url: &str) -> Vec<(String, String)> {
    Url::parse(url)
        .expect("rendered safe URL should parse")
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

#[test]
fn safe_url_preserves_order_duplicates_public_values_and_source() {
    let logical = Url::parse(
        "https://example.invalid/a%2Fb/segment?first=one&dup=before&client_secret=hidden&dup=after&last=two#frag%2Fment",
    )
    .expect("logical URL should parse");
    let original = logical.to_string();
    let safe = render_safe_url(&logical, &HashSet::new());

    assert_eq!(logical.to_string(), original);
    assert_eq!(
        query_pairs(&safe),
        [
            ("first".into(), "one".into()),
            ("dup".into(), "before".into()),
            ("client_secret".into(), REDACTED.into()),
            ("dup".into(), "after".into()),
            ("last".into(), "two".into()),
        ]
    );

    let safe = Url::parse(&safe).expect("safe URL should parse");
    assert_eq!(safe.scheme(), logical.scheme());
    assert_eq!(safe.username(), logical.username());
    assert_eq!(safe.path(), logical.path());
    assert_eq!(safe.fragment(), logical.fragment());
    assert_eq!(safe.host_str(), logical.host_str());
}

#[test]
fn safe_url_redacts_ordinary_looking_and_multiple_auth_keys() {
    let logical = Url::parse(
        "https://example.invalid/path?signature=one&public=visible&proof=two&signature=three",
    )
    .expect("logical URL should parse");
    let auth_keys = HashSet::from(["signature".to_owned(), "proof".to_owned()]);
    let safe = render_safe_url(&logical, &auth_keys);

    assert_eq!(
        query_pairs(&safe),
        [
            ("signature".into(), REDACTED.into()),
            ("public".into(), "visible".into()),
            ("proof".into(), REDACTED.into()),
            ("signature".into(), REDACTED.into()),
        ]
    );
    assert!(!safe.contains("one"));
    assert!(!safe.contains("two"));
    assert!(!safe.contains("three"));
}

#[test]
fn safe_url_property_auth_values_never_survive_rendering() {
    const SEED: u64 = 0x5afe_c0de_2026_0717;
    let auth_keys = [
        "signature",
        "opaque_key",
        "proof-slot",
        "ordinary key",
        "client_secret",
    ];
    let auth_values = [
        "AUTH_SENTINEL_plain_7f39",
        "AUTH_SENTINEL_ampersand_7f39&next=value",
        "AUTH_SENTINEL_equals_7f39=value",
        "AUTH_SENTINEL_percent_7f39%2Ftail",
        "AUTH_SENTINEL_reserved_7f39/?#[]@!$'()*+,;=",
    ];
    let public_layouts: &[&[(&str, &str)]] = &[
        &[("public", "one"), ("dup", "first"), ("dup", "second")],
        &[("space key", "space value"), ("public", "a&b=c")],
        &[("z", "%2F"), ("a", "?fragment-like"), ("z", "last")],
    ];

    let mut case_index = 0_usize;
    for auth_key in auth_keys {
        for auth_value in auth_values {
            for layout in public_layouts {
                for insertion_index in 0..=layout.len() {
                    let mut logical = Url::parse(
                        "https://user@example.invalid/a%2Fb/deep/path?baseline=kept#fragment%2Fkept",
                    )
                    .expect("base logical URL should parse");
                    {
                        let mut query = logical.query_pairs_mut();
                        query.clear();
                        for (index, (key, value)) in layout.iter().enumerate() {
                            if index == insertion_index {
                                query.append_pair(auth_key, auth_value);
                            }
                            query.append_pair(key, value);
                        }
                        if insertion_index == layout.len() {
                            query.append_pair(auth_key, auth_value);
                        }
                    }

                    let original = logical.to_string();
                    let safe = render_safe_url(&logical, &HashSet::from([auth_key.to_owned()]));
                    let safe_pairs = query_pairs(&safe);
                    let logical_pairs: Vec<_> = logical
                        .query_pairs()
                        .map(|(key, value)| (key.into_owned(), value.into_owned()))
                        .collect();
                    let expected: Vec<_> = logical_pairs
                        .iter()
                        .map(|(key, value)| {
                            let value = if key == auth_key || super::names::is_sensitive_name(key) {
                                REDACTED.to_owned()
                            } else {
                                value.clone()
                            };
                            (key.clone(), value)
                        })
                        .collect();
                    let context = format!(
                        "seed={SEED:#x} deterministic_case_index={case_index} auth_key={auth_key:?} auth_value={auth_value:?}"
                    );

                    assert_eq!(logical.to_string(), original, "{context}");
                    assert_eq!(safe_pairs, expected, "{context}");
                    assert!(
                        safe_pairs.iter().all(|(_, value)| value != auth_value),
                        "marked auth value survived; {context}; safe={safe}"
                    );
                    assert!(
                        safe_pairs
                            .iter()
                            .filter(|(key, _)| key == auth_key)
                            .all(|(_, value)| value == REDACTED),
                        "auth key was not structurally redacted; {context}; safe={safe}"
                    );

                    let parsed_safe = Url::parse(&safe).expect("safe URL should parse");
                    assert_eq!(parsed_safe.path(), logical.path(), "{context}");
                    assert_eq!(parsed_safe.fragment(), logical.fragment(), "{context}");
                    case_index += 1;
                }
            }
        }
    }

    assert_eq!(case_index, 275, "seed={SEED:#x}");
}
