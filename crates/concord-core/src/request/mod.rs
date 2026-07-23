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
pub(crate) use head::validate_public_header;
#[doc(hidden)]
pub use head::{HeadBuilder, PreparedHead, RequestMeta};
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};
#[cfg(test)]
mod tests;
