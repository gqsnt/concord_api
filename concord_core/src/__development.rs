//! Unstable scoped support for deterministic managed-boundary tests.
//!
//! This module is available only with `dangerous-dev-tools`. Its API is not
//! covered by Concord's compatibility guarantees.

pub use concord_test_support::executor::{
    ApplicationChannel, DeterministicExecutor, ProviderChannel,
};
pub use concord_test_support::gate::Gate;
pub use concord_test_support::script::{
    BodyCategory, CapturedHeader, CapturedHeaderValue, CapturedRequest, Channel, ExecutionFailure,
    ExecutionFailureKind, ExpectedRequest, ScriptedResponse, TransportFailure,
};
