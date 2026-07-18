mod endpoint;
mod head;
mod options;
mod pending;
mod prepare;

pub use pending::{Request, ResponseRequest};

pub(crate) use prepare::{
    OperationPreparation, PreparationFailure, PreparedOperation, prepare_operation,
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
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};

#[cfg(test)]
mod tests;
