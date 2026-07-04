use super::helpers::assert_behavior_error_contains;

#[test]
fn behavior_diagnostics_reject_unknown_behavior_use() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Me
                path ["me"]
                behavior missing
                -> Json<()>
        }
        "#,
        "unknown behavior `missing`",
    );
}

#[test]
fn behavior_diagnostics_reject_duplicate_behavior_names() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behaviors {
                    behavior read {
                        retry off
                    }

                    behavior read {
                        retry off
                    }
                }
            }
        }
        "#,
        "duplicate behavior `read`",
    );
}

#[test]
fn behavior_diagnostics_reject_duplicate_behavior_at_same_attachment_site() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behavior read {
                    retry off
                }

                defaults {
                    behavior read
                    behavior read
                }
            }
        }
        "#,
        "duplicate behavior `read` at this attachment site",
    );
}

#[test]
fn behavior_diagnostics_reject_behavior_cycles() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behaviors {
                    behavior read extends write {
                        retry off
                    }

                    behavior write extends read {
                        retry off
                    }
                }
            }
        }
        "#,
        "behavior inheritance cycle",
    );
}
