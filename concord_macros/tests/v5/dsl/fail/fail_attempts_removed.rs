use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AttemptsRemovedApi {
        base https "example.com"

        retry read {
            attempts 2
            methods [GET]
            on [500]
        }
    }

    GET Ping
        path ["ping"]
        -> Json<String>
}

fn main() {}
