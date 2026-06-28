pub mod auth;
mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
mod media;
mod multipart;
mod multipart_response;
mod pagination;
mod policy;
mod rate_limit;
mod record;
mod redaction;
mod request;
mod response_classify;
mod retry;
pub mod runtime;
mod runtime_hooks;
mod runtime_state;
mod secret;
mod sse;
mod stream_body;
mod stream_response;
mod timeout;
pub mod transport;
mod types;

pub mod internal {
    #[doc(hidden)]
    pub use crate::auth::{CredentialSlot, NoAuthState};
    #[doc(hidden)]
    pub use crate::codec::{
        BodyCodec, CodecError, ContentType, DecodeContext, Decodes, EncodeContext, EncodedBody,
        Encodes, Format, FormatType, ResponseCodec,
    };
    pub use crate::endpoint::{
        BodyPlan, ClientPlanContext, Decoded, EndpointMeta, EndpointPlan, Mapped, MappedResp,
        MultipartResponseEndpoint, PaginatedEndpoint, PaginationPlan, RecordResponseEndpoint,
        RequestArgs, RequestOverrides, RequestPlan, RequestPlanView, ResolvedRoute, ResponsePlan,
        ResponseSpec, StreamResponseEndpoint, Transform, TransformResp,
    };
    pub use crate::multipart::{
        FormData, Mixed, MultipartBody, MultipartBodyError, MultipartBodyErrorKind,
        MultipartFormat, RawPart,
    };
    pub use crate::multipart_response::{MultipartDecodePart, MultipartStream, RawResponsePart};
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, CursorPagination, HasNextCursor, OffsetLimitPagination, PageAdvance, PageDecision,
        PageInit, PageRequest, PagedPagination, PaginationCaps, PaginationController,
        PaginationTermination, ProgressKey,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
    pub use crate::record::{
        NdJson, RecordBody, RecordDecoder, RecordEncoder, RecordFormat, RecordStream,
    };
    pub use crate::retry::RetrySetting;
    pub use crate::sse::{JsonSse, SseCodec, SseEvent, SseRawEvent, SseStream};
}
pub mod prelude {
    pub use crate::auth::{AccessToken, ApiKey, BasicCredential};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{NoContent, text::Text};
    pub use crate::debug::DebugLevel;
    pub use crate::endpoint::{
        Endpoint, MultipartResponseEndpoint, PaginatedEndpoint, RecordResponseEndpoint,
        StreamResponseEndpoint,
    };
    pub use crate::error::{ApiClientError, ErrorCategory};
    pub use crate::pagination::{
        CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        PaginationTermination,
    };
    pub use crate::rate_limit::{
        RateLimitObservation, RateLimitObserver, RateLimitResponseContext,
    };
    pub use crate::request::{PaginatedRequest, PendingRequest};
    pub use crate::secret::SecretString;
}

pub mod advanced {
    #[cfg(feature = "json")]
    pub use crate::auth::OAuth2ClientCredentialsProvider;
    pub use crate::auth::{
        AuthApplication, AuthApplicationRequest, AuthAppliedCredential, AuthAttemptSummary,
        AuthChallengePolicy, AuthDecision, AuthError, AuthErrorKind, AuthFuture, AuthHttpExecutor,
        AuthHttpRequest, AuthHttpResponse, AuthInternalPolicy, AuthMode, AuthPlacement, AuthPlan,
        AuthProvenance, AuthRejectionDecision, AuthRequirement, AuthRequirementId, AuthRetryReason,
        AuthStepPolicy, AuthUsageId, ClientCertificate, CredentialContext, CredentialId,
        CredentialLease, CredentialMaterial, CredentialProvider, CredentialRef,
        CredentialRefreshReason, CredentialSlot, InvalidateReason, ManualCredentialProvider,
        PendingAuthPlacement, PendingAuthSlot, PreparedAuthCredential, PreparedInternalAuth,
        SecretCredential, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
        apply_basic_credential, apply_certificate_credential, apply_secret_credential,
        auth_decision_for_status, invalidate_rejected_credential, read_auth_lock, write_auth_lock,
    };
    pub use crate::codec::{
        BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
    };
    pub use crate::debug::{DebugSink, NoopDebugSink, StderrDebugSink};
    pub use crate::endpoint::{
        MultipartResponseEndpoint, RecordResponseEndpoint, StreamResponseEndpoint,
    };
    pub use crate::error::{ErrorContext, FxError};
    pub use crate::media::{Jpeg, MediaType, Mp3, Mp4, OctetStream, Pdf, Png, Zip};
    pub use crate::multipart::{
        FormData, Mixed, MultipartBody, MultipartBodyError, MultipartBodyErrorKind,
        MultipartFormat, RawPart,
    };
    pub use crate::multipart_response::{MultipartDecodePart, MultipartStream, RawResponsePart};
    pub use crate::pagination::{
        Control, HasNextCursor, PageAdvance, PageDecision, PageInit, PageItems, PageRequest,
        PaginationCaps, PaginationController, PaginationTermination, ProgressKey,
    };
    pub use crate::rate_limit::{
        DefaultRateLimitResponsePolicy, DefaultRateLimiter, GovernorRateLimiter, NoopRateLimiter,
        RateLimitBucketId, RateLimitBucketUse, RateLimitContext, RateLimitFuture, RateLimitKey,
        RateLimitKeyPart, RateLimitKeyValue, RateLimitPermit, RateLimitPlan,
        RateLimitResponseAction, RateLimitResponseContext, RateLimitResponsePolicy,
        RateLimitScopeHint, RateLimitSetting, RateLimitWindow, RateLimiter, parse_retry_after,
    };
    pub use crate::record::{
        NdJson, RecordBody, RecordDecoder, RecordEncoder, RecordFormat, RecordStream,
    };
    pub use crate::retry::{
        ConfiguredRetryPolicy, NoRetryPolicy, RetryBackoff, RetryConfig, RetryContext,
        RetryDecision, RetryIdempotency, RetryOutcome, RetryPolicy,
    };
    #[allow(deprecated)]
    pub use crate::runtime::{AuthRuntimeConfig, DebugConfig, DevBodyCaptureConfig, RuntimeConfig};
    pub use crate::runtime_hooks::{
        HookMeta, NoopRuntimeHooks, PostResponseHookContext, PreSendHookContext, RuntimeHooks,
        TransportErrorHookContext,
    };
    pub use crate::runtime_state::ClientRuntimeState;
    pub use crate::sse::{JsonSse, SseCodec, SseEvent, SseRawEvent, SseStream};
    pub use crate::stream_body::{BodySizeHint, StreamBody, StreamBodyError};
    pub use crate::stream_response::StreamResponse;
    pub use crate::transport::{
        BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta, ReqwestTransport, Transport,
        TransportAuth, TransportBody, TransportByteStream, TransportError, TransportErrorKind,
        TransportRequest, TransportRequestBody, TransportResponse,
    };
    pub use crate::types::{
        HostLabelSource, HostParts as HostMap, HostSpec, RouteBuilder, UrlPath,
    };
}
