use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorDefaultApi {
        base "https://example.com"
        secret token: String
        credential session = bearer(secret.token)

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
        }

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }

        behavior protected_read {
            auth bearer session
            retry read
            rate_limit app
        }

        default {
            behavior protected_read
        }
    }

    scope users {
        path ["users"]

        GET Me
        as me
        path ["me"]
        -> Json<User>
    }
}

fn main() {}
