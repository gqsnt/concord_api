use bytes::Bytes;
use concord_core::advanced::{OctetStream, StreamBody};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::stream_request_buffered_response_api::StreamRequestBufferedResponseApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult;

api! {
    client StreamRequestBufferedResponseApi {
        base "https://example.com"
    }

    POST Upload(body: Stream<OctetStream>)
        path ["upload"]
        -> Json<UploadResult>
}

async fn usage(api: StreamRequestBufferedResponseApi) {
    let _ = api.upload(StreamBody::from_bytes(Bytes::from_static(b"payload")));
    let _ = api
        .upload(StreamBody::from_bytes(Bytes::from_static(b"payload")))
        .execute()
        .await;
}

fn main() {}
