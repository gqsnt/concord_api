use super::helpers::assert_profile_error_contains;

#[test]
fn profile_diagnostics_reject_unknown_profile_use() {
    assert_profile_error_contains(
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
fn profile_diagnostics_reject_duplicate_profile_names() {
    assert_profile_error_contains(
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
fn profile_diagnostics_reject_duplicate_profile_at_same_attachment_site() {
    assert_profile_error_contains(
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
fn profile_diagnostics_reject_profile_cycles() {
    assert_profile_error_contains(
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
