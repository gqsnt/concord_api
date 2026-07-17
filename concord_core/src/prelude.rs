//! Common types used by generated Concord APIs.

pub use bytes::Bytes;

#[cfg(feature = "json")]
pub use crate::codec::Json;
pub use crate::codec::{OctetStream, Text};
pub use crate::error::{ApiError, ClientBuildError, ConfigurationError, ErrorCategory};
pub use crate::hooks::DebugLevel;
pub use crate::response::{DecodedResponse, NoContent};
pub use crate::{Request, ResponseRequest};
