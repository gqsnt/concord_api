use concord_core::advanced::{ContentType, StreamBody, StreamResponse};
use concord_macros::api;
use self::stream_content_type_api::StreamContentTypeApi;

#[derive(Debug, Default)]
pub struct AudioWave;

impl ContentType for AudioWave {
    const CONTENT_TYPE: &'static str = "audio/wav";
}

api! {
    client StreamContentTypeApi {
        base "https://example.com"
    }

    POST Upload(body: Stream<AudioWave>)
        path ["upload"]
        -> Stream<AudioWave>
}

async fn usage(api: StreamContentTypeApi) {
    let _request = api.upload(StreamBody::from_bytes(bytes::Bytes::from_static(b"wave")));
    let _response: StreamResponse<AudioWave> = api.upload(StreamBody::from_bytes(bytes::Bytes::from_static(b"wave"))).execute_stream().await.unwrap();
}

fn main() {}
