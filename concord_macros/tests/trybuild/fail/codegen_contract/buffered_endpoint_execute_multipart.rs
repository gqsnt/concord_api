use concord_core::prelude::Json;
use concord_macros::api;
use self::buffered_endpoint_execute_multipart_api::BufferedEndpointExecuteMultipartApi;

api! {
    client BufferedEndpointExecuteMultipartApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<String>
}

async fn usage(api: BufferedEndpointExecuteMultipartApi) {
    let _ = api.user().execute_multipart().await.unwrap();
}

fn main() {}
