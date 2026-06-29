use concord_core::advanced::RawResponsePart;
use concord_macros::api;
use crate::multipart_endpoint_execute_web_socket_api::MultipartEndpointExecuteWebSocketApi;

api! {
    client MultipartEndpointExecuteWebSocketApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Multipart<RawResponsePart>
}

async fn usage(api: MultipartEndpointExecuteWebSocketApi) {
    let _ = api.download().execute_websocket().await.unwrap();
}

fn main() {}
