mod endpoint;
mod head;
mod options;
mod pending;
mod prepare;

pub use pending::{Request, ResponseRequest};

#[allow(
    unused_imports,
    reason = "used by the Phase 2 production executor in PR 2.3"
)]
pub(crate) use prepare::{PreparedOperation, prepare_operation};

#[doc(hidden)]
pub use endpoint::{
    BodyMetadata, BodySpec, BytesResponse, EmptyBody, EndpointSpec, NoContentResponse,
    PreparedBody, ResponseHandler,
};
#[doc(hidden)]
pub use head::{HeadBuilder, PreparedHead, RequestMeta};
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};

#[cfg(test)]
mod tests;
