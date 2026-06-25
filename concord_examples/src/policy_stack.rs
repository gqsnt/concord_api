use concord_core::prelude::*;
use concord_macros::api;

api! {
    client PolicyApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [429, 500]
                retry_after
            }

            cache standard {
                ttl 60s
                revalidate
                on_error serve_stale
            }

            rate_limit app {
                bucket application by [host] {
                    100 / 1s
                }
            }

        }

        behaviors {
            behavior read {
                retry read
                cache standard
                rate_limit app
            }
        }

        defaults {
            retry read
            cache standard
            rate_limit app
        }
    }

    GET Text
        as text
        path ["text"]
        -> Text<String>

    GET RetryOnly
        as retry_only
        path ["retry"]
        cache off
        rate_limit off
        -> Text<String>

    GET RateLimited
        as rate_limited
        path ["limited"]
        cache off
        -> Text<String>

    GET BehaviorText
        as behavior_text
        path ["behavior-text"]
        behavior read
        -> Text<String>
}

pub use self::policy_api::PolicyApi;

#[cfg(test)]
mod tests {
    use super::PolicyApi;

    #[test]
    fn policy_behavior_merge_docs_examples_compile() {
        let _ = std::mem::size_of::<PolicyApi>();
    }
}
