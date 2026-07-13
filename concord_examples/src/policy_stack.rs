use concord_core::prelude::*;
use concord_macros::api;

api! {
    client PolicyApi {
        base "https://example.com"

        policies {
            rate_limit app {
                bucket application by [host] {
                    100 / 1s
                }
            }

        }

        profiles {
            profile read {
                rate_limit app
            }
        }

        default {
            rate_limit app
        }
    }

    GET Text
        as text
        path ["text"]
        -> Text<String>

    GET Unrated
        as unrated
        path ["unrated"]
        rate_limit off
        -> Text<String>

    GET RateLimited
        as rate_limited
        path ["limited"]
        -> Text<String>

    GET ProfileText
        as profile_text
        path ["profile-text"]
        profile read
        -> Text<String>
}

pub use self::policy_api::PolicyApi;

#[cfg(test)]
mod tests {
    use super::PolicyApi;

    #[test]
    fn policy_profile_merge_docs_examples_compile() {
        let _ = std::mem::size_of::<PolicyApi>();
    }
}
