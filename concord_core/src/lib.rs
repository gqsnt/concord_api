pub mod auth;
mod cache;
mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
mod inflight;
mod pagination;
mod policy;
mod rate_limit;
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
    pub use crate::codec::{ContentType, Decodes, Encodes, Format, FormatType};
    pub use crate::endpoint::{
        BodyPlan, ClientPlanContext, Decoded, EndpointMeta, EndpointPlan, Mapped, MappedResp,
        PaginationPlan, RequestArgs, RequestOverrides, RequestPlan, ResolvedRoute, ResponsePlan,
        ResponseSpec, Transform, TransformResp,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, CursorPagination, HasNextCursor, OffsetLimitPagination, PagedPagination,
        ProgressKey,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
}
pub mod prelude {
    pub use crate::auth::{AccessToken, ApiKey, BasicCredential};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{NoContent, text::Text};
    pub use crate::debug::DebugLevel;
    pub use crate::endpoint::Endpoint;
    pub use crate::error::ApiClientError;
    pub use crate::pagination::{
        CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination, Stop,
    };
    pub use crate::rate_limit::{
        RateLimitObservation, RateLimitObserver, RateLimitResponseContext,
    };
    pub use crate::request::{PaginatedRequest, PendingRequest};
    pub use crate::secret::SecretString;
}

pub mod advanced {
    pub use crate::auth::{
        AuthAppliedCredential, AuthAttemptSummary, AuthChallengePolicy, AuthDecision, AuthError,
        AuthErrorKind, AuthFuture, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse,
        AuthIdentity, AuthInternalPolicy, AuthMode, AuthPlacement, AuthPlan, AuthProvenance,
        AuthRequirement, AuthRequirementId, AuthRetryReason, AuthStepPolicy, AuthUsageId,
        ClientCertificate, CredentialContext, CredentialId, CredentialLease, CredentialMaterial,
        CredentialProvider, CredentialRef, CredentialRefreshReason, CredentialSlot,
        InvalidateReason, ManualCredentialProvider, OAuth2ClientCredentialsProvider,
        SecretCredential, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
        TransportAuth, apply_basic_credential, apply_certificate_credential,
        apply_secret_credential, invalidate_rejected_credential,
    };
    pub use crate::cache::{
        CacheAfter, CacheBefore, CacheCapacity, CacheConfig, CacheEntryId, CacheFailureMode,
        CacheKey, CacheMode, CachePrimaryKey, CacheRequestMode, CacheRevalidation, CacheSetting,
        CacheSkipReason, CacheStore, NoopCacheStore, default_cache_key,
    };
    #[cfg(feature = "cache-moka")]
    pub use crate::cache::{MokaCacheConfig, MokaCacheStore};
    pub use crate::debug::{DebugSink, NoopDebugSink, StderrDebugSink};
    pub use crate::error::{ErrorContext, FxError};
    pub use crate::inflight::{
        InflightPolicy, InflightRegistry, NoopInflightPolicy, RequestKey, SafeMethodInflightPolicy,
    };
    pub use crate::pagination::{Caps, Control, ProgressKey};
    pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
    pub use crate::rate_limit::{
        DefaultRateLimitResponsePolicy, DefaultRateLimiter, GovernorRateLimiter, NoopRateLimiter,
        RateLimitBucketId, RateLimitBucketUse, RateLimitContext, RateLimitKey, RateLimitKeyPart,
        RateLimitKeyValue, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
        RateLimitResponsePolicy, RateLimitScopeHint, RateLimitSetting, RateLimitWindow,
        RateLimiter, parse_retry_after,
    };
    pub use crate::retry::{
        ConfiguredRetryPolicy, NoRetryPolicy, RetryBackoff, RetryConfig, RetryContext,
        RetryDecision, RetryIdempotency, RetryOutcome, RetryPolicy, RetrySetting,
    };
    pub use crate::runtime::{AuthRuntimeConfig, DebugConfig, RuntimeConfig};
    pub use crate::runtime_hooks::{
        HookMeta, NoopRuntimeHooks, PostResponseHookContext, PreSendHookContext, RuntimeHooks,
        TransportErrorHookContext,
    };
    pub use crate::runtime_state::ClientRuntimeState;
    pub use crate::transport::{DecodedResponse, RequestMeta, ReqwestTransport, Transport};
    pub use crate::types::{
        HostLabelSource, HostParts as HostMap, HostSpec, RouteBuilder, UrlPath,
    };
}
