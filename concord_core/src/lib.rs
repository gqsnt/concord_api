#[cfg(test)]
extern crate self as concord_core;

mod auth;
mod body;
mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
mod execution_meta;
mod header_ownership;
mod io;
mod media;
#[cfg(feature = "multipart")]
mod multipart;
mod pagination;
mod policy;
mod rate_limit;
mod redaction;
mod request;
mod response_classify;
mod retry_mode;
mod runtime;
mod runtime_hooks;
mod runtime_state;
mod secret;
mod stream_body;
mod stream_response;
mod timeout;
mod transport;
mod types;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
#[allow(unused_imports)]
mod support;

#[doc(hidden)]
pub mod __private;

/// Explicitly enabled support for Concord's deterministic development seam.
///
/// This unstable test-observation surface is unavailable unless the
/// `dangerous-dev-tools` feature is selected. Generated clients never
/// reference it.
#[cfg(feature = "dangerous-dev-tools")]
#[doc(hidden)]
pub mod __development;

pub mod prelude {
    pub use crate::auth::{AccessToken, ApiKey, AuthError, BasicCredential};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{ContentType, NoContent, text::Text};
    pub use crate::debug::DebugLevel;
    pub use crate::error::{
        ApiClientError, ClientBuildErrorKind, ErrorCategory, PaginationError, PaginationErrorKind,
        RequestErrorSource, RequestErrorSourceKind,
    };
    pub use crate::execution_meta::RequestExecutionMeta;
    pub use crate::header_ownership::HeaderOwnershipError;
    pub use crate::pagination::{
        CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        PaginationTermination,
    };
    pub use crate::policy::ClientPolicyBuilder;
    pub use crate::rate_limit::{
        RateLimitError, RateLimitErrorKind, RateLimitObservation, RateLimitObserver,
        RateLimitResponseContext,
    };
    pub use crate::request::{PaginatedRequest, PendingRequest};
    pub use crate::retry_mode::{RetryMode, RetryModeError, StatusRetryConfig};
    pub use crate::secret::SecretString;
    pub use crate::transport::DecodedResponse;
}

pub mod advanced {
    #[cfg(feature = "json")]
    pub use crate::auth::OAuth2ClientCredentialsProvider;
    pub use crate::auth::{
        AuthChallengeMode, AuthChallengePolicy, AuthError, AuthErrorKind, AuthFuture,
        AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse, AuthInternalPolicy, AuthMode,
        AuthPlacement, AuthPlan, AuthPreparationMode, AuthProviderBinding, AuthRecoveryReason,
        AuthRejectionDecision, AuthRequirement, AuthRequirementId, AuthStepPolicy,
        CredentialContext, CredentialId, CredentialLease, CredentialMaterial, CredentialProvider,
        CredentialProviderState, CredentialRefreshReason, InvalidateReason, SecretCredential,
    };
    pub use crate::body::{BodyError, BodyErrorKind};
    pub use crate::codec::{
        BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody,
        ResponseCodec,
    };
    pub use crate::debug::{
        DebugSink, NoopDebugSink, SanitizedHeaderValue, SanitizedHeaders, StderrDebugSink,
    };
    pub use crate::error::{
        ClientBuildErrorKind, ErrorContext, FxError, PaginationError, PaginationErrorKind,
    };
    pub use crate::execution_meta::RequestExecutionMeta;
    pub use crate::io::{
        AdvancedRequestBody, PreparedBody, PreparedEndpoint, PreparedRequestEntity,
        PreparedStreamEndpoint, RequestAuthentication, RequestEntity,
    };
    pub use crate::media::{
        Jpeg, JsonContentType, Mp3, Mp4, OctetStream, Pdf, Png, TextContentType, Zip,
    };
    #[cfg(feature = "multipart")]
    pub use crate::multipart::{
        FormData, MultipartBody, MultipartBodyError, MultipartBodyErrorKind,
        MultipartReplayFactory, RawPart,
    };
    pub use crate::pagination::{
        Control, CursorPagination, EndpointPagination, HasNextCursor, OffsetLimitPagination,
        PageAdvance, PageApply, PageDecision, PageItems, PagedPagination, PaginateBinding,
        PaginationCaps, PaginationRuntime, PaginationRuntimeAdapter, PaginationTermination,
        ProgressKey,
    };
    pub use crate::policy::ClientPolicyBuilder;
    pub use crate::rate_limit::{
        DefaultRateLimitResponsePolicy, DefaultRateLimiter, GovernorRateLimiter, NoopRateLimiter,
        RateLimitBucketId, RateLimitBucketUse, RateLimitContext, RateLimitError,
        RateLimitErrorKind, RateLimitFuture, RateLimitKey, RateLimitKeyPart, RateLimitKeyValue,
        RateLimitPermit, RateLimitPlan, RateLimitResponseAction, RateLimitResponseContext,
        RateLimitResponsePolicy, RateLimitScopeHint, RateLimitSetting, RateLimitWindow,
        RateLimiter, parse_retry_after,
    };
    pub use crate::retry_mode::{RetryMode, RetryModeError, StatusRetryConfig};
    pub use crate::runtime::{DebugConfig, RuntimeConfig};
    pub use crate::runtime_hooks::{
        HookMeta, NoopRuntimeHooks, PostResponseHookContext, PreSendHookContext,
        RequestErrorHookContext, RuntimeHooks,
    };
    pub use crate::stream_body::{StreamBody, StreamBodyError};
    pub use crate::stream_response::StreamResponse;
    pub use crate::transport::{
        ReqwestClientBuildError, SafeProxy, SafeProxyError, SafeReqwestBuilder,
    };
    pub use crate::types::{
        HostLabelSource, HostParts as HostMap, HostSpec, RouteBuilder, UrlPath,
    };
}

pub mod dangerous {
    #[cfg(feature = "dangerous-raw-response")]
    pub use crate::transport::BuiltResponse;
}

#[cfg(test)]
mod regression_tests;
