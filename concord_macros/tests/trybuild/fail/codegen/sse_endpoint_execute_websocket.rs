use concord_macros::api;
use crate::sse_endpoint_execute_web_socket_api::SseEndpointExecuteWebSocketApi;

#[derive(Debug, serde::Deserialize)]
pub struct LogEvent {
    id: u64,
}

api! {
    client SseEndpointExecuteWebSocketApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<LogEvent>
}

async fn usage(api: SseEndpointExecuteWebSocketApi) {
    let _ = api.events().execute_websocket().await.unwrap();
}

fn main() {}
