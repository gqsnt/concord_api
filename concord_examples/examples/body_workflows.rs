#![forbid(unsafe_code)]

use concord::api;
use concord::body::{MultipartBody, MultipartPart};
use concord::prelude::{Bytes, StreamBody};

api! {
    client BodyApi {
        base "https://body.example.test/";
    }

    POST json_round_trip(#[body] body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    POST text_round_trip(#[body] body: Text) -> Text;
    POST custom_round_trip(#[body] body: concord_examples::ExampleCodec) -> concord_examples::ExampleCodec;
    POST upload(#[body] body: Stream<OctetStream>) -> Bytes;
    GET download() -> Stream<OctetStream>;
    POST multipart_upload(#[body] body: Multipart) -> NoContent;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = BodyApi::builder().build()?;
    let _: body_api::JsonRoundTripRequest = client.json_round_trip(vec![1, 2, 3]);
    let _: body_api::TextRoundTripRequest = client.text_round_trip("bounded text".to_owned());
    let _: body_api::CustomRoundTripRequest = client.custom_round_trip("custom payload".to_owned());
    let _: body_api::UploadRequest = client.upload(StreamBody::from_bytes(Bytes::from_static(
        b"stream payload",
    )));
    let _: body_api::DownloadRequest = client.download();
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("deterministic example"))?
        .part(
            "payload",
            MultipartPart::bytes(Bytes::from_static(b"multipart bytes")),
        )?;
    let _: body_api::MultipartUploadRequest = client.multipart_upload(multipart);
    Ok(())
}
