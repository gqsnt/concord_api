mod endpoint;
mod head;
mod options;
mod owner;
mod pending;
mod prepare;
mod program;
mod runtime_endpoint;

pub use pending::{AcceptedBytesRequest, Request, ResponseRequest};

/// Crate-internal erasure contract of the terminal wrappers.
pub(crate) mod pending_erasure {
    pub(crate) use super::pending::ErasedOperation;
}
#[doc(hidden)]
pub use owner::{OwnerProjection, ProgramVisitor};
#[cfg(test)]
pub(crate) use runtime_endpoint::test_owner;
pub use runtime_endpoint::{
    BoundRuntimeRequest, MAX_RUNTIME_KEY_BYTES, RuntimeAuthDisposition, RuntimeBindingFacts,
    RuntimeEndpoint, RuntimeEndpointBuilder, RuntimeFamily, RuntimeFamilyMode, RuntimeOperation,
    RuntimeOperationFact, RuntimeRouteMode,
};
pub(crate) use runtime_endpoint::{bind_runtime_at_owner, plan_runtime_at_owner};

pub(crate) use prepare::{
    OperationPreparation, PreparationFailure, PreparedOperation, PreparedPaginationPage,
    precheck_controls, prepare_operation, prepare_pagination_page,
};

#[doc(hidden)]
pub use endpoint::EndpointSpec;
pub(crate) use head::reserved_public_header;
#[doc(hidden)]
pub use head::{HeadBuilder, PreparedHead, RequestMeta};
#[doc(hidden)]
pub use options::{RequestOptions, TimeoutOverride};
#[doc(hidden)]
pub use program::{
    HeadEmissions, NO_EMISSION, OccurrenceSink, PatchFamily, PatchKind, PatchOp, ProgramSegment,
    apply_head_program, surviving_keys, visit_extended_program,
};
#[cfg(test)]
mod tests;
