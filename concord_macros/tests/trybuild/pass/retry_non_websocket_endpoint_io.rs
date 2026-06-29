use concord_core::advanced::{Mixed, NdJson, OctetStream, RawResponsePart};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LogEntry {
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct Event {
    id: u64,
}

api! {
    client RetryNonWebSocketEndpointIoApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET, POST]
                on [500]
            }
        }
    }

    GET Users
        retry read
        path ["users"]
        -> Json<Vec<User>>

    GET Download
        retry read
        path ["download"]
        -> Stream<OctetStream>

    GET Tail
        retry read
        path ["tail"]
        -> Records<LogEntry, NdJson>

    GET Parts
        retry read
        path ["parts"]
        -> Multipart<RawResponsePart, Mixed>

    GET Events
        retry read
        path ["events"]
        -> Sse<Event>
}

fn main() {}
