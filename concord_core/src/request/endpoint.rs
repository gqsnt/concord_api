use crate::body::{RequestBodyContext, RequestBodyMetadata, RequestBodySpec};
use crate::error::ApiError;
use crate::response::ResponseBodySpec;

use super::{HeadBuilder, PreparedHead, RequestMeta};

/// Generated-private bridge from one concrete endpoint value into core.
#[doc(hidden)]
pub trait EndpointSpec: Send + 'static {
    /// Selected request specification, including the private empty bodyless specification.
    type Body: RequestBodySpec;
    /// Selected successful-response specification.
    type Response: ResponseBodySpec;

    /// Returns stable endpoint identity and inherited timeout metadata.
    fn request_meta(&self) -> RequestMeta;

    /// Resolves the public request head without materializing the body.
    fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError>;

    /// Computes request-specification metadata before the before-send hook.
    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError>;

    /// Transfers the selected request-specification input exactly once.
    fn into_body(self) -> <Self::Body as RequestBodySpec>::Input;
}

/// Generated-private capability for endpoints whose shape permits the
/// dangerous raw-success terminal.
///
/// Every generated endpoint implements this structural capability. Core then
/// combines it with the selected response specification's private acquisition-
/// mode capability. Public runtime endpoints intentionally do not implement
/// it.
#[doc(hidden)]
pub trait RawResponseEndpoint: EndpointSpec {}
