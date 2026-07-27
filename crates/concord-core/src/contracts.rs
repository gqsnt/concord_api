//! Stable semantic contracts used to integrate with Concord.

pub use bytes::{Bytes, BytesMut};
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use url::Url;

pub use crate::auth::{
    AccessToken, ApiKey, Auth, AuthError, BasicCredential, ChallengePolicy, CredentialHandle,
    CredentialMaterial, CredentialMaterialError, CredentialMaterialErrorKind, CredentialProvider,
    CredentialReason, CredentialRef, CredentialRequest, MAX_AUTH_PLACEMENT_NAME_BYTES,
    MAX_AUTH_PLACEMENTS, MAX_AUTH_QUERY_VALUE_BYTES, MAX_AUTHORIZATION_SCHEME_BYTES,
    MAX_AUTHORIZATION_VALUE_BYTES, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES, RecoveryMode, SecretCredential,
    SecretString,
};
pub use crate::body::{
    BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, MAX_EMPTY_REQUEST_CHUNKS,
    PageRequestBodySpec, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
    RequestContentType, Stream, StreamBody,
};
#[cfg(feature = "multipart")]
pub use crate::body::{
    MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
    MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
    MAX_MULTIPART_PARTS, Multipart, MultipartBody, MultipartFactory, MultipartPart,
};
pub use crate::codec::{
    AnyMedia, CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
    MediaType, OctetStream, Text,
};
pub use crate::endpoint::{
    EndpointCapabilities, EndpointDescription, EndpointId, MAX_ENDPOINT_ID_BYTES,
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
    MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS,
    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, ResponseAcquisition, SafeRequestPlan,
    TimeoutSelection,
};
pub use crate::error::{
    ApiError, AuthErrorKind, ClientBuildError, ClientBuildErrorKind, ClientInputKind,
    ConfigurationError, ConfigurationErrorKind, ErrorCategory, HookError, HttpResponseFailure,
    PaginationErrorKind, RateLimitErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
};
pub use crate::hooks::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
pub use crate::http::provider::{
    ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
};
pub use crate::http::{
    HttpConfigBuilder, MAX_IDENTITY_PEM_BYTES, MAX_PROXY_URL_BYTES, MAX_ROOT_CERTIFICATE_BYTES,
    MAX_ROOT_CERTIFICATES, MAX_ROOT_CERTIFICATES_TOTAL_BYTES, ProviderRetry, SafeProxy,
};
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
    BufferedRecords, RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream,
    Records,
};
pub use crate::redaction::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub use crate::request::{ResponseBytesRequest, RuntimeEndpoint, RuntimeEndpointBuilder};
pub use crate::response::{
    ApiResponse, Buffered, ByteStream, Discard, ExecutionMeta, MAX_EMPTY_RESPONSE_CHUNKS,
    NoContent, PageResponseSpec, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec,
    ResponseBytesMode, ResponseHead, ResponseMode, Streaming,
};
pub use crate::retry::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryClassifier, RetryDecision, RetryFailure,
    RetryPolicy, RetryPolicyBuildError, RetryPolicyBuildErrorKind, RetryPolicyBuilder,
};
pub use crate::runtime::RuntimeConfigBuilder;
