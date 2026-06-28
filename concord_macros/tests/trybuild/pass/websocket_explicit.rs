use concord_core::advanced::{JsonWebSocket, WebSocketClient};
use concord_macros::api;
use crate::ws_explicit_api::WsExplicitApi;

#[derive(Debug, serde::Serialize)]
pub struct ClientMsg {
    id: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerMsg {
    id: u64,
}

api! {
    client WsExplicitApi {
        base "https://example.com"
    }

    WS Connect
        path ["ws"]
        -> WebSocket<ClientMsg, ServerMsg, JsonWebSocket>
}

async fn usage(api: WsExplicitApi) {
    let _: WebSocketClient<ClientMsg, ServerMsg> =
        api.connect().execute_websocket().await.unwrap();
    let _: WebSocketClient<ClientMsg, ServerMsg> = api.connect().execute().await.unwrap();
}

fn main() {}
