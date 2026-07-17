use std::marker::PhantomData;
use std::num::NonZeroUsize;

use bytes::Bytes;
use http::Method;

use crate::codec::{DecodeBody, EncodeBody, EncodeContext};
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::BodyKind;

use super::{HeadBuilder, PreparedHead, RequestMeta};

mod sealed {
    pub trait Body {}
    pub trait Response {}
}

/// Empty request-body value used by bodyless generated endpoints.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyBody;

impl EmptyBody {
    #[doc(hidden)]
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn metadata() -> BodyMetadata {
        BodyMetadata {
            kind: BodyKind::Empty,
            known_length: Some(0),
            content_type: None,
        }
    }
}

/// Generated-private safe request-body facts available before materialization.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodyMetadata {
    kind: BodyKind,
    known_length: Option<u64>,
    content_type: Option<&'static str>,
}

impl BodyMetadata {
    #[doc(hidden)]
    pub const fn kind(self) -> BodyKind {
        self.kind
    }

    #[doc(hidden)]
    pub const fn known_length(self) -> Option<u64> {
        self.known_length
    }

    pub(crate) const fn content_type(self) -> Option<&'static str> {
        self.content_type
    }

    pub(crate) const fn with_known_length(self, known_length: u64) -> Self {
        Self {
            known_length: Some(known_length),
            ..self
        }
    }
}

/// The one generated-private materialized application body representation.
#[doc(hidden)]
pub struct PreparedBody {
    bytes: Option<Bytes>,
}

impl PreparedBody {
    pub(crate) const fn empty() -> Self {
        Self { bytes: None }
    }

    pub(crate) fn buffered(bytes: Bytes) -> Self {
        Self { bytes: Some(bytes) }
    }

    pub(crate) fn known_length(&self) -> u64 {
        self.bytes
            .as_ref()
            .map_or(0, |bytes| bytes.len().try_into().unwrap_or(u64::MAX))
    }

    pub(crate) fn boundary_bytes(&self) -> Bytes {
        self.bytes.clone().unwrap_or_default()
    }

    pub(crate) fn into_bytes(self) -> Option<Bytes> {
        self.bytes
    }
}

/// Generated-private body integration implemented only by core body types.
#[doc(hidden)]
pub trait BodySpec: sealed::Body + Send + 'static {
    fn metadata() -> BodyMetadata;

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError>;
}

impl BodySpec for EmptyBody {
    fn metadata() -> BodyMetadata {
        Self::metadata()
    }

    fn materialize(
        self,
        _endpoint: &'static str,
        _method: &Method,
        _limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        Ok(PreparedBody::empty())
    }
}

impl sealed::Body for EmptyBody {}

/// Generated-private request body backed by one public buffered codec.
#[doc(hidden)]
pub struct CodecBody<C>
where
    C: EncodeBody,
{
    input: C::Input,
    marker: PhantomData<C>,
}

impl<C> CodecBody<C>
where
    C: EncodeBody,
{
    #[doc(hidden)]
    #[must_use]
    pub fn new(input: C::Input) -> Self {
        Self {
            input,
            marker: PhantomData,
        }
    }
}

impl<C> BodySpec for CodecBody<C>
where
    C: EncodeBody,
{
    fn metadata() -> BodyMetadata {
        BodyMetadata {
            kind: BodyKind::Buffered,
            known_length: None,
            content_type: C::CONTENT_TYPE,
        }
    }

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        let bytes = C::encode(
            self.input,
            EncodeContext {
                endpoint,
                method,
                body_limit: Some(limit.get()),
            },
        )
        .map_err(|error| {
            drop(error);
            ApiError::classified(endpoint, method.clone(), ErrorCategory::RequestEncoding)
        })?;
        if bytes.len() > limit.get() {
            return Err(ApiError::classified(
                endpoint,
                method.clone(),
                ErrorCategory::RequestBody,
            ));
        }
        Ok(PreparedBody::buffered(bytes))
    }
}

impl<C> sealed::Body for CodecBody<C> where C: EncodeBody {}

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
