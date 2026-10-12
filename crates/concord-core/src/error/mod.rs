//! Structured safe public errors and complete-only terminal status bodies.

mod category;
mod context;
mod journal;
pub(crate) mod status;
mod transport;

pub use category::{
    AuthErrorKind, ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind,
    DispatchKnowledge, ErrorCategory, FailureStage, PaginationErrorKind, RateLimitErrorKind,
};
pub use context::{ClientBuildDiagnostic, ClientBuildError, ConfigurationError};
pub(crate) use journal::ErrorJournal;
pub use journal::{
    AttemptFacts, OperationCounts, ProviderAcquisitionFacts, ProviderAcquisitionOutcome,
};
pub use status::{
    HttpResponseFailure, StatusBody, StatusBodyDecodeError, StatusBodyDecodeErrorKind,
    StatusBodyState,
};
pub use transport::ApiError;
pub(crate) use transport::reqwest_category;
