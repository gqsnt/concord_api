use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorGroupApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
        }

        behaviors {
            behavior read {
                retry read
            }
        }

        default {
            behavior read
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

fn main() {}
