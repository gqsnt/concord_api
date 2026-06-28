use bytes::Bytes;
use concord_core::advanced::{OctetStream, StreamBody, StreamResponse};
use concord_macros::api;
use self::stream_pipe_api::StreamPipeApi;

api! {
    client StreamPipeApi {
        base "https://example.com"
    }

    POST Pipe(body: Stream<OctetStream>)
        path ["pipe"]
        -> Stream<OctetStream>
}

async fn usage(api: StreamPipeApi) {
    let _resp: StreamResponse<OctetStream> = api
        .pipe(StreamBody::from_bytes(Bytes::from_static(b"payload")))
        .execute()
        .await
        .unwrap();
}

fn main() {}
