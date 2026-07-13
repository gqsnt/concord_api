use super::helpers::*;
use quote::quote;

#[test]
fn generated_rate_limit_contains_runtime_plan() {
    let out = expanded(quote! {
        client SnapshotRateLimit {
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
            as ping
            path ["ping"]
            -> Json<()>;
    });

    assert_contains_all(
        &out,
        &[
            "policy . add_rate_limit (:: concord_core :: __private :: RateLimitPlan :: from_buckets",
            "RateLimitBucketUse :: new (\"application\" , \"app_0\"",
            "RateLimitBucketUse :: new",
            "ApiClientError :: rate_limit",
            "RateLimitErrorKind :: InvalidConfiguration",
        ],
    );
    assert!(!out.contains("compile_error!(concat!(\"unresolvedrate_limitkey"));
    assert!(!out.contains("endpoint/scoperate_limitkeycannotbeusedinclientbasepolicy"));
}

#[test]
fn generated_rate_limit_materializes_resolved_policy() {
    let out = expanded(quote! {
        client SnapshotPolicy {
            base "https://example.com"

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }

            default {
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
            "RateLimitWindow::new(::std::num::NonZeroU32::new(10u32).ok_or_else",
            "RateLimitBucketUse::new(\"application\",\"app_0\"",
            "policy.add_rate_limit(::concord_core::__private::RateLimitPlan::from_buckets",
            "ApiClientError :: rate_limit",
        ],
    );
}
