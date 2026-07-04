use concord_core::prelude::Json;
use concord_macros::api;
use self::buffered_endpoint_execute_records_api::BufferedEndpointExecuteRecordsApi;

api! {
    client BufferedEndpointExecuteRecordsApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<String>
}

async fn usage(api: BufferedEndpointExecuteRecordsApi) {
    let _ = api.user().execute_records().await.unwrap();
}

fn main() {}
