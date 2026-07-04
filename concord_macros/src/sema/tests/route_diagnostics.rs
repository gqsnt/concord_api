use super::helpers::assert_route_error_contains;

#[test]
fn route_diagnostics_reject_explicit_ep_reference_in_scope_route() {
    assert_route_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope users(user_id: String) {
                path [ep.user_id]

                GET Show
                    path ["show"]
                    -> Json<()>
            }
        }
        "#,
        "`ep.*` is not allowed in scope routes",
    );
}

#[test]
fn route_diagnostics_reject_unknown_route_reference() {
    assert_route_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show(user_id: String)
                path [missing]
                -> Json<()>
        }
        "#,
        "unknown endpoint var",
    );
}

#[test]
fn route_diagnostics_reject_unknown_fmt_route_reference() {
    assert_route_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show(user_id: String)
                path [fmt["user-", missing]]
                -> Json<()>
        }
        "#,
        "unknown endpoint var",
    );
}

#[test]
fn route_diagnostics_reject_duplicate_endpoint_identity_in_same_module() {
    assert_route_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show
                path ["one"]
                -> Json<()>

            GET Show
                path ["two"]
                -> Json<()>
        }
        "#,
        "duplicate endpoint",
    );
}
