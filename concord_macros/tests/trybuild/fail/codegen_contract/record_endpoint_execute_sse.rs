use concord_core::advanced::NdJson;
use concord_macros::api;
use self::record_endpoint_execute_sse_api::RecordEndpointExecuteSseApi;

api! {
    client RecordEndpointExecuteSseApi {
        base "https://example.com"
    }

    GET Tail
        path ["tail"]
        -> Records<String, NdJson>
}

async fn usage(api: RecordEndpointExecuteSseApi) {
    let _ = api.tail().execute_sse().await.unwrap();
}

fn main() {}
