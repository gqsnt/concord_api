//! Public semantic request/response extensions and runtime endpoint parity.
//!
//! Required features: none. Mode: construction and deterministic local
//! execution. Documentation owners:
//! `docs/request_bodies.md`, `docs/responses.md`, and `docs/extensions.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! A request representation has one authority, `prepare`, which returns the
//! managed [`PreparedBody`] that owns any exact `Content-Type`; there is no
//! second metadata callback (BODY-01). Response representations declare
//! their core driver and a static `ACCEPT`.

use concord::api;
use concord::body::{BodyError, PreparedBody, RequestBodyContext, RequestRepresentation};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
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
        if content_type == <Self as DecodeBody>::ACCEPT {
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
/// A raw-bytes request representation without a media type.
pub struct ExampleRequestSpec;

impl RequestRepresentation<Bytes> for ExampleRequestSpec {
    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::bytes(input))
    }
}
// ANCHOR_END: request_body_spec_extension

// ANCHOR: response_body_spec_extension
/// A buffered representation returning the accepted body length.
pub struct ExampleBufferedSpec;

impl ResponseRepresentation<usize> for ExampleBufferedSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

/// A streaming representation returning the managed byte stream itself.
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

/// A discarding representation: the accepted body is dropped unread.
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
        base "http://generic-specifications.example.test/";
        timeout 30s;

        POST buffered(body body: Bytes via ExampleRequestSpec) -> usize via ExampleBufferedSpec;
        GET streaming() -> ByteStream via ExampleStreamingSpec;
        GET discard() -> () via ExampleDiscardSpec;
    }
}

/// Constructs generated and runtime-defined requests over the same extensions.
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = GenericSpecificationApi::new()?;
    let _: generic_specification_api::BufferedRequest =
        client.buffered(Bytes::from_static(b"generic request"));
    let _: generic_specification_api::StreamingRequest = client.streaming();
    let _: generic_specification_api::DiscardRequest = client.discard();

    let runtime = RuntimeEndpoint::builder(
        EndpointId::new("generic_specification_api.runtime")?,
        Method::POST,
    )
    .body_repr::<ExampleRequestSpec, _>(Bytes::from_static(b"runtime generic request"))
    .response::<ExampleBufferedSpec, usize>()
    .build()?;
    let _bound = client.bind(runtime)?;
    Ok(())
}
// ANCHOR_END: body_spec_usage
