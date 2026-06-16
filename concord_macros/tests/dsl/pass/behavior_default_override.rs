use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client BehaviorDefaultOverrideApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
        }

        cache standard {
            ttl 60s
        }

        behavior cached_read {
            retry read
            cache standard
        }

        default {
            behavior cached_read
            cache off
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

fn main() {}
