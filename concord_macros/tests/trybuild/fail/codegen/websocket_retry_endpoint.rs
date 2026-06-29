use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientMsg {
    id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerMsg {
    id: u64,
}

api! {
    client WsRetryEndpointApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET, POST]
                on [500]
            }
        }
    }

    WS Connect
        path ["ws"]
        retry read
        -> WebSocket<ClientMsg, ServerMsg>
}

fn main() {}
