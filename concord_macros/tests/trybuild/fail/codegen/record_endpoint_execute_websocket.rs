use concord_core::advanced::NdJson;
use concord_macros::api;
use crate::record_endpoint_execute_web_socket_api::RecordEndpointExecuteWebSocketApi;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LogEntry {
    id: u64,
}

api! {
    client RecordEndpointExecuteWebSocketApi {
        base "https://example.com"
    }

    GET Tail
        path ["tail"]
        -> Records<LogEntry, NdJson>
}

async fn usage(api: RecordEndpointExecuteWebSocketApi) {
    let _ = api.tail().execute_websocket().await.unwrap();
}

fn main() {}
