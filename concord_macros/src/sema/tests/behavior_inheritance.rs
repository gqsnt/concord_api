use super::helpers::{
    analyze_ok, auth_for_endpoint, auth_requirement_names, auth_requirement_provenance_labels,
    client_policy, endpoint_by_name, endpoint_policy, scope_policy,
};
use crate::sema::{RateLimitResolved, RetryResolved};

#[test]
fn behavior_use_applies_policy_and_auth_to_endpoint() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401]
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                profiles {
                    profile read_behavior {
                        auth bearer session
                        retry read
                        rate_limit app
                    }
                }
            }

            GET Me
                path ["me"]
                profile read_behavior
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(
        auth_requirement_names(auth_for_endpoint(&api, "Me")),
        vec!["session".to_string()]
    );
    assert_eq!(
        auth_requirement_provenance_labels(auth_for_endpoint(&api, "Me")),
        vec!["endpoint".to_string()]
    );
    assert_eq!(
        endpoint.behavior_doc.names,
        vec!["read_behavior".to_string()]
    );
    assert!(matches!(
        endpoint.policy.endpoint.retry,
        Some(RetryResolved::Set(_))
    ));
    assert!(matches!(
        endpoint.policy.endpoint.rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
}

#[test]
fn client_default_behavior_applies_to_endpoint_policy() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401]
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                profiles {
                    profile read_behavior {
                        auth bearer session
                        retry read
                        rate_limit app
                    }
                }

                default {
                    profile read_behavior
                }
            }

            GET Me
                path ["me"]
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(
        auth_requirement_names(auth_for_endpoint(&api, "Me")),
        vec!["session".to_string()]
    );
    assert_eq!(
        endpoint.behavior_doc.names,
        vec!["read_behavior".to_string()]
    );
    assert!(matches!(
        client_policy(&api).retry,
        Some(RetryResolved::Set(_))
    ));
    assert!(matches!(
        client_policy(&api).rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
}

#[test]
fn default_behavior_applies_to_client_policy() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401]
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }

                profiles {
                    profile protected_read {
                        retry read
                        rate_limit app
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

    assert!(matches!(
        client_policy(&api).retry,
        Some(RetryResolved::Set(_))
    ));
    assert!(matches!(
        client_policy(&api).rate_limit,
        Some(RateLimitResolved::Add(_))
    ));
}

#[test]
fn scope_behavior_is_inherited_by_nested_endpoint() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401]
                }

                profiles {
                    profile scope_read {
                        retry read
                    }
                }
            }

            scope users {
                path ["users"]
                profile scope_read

                GET Me
                    path ["me"]
                    -> Json<()>
            }

            GET Root
                path ["root"]
                -> Json<()>
        }
        "#,
    );
    let nested = endpoint_policy(&api, "Me");

    assert!(matches!(
        scope_policy(nested, 0).retry,
        Some(RetryResolved::Set(_))
    ));
    assert!(endpoint_policy(&api, "Root").scopes.is_empty());
    assert!(endpoint_by_name(&api, "Root").behavior_doc.names.is_empty());
    assert_eq!(
        endpoint_by_name(&api, "Me").behavior_doc.names,
        vec!["scope_read".to_string()]
    );
}

#[test]
fn endpoint_behavior_adds_policy_without_losing_inherited_auth() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)

                retry endpoint_retry {
                    max_attempts 2
                    methods [GET]
                    on [401]
                }

                profiles {
                    profile default_auth {
                        auth bearer session
                    }

                    profile endpoint_read {
                        retry endpoint_retry
                    }
                }

                default {
                    profile default_auth
                }
            }

            GET Me
                path ["me"]
                profile endpoint_read
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(endpoint.policy.auth.len(), 1);
    let auth = &endpoint.policy.auth[0];
    assert_eq!(auth.credential.to_string(), "session");
    assert_eq!(auth.usage_id, "bearer");
    assert_eq!(auth.provenance.label, "client");
    assert_eq!(
        endpoint.behavior_doc.names,
        vec!["default_auth".to_string(), "endpoint_read".to_string()]
    );
    assert!(matches!(
        endpoint.policy.endpoint.retry,
        Some(RetryResolved::Set(_))
    ));
}
