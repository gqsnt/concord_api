use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

#[cfg(feature = "cache-moka")]
api! {
    client PoliciesGroupApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [429, 500]
            }

            cache standard {
                ttl 60s
            }

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }
        }

        behaviors {
            behavior read_cached {
                retry read
                cache standard
                rate_limit app
            }
        }

        default {
            behavior read_cached
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

#[cfg(feature = "cache-moka")]
fn main() {}

#[cfg(not(feature = "cache-moka"))]
fn main() {}
