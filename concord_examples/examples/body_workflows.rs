#![forbid(unsafe_code)]

use concord::Request;
use concord::api;
use concord::body::{MultipartBody, MultipartPart};
use concord::prelude::{Bytes, StreamBody};

api! {
    client BodyApi {
        base "https://body.example.test/";
    }

    POST JsonRoundTrip(body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    POST TextRoundTrip(body: Text) -> Text;
    POST CustomRoundTrip(body: concord_examples::ExampleCodec) -> concord_examples::ExampleCodec;
    POST Upload(body: Stream<OctetStream>) -> Bytes;
    GET Download() -> Stream<OctetStream>;
    POST MultipartUpload(body: Multipart) -> NoContent;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = BodyApi::builder().build()?;
    let _: Request<_> = client.json_round_trip(vec![1, 2, 3]);
    let _: Request<_> = client.text_round_trip("bounded text".to_owned());
    let _: Request<_> = client.custom_round_trip("custom payload".to_owned());
    let _: Request<_> = client.upload(StreamBody::from_bytes(Bytes::from_static(
        b"stream payload",
    )));
    let _: Request<_> = client.download();
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("deterministic example"))?
        .part(
            "payload",
            MultipartPart::bytes(Bytes::from_static(b"multipart bytes")),
        )?;
    let _: Request<_> = client.multipart_upload(multipart);
    Ok(())
}
