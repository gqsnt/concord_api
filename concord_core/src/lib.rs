pub mod auth;
mod cache;
mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
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
        PaginatedEndpoint, PaginationPlan, RequestArgs, RequestOverrides, RequestPlan,
        ResolvedRoute, ResponsePlan, ResponseSpec, Transform, TransformResp,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, CursorPagination, HasNextCursor, OffsetLimitPagination, PageAdvance, PageDecision,
        PageInit, PageRequest, PagedPagination, PaginationCaps, PaginationController,
        PaginationTermination, ProgressKey,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
    pub use crate::{cache::CacheSetting, retry::RetrySetting};
}
pub mod prelude {
    pub use crate::auth::{AccessToken, ApiKey, BasicCredential};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{NoContent, text::Text};
    pub use crate::debug::DebugLevel;
    pub use crate::endpoint::{Endpoint, PaginatedEndpoint};
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
        AuthHttpRequest, AuthHttpResponse, AuthIdentity, AuthInternalPolicy, AuthMode,
        AuthPlacement, AuthPlan, AuthProvenance, AuthRejectionDecision, AuthRequirement,
        AuthRequirementId, AuthRetryReason, AuthStepPolicy, AuthUsageId, ClientCertificate,
        CredentialContext, CredentialId, CredentialLease, CredentialMaterial, CredentialProvider,
        CredentialRef, CredentialRefreshReason, CredentialSlot, InvalidateReason,
        ManualCredentialProvider, PendingAuthPlacement, PendingAuthSlot, PreparedAuthCredential,
        PreparedInternalAuth, SecretCredential, StaticApiKeyProvider, StaticBasicProvider,
        StaticBearerProvider, apply_basic_credential, apply_certificate_credential,
        apply_secret_credential, auth_decision_for_status, invalidate_rejected_credential,
        read_auth_lock, write_auth_lock,
    };
    pub use crate::cache::{
        CacheAfter, CacheBefore, CacheCapacity, CacheConfig, CacheEntryId, CacheFailureMode,
        CacheFuture, CacheKey, CacheMode, CachePrimaryKey, CacheRequestMode, CacheRevalidation,
        CacheSkipReason, CacheStore, NoopCacheStore, default_cache_key,
    };
    #[cfg(feature = "cache-moka")]
    pub use crate::cache::{MokaCacheConfig, MokaCacheStore};
    pub use crate::codec::{
        BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
    };
    pub use crate::debug::{DebugSink, NoopDebugSink, StderrDebugSink};
    pub use crate::error::{ErrorContext, FxError};
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
    pub use crate::transport::{
        BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta, ReqwestTransport, Transport,
        TransportAuth, TransportBody, TransportError, TransportErrorKind, TransportRequest,
        TransportResponse,
    };
    pub use crate::types::{
        HostLabelSource, HostParts as HostMap, HostSpec, RouteBuilder, UrlPath,
    };
}
