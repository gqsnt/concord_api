use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorRateLimitMergeApi {
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
        as list
        path []
        -> Json<Vec<User>>
    }
}

fn main() {}
