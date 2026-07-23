//! Unstable scoped support for deterministic managed-boundary tests.
//!
//! This module is available only with `dangerous-dev-tools`. Its API is not
//! covered by Concord's compatibility guarantees.

use crate::error::ApiError;
use crate::request::{EndpointSpec, OperationPreparation, Request, prepare_operation};

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

async fn prepared_operation<E>(
    request: Request<E>,
) -> Result<crate::request::PreparedOperation<E>, ApiError>
where
    E: EndpointSpec,
{
    match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => Ok(prepared),
        OperationPreparation::Failed(failure) => Err(failure.error),
    }
}

/// Prepares and validates one public request head without hooks, body
/// materialization, credentials, limiting, native HTTP, or response handling.
#[doc(hidden)]
pub async fn benchmark_public_head<E>(request: Request<E>) -> Result<BenchmarkObservation, ApiError>
where
    E: EndpointSpec,
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
pub async fn benchmark_ready_credentials<E>(
    request: Request<E>,
) -> Result<BenchmarkObservation, ApiError>
where
    E: EndpointSpec,
{
    let prepared = prepared_operation(request).await?;
    let work_units = crate::execute::benchmark_ready_credentials(prepared).await?;
    Ok(BenchmarkObservation {
        operation: BenchmarkOperation::CredentialReadyPreparation,
        work_units,
    })
}

pub use concord_test_support::executor::{
    ApplicationChannel, DeterministicExecutor, ProviderChannel,
};
pub use concord_test_support::gate::Gate;
pub use concord_test_support::script::{
    BodyCategory, CapturedHeader, CapturedHeaderValue, CapturedRequest, Channel, ExecutionFailure,
    ExecutionFailureKind, ExpectedRequest, ScriptedResponse, TransportFailure,
};
