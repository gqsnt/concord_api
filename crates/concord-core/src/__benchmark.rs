//! Unstable, body-free probes for workspace performance tooling.
//!
//! The helpers in this hidden module reuse ordinary production preparation
//! operations and stop before native transport. They expose no deterministic
//! transport types and are not covered by Concord's compatibility guarantees.

use crate::error::ApiError;
use crate::request::{EndpointSpec, OperationPreparation, Request, prepare_operation};

/// An unconstructible capability used by generated wrappers to surrender their
/// internal request carrier only to this module's benchmark helpers.
#[doc(hidden)]
pub struct GeneratedRequestToken {
    _private: (),
}

/// Hidden conversion implemented only for macro-generated named request
/// wrappers.
///
/// The benchmark helpers require Concord's sealed endpoint bound, and callers
/// cannot invoke the conversion because [`GeneratedRequestToken`] has no
/// public constructor.
#[doc(hidden)]
pub trait GeneratedRequest {
    /// The generated endpoint stored by this named wrapper.
    type Endpoint;

    /// Consumes the named wrapper into Concord's ordinary internal request
    /// carrier.
    #[doc(hidden)]
    fn __concord_into_internal_request(
        self,
        token: GeneratedRequestToken,
    ) -> Request<Self::Endpoint>;
}

/// The exact production operation exercised by a local benchmark probe.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchmarkOperation {
    /// The ordinary public request head was prepared and validated.
    PublicHeadPreparation,
    /// Ready credential values were preflighted and finalized.
    CredentialReadyPreparation,
}

/// A body-free aggregate observation from one local benchmark probe.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BenchmarkObservation {
    operation: BenchmarkOperation,
    work_units: usize,
}

impl BenchmarkObservation {
    /// Returns the exact production operation that completed.
    #[must_use]
    pub const fn operation(&self) -> BenchmarkOperation {
        self.operation
    }

    /// Returns a non-sensitive aggregate count produced by that operation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work_units
    }
}

async fn prepared_operation<R>(
    request: R,
) -> Result<crate::request::PreparedOperation<R::Endpoint>, ApiError>
where
    R: GeneratedRequest,
    R::Endpoint: EndpointSpec,
{
    let request = request.__concord_into_internal_request(GeneratedRequestToken { _private: () });
    match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => Ok(prepared),
        OperationPreparation::Failed(failure) => Err(failure.error),
    }
}

/// Prepares and validates one public request head without hooks, body
/// materialization, credentials, limiting, native HTTP, or response handling.
#[doc(hidden)]
pub async fn benchmark_public_head<R>(request: R) -> Result<BenchmarkObservation, ApiError>
where
    R: GeneratedRequest,
    R::Endpoint: EndpointSpec,
{
    let prepared = prepared_operation(request).await?;
    let path_segments = prepared
        .head
        .logical_url()
        .path_segments()
        .map_or(0, Iterator::count);
    let work_units = 1
        + path_segments
        + prepared.head.logical_url().query_pairs().count()
        + prepared.head.public_headers().len()
        + prepared.head.auth().len()
        + prepared.head.rate_limit_plan().rules().len();
    drop(prepared);
    Ok(BenchmarkObservation {
        operation: BenchmarkOperation::PublicHeadPreparation,
        work_units,
    })
}

/// Runs the ordinary ready credential preflight and post-limiter finalization
/// operations, stopping before native materialization or managed HTTP.
#[doc(hidden)]
pub async fn benchmark_ready_credentials<R>(request: R) -> Result<BenchmarkObservation, ApiError>
where
    R: GeneratedRequest,
    R::Endpoint: EndpointSpec,
{
    let prepared = prepared_operation(request).await?;
    let work_units = crate::execute::benchmark_ready_credentials(prepared).await?;
    Ok(BenchmarkObservation {
        operation: BenchmarkOperation::CredentialReadyPreparation,
        work_units,
    })
}
