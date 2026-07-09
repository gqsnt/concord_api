use super::helpers::{parse_err, parse_ok};

fn nested_scope_source(depth: usize, leaf: &str) -> String {
    let mut source = String::from(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }
        "#,
    );
    for idx in 0..depth {
        source.push_str(&format!("\n            scope scope_{idx} {{"));
    }
    source.push_str(&format!(
        "\n                GET {leaf}\n                    path [\"ping\"]\n                    -> Json<String>\n"
    ));
    for _ in 0..depth {
        source.push_str("\n            }");
    }
    source.push_str("\n        }\n        ");
    source
}

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

#[test]
fn legacy_behavior_keyword_fails_with_migration_diagnostic() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behavior read {
                    retry off
                }
            }
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("`behavior` is not valid V1 DSL; use `profile`"),
        "{err}"
    );
}

#[test]
fn legacy_behaviors_block_fails_with_migration_diagnostic() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behaviors {
                    behavior read {
                        retry off
                    }
                }
            }
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("`behaviors` is not valid V1 DSL; use `profiles"),
        "{err}"
    );
}

#[test]
fn legacy_defaults_block_fails_with_migration_diagnostic() {
    let err = parse_err(
        r#"
        api! {
            client Api {
                base "https://example.com"

                defaults {
                    retry off
                }
            }
        }
        "#,
    );

    assert!(
        err.to_string()
            .contains("`defaults` is not valid V1 DSL; use `default"),
        "{err}"
    );
}

#[test]
fn nested_scope_depth_limit_is_enforced_at_parse_time() {
    let source = nested_scope_source(64, "LimitPing");
    let ast = parse_ok(&source);
    let mut current = &ast.items[0];
    for _ in 0..64 {
        let scope = match current {
            crate::ast::RawItem::Layer(scope) => scope,
            other => panic!("expected nested scope, got {other:?}"),
        };
        if scope.items.is_empty() {
            break;
        }
        current = &scope.items[0];
    }
    match current {
        crate::ast::RawItem::Endpoint(endpoint) => {
            assert_eq!(endpoint.name, "LimitPing");
        }
        other => panic!("expected terminal endpoint, got {other:?}"),
    }
}

#[test]
fn nested_scope_depth_over_limit_fails_closed_without_panic() {
    let source = nested_scope_source(65, "LEAK_SENTINEL_DEPTH_PING");
    let err = parse_err(&source);
    assert!(
        err.to_string()
            .contains("DSL scope nesting exceeds maximum supported depth of 64"),
        "{err}"
    );
    assert!(!err.to_string().contains("LEAK_SENTINEL_DEPTH_PING"));
}
