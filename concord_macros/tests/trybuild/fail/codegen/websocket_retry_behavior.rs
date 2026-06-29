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
    client WsRetryBehaviorApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET, POST]
                on [500]
            }
        }

        behaviors {
            behavior realtime {
                retry read
            }
        }
    }

    WS Connect
        behavior realtime
        path ["ws"]
        -> WebSocket<ClientMsg, ServerMsg>
}

fn main() {}
