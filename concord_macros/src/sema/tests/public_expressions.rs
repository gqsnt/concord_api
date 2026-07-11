use super::helpers::{analyze_err, analyze_ok, assert_error_contains, single_endpoint};
use crate::sema::{KeyResolved, PolicyOp, PolicySetValue, PublicValueKind};

#[test]
fn resolved_query_shorthand_lowers_to_endpoint_field_semantics() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search(q: String)
                path ["search"]
                query {
                    q
                }
                -> Json<String>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    match endpoint.policy.endpoint.query.as_slice() {
        [
            PolicyOp::Set {
                key: KeyResolved::Ident(key),
                value: PolicySetValue::Value(PublicValueKind::EpField(field)),
                ..
            },
        ] => {
            assert_eq!(key.to_string(), "q");
            assert_eq!(field.to_string(), "q");
        }
        other => panic!("query shorthand did not lower to endpoint field semantics: {other:?}"),
    }
}

#[test]
fn direct_secret_policy_expressions_are_rejected_during_analysis() {
    for (_label, source) in [
        (
            "headers",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                }

                GET HeaderRef
                    path ["header"]
                    headers {
                        "x-api-key" = secret.token
                    }
                    -> Json<String>
            }
            "#,
        ),
        (
            "query",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                }

                GET QueryRef
                    path ["query"]
                    query {
                        token = secret.token
                    }
                    -> Json<String>
            }
            "#,
        ),
        (
            "timeout",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                }

                GET TimeoutRef
                    path ["timeout"]
                    timeout: secret.token
                    -> Json<String>
            }
            "#,
        ),
        (
            "pagination",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                }

                GET PageRef(start: u64 = 0)
                    path ["page"]
                    query {
                        start
                    }
                    paginate OffsetLimitPagination {
                        offset = secret.token,
                        limit = start
                    }
                    -> Json<Vec<String>>
            }
            "#,
        ),
    ] {
        let err = analyze_err(source);
        assert_error_contains(&err, "DSL-010");
    }
}
