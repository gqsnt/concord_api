use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorListApi {
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

        behavior base_read {
            retry read
            rate_limit app
        }

        behavior protected_extra {
            auth bearer session
        }
    }

    scope users {
        path ["users"]
        behavior [base_read, protected_extra]

        GET Me
        path ["me"]
        -> Json<User>
    }
}

fn main() {}
