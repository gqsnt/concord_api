use concord_core::prelude::Json;
use concord_macros::api;
use self::buffered_endpoint_execute_sse_api::BufferedEndpointExecuteSseApi;

api! {
    client BufferedEndpointExecuteSseApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<String>
}

async fn usage(api: BufferedEndpointExecuteSseApi) {
    let _ = api.user().execute_sse().await.unwrap();
}

fn main() {}
