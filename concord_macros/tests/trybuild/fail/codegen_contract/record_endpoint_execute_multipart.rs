use concord_core::advanced::NdJson;
use concord_macros::api;
use self::record_endpoint_execute_multipart_api::RecordEndpointExecuteMultipartApi;

api! {
    client RecordEndpointExecuteMultipartApi {
        base "https://example.com"
    }

    GET Tail
        path ["tail"]
        -> Records<String, NdJson>
}

async fn usage(api: RecordEndpointExecuteMultipartApi) {
    let _ = api.tail().execute_multipart().await.unwrap();
}

fn main() {}
