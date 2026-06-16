use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client PoliciesGroupMixedApi {
        base "https://example.com"

        retry flat_read {
            max_attempts 2
            methods [GET]
        }

        policies {
            retry grouped_read {
                max_attempts 3
                methods [GET]
            }

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }
        }

        behaviors {
            behavior read {
                retry grouped_read
                rate_limit app
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
