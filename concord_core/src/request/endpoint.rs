use std::marker::PhantomData;

use crate::body::{BodyMetadata, BodySpec};
use crate::codec::DecodeBody;
use crate::error::ApiError;

use super::{HeadBuilder, PreparedHead, RequestMeta};

mod sealed {
    pub trait Response {}
}

/// Generated-private typed successful-response integration.
#[doc(hidden)]
pub trait ResponseHandler: sealed::Response + Send + Sync + 'static {
    type Output: Send + 'static;

    const ACCEPT: Option<&'static str>;
}

/// Generated-private capability for successful response modes that support the
/// metadata-bearing buffered completion terminal.
#[doc(hidden)]
pub trait MetadataResponse: ResponseHandler {}

/// Built-in finite bytes response handler marker.
#[doc(hidden)]
pub struct BytesResponse;

impl ResponseHandler for BytesResponse {
    type Output = bytes::Bytes;

    const ACCEPT: Option<&'static str> = None;
}

impl sealed::Response for BytesResponse {}

impl MetadataResponse for BytesResponse {}

/// Built-in all-2xx body-ignoring response handler marker.
#[doc(hidden)]
pub struct NoContentResponse;

impl ResponseHandler for NoContentResponse {
    type Output = ();

    const ACCEPT: Option<&'static str> = None;
}

impl sealed::Response for NoContentResponse {}

impl MetadataResponse for NoContentResponse {}

/// Generated-private successful buffered response backed by one public codec.
#[doc(hidden)]
pub struct BufferedResponse<C>(PhantomData<C>);

impl<C> ResponseHandler for BufferedResponse<C>
where
    C: DecodeBody,
{
    type Output = C::Output;

    const ACCEPT: Option<&'static str> = C::ACCEPT;
}

impl<C> sealed::Response for BufferedResponse<C> where C: DecodeBody {}

impl<C> MetadataResponse for BufferedResponse<C> where C: DecodeBody {}

/// Generated-private bridge from one concrete endpoint value into core.
#[doc(hidden)]
pub trait EndpointSpec: Send + 'static {
    type Body: BodySpec;
    type Response: ResponseHandler;

    fn request_meta(&self) -> RequestMeta;

    fn prepare_head_and_body_metadata(
        &self,
        builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError>;

    fn into_body(self) -> Self::Body;
}
