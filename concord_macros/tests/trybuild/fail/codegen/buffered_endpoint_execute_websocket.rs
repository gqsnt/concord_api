use concord_core::prelude::Json;
use concord_macros::api;
use crate::buffered_endpoint_execute_web_socket_api::BufferedEndpointExecuteWebSocketApi;

#[derive(Debug, serde::Deserialize)]
pub struct User {
    id: u64,
}

api! {
    client BufferedEndpointExecuteWebSocketApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<User>
}

async fn usage(api: BufferedEndpointExecuteWebSocketApi) {
    let _ = api.user().execute_websocket().await.unwrap();
}

fn main() {}
