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
                profile missing
                -> Json<()>
        }
        "#,
        "unknown profile `missing`",
    );
}

#[test]
fn behavior_diagnostics_reject_duplicate_behavior_names() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profiles {
                    profile read {
                        rate_limit off
                    }

                    profile read {
                        rate_limit off
                    }
                }
            }
        }
        "#,
        "duplicate profile `read`",
    );
}

#[test]
fn behavior_diagnostics_reject_duplicate_behavior_at_same_attachment_site() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profile read {
                    rate_limit off
                }

                default {
                    profile read
                    profile read
                }
            }
        }
        "#,
        "duplicate profile `read` at this attachment site",
    );
}

#[test]
fn behavior_diagnostics_reject_behavior_cycles() {
    assert_behavior_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profiles {
                    profile read extends write {
                        rate_limit off
                    }

                    profile write extends read {
                        rate_limit off
                    }
                }
            }
        }
        "#,
        "profile inheritance cycle",
    );
}
