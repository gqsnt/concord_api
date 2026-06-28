use concord_core::advanced::OctetStream;
use concord_macros::api;
use self::stream_endpoint_execute_sse_api::StreamEndpointExecuteSseApi;

api! {
    client StreamEndpointExecuteSseApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamEndpointExecuteSseApi) {
    let _ = api.download().execute_sse().await.unwrap();
}

fn main() {}
