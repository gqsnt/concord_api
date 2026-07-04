use concord_core::advanced::{OctetStream, StreamResponse};
use concord_macros::api;
use self::stream_response_api::StreamResponseApi;

api! {
    client StreamResponseApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamResponseApi) {
    let _resp: StreamResponse<OctetStream> = api.download().execute().await.unwrap();
}

fn main() {}
