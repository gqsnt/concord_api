use super::helpers::{endpoint_in_scope, parse_ok, scope_at_top_level};
use crate::ast::RawItem;

#[test]
fn parses_compact_current_dsl_fixture() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String
                secret api_key: String

                retry read {
                    max_attempts 2
                }

                rate_limit api {
                    bucket request by [endpoint] {
                        10 / 1s
                    }
                }

                default {
                    retry read
                    rate_limit api
                }
            }

            scope users(user_id: String) {
                path ["users", user_id]

                POST Create(body: Json<CreateUser>)
                    path ["create"]
                    -> Json<CreateResponse>
            }
        }
        "#,
    );

    assert_eq!(ast.client.name, "Api");
    assert!(ast.client.vars.is_some());
    assert_eq!(ast.client.vars.as_ref().unwrap().decls.len(), 1);
    assert!(ast.client.auth_vars.is_some());
    assert_eq!(ast.client.auth_vars.as_ref().unwrap().decls.len(), 1);
    assert!(ast.client.retry.is_some());
    assert_eq!(ast.client.rate_limit.as_ref().unwrap().default.len(), 1);
    assert_eq!(
        ast.client.rate_limit.as_ref().unwrap().default[0].to_string(),
        "api"
    );
    assert_eq!(ast.items.len(), 1);

    let scope = scope_at_top_level(&ast, 0);
    assert_eq!(
        scope
            .scope_name
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("users")
    );
    assert_eq!(scope.items.len(), 1);

    let endpoint = endpoint_in_scope(scope, 0);
    assert_eq!(endpoint.line.name, "Create");
    assert!(endpoint.body.is_some());
    assert_eq!(
        endpoint.response.marker,
        syn::parse_quote!(Json<CreateResponse>)
    );
    assert!(endpoint.response.had_angle_args);

    match &ast.items[0] {
        RawItem::Layer(_) => {}
        other => panic!("expected top-level scope, got {other:?}"),
    }
}

#[test]
fn parses_current_api_wrapper_and_base_url_literal() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                -> Json<String>
        }
        "#,
    );

    assert_eq!(ast.client.name, "Api");
    assert_eq!(ast.client.host.value(), "example.com");
    assert_eq!(ast.items.len(), 1);
}

#[test]
fn parses_current_nested_scopes() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope org(org_id: u64) {
                path ["orgs", org_id]

                scope users {
                    path ["users"]

                    GET List
                        path ["list"]
                        -> Json<Vec<String>>
                }
            }
        }
        "#,
    );

    let org = scope_at_top_level(&ast, 0);
    assert_eq!(
        org.scope_name.as_ref().map(ToString::to_string).as_deref(),
        Some("org")
    );
    assert_eq!(org.items.len(), 1);
    let users = match &org.items[0] {
        RawItem::Layer(scope) => scope,
        other => panic!("expected nested scope, got {other:?}"),
    };
    assert_eq!(
        users
            .scope_name
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("users")
    );
    assert_eq!(users.items.len(), 1);
}

#[test]
fn parses_current_policy_profiles() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                default {
                    retry read
                    rate_limit app
                }

                retry read {
                    max_attempts 2
                    methods [GET]
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }
        }
        "#,
    );

    assert!(ast.client.retry_profiles.is_some());
    assert!(ast.client.rate_limit.is_some());
    assert!(ast.client.retry.is_some());
    assert_eq!(ast.client.rate_limit.as_ref().unwrap().default.len(), 1);
    assert_eq!(
        ast.client.rate_limit.as_ref().unwrap().default[0].to_string(),
        "app"
    );
}

#[test]
fn parses_grouped_policy_profiles() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                policies {
                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }

                    observe rate_limit ExampleObserver
                }
            }
        }
        "#,
    );

    assert!(ast.client.retry_profiles.is_some());
    assert!(ast.client.rate_limit.is_some());
}

#[test]
fn parses_grouped_profiles_and_singular_default_profile() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profiles {
                    profile protected_read {
                        retry off
                    }
                }

                default {
                    profile protected_read
                }
            }

            GET Me
                path ["me"]
                -> Json<()>
        }
        "#,
    );

    assert_eq!(
        ast.client
            .behavior_profiles
            .as_ref()
            .unwrap()
            .profiles
            .len(),
        1
    );
    assert_eq!(ast.client.default_behavior_uses.len(), 1);
    assert_eq!(
        ast.client.default_behavior_uses[0].names[0].to_string(),
        "protected_read"
    );
}
