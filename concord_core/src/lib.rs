pub mod auth;
mod body;
mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
mod io;
mod media;
mod multipart;
mod pagination;
mod policy;
mod rate_limit;
mod redaction;
mod request;
mod response_classify;
mod retry;
pub mod runtime;
mod runtime_hooks;
mod runtime_state;
mod secret;
mod stream_body;
mod stream_response;
mod timeout;
pub mod transport;
mod types;

#[doc(hidden)]
pub mod __private {
    #[doc(hidden)]
    pub use crate::auth::{CredentialSlot, NoAuthState};
    #[doc(hidden)]
    pub use crate::codec::{
        BodyCodec, CodecError, ContentType, DecodeContext, Decodes, EncodeContext, EncodedBody,
        Encodes, Format, FormatType, ResponseCodec,
    };
    pub use crate::endpoint::{
        ClientPlanContext, EndpointMeta, EndpointPlan, IntoEndpointPlan, PaginatedEndpoint,
        PaginationMarker, RequestOverrides, RequestPlan, RequestPlanView, ResolvedRoute,
        ResponsePlan, ResponseTerminalEndpoint, ReusableEndpoint,
    };
    pub use crate::io::{
        BufferedResponse, BytesResponse, EncodedRequest, MultipartRequest, NoContentResponse,
        NoRequestBody, PreparedBody, PreparedRequestEntity, RawStreamRequest, RawStreamResponse,
        RequestEntity, ResponseEntity, ResponseEntityCapabilities, ResponseEntityPlan,
        ResponseEntityWithMeta,
    };
    pub use crate::multipart::{
        FormData, MultipartBody, MultipartBodyError, MultipartBodyErrorKind, RawPart,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, CursorPagination, EndpointPagination, HasNextCursor, OffsetLimitPagination,
        PageAdvance, PageApply, PageDecision, PageItems, PagedPagination, PaginateBinding,
        PaginationCaps, PaginationRuntime, PaginationRuntimeAdapter, PaginationTermination,
        ProgressKey,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
    pub use crate::retry::RetrySetting;
}
#[doc(hidden)]
#[deprecated(note = "use concord_core::__private for generated-code internals")]
pub use self::__private as internal;

pub mod prelude {
    pub use crate::auth::{AccessToken, ApiKey, BasicCredential};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{ContentType, NoContent, text::Text};
    pub use crate::debug::DebugLevel;
    pub use crate::endpoint::{Endpoint, IntoEndpointPlan, PaginatedEndpoint, ReusableEndpoint};
    pub use crate::error::{ApiClientError, ErrorCategory, PaginationError, PaginationErrorKind};
    pub use crate::pagination::{
        CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        PaginationTermination,
    };
    pub use crate::rate_limit::{
        RateLimitError, RateLimitErrorKind, RateLimitObservation, RateLimitObserver,
        RateLimitResponseContext,
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
        AuthPreparationReuse, AuthProvenance, AuthRejectionDecision, AuthRequirement,
        AuthRequirementId, AuthRetryReason, AuthStepPolicy, AuthUsageId, CredentialContext,
        CredentialId, CredentialLease, CredentialMaterial, CredentialProvider, CredentialRef,
        CredentialRefreshReason, CredentialSlot, InvalidateReason, ManualCredentialProvider,
        PendingAuthPlacement, PendingAuthSlot, PreparedAuthCredential, PreparedInternalAuth,
        SecretCredential, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
        apply_basic_credential, apply_secret_credential, auth_decision_for_status,
        invalidate_rejected_credential, read_auth_lock, write_auth_lock,
    };
    pub use crate::body::{BodyError, BodyErrorKind, DynBody, LimitedBody};
    pub use crate::codec::{
        BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody,
        ResponseCodec,
    };
    pub use crate::debug::{
        DebugSink, NoopDebugSink, SanitizedHeaderValue, SanitizedHeaders, StderrDebugSink,
    };
    pub use crate::endpoint::{Endpoint, IntoEndpointPlan, PaginatedEndpoint, ReusableEndpoint};
    pub use crate::error::{ErrorContext, FxError, PaginationError, PaginationErrorKind};
    pub use crate::io::{
        BufferedResponse, BytesResponse, EncodedRequest, MultipartRequest, NoContentResponse,
        NoRequestBody, PreparedBody, PreparedRequestEntity, RawStreamRequest, RawStreamResponse,
        RequestEntity, ResponseEntity, ResponseEntityCapabilities, ResponseEntityPlan,
    };
    pub use crate::media::{
        Jpeg, JsonContentType, Mp3, Mp4, OctetStream, Pdf, Png, TextContentType, Zip,
    };
    pub use crate::multipart::{
        FormData, MultipartBody, MultipartBodyError, MultipartBodyErrorKind, RawPart,
    };
    pub use crate::pagination::{
        Control, CursorPagination, EndpointPagination, HasNextCursor, OffsetLimitPagination,
        PageAdvance, PageApply, PageDecision, PageItems, PagedPagination, PaginateBinding,
        PaginationCaps, PaginationRuntime, PaginationRuntimeAdapter, PaginationTermination,
        ProgressKey,
    };
    pub use crate::rate_limit::{
        DefaultRateLimitResponsePolicy, DefaultRateLimiter, GovernorRateLimiter, NoopRateLimiter,
        RateLimitBucketId, RateLimitBucketUse, RateLimitContext, RateLimitError,
        RateLimitErrorKind, RateLimitFuture, RateLimitKey, RateLimitKeyPart, RateLimitKeyValue,
        RateLimitPermit, RateLimitPlan, RateLimitResponseAction, RateLimitResponseContext,
        RateLimitResponsePolicy, RateLimitScopeHint, RateLimitSetting, RateLimitWindow,
        RateLimiter, parse_retry_after,
    };
    pub use crate::retry::{
        ConfiguredRetryPolicy, NoRetryPolicy, RetryBackoff, RetryConfig, RetryContext,
        RetryDecision, RetryIdempotency, RetryOutcome, RetryPolicy,
    };
    #[allow(deprecated)]
    pub use crate::runtime::{AuthRuntimeConfig, DebugConfig, RuntimeConfig};
    pub use crate::runtime_hooks::{
        HookMeta, NoopRuntimeHooks, PostResponseHookContext, PreSendHookContext, RuntimeHooks,
        TransportErrorHookContext,
    };
    pub use crate::runtime_state::ClientRuntimeState;
    pub use crate::stream_body::{BodySizeHint, StreamBody, StreamBodyError};
    pub use crate::stream_response::StreamResponse;
    #[cfg(feature = "transport-reqwest")]
    pub use crate::transport::ReqwestTransport;
    pub use crate::transport::{
        DecodedResponse, DefaultTransport, DefaultTransportMarker, RequestMeta, Transport,
        TransportBody, TransportByteStream, TransportError, TransportErrorKind, TransportRequest,
        TransportRequestBody, TransportResponse,
    };
    pub use crate::types::{
        HostLabelSource, HostParts as HostMap, HostSpec, RouteBuilder, UrlPath,
    };
}

pub mod dangerous {
    #[cfg(feature = "dangerous-dev-tools")]
    #[allow(deprecated)]
    pub use crate::runtime::DevBodyCaptureConfig;
    #[cfg(feature = "dangerous-raw-response")]
    pub use crate::transport::BuiltResponse;
}
