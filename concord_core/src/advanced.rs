//! Stable advanced types used to integrate with Concord.

pub use bytes::Bytes;
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use url::Url;

pub use crate::codec::{CodecError, DecodeBody, DecodeContext};
pub use crate::error::{
    ApiError, AuthErrorKind, ClientBuildError, ClientBuildErrorKind, ConfigurationError,
    ConfigurationErrorKind, ErrorCategory, HttpResponseFailure, PaginationErrorKind,
    RateLimitErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
};
pub use crate::http::{HttpConfigBuilder, ProviderRetry, SafeProxy};
pub use crate::redaction::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub use crate::runtime::RuntimeConfigBuilder;
