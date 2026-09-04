use crate::body::{RequestBodyContext, RequestBodyMetadata, RequestRepresentation};
use crate::error::ApiError;
use crate::response::ResponseRepresentation;

use super::{HeadBuilder, PreparedHead, RequestMeta};

/// Generated-private bridge from one concrete endpoint value into core.
#[doc(hidden)]
pub trait EndpointSpec: Send + 'static {
    /// Exact caller-supplied request-body input, or `()` for a bodyless endpoint.
    type Input: Send + 'static;
    /// Selected request specification, including the private empty bodyless specification.
    type Body: RequestRepresentation<Self::Input>;
    /// Exact value produced by ordinary `.await`.
    type Output: Send + 'static;
    /// Selected accepted-response specification.
    type Response: ResponseRepresentation<Self::Output>;

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
    fn into_body(self) -> Self::Input;
}
