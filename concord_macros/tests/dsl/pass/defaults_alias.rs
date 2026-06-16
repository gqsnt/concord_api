use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client DefaultsAliasApi {
        base "https://example.com"

        policies {
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
        }

        behaviors {
            behavior read {
                retry read
                rate_limit app
            }
        }

        defaults {
            behavior read
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

fn main() {}
