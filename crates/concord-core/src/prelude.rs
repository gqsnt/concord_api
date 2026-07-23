//! Common types used by generated Concord APIs.

pub use bytes::Bytes;

pub use crate::auth::{AccessToken, ApiKey, BasicCredential, SecretString};
#[cfg(feature = "multipart")]
pub use crate::body::Multipart;
pub use crate::body::{RequestBody, Stream, StreamBody};
pub use crate::codec::{OctetStream, Text};
pub use crate::error::{
    ApiError, ClientBuildError, ClientInputKind, ConfigurationError, ErrorCategory,
};
pub use crate::hooks::DebugLevel;
pub use crate::pagination::PageLimit;
#[cfg(feature = "records")]
pub use crate::records::{RecordBatches, RecordError, RecordStream, Records};
pub use crate::response::{ApiResponse, ByteStream, NoContent};
pub use crate::{PaginatedRequest, Request, ResponseRequest};
