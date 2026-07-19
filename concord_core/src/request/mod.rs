mod endpoint;
mod head;
mod options;
mod pending;
mod prepare;
mod prepared;

pub use pending::{Request, ResponseRequest};
pub use prepared::{
    PreparedBuffered, PreparedBytes, PreparedEndpoint, PreparedEndpointBuilder, PreparedNoContent,
    PreparedStream,
};

pub(crate) use prepare::{
    OperationPreparation, PreparationFailure, PreparedOperation, PreparedPaginationPage,
    prepare_operation, prepare_pagination_page,
};

#[doc(hidden)]
pub use crate::body::{BodyMetadata, BodySpec, CodecBody, EmptyBody, PreparedBody};
#[doc(hidden)]
pub use endpoint::{
    BufferedResponse, BytesResponse, EndpointSpec, MetadataResponse, NoContentResponse,
    ResponseHandler, StreamResponseHandler,
};
#[doc(hidden)]
pub use head::{HeadBuilder, PreparedHead, RequestMeta};
pub(crate) use head::{reserved_public_header, validate_public_header};
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};
#[doc(hidden)]
pub use prepared::PreparedResponseMarker;

#[cfg(test)]
mod tests;
