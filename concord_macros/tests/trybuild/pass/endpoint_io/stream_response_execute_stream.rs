use concord_core::advanced::{OctetStream, StreamResponse};
use concord_macros::api;
use self::stream_response_execute_stream_api::StreamResponseExecuteStreamApi;

api! {
    client StreamResponseExecuteStreamApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamResponseExecuteStreamApi) {
    let _resp: StreamResponse<OctetStream> = api.download().execute_stream().await.unwrap();
}

fn main() {}
