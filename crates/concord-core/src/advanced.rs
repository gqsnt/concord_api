//! Stable advanced types used to integrate with Concord.

pub use bytes::{Bytes, BytesMut};
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use url::Url;

pub use crate::auth::{
    Auth, AuthError, ChallengePolicy, CredentialHandle, CredentialMaterial, CredentialProvider,
    CredentialReason, CredentialRef, CredentialRequest, RecoveryMode, SecretCredential,
};
pub use crate::body::{
    BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, RequestBody, RequestBodyContext,
    RequestBodyMetadata, RequestBodySpec, RequestContentType, Stream, StreamBody,
};
#[cfg(feature = "multipart")]
pub use crate::body::{Multipart, MultipartBody, MultipartFactory, MultipartPart};
pub use crate::codec::{
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream, Text,
};
pub use crate::endpoint::{
    EndpointCapabilities, EndpointDescription, EndpointId, MAX_ENDPOINT_ID_BYTES,
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS,
    MAX_RUNTIME_ENDPOINT_COMPONENTS, MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory,
    ResponseAcquisition, SafeRequestPlan, TimeoutSelection,
};
pub use crate::error::{
    ApiError, AuthErrorKind, ClientBuildError, ClientBuildErrorKind, ClientInputKind,
    ConfigurationError, ConfigurationErrorKind, ErrorCategory, HookError, HttpResponseFailure,
    PaginationErrorKind, RateLimitErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
};
pub use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
pub use crate::http::provider::{
    ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
};
pub use crate::http::{HttpConfigBuilder, ProviderRetry, SafeProxy};
pub use crate::operation::OperationId;
pub use crate::pagination::{PaginationController, PaginationError};
pub use crate::rate_limit::{
    Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_ID_BYTES,
    MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
    MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE, NoopRateLimiter,
    RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver,
    RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead,
    RateLimitRule, RateLimitWindow, RateLimiter,
};
#[cfg(feature = "records")]
pub use crate::records::{
    RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
};
pub use crate::redaction::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub use crate::request::{RuntimeEndpoint, RuntimeEndpointBuilder};
pub use crate::response::{
    ApiResponse, Buffered, ByteStream, Discard, ExecutionMeta, NoContent, PageResponseSpec,
    ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead, ResponseMode,
    Streaming,
};
pub use crate::runtime::RuntimeConfigBuilder;
