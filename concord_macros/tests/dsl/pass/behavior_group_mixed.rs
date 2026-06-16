use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorGroupMixedApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
        }

        behavior base_read {
            retry read
        }

        behaviors {
            behavior protected_read extends base_read {
                retry read
            }
        }

        default {
            behavior protected_read
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

fn main() {}
