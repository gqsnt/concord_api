use super::helpers::{
    analyze_ok, client_policy, effective_endpoint_rate_limit_bucket_names, endpoint_by_name,
    rate_limit_plan,
};
use crate::sema::{RateLimitKeyResolved, RateLimitResolved};

#[test]
fn rate_limit_inheritance_default_rate_limit_overrides_endpoint_only() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                default {
                    rate_limit app
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            GET Ping
                path ["ping"]
                rate_limit only app
                -> Json<()>
        }
        "#,
    );

    let Some(RateLimitResolved::Add(client_rate_limit)) = &client_policy(&api).rate_limit else {
        panic!("expected default client rate limit");
    };
    assert_eq!(client_rate_limit.buckets.len(), 1);

    let endpoint = endpoint_by_name(&api, "Ping");
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Replace(_))
    ));
}

#[test]
fn rate_limit_inheritance_applies_client_scope_endpoint_layers() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                default {
                    rate_limit client_limit
                }

                rate_limit client_limit {
                    bucket client by [host] {
                        1 / 1s
                    }
                }

                rate_limit outer_limit {
                    bucket outer by [host] {
                        2 / 1s
                    }
                }

                rate_limit inner_limit {
                    bucket inner by [host] {
                        3 / 1s
                    }
                }

                rate_limit endpoint_limit {
                    bucket endpoint by [host] {
                        4 / 1s
                    }
                }
            }

            scope outer {
                path ["outer"]
                rate_limit outer_limit

                scope inner {
                    path ["inner"]
                    rate_limit inner_limit

                    GET Show
                        path ["show"]
                        rate_limit endpoint_limit
                        -> Json<()>
                }
            }
        }
        "#,
    );

    let endpoint = endpoint_by_name(&api, "Show");
    assert!(matches!(
        client_policy(&api).rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert_eq!(endpoint.policy.scopes.len(), 2);
    assert!(matches!(
        endpoint.policy.scopes[0].rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert!(matches!(
        endpoint.policy.scopes[1].rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert_eq!(
        effective_endpoint_rate_limit_bucket_names(&api, endpoint),
        vec![
            "client_limit_0".to_string(),
            "outer_limit_0".to_string(),
            "inner_limit_0".to_string(),
            "endpoint_limit_0".to_string(),
        ]
    );
}

#[test]
fn rate_limit_inheritance_behavior_and_direct_endpoint_limits_combine() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                rate_limit method {
                    bucket method by [host, endpoint] {
                        5 / 1s
                    }
                }

                behavior read_behavior {
                    rate_limit app
                }
            }

            GET Me
                path ["me"]
                behavior read_behavior
                rate_limit method
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(
        effective_endpoint_rate_limit_bucket_names(&api, endpoint),
        vec!["app_0".to_string(), "method_0".to_string()]
    );
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
}

#[test]
fn rate_limit_inheritance_scope_behavior_rate_limit_combines_with_endpoint_rate_limit() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                rate_limit method {
                    bucket method by [host, endpoint] {
                        5 / 1s
                    }
                }

                behavior scope_read {
                    rate_limit app
                }
            }

            scope users {
                path ["users"]
                behavior scope_read

                GET Me
                    path ["me"]
                    rate_limit method
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(
        effective_endpoint_rate_limit_bucket_names(&api, endpoint),
        vec!["app_0".to_string(), "method_0".to_string()]
    );
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
}

#[test]
fn rate_limit_inheritance_behavior_and_direct_scope_limits_combine() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                rate_limit users {
                    bucket method by [host, endpoint] {
                        5 / 1s
                    }
                }

                behavior base_read {
                    rate_limit app
                }
            }

            scope users {
                path ["users"]
                behavior base_read
                rate_limit users

                GET List
                    path []
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "List");

    assert_eq!(endpoint.policy.scopes.len(), 1);
    let scope_rate_limit = endpoint.policy.scopes[0]
        .rate_limit
        .as_ref()
        .expect("scope rate limit");
    let plan = rate_limit_plan(scope_rate_limit);
    assert_eq!(
        plan.buckets
            .iter()
            .map(|bucket| bucket.name.clone())
            .collect::<Vec<_>>(),
        vec!["app_0".to_string(), "users_0".to_string()]
    );
}

#[test]
fn rate_limit_inheritance_behavior_contributed_layers_preserve_order() {
    let api = analyze_ok(
        r#"
        api! {
            client MergeRateLimitApi {
                base "https://example.com"

                rate_limit client_limit {
                    bucket client by [host] {
                        1 / 1s
                    }
                }

                rate_limit outer_limit {
                    bucket outer by [host] {
                        2 / 1s
                    }
                }

                rate_limit inner_limit {
                    bucket inner by [host] {
                        3 / 1s
                    }
                }

                rate_limit endpoint_limit {
                    bucket endpoint by [host] {
                        4 / 1s
                    }
                }

                behaviors {
                    behavior client_behavior {
                        rate_limit client_limit
                    }

                    behavior outer_behavior {
                        rate_limit outer_limit
                    }

                    behavior inner_behavior {
                        rate_limit inner_limit
                    }

                    behavior endpoint_behavior {}
                }

                defaults {
                    behavior client_behavior
                }
            }

            scope outer {
                path ["outer"]
                behavior outer_behavior

                scope inner {
                    path ["inner"]
                    behavior inner_behavior

                    GET Show
                        path ["show"]
                        behavior endpoint_behavior
                        rate_limit endpoint_limit
                        -> Json<()>
                }
            }
        }
        "#,
    );

    let Some(RateLimitResolved::Add(client_plan)) = &client_policy(&api).rate_limit else {
        panic!("expected client rate limit to resolve");
    };
    assert_eq!(client_plan.buckets.len(), 1);
    let client_bucket = &client_plan.buckets[0];
    assert_eq!(client_bucket.kind, "client");
    assert_eq!(client_bucket.name, "client_limit_0");
    assert!(matches!(
        client_bucket.key.as_slice(),
        [RateLimitKeyResolved::RouteHost]
    ));
    assert_eq!(client_bucket.cost, 1);
    assert_eq!(
        client_bucket
            .windows
            .iter()
            .map(|window| (window.max, window.per_secs))
            .collect::<Vec<_>>(),
        vec![(1, 1)]
    );

    let endpoint = endpoint_by_name(&api, "Show");
    assert_eq!(endpoint.policy.scopes.len(), 2);
    assert!(matches!(
        endpoint.policy.scopes[0].rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert!(matches!(
        endpoint.policy.scopes[1].rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert_eq!(
        effective_endpoint_rate_limit_bucket_names(&api, endpoint),
        vec![
            "client_limit_0".to_string(),
            "outer_limit_0".to_string(),
            "inner_limit_0".to_string(),
            "endpoint_limit_0".to_string(),
        ]
    );
}

#[test]
fn rate_limit_inheritance_endpoint_off_clears_inherited_behavior_limit() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                behavior read_behavior {
                    rate_limit app
                }

                defaults {
                    behavior read_behavior
                }
            }

            GET Me
                path ["me"]
                rate_limit off
                -> Json<()>
        }
        "#,
    );

    let Some(RateLimitResolved::Add(_)) = &client_policy(&api).rate_limit else {
        panic!("expected client rate limit to resolve");
    };

    let endpoint = endpoint_by_name(&api, "Me");
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Clear)
    ));
    assert!(effective_endpoint_rate_limit_bucket_names(&api, endpoint).is_empty());
}

#[test]
fn rate_limit_inheritance_clear_and_replace_semantics() {
    let api = analyze_ok(
        r#"
        api! {
            client RateLimitSnapshotApi {
                base "https://example.com"

                rate_limit client_limit {
                    bucket client by [host] {
                        1 / 1s
                    }
                }

                rate_limit scope_limit {
                    bucket scope by [host] {
                        2 / 1s
                    }
                }

                rate_limit endpoint_limit {
                    bucket endpoint by [host] {
                        3 / 1s
                    }
                }

                behaviors {
                    behavior client_limit_behavior {
                        rate_limit client_limit
                    }

                    behavior scope_limit_behavior {
                        rate_limit scope_limit
                    }

                    behavior endpoint_limit_behavior {
                        rate_limit endpoint_limit
                    }

                    behavior clear_limit_behavior {
                        rate_limit off
                    }
                }

                defaults {
                    behavior client_limit_behavior
                }
            }

            scope protected {
                path ["protected"]
                behavior scope_limit_behavior

                GET Append
                    path ["append"]
                    behavior endpoint_limit_behavior
                    -> Json<()>

                GET Clear
                    path ["clear"]
                    behavior clear_limit_behavior
                    -> Json<()>
            }
        }
        "#,
    );

    let append_endpoint = endpoint_by_name(&api, "Append");
    assert_eq!(
        effective_endpoint_rate_limit_bucket_names(&api, append_endpoint),
        vec![
            "client_limit_0".to_string(),
            "scope_limit_0".to_string(),
            "endpoint_limit_0".to_string(),
        ]
    );
    assert!(matches!(
        append_endpoint.policy.scopes[0].rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    assert!(matches!(
        append_endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
    let append_plan = rate_limit_plan(
        append_endpoint
            .policy
            .endpoint
            .rate_limit
            .as_ref()
            .expect("append endpoint rate limit"),
    );
    assert_eq!(append_plan.buckets.len(), 1);
    let bucket = &append_plan.buckets[0];
    assert_eq!(bucket.kind, "endpoint");
    assert_eq!(bucket.name, "endpoint_limit_0");
    assert!(matches!(
        bucket.key.as_slice(),
        [RateLimitKeyResolved::RouteHost]
    ));
    assert_eq!(bucket.cost, 1);
    assert_eq!(
        bucket
            .windows
            .iter()
            .map(|window| (window.max, window.per_secs))
            .collect::<Vec<_>>(),
        vec![(3, 1)]
    );

    let clear_endpoint = endpoint_by_name(&api, "Clear");
    assert!(matches!(
        clear_endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Clear)
    ));
    assert!(effective_endpoint_rate_limit_bucket_names(&api, clear_endpoint).is_empty());
}
