use concord_core::advanced::OctetStream;
use concord_core::prelude::Json;
use concord_macros::api;

api! {
    client RetryHttpResponseApi {
        base "https://example.com"
        retry read {
            max_attempts 2
            methods [GET]
            on [500]
        }
    }
    GET Buffered
        retry read
        path ["buffered"]
        -> Json<Item>
    GET Streamed
        retry read
        path ["streamed"]
        -> Stream<OctetStream>
}

#[derive(serde::Deserialize)]
pub struct Item;

fn main() {}
