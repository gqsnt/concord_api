use super::helpers::*;
use quote::quote;

#[test]
fn generated_retry_materializes_resolved_policy() {
    let out = expanded(quote! {
        client SnapshotPolicy {
            base "https://example.com"

            retry read {
                max_attempts 2
                methods [GET]
                on [401, 403]
                retry_after
            }

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }

            default {
                retry read
                rate_limit app
            }
        }

        GET Ping
            as ping
            path ["ping"]
            -> Json<String>;
    });

    assert_contains_all(
        &out,
        &[
            "::http::StatusCode::from_u16(401u16)",
            "::http::StatusCode::from_u16(403u16)",
            "policy.set_retry(::concord_core::advanced::RetryConfig",
        ],
    );
    assert_not_contains_all(
        &out,
        &[
            "drive_attempts",
            "send_and_classify_once",
            "RetryAdmissionRegistry",
            "tokio::time::sleep",
            "should_retry(",
        ],
    );
    assert!(!out.contains(&forbidden_reqwest_request_try_clone()));
}
