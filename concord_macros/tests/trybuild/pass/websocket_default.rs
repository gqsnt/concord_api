use concord_core::advanced::WebSocketClient;
use concord_macros::api;
use crate::ws_default_api::WsDefaultApi;

#[derive(Debug, serde::Serialize)]
pub struct ClientMsg {
    id: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerMsg {
    id: u64,
}

api! {
    client WsDefaultApi {
        base "https://example.com"
    }

    WS Connect
        path ["ws"]
        -> WebSocket<ClientMsg, ServerMsg>
}

async fn usage(api: WsDefaultApi) {
    let _: WebSocketClient<ClientMsg, ServerMsg> =
        api.connect().execute_websocket().await.unwrap();
    let _: WebSocketClient<ClientMsg, ServerMsg> = api.connect().execute().await.unwrap();
}

fn main() {}
