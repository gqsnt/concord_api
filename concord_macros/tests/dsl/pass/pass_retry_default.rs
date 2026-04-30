use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Pong;

api! {
    client CurrentRetryDefaultApi {
        base "https://example.com"

        default {
            retry read
            rate_limit app
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500, 503]
            retry_after
        }

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }
    }

    GET Ping
        as ping
        path ["ping"]
        -> Json<Pong>
}

fn main() {}
