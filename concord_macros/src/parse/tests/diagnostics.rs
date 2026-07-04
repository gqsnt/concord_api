use super::helpers::parse_err;

#[test]
fn malformed_current_client_fails() {
    for (source, expected) in [
        (
            r#"
            api! {
                client Api {
                    base "ftp://example.com"
                }
            }
            "#,
            "base URL must start",
        ),
        (
            r#"
            api! {
                client Api {
                    base example.com
                }
            }
            "#,
            "base must use a single URL literal",
        ),
    ] {
        let err = parse_err(source);
        assert!(err.to_string().contains(expected));
    }
}

#[test]
fn endpoint_missing_response_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
        }
        "#,
    );

    assert!(err.to_string().contains("endpoint declarations must use"));
}

#[test]
fn endpoint_duplicate_response_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                -> Json<String>
                -> Json<String>
        }
        "#,
    );

    assert!(err.to_string().contains("duplicate endpoint response"));
}

#[test]
fn endpoint_braced_block_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                -> Json<String>
                {
                    path ["ping"]
                }
        }
        "#,
    );

    assert!(err.to_string().contains("DSL-002"));
}

#[test]
fn endpoint_unknown_clause_fails_with_code() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                frobnicate true
                -> Json<String>
        }
        "#,
    );

    assert!(err.to_string().contains("DSL-001"));
}

#[test]
fn fmt_empty_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path [fmt[]]
                -> Json<String>
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("fmt[...] requires at least one piece")
    );
}

#[test]
fn fmt_nested_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping(id: String)
                path [fmt["x", fmt[id]]]
                -> Json<String>
        }
        "#,
    );

    assert!(err.to_string().contains("nested fmt"));
}

#[test]
fn fmt_path_slash_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping(id: String)
                path [fmt["users/", id]]
                -> Json<String>
        }
        "#,
    );

    assert!(err.to_string().contains("must not contain `/`"));
}

#[test]
fn header_identifier_key_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search(trace_id: String)
                path ["search"]
                headers {
                    x_trace = trace_id
                }
                -> Json<String>
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("header keys must be explicit string literals")
    );
}

#[test]
fn boolean_query_flag_fails() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search
                path ["search"]
                query {
                    "debug" = true
                }
                -> Json<String>
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("boolean query flags are not supported")
    );
}
