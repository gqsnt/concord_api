//! Structured safe public errors and complete-only terminal status bodies.

mod category;
mod context;
pub(crate) mod status;
mod transport;

pub use category::{
    AuthErrorKind, ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind,
    DispatchKnowledge, ErrorCategory, FailureStage, PaginationErrorKind, RateLimitErrorKind,
};
pub use context::{ClientBuildError, ConfigurationError};
pub use status::{HttpResponseFailure, StatusBody, StatusBodyDecodeError, StatusBodyState};
pub use transport::ApiError;
pub(crate) use transport::reqwest_category;
