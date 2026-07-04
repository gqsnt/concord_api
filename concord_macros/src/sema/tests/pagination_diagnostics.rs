use super::helpers::{analyze_err, assert_error_contains};

#[test]
fn pagination_diagnostics_reject_unknown_endpoint_field_reference() {
    let err = analyze_err(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET List(count: u64 = 2)
                paginate HeaderPagePagination {
                    page = does_not_exist
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("unknown endpoint var `ep.does_not_exist`")
            || err.to_string().contains("available endpoint vars"),
        "{err}"
    );
}

#[test]
fn pagination_diagnostics_reject_unknown_endpoint_field_binding() {
    let err = analyze_err(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
            }

            GET List(count: u64 = 20)
                paginate OffsetLimitPagination {
                    offset = does_not_exist,
                    limit = count
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("unknown endpoint var `ep.does_not_exist`")
            || err.to_string().contains("available endpoint vars"),
        "{err}"
    );
}

#[test]
fn pagination_diagnostics_reject_client_var_or_secret_in_assignment() {
    let err = analyze_err(
        r#"
        api! {
            client PageApi {
                base "https://example.com"
                secret token: String
                var cursor: String
            }

            GET List(count: u64 = 20)
                paginate OffsetLimitPagination {
                    offset = vars.cursor,
                    limit = secret.token
                }
                -> Json<Vec<String>>
        }
        "#,
    );

    assert_error_contains(
        &err,
        "paginate assignments must not reference client variables or secrets",
    );
}
