use concord_core::advanced::OctetStream;
use concord_macros::api;
use crate::stream_endpoint_execute_web_socket_api::StreamEndpointExecuteWebSocketApi;

api! {
    client StreamEndpointExecuteWebSocketApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamEndpointExecuteWebSocketApi) {
    let _ = api.download().execute_websocket().await.unwrap();
}

fn main() {}
