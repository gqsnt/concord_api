use super::helpers::{analyze_ok, client_policy, single_endpoint};
use crate::sema::RetryResolved;

#[test]
fn retry_resolution_lowers_named_retry_profile() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry read {
                    max_attempts 3
                    methods [GET, POST]
                    on [429, 503]
                    retry_after
                }
            }

            GET Ping
                path ["ping"]
                retry read
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    let Some(RetryResolved::Set(retry)) = &endpoint.policy.endpoint.retry else {
        panic!("expected named retry profile to resolve on endpoint");
    };
    assert_eq!(retry.max_attempts, 3);
    assert_eq!(
        retry
            .methods
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["GET".to_string(), "POST".to_string()]
    );
    assert_eq!(retry.statuses, vec![429, 503]);
    assert!(retry.respect_retry_after);
}

#[test]
fn retry_resolution_lowers_retry_off() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                retry off
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert!(matches!(
        endpoint.policy.endpoint.retry,
        Some(RetryResolved::Clear)
    ));
}

#[test]
fn retry_resolution_lowers_extended_retry_profile_and_endpoint_clear() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                retry read {
                    max_attempts 2
                    methods [GET]
                }

                retry read_child extends read {
                    on [429]
                    retry_after
                }

                default {
                    retry read_child
                }
            }

            GET Ping
                path ["ping"]
                retry off
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    let Some(RetryResolved::Set(client_retry)) = &client_policy(&api).retry else {
        panic!("expected inherited client retry profile to resolve");
    };
    assert_eq!(client_retry.max_attempts, 2);
    assert_eq!(client_retry.statuses, vec![429]);
    assert!(client_retry.respect_retry_after);

    assert!(matches!(
        endpoint.policy.endpoint.retry,
        Some(RetryResolved::Clear)
    ));
}
