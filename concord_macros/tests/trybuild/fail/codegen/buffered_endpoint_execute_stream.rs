use concord_core::prelude::Json;
use concord_macros::api;
use self::buffered_endpoint_execute_stream_api::BufferedEndpointExecuteStreamApi;

api! {
    client BufferedEndpointExecuteStreamApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<String>
}

async fn usage(api: BufferedEndpointExecuteStreamApi) {
    let _ = api.user().execute_stream().await.unwrap();
}

fn main() {}
