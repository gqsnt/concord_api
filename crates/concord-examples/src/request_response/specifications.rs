//! Public semantic request/response extensions and runtime endpoint parity.
//!
//! Required features: none. Mode: construction and deterministic local
//! execution. Documentation owners:
//! `docs/request_bodies.md`, `docs/responses.md`, and `docs/extensions.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;
use concord::body::{
    BodyError, BodyKind, LengthHint, RequestBody, RequestBodyContext, RequestBodyMetadata,
    RequestContentType, RequestRepresentation,
};
use concord::endpoint::{EndpointId, Request, RuntimeEndpoint};
use concord::http::Method;
use concord::prelude::Bytes;
use concord::response::{
    Buffered, ByteStream, DiscardDriver, ResponseBodyError, ResponseHead, ResponseRepresentation,
    Streaming,
};

use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};

/// A complete buffered custom media codec.
// ANCHOR: codec_extension
pub struct ExampleCodec;

impl EncodeBody for ExampleCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.concord.example");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExampleCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.example");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}
// ANCHOR_END: codec_extension

// ANCHOR: request_body_spec_extension
pub struct ExampleRequestSpec;

impl RequestRepresentation<Bytes> for ExampleRequestSpec {
    fn metadata(
        input: &Bytes,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(u64::try_from(input.len()).unwrap_or(u64::MAX)),
            content_type: RequestContentType::None,
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::bytes(input))
    }
}
// ANCHOR_END: request_body_spec_extension

// ANCHOR: response_body_spec_extension
pub struct ExampleBufferedSpec;

impl ResponseRepresentation<usize> for ExampleBufferedSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExampleStreamingSpec;

impl ResponseRepresentation<ByteStream> for ExampleStreamingSpec {
    type Driver = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExampleDiscardSpec;

impl ResponseRepresentation<()> for ExampleDiscardSpec {
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}
// ANCHOR_END: response_body_spec_extension

// ANCHOR: body_spec_usage
api! {
    client GenericSpecificationApi {
        timeout 30s;
        base "http://generic-specifications.example.test/";


    POST buffered(
        body body: Bytes via crate::request_response::specifications::ExampleRequestSpec
    ) -> usize via crate::request_response::specifications::ExampleBufferedSpec;
    GET streaming() -> concord::response::ByteStream via crate::request_response::specifications::ExampleStreamingSpec;
    GET discard() -> () via crate::request_response::specifications::ExampleDiscardSpec;
}}

/// Constructs generated and runtime-defined requests over the same extensions.
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = GenericSpecificationApi::builder().build()?;
    let _: generic_specification_api::BufferedRequest =
        client.buffered(Bytes::from_static(b"generic request"));
    let _: generic_specification_api::StreamingRequest = client.streaming();
    let _: generic_specification_api::DiscardRequest = client.discard();

    let runtime = RuntimeEndpoint::builder(
        EndpointId::new("generic_specification_api.runtime")?,
        Method::POST,
    )
    .body_spec::<ExampleRequestSpec, _>(Bytes::from_static(b"runtime generic request"))
    .response::<ExampleBufferedSpec, usize>()
    .build()?;
    let _: Request<_> = runtime.try_bind(&client)?;
    Ok(())
}
// ANCHOR_END: body_spec_usage
