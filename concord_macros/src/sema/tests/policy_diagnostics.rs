use super::helpers::{analyze_err, assert_error_contains, assert_policy_error_contains};

#[test]
fn duplicate_header_names_in_same_layer_fail_case_insensitively() {
    assert_policy_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search
                path ["search"]
                headers {
                    "X-Trace" = "one",
                    "x-trace" = "two"
                }
                -> Json<()>
        }
        "#,
        "duplicate header `x-trace`",
    );
}

#[test]
fn policy_diagnostics_reject_endpoint_refs_in_client_policy() {
    assert_policy_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
                headers {
                    "x-trace" = ep.trace_id
                }
            }

            GET Search(trace_id: String)
                path ["search"]
                -> Json<()>
        }
        "#,
        "ep.* is not allowed here",
    );
}

#[test]
fn policy_diagnostics_reject_unknown_client_and_endpoint_vars() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String
                headers {
                    "x-trace" = vars.missing
                }
            }

            GET Search
                path ["search"]
                -> Json<()>
        }
        "#,
    );
    assert_error_contains(&err, "unknown client var");
    assert_error_contains(&err, "available client vars: `vars.trace_id`");

    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search(q: String)
                path ["search"]
                query {
                    "q" = ep.missing
                }
                -> Json<()>
        }
        "#,
    );
    assert_error_contains(&err, "unknown endpoint var");
    assert_error_contains(&err, "available endpoint vars: `ep.q`");
}

#[test]
fn policy_diagnostics_reject_invalid_header_values() {
    assert_policy_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search
                path ["search"]
                headers {
                    "x-bad" = "bad\r\nvalue"
                }
                -> Json<()>
        }
        "#,
        "header value literal is not a valid HTTP header value",
    );
}
