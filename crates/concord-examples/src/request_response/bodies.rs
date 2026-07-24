//! Buffered, streamed, custom-codec, and multipart request/response shapes.
//!
//! Required features: `json` and `multipart`. Mode: construction and
//! deterministic local execution. Documentation owner:
//! `docs/guide/05-bodies-responses-and-streams.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;
use concord::body::{Multipart, MultipartBody, MultipartPart, Stream, StreamBody};
use concord::codec::OctetStream;
use concord::{Bytes, Json, NoContent, Text};

api! {
    client BodyApi {
        base "http://body.example.test/";
    }

    POST json_round_trip(#[body] body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    POST text_round_trip(#[body] body: Text) -> Text;
    POST custom_round_trip(
        #[body] body: crate::request_response::specifications::ExampleCodec
    ) -> crate::request_response::specifications::ExampleCodec;
    POST upload(#[body] body: Stream<OctetStream>) -> Bytes;
    GET download() -> Stream<OctetStream>;
    POST multipart_upload(#[body] body: Multipart) -> NoContent;
}

/// Constructs every request-body shape without executing transport work.
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = BodyApi::builder().build()?;
    let _: body_api::JsonRoundTripRequest = client.json_round_trip(vec![1, 2, 3]);
    let _: body_api::TextRoundTripRequest = client.text_round_trip("bounded text".to_owned());
    let _: body_api::CustomRoundTripRequest = client.custom_round_trip("custom payload".to_owned());
    let _: body_api::UploadRequest = client.upload(StreamBody::from_bytes(Bytes::from_static(
        b"stream payload",
    )));
    let _: body_api::DownloadRequest = client.download();
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("deterministic example")?)?
        .part(
            "payload",
            MultipartPart::bytes(Bytes::from_static(b"multipart bytes"))?,
        )?;
    let _: body_api::MultipartUploadRequest = client.multipart_upload(multipart);
    Ok(())
}
