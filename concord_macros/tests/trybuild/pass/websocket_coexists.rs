use concord_core::advanced::{
    JsonSse, JsonWebSocket, Mixed, MultipartStream, RawResponsePart, RecordStream, SseStream,
    StreamResponse, WebSocketClient,
};
use concord_core::prelude::Json;
use concord_macros::api;
use crate::ws_coexist_api::WsCoexistApi;

#[derive(Debug, serde::Serialize)]
pub struct ClientMsg {
    id: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerMsg {
    id: u64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Item {
    id: u64,
}

api! {
    client WsCoexistApi {
        base "https://example.com"
    }

    GET Buffered
        path ["buffered"]
        -> Json<Item>

    GET Streamed
        path ["streamed"]
        -> Stream<concord_core::advanced::OctetStream>

    GET Records
        path ["records"]
        -> Records<Item, concord_core::advanced::NdJson>

    GET Multipart
        path ["multipart"]
        -> Multipart<RawResponsePart, Mixed>

    GET Events
        path ["events"]
        -> Sse<Item, JsonSse>

    WS Connect
        path ["ws"]
        -> WebSocket<ClientMsg, ServerMsg, JsonWebSocket>
}

async fn usage(api: WsCoexistApi) {
    let _: Item = api.buffered().execute().await.unwrap();
    let _: StreamResponse<concord_core::advanced::OctetStream> =
        api.streamed().execute().await.unwrap();
    let _: RecordStream<Item> = api.records().execute_records().await.unwrap();
    let _: MultipartStream<RawResponsePart> = api.multipart().execute().await.unwrap();
    let _: SseStream<Item> = api.events().execute().await.unwrap();
    let _: WebSocketClient<ClientMsg, ServerMsg> =
        api.connect().execute_websocket().await.unwrap();
}

fn main() {}
