//! Structured safe public errors and complete-only terminal status bodies.

mod category;
mod context;
mod hook;
mod status;
mod transport;

pub use category::{
    AuthErrorKind, ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind, ErrorCategory,
    PaginationErrorKind, RateLimitErrorKind,
};
pub use context::{ClientBuildError, ConfigurationError};
pub use hook::HookError;
pub use status::{HttpResponseFailure, StatusBody, StatusBodyDecodeError, StatusBodyState};
pub use transport::ApiError;
pub(crate) use transport::reqwest_category;
