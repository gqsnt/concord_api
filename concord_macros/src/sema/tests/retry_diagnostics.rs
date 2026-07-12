use super::helpers::{analyze_err, assert_error_contains};

#[test]
fn retry_diagnostics_reject_unknown_retry_profile() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                retry missing
                -> Json<()>
        }
        "#,
    );

    assert_error_contains(&err, "unknown retry profile");
}

#[test]
fn retry_diagnostics_reject_duplicate_retry_profiles() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry read {
                    max_attempts 2
                }

                retry read {
                    max_attempts 3
                }
            }
        }
        "#,
    );

    assert_error_contains(&err, "duplicate retry profile `read`");
}

#[test]
fn retry_diagnostics_reject_attempts_outside_absolute_range() {
    for value in ["0", "4"] {
        let source = format!(
            r#"
            api! {{
                client Api {{
                    base "https://example.com"
                    retry read {{ max_attempts {value} }}
                }}
            }}
            "#
        );
        let err = analyze_err(&source);
        assert_error_contains(&err, "retry max_attempts must be between 1 and 3");
    }
}
