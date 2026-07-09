use concord_core::advanced::OctetStream;
use concord_macros::api;
use self::stream_endpoint_response_api::StreamEndpointResponseApi;

api! {
    client StreamEndpointResponseApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamEndpointResponseApi) {
    let _ = api.download().response().await.unwrap();
}

fn main() {}
