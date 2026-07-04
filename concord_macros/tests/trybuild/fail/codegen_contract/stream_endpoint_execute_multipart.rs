use concord_core::advanced::OctetStream;
use concord_macros::api;
use self::stream_endpoint_execute_multipart_api::StreamEndpointExecuteMultipartApi;

api! {
    client StreamEndpointExecuteMultipartApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamEndpointExecuteMultipartApi) {
    let _ = api.download().execute_multipart().await.unwrap();
}

fn main() {}
