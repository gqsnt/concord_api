use super::helpers::{analyze_ok, client_policy, endpoint_by_name, scope_policy};
use crate::sema::RetryResolved;

#[test]
fn retry_inheritance_applies_client_scope_endpoint_layers() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry client_retry {
                    max_attempts 2
                    methods [GET]
                    on [401]
                    retry_after
                }

                retry scope_retry {
                    max_attempts 3
                    methods [POST]
                    on [429]
                }

                retry endpoint_retry {
                    max_attempts 4
                    methods [PUT]
                    on [502]
                }

                default {
                    retry client_retry
                }
            }

            scope users {
                path ["users"]
                retry scope_retry

                GET Show
                    path ["show"]
                    retry endpoint_retry
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Show");

    let Some(RetryResolved::Set(client_retry)) = &client_policy(&api).retry else {
        panic!("expected client retry");
    };
    assert_eq!(client_retry.max_attempts, 2);
    assert_eq!(client_retry.statuses, vec![401]);
    assert!(client_retry.respect_retry_after);

    let Some(RetryResolved::Set(scope_retry)) = &scope_policy(&endpoint.policy, 0).retry else {
        panic!("expected scope retry");
    };
    assert_eq!(scope_retry.max_attempts, 3);
    assert_eq!(scope_retry.statuses, vec![429]);

    let Some(RetryResolved::Set(endpoint_retry)) = &endpoint.policy.endpoint.retry else {
        panic!("expected endpoint retry");
    };
    assert_eq!(endpoint_retry.max_attempts, 4);
    assert_eq!(endpoint_retry.statuses, vec![502]);
}

#[test]
fn retry_inheritance_explicit_default_retry_overrides_default_behavior_retry() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry from_behavior {
                    max_attempts 5
                    methods [GET]
                }

                retry explicit {
                    max_attempts 2
                    methods [GET]
                }

                behavior read_behavior {
                    retry from_behavior
                }

                default {
                    behavior read_behavior
                    retry explicit
                }
            }

            GET Me
                path ["me"]
                -> Json<()>
        }
        "#,
    );

    let Some(RetryResolved::Set(client_retry)) = &client_policy(&api).retry else {
        panic!("expected explicit default retry");
    };
    assert_eq!(client_retry.max_attempts, 2);
}

#[test]
fn retry_inheritance_endpoint_retry_overrides_behavior_retry_at_same_layer() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry behavior_retry {
                    max_attempts 1
                    methods [GET]
                }

                retry explicit_retry {
                    max_attempts 2
                    methods [GET]
                }

                behavior read_behavior {
                    retry behavior_retry
                }
            }

            GET Me
                path ["me"]
                behavior read_behavior
                retry explicit_retry
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    let Some(RetryResolved::Set(endpoint_retry)) = &endpoint.policy.endpoint.retry else {
        panic!("expected explicit endpoint retry");
    };
    assert_eq!(endpoint_retry.max_attempts, 2);
}

#[test]
fn retry_patches_materialize_inherited_and_after_clear() {
    let api = analyze_ok(
        r#"
        api! {
            client RetryPatchApi {
                base "https://example.com"

                retry base {
                    max_attempts 2
                    methods [GET]
                    on [429]
                }

                behaviors {
                    behavior client_base {
                        retry base
                    }

                    behavior patch_methods {
                        retry {
                            methods [POST]
                        }
                    }

                    behavior clear_retry {
                        retry off
                    }

                    behavior patch_after_clear {
                        retry {
                            max_attempts 7
                        }
                    }
                }

                defaults {
                    behavior client_base
                }
            }

            scope inherited {
                path ["inherited"]
                behavior patch_methods

                GET Patched
                    path ["patched"]
                    -> Json<()>
            }

            scope cleared {
                path ["cleared"]
                behavior clear_retry

                GET Reenabled
                    path ["reenabled"]
                    behavior patch_after_clear
                    -> Json<()>
            }
        }
        "#,
    );

    let Some(RetryResolved::Set(client_retry)) = &client_policy(&api).retry else {
        panic!("expected client retry to materialize as set");
    };
    assert_eq!(client_retry.max_attempts, 2);
    assert_eq!(
        client_retry
            .methods
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["GET".to_string()]
    );
    assert_eq!(client_retry.statuses, vec![429]);

    let patched_endpoint = endpoint_by_name(&api, "Patched");
    let Some(RetryResolved::Set(scope_retry)) = &scope_policy(&patched_endpoint.policy, 0).retry
    else {
        panic!("expected inherited patch to materialize as set");
    };
    assert_eq!(scope_retry.max_attempts, 2);
    assert_eq!(
        scope_retry
            .methods
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["POST".to_string()]
    );
    assert_eq!(scope_retry.statuses, vec![429]);
    assert!(patched_endpoint.policy.endpoint.retry.is_none());

    let reenabled_endpoint = endpoint_by_name(&api, "Reenabled");
    assert!(matches!(
        scope_policy(&reenabled_endpoint.policy, 0).retry,
        Some(RetryResolved::Clear)
    ));
    let Some(RetryResolved::Set(endpoint_retry)) = &reenabled_endpoint.policy.endpoint.retry else {
        panic!("expected patch after clear to re-enable retry");
    };
    assert_eq!(endpoint_retry.max_attempts, 7);
    assert!(endpoint_retry.methods.is_empty());
    assert!(endpoint_retry.statuses.is_empty());
}
