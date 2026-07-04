use concord_core::advanced::OctetStream;
use concord_macros::api;
use self::stream_endpoint_execute_records_api::StreamEndpointExecuteRecordsApi;

api! {
    client StreamEndpointExecuteRecordsApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamEndpointExecuteRecordsApi) {
    let _ = api.download().execute_records().await.unwrap();
}

fn main() {}
