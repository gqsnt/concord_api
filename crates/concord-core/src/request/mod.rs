mod endpoint;
mod head;
mod options;
mod pending;
mod prepare;
mod prepared;

#[cfg(feature = "dangerous-raw-response")]
pub use pending::RawResponseRequest;
pub use pending::{Request, ResponseRequest};
pub use prepared::{RuntimeEndpoint, RuntimeEndpointBuilder};

pub(crate) use prepare::{
    OperationPreparation, PreparationFailure, PreparedOperation, PreparedPaginationPage,
    prepare_operation, prepare_pagination_page,
};

pub(crate) use crate::body::PreparedBody;
#[doc(hidden)]
pub use endpoint::{EndpointSpec, RawResponseEndpoint};
#[doc(hidden)]
pub use head::{HeadBuilder, PreparedHead, RequestMeta};
pub(crate) use head::{
    reserved_public_header, validate_public_header, validate_public_header_name,
};
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};
#[cfg(test)]
mod tests;
