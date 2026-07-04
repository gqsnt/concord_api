use super::helpers::{analyze_err, assert_error_contains};

#[test]
fn rate_limit_diagnostics_reject_unknown_rate_limit_profile() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                rate_limit missing
                -> Json<()>
        }
        "#,
    );

    assert_error_contains(&err, "unknown rate_limit profile");
}

#[test]
fn rate_limit_diagnostics_reject_default_behavior_rate_limit_key_in_client_base_policy() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit match_bucket {
                    bucket method by [match_key] {
                        5 / 1s
                    }
                }

                behavior match_read {
                    rate_limit match_bucket
                }

                defaults {
                    behavior match_read
                }
            }

            GET Match(match_id: String)
                path ["match", match_id]
                rate_limit key match_key = match_id
                -> Json<()>
        }
        "#,
    );

    assert_error_contains(
        &err,
        "endpoint/scope rate_limit key cannot be used in client base policy",
    );
}

#[test]
fn rate_limit_diagnostics_reject_unknown_endpoint_var_in_key() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit match_bucket {
                    bucket method by [match_key] {
                        5 / 1s
                    }
                }

                behavior match_read {
                    rate_limit match_bucket
                }
            }

            GET Match(match_id: String)
                path ["match", match_id]
                behavior match_read
                -> Json<()>
        }
        "#,
    );

    assert_error_contains(&err, "unknown rate_limit key `match_key`");
    assert_error_contains(&err, "match_id");
}

#[test]
fn rate_limit_diagnostics_reject_scope_behavior_key_without_binding() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit match_bucket {
                    bucket method by [match_key] {
                        5 / 1s
                    }
                }

                behavior match_read {
                    rate_limit match_bucket
                }
            }

            scope MatchScope {
                path ["match"]
                behavior match_read

                GET Match(match_id: String)
                    path [match_id]
                    -> Json<()>
            }
        }
        "#,
    );

    assert_error_contains(&err, "rate_limit key");
    assert_error_contains(&err, "match_key");
}

#[test]
fn rate_limit_diagnostics_reject_duplicate_rate_limit_profiles() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit read {
                    bucket application by [host] {
                        1 / 1s
                    }
                }

                rate_limit read {
                    bucket application by [host] {
                        2 / 1s
                    }
                }
            }
        }
        "#,
    );

    assert_error_contains(&err, "duplicate rate_limit profile `read`");
}
