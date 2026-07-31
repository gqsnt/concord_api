//! Buffered, streamed, custom-codec, and multipart request/response shapes.
//!
//! Required features: `json` and `multipart`. Mode: construction and
//! deterministic local execution. Documentation owners:
//! `docs/request_bodies.md`, `docs/responses.md`, and `docs/extensions.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::pin::Pin;

use concord::api;
use concord::body::{Multipart, MultipartBody, MultipartPart, Stream, StreamBody};
use concord::codec::OctetStream;
use concord::{Bytes, Json, NoContent, Text};
use futures_core::Stream as _;

// ANCHOR: body_contract
api! {
    client BodyApi {
        base "http://body.example.test/";
    }

    // ANCHOR: buffered_endpoints
    POST json_round_trip(#[body] body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    POST text_round_trip(#[body] body: Text) -> Text;
    // ANCHOR: custom_codec_endpoint
    POST custom_round_trip(
        #[body] body: crate::request_response::specifications::ExampleCodec
    ) -> crate::request_response::specifications::ExampleCodec;
    // ANCHOR_END: custom_codec_endpoint
    // ANCHOR_END: buffered_endpoints

    // ANCHOR: streaming_endpoints
    POST upload(#[body] body: Stream) -> Bytes;
    GET download() -> Stream;
    POST exact_upload(#[body] body: Stream<OctetStream>) -> Bytes;
    GET exact_download() -> Stream<OctetStream>;
    // ANCHOR_END: streaming_endpoints

    // ANCHOR: multipart_endpoint
    POST multipart_upload(#[body] body: Multipart) -> NoContent;
    // ANCHOR_END: multipart_endpoint
}
// ANCHOR_END: body_contract

/// Constructs every request-body shape without executing transport work.
// ANCHOR: body_requests
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = BodyApi::builder().build()?;
    // ANCHOR: buffered_requests
    let _: body_api::JsonRoundTripRequest = client.json_round_trip(vec![1, 2, 3]);
    let _: body_api::TextRoundTripRequest = client.text_round_trip("bounded text".to_owned());
    let _: body_api::CustomRoundTripRequest = client.custom_round_trip("custom payload".to_owned());
    // ANCHOR_END: buffered_requests

    // ANCHOR: streaming_requests
    let _: body_api::UploadRequest = client.upload(StreamBody::from_bytes(Bytes::from_static(
        b"stream payload",
    )));
    let _: body_api::DownloadRequest = client.download();
    let _: body_api::ExactUploadRequest = client.exact_upload(StreamBody::from_bytes(
        Bytes::from_static(b"binary payload"),
    ));
    let _: body_api::ExactDownloadRequest = client.exact_download();
    // ANCHOR_END: streaming_requests

    // ANCHOR: multipart_recipe
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("deterministic example")?)?
        .part(
            "payload",
            MultipartPart::bytes(Bytes::from_static(b"multipart bytes"))?,
        )?;
    let _: body_api::MultipartUploadRequest = client.multipart_upload(multipart);
    // ANCHOR_END: multipart_recipe
    Ok(())
}
// ANCHOR_END: body_requests

/// Opens and consumes a bounded response stream without buffering it as one value.
// ANCHOR: consume_byte_stream
pub async fn download_size(client: &BodyApi) -> Result<u64, concord::ApiError> {
    let mut stream = client.download().await?;
    let mut received = 0_u64;
    while let Some(chunk) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        let chunk = chunk?;
        received = received.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
    }
    Ok(received)
}
// ANCHOR_END: consume_byte_stream

/// Builds a one-shot multipart request from an opened file stream.
// ANCHOR: multipart_file
pub async fn file_upload(
    client: &BodyApi,
    path: impl AsRef<std::path::Path>,
) -> Result<body_api::MultipartUploadRequest, concord::body::BodyError> {
    let file = StreamBody::from_file(path).await?;
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("file upload")?)?
        .part(
            "payload",
            MultipartPart::stream(file)
                .file_name("payload.bin")?
                .content_type("application/octet-stream")?,
        )?;
    Ok(client.multipart_upload(multipart))
}
// ANCHOR_END: multipart_file
