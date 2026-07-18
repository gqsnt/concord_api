//! Stable advanced types used to integrate with Concord.

pub use bytes::Bytes;
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use url::Url;

pub use crate::body::{BodyError, BodyErrorKind, BodyFactory, BodyKind, RequestBody, StreamBody};
#[cfg(feature = "json")]
pub use crate::codec::Json;
pub use crate::codec::{
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream, Text,
};
pub use crate::error::{
    ApiError, AuthErrorKind, ClientBuildError, ClientBuildErrorKind, ConfigurationError,
    ConfigurationErrorKind, ErrorCategory, HookError, HttpResponseFailure, PaginationErrorKind,
    RateLimitErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
};
pub use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
pub use crate::http::{HttpConfigBuilder, ProviderRetry, SafeProxy};
pub use crate::operation::OperationId;
pub use crate::redaction::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub use crate::response::{DecodedResponse, ExecutionMeta, NoContent, StreamResponse};
pub use crate::runtime::RuntimeConfigBuilder;
