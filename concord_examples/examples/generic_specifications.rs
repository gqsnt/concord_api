#![forbid(unsafe_code)]

use concord::Request;
use concord::api;
use concord::body::{
    BodyError, BodyKind, LengthHint, RequestBody, RequestBodyContext, RequestBodyMetadata,
    RequestBodySpec, RequestContentType,
};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;
use concord::prelude::Bytes;
use concord::response::{
    Buffered, ByteStream, Discard, ResponseBodyError, ResponseBodySpec, ResponseHead, Streaming,
};

pub struct ExampleRequestSpec;

impl RequestBodySpec for ExampleRequestSpec {
    type Input = Bytes;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(u64::try_from(input.len()).unwrap_or(u64::MAX)),
            content_type: RequestContentType::None,
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::bytes(input))
    }
}

pub struct ExampleBufferedSpec;

impl ResponseBodySpec for ExampleBufferedSpec {
    type Output = usize;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExampleStreamingSpec;

impl ResponseBodySpec for ExampleStreamingSpec {
    type Output = ByteStream;
    type Mode = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExampleDiscardSpec;

impl ResponseBodySpec for ExampleDiscardSpec {
    type Output = ();
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

api! {
    client GenericSpecificationApi {
        base "https://generic-specifications.example.test/";
    }

    POST buffered(#[body] body: crate::ExampleRequestSpec) -> crate::ExampleBufferedSpec;
    GET streaming() -> crate::ExampleStreamingSpec;
    GET discard() -> crate::ExampleDiscardSpec;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = GenericSpecificationApi::builder().build()?;
    let _: generic_specification_api::BufferedRequest =
        client.buffered(Bytes::from_static(b"generic request"));
    let _: generic_specification_api::StreamingRequest = client.streaming();
    let _: generic_specification_api::DiscardRequest = client.discard();

    let runtime = RuntimeEndpoint::builder(
        EndpointId::new("generic_specification_api.runtime")?,
        Method::POST,
    )
    .body_spec::<ExampleRequestSpec>(Bytes::from_static(b"runtime generic request"))
    .response::<ExampleBufferedSpec>()
    .build()?;
    let _: Request<_> = client.request(runtime);
    Ok(())
}
