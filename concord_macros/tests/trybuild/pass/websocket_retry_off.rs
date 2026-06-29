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
    client WsRetryOffApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [500]
            }
        }

        defaults {
            retry read
        }
    }

    WS Connect
        path ["ws"]
        retry off
        -> WebSocket<ClientMsg, ServerMsg>
}

api! {
    client WsRetryOffInheritedApi {
        base "https://example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [500]
            }
        }

        defaults {
            retry read
        }
    }

    scope realtime {
        retry read

        WS Connect
            path ["ws"]
            retry off
            -> WebSocket<ClientMsg, ServerMsg>
    }
}

fn main() {}
