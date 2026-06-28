use concord_core::advanced::{MediaType, StreamResponse};
use concord_macros::api;
use self::stream_custom_media_api::StreamCustomMediaApi;

pub struct AudioWave;

impl MediaType for AudioWave {
    const CONTENT_TYPE: &'static str = "audio/wav";
}

api! {
    client StreamCustomMediaApi {
        base "https://example.com"
    }

    GET Play
        path ["play"]
        -> Stream<AudioWave>
}

async fn usage(api: StreamCustomMediaApi) {
    let _resp: StreamResponse<AudioWave> = api.play().execute().await.unwrap();
}

fn main() {}
