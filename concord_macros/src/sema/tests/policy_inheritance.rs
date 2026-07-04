use super::helpers::{analyze_ok, client_policy, endpoint_policy, header_ops, scope_policy};
use crate::sema::{KeyResolved, PolicyOp};

#[test]
fn policy_inheritance_combines_client_scope_endpoint_layers_in_order() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                headers {
                    "x-client" = "client"
                }
            }

            scope outer {
                path ["outer"]
                headers {
                    "x-outer" = "outer"
                }

                scope inner {
                    path ["inner"]
                    headers {
                        "x-inner" = "inner"
                    }

                    GET Me
                        path ["me"]
                        headers {
                            "x-endpoint" = "endpoint"
                        }
                        -> Json<()>
                }
            }
        }
        "#,
    );
    let policy = endpoint_policy(&api, "Me");

    assert_eq!(client_policy(&api).headers.len(), 1);
    assert_eq!(policy.scopes.len(), 2);
    assert_eq!(header_ops(client_policy(&api)).len(), 1);
    assert_eq!(header_ops(scope_policy(policy, 0)).len(), 1);
    assert_eq!(header_ops(scope_policy(policy, 1)).len(), 1);
    assert_eq!(header_ops(&policy.endpoint).len(), 1);

    match &header_ops(client_policy(&api))[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            ..
        } => assert_eq!(key.value(), "x-client"),
        other => panic!("unexpected client header policy op: {other:?}"),
    }
    match &header_ops(scope_policy(policy, 0))[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            ..
        } => assert_eq!(key.value(), "x-outer"),
        other => panic!("unexpected outer scope header policy op: {other:?}"),
    }
    match &header_ops(scope_policy(policy, 1))[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            ..
        } => assert_eq!(key.value(), "x-inner"),
        other => panic!("unexpected inner scope header policy op: {other:?}"),
    }
    match &header_ops(&policy.endpoint)[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            ..
        } => assert_eq!(key.value(), "x-endpoint"),
        other => panic!("unexpected endpoint header policy op: {other:?}"),
    }
}

#[test]
fn policy_inheritance_allows_same_header_across_layers_until_runtime_policy_stack() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                headers {
                    "x-trace" = "client"
                }
            }

            scope outer {
                path ["outer"]
                headers {
                    "x-trace" = "scope"
                }

                GET Me
                    path ["me"]
                    headers {
                        "x-trace" = "endpoint"
                    }
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_policy(&api, "Me");

    assert_eq!(client_policy(&api).headers.len(), 1);
    assert_eq!(header_ops(client_policy(&api)).len(), 1);
    assert_eq!(endpoint.scopes.len(), 1);
    assert_eq!(header_ops(scope_policy(endpoint, 0)).len(), 1);
    assert_eq!(header_ops(&endpoint.endpoint).len(), 1);
}
