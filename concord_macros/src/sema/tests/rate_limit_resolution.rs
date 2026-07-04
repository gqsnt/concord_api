use super::helpers::{analyze_ok, rate_limit_plan, single_endpoint};
use crate::sema::{RateLimitKeyResolved, RateLimitResolved};

#[test]
fn rate_limit_resolution_lowers_named_rate_limit_profile() {
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
            }

            GET Ping
                path ["ping"]
                rate_limit app
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    let Some(RateLimitResolved::Add(plan)) = &endpoint.policy.endpoint.rate_limit else {
        panic!("expected named rate limit profile to resolve on endpoint");
    };
    assert_eq!(plan.buckets.len(), 1);
    let bucket = &rate_limit_plan(endpoint.policy.endpoint.rate_limit.as_ref().unwrap()).buckets[0];
    assert_eq!(bucket.kind, "application");
    assert_eq!(bucket.name, "app_0");
    assert!(matches!(
        bucket.key.as_slice(),
        [RateLimitKeyResolved::RouteHost]
    ));
    assert_eq!(bucket.cost, 1);
    assert_eq!(
        bucket
            .windows
            .iter()
            .map(|w| (w.max, w.per_secs))
            .collect::<Vec<_>>(),
        vec![(10, 1)]
    );
}

#[test]
fn rate_limit_resolution_lowers_rate_limit_off() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                rate_limit off
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Clear)
    ));
}

#[test]
fn rate_limit_resolution_lowers_observer_path() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                observe rate_limit crate::Observer
            }

            GET Ping
                path ["ping"]
                -> Json<()>
        }
        "#,
    );

    let observer = api
        .rate_limit_response_policy
        .as_ref()
        .expect("rate limit observer");
    assert!(quote::quote!(#observer).to_string().contains("Observer"));
}

#[test]
fn rate_limit_keys_resolve_route_host_and_endpoint_fields() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit tenant_bucket {
                    bucket method by [host, endpoint, method, tenant_key] {
                        5 / 1s
                    }
                }
            }

            scope tenants(tenant: String) {
                path ["tenants", tenant]
                rate_limit key tenant_key = tenant

                GET List
                    path ["items"]
                    rate_limit tenant_bucket
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = single_endpoint(&api);
    let rate_limit = endpoint
        .policy
        .endpoint
        .rate_limit
        .as_ref()
        .expect("endpoint rate limit");
    let plan = rate_limit_plan(rate_limit);
    assert_eq!(plan.buckets.len(), 1);
    let bucket = &plan.buckets[0];
    assert!(matches!(
        bucket.key.as_slice(),
        [
            RateLimitKeyResolved::RouteHost,
            RateLimitKeyResolved::Endpoint,
            RateLimitKeyResolved::Method,
            RateLimitKeyResolved::EpField { name, field }
        ] if name == "tenant_key" && *field == "tenant"
    ));
}

#[test]
fn rate_limit_keys_resolve_scope_behavior_key_binding() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit tenant_bucket {
                    bucket method by [tenant_key] {
                        5 / 1s
                    }
                }

                behavior tenant_read {
                    rate_limit tenant_bucket
                }
            }

            scope tenants(tenant: String) {
                path ["tenants", tenant]
                rate_limit key tenant_key = tenant
                behavior tenant_read

                GET List
                    path ["items"]
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert_eq!(endpoint.policy.scopes.len(), 1);
    let scope_rate_limit = endpoint.policy.scopes[0]
        .rate_limit
        .as_ref()
        .expect("scope rate limit");
    let plan = rate_limit_plan(scope_rate_limit);
    assert_eq!(plan.buckets.len(), 1);
    let bucket = &plan.buckets[0];
    assert!(matches!(
        bucket.key.as_slice(),
        [RateLimitKeyResolved::EpField { name, field }]
            if name == "tenant_key" && *field == "tenant"
    ));
}

#[test]
fn rate_limit_keys_resolve_endpoint_behavior_key_binding() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                rate_limit match_bucket {
                    bucket method by [match_key] {
                        5 / 1s
                    }
                }

                behavior match_read {
                    rate_limit match_bucket
                }
            }

            GET Match(match_id: String)
                path ["match", match_id]
                rate_limit key match_key = match_id
                behavior match_read
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    let Some(RateLimitResolved::Add(plan)) = &endpoint.policy.endpoint.rate_limit else {
        panic!("expected endpoint behavior rate limit to resolve");
    };
    assert_eq!(plan.buckets.len(), 1);
    let bucket = &rate_limit_plan(endpoint.policy.endpoint.rate_limit.as_ref().unwrap()).buckets[0];
    assert!(matches!(
        bucket.key.as_slice(),
        [RateLimitKeyResolved::EpField { name, field }]
            if name == "match_key" && *field == "match_id"
    ));
}
