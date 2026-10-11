//! Stable semantic contracts used to integrate with Concord.

pub use bytes::{Bytes, BytesMut};
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use url::Url;

pub use crate::atom::{
    AtomEncoder, AtomError, AtomErrorKind, BoundedAtomWriter, OrderedValues, RequestAtom,
};
pub use crate::auth::{
    AccessToken, ApiKey, Auth, AuthError, AuthTarget, BasicCredential, ChallengePolicy,
    CredentialHandle, CredentialMaterial, CredentialMaterialError, CredentialMaterialErrorKind,
    CredentialProvider, CredentialReason, CredentialRef, CredentialRequest,
    MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_PLACEMENTS, MAX_AUTH_QUERY_VALUE_BYTES,
    MAX_AUTHORIZATION_SCHEME_BYTES, MAX_AUTHORIZATION_VALUE_BYTES, MAX_BASIC_PASSWORD_BYTES,
    MAX_BASIC_USERNAME_BYTES, MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES, RecoveryMode,
    SecretCredential, SecretString,
};
// The body module's static `BodyDescription` is exported as
// `concord_core::body::BodyDescription`.
pub use crate::auth::{
    ChallengeRecovery, CredentialVersion, MAX_CREDENTIAL_FLIGHT_WAITERS, MAX_PROVIDER_ORIGIN_BYTES,
    ProviderOrigin, ProviderOriginError, ProviderOriginErrorKind, StoreOutcome,
};
#[allow(deprecated)]
pub use crate::body::RequestBody;
pub use crate::body::{
    BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, MAX_BODY_MEDIA_HINT_BYTES,
    MAX_EMPTY_REQUEST_CHUNKS, MAX_REQUEST_CONTENT_TYPE_BYTES, PageRequestRepresentation,
    PreparedBody, RequestBodyContext, RequestRepresentation, RequestStream,
};
#[cfg(feature = "multipart")]
pub use crate::body::{
    MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
    MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
    MAX_MULTIPART_PARTS, Multipart, MultipartBody, MultipartFactory, MultipartPart,
};
pub use crate::codec::{
    Binary, CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext, Text,
};
pub use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, MAX_CANCELLATION_SOURCES,
    RequestControls,
};
pub use crate::endpoint::{
    AbsentStatus, AcceptedStatus, Effect, EffectSource, EndpointCapabilities, EndpointDescription,
    EndpointId, MAX_ENDPOINT_ID_BYTES, MAX_FORMATTED_PUBLIC_VALUE_BYTES,
    MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
    MAX_REQUEST_URL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS, MAX_RUNTIME_ENDPOINT_METHOD_BYTES,
    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, RequestPlan, ResponseAcquisition,
    StatusSource, TimeoutSelection,
};
pub use crate::error::{
    ApiError, AttemptFacts, AuthErrorKind, ClientBuildError, ClientBuildErrorKind, ClientInputKind,
    ConfigurationError, ConfigurationErrorKind, DispatchKnowledge, ErrorCategory, FailureStage,
    HttpResponseFailure, OperationCounts, PaginationErrorKind, ProviderAcquisitionFacts,
    ProviderAcquisitionOutcome, RateLimitErrorKind, StatusBody, StatusBodyDecodeError,
    StatusBodyDecodeErrorKind, StatusBodyState,
};
pub use crate::execute::MAX_VISIBLE_ATTEMPTS_PER_OPERATION;
pub use crate::execute::StoreRequest;
pub use crate::hooks::{
    AttemptInfo, AttemptStarted, BoxFuture, CompletionOutcome, DebugLevel, OperationEvent,
    OperationFinished, OperationObserver, PaginationFinished, ProviderAcquisitionFinished,
    RequestTarget, ResponseHeadObserved, StreamFinished, StreamKind, StreamOutcome,
};
pub use crate::http::provider::{
    ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
};
pub use crate::http::{
    HttpConfigBuilder, MAX_IDENTITY_PEM_BYTES, MAX_PROXY_URL_BYTES, MAX_ROOT_CERTIFICATE_BYTES,
    MAX_ROOT_CERTIFICATES, MAX_ROOT_CERTIFICATES_TOTAL_BYTES, ProviderRetry, SafeProxy,
};
// Effective contract descriptions live in `concord_core::inspection`; the
// credential source kind is re-exported here because `ChallengeRecovery` is
// defined on it.
pub use crate::inspection::CredentialSourceKind;
pub use crate::limits::{MAX_PROVIDER_ORIGINS, MAX_PROVIDER_SENDS};
pub use crate::operation::OperationId;
pub use crate::pagination::{
    CollectAll, CollectPrefix, ContinuationKnowledge, MAX_PAGINATION_HEADER_BYTES,
    MAX_PAGINATION_HEADER_INTERESTS, Page, PageCorrelation, PageDecisionLimits, PageResponse,
    PageStep, Pages, PaginationController, PaginationDescription, PaginationError, Prefix,
    StopReason, TraversalSummary,
};
pub use crate::rate_limit::{
    CanonicalKeyWriter, DEFAULT_RATE_LIMIT_KEY_BYTES, EncodedKeyPart, Enforcement,
    LimiterCapability, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES,
    MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_POLICIES_PER_PLAN,
    MAX_RATE_LIMIT_WINDOWS_PER_POLICY, NoopRateLimiter, RateKeyEncoder, RateKeyPart,
    RateLimitDeclaration, RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart,
    RateLimitPermit, RateLimitPlan, RateLimitPolicy, RateLimitRequest, RateLimitResponse,
    RateLimitWindow, RateLimiter, RatePlanSummary, RatePolicySchema,
};
#[cfg(feature = "records")]
pub use crate::records::{ItemBatches, ItemStream, RecordDecoder, RecordError, RecordErrorKind};
pub use crate::redaction::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub use crate::request::{
    AcceptedBytesRequest, BoundRuntimeRequest, MAX_RUNTIME_KEY_BYTES, RuntimeAuthDisposition,
    RuntimeBindingFacts, RuntimeEndpoint, RuntimeEndpointBuilder, RuntimeFamily, RuntimeFamilyMode,
    RuntimeOperation, RuntimeOperationFact, RuntimeRouteMode,
};
pub use crate::response::{
    AcceptedBytesDriver, ApiResponse, BodyDisposition, Buffered, ByteStream, Discard,
    DiscardDriver, Empty, ExecutionMeta, MAX_ACCEPT_BYTES, MAX_EMPTY_RESPONSE_CHUNKS,
    OptionalOutcome, ResponseBodyError, ResponseBodyErrorKind, ResponseDriver, ResponseHead,
    ResponseRepresentation, StreamErrorKind, StreamState, StreamSummary, Streaming,
};
pub use crate::retry::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryBackoff, RetryBackoffError,
    RetryClassifier, RetryDecision, RetryFailure, RetryJitter, RetryPolicy, RetryPolicyBuildError,
    RetryPolicyBuildErrorKind, RetryPolicyBuilder,
};
pub use crate::runtime::RuntimeConfigBuilder;
pub use crate::value::IntoValue;
