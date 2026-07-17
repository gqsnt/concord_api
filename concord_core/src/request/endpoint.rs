use crate::error::ApiError;
use crate::hooks::BodyKind;

use super::{HeadBuilder, PreparedHead, RequestMeta};

mod sealed {
    pub trait Body {}
    pub trait Response {}
}

/// Empty request-body value used by Phase 2 generated endpoints.
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
        }
    }
}

/// Generated-private safe request-body facts available before materialization.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodyMetadata {
    kind: BodyKind,
    known_length: Option<u64>,
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
}

/// Private materialized body wrapper that never exposes a native body.
#[doc(hidden)]
pub struct PreparedBody {
    _private: (),
}

/// Generated-private body integration implemented only by core body types.
#[doc(hidden)]
pub trait BodySpec: sealed::Body + Send + 'static {
    fn materialize(self) -> Result<PreparedBody, ApiError>;
}

impl BodySpec for EmptyBody {
    fn materialize(self) -> Result<PreparedBody, ApiError> {
        Ok(PreparedBody { _private: () })
    }
}

impl sealed::Body for EmptyBody {}

/// Generated-private typed successful-response integration.
#[doc(hidden)]
pub trait ResponseHandler: sealed::Response + Send + Sync + 'static {
    type Output: Send + 'static;
}

/// Built-in finite bytes response handler marker.
#[doc(hidden)]
pub struct BytesResponse;

impl ResponseHandler for BytesResponse {
    type Output = bytes::Bytes;
}

impl sealed::Response for BytesResponse {}

/// Built-in all-2xx body-ignoring response handler marker.
#[doc(hidden)]
pub struct NoContentResponse;

impl ResponseHandler for NoContentResponse {
    type Output = ();
}

impl sealed::Response for NoContentResponse {}

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
