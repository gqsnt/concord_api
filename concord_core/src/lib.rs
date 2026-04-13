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
mod runtime_hooks;
mod runtime_state;
mod secret;
mod timeout;
pub mod transport;
mod types;

pub mod internal {
    #[doc(hidden)]
    pub use crate::auth::{
        AuthChain, AuthController, AuthPart, AuthResponseAction, NoAuth, NoAuthController,
        NoAuthState,
    };
    #[doc(hidden)]
    pub use crate::endpoint::{
        BodyPart, Chain, Decoded, Mapped, MappedResp, NoBody, NoPolicy, NoRoute, PolicyPart,
        ResponseSpec, RoutePart, Transform, TransformResp,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, Controller, CursorPagination, HasNextCursor, NoController, NoPagination,
        OffsetLimitPagination, PagedPagination, PaginationPart, ProgressKey,
    };
}
pub mod prelude {
    pub use crate::auth::{
        AccessToken, ApiKey, AuthAppliedPart, AuthApplyContext, AuthAttempt, AuthBuildContext,
        AuthChain, AuthChallengeContext, AuthChallengeDecision, AuthController, AuthError,
        AuthErrorKind, AuthFuture, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse,
        AuthIdentity, AuthInternalPolicy, AuthMode, AuthPart,
        AuthPrepareContext as EndpointAuthPrepareContext, AuthProvenance, AuthRequirementId,
        AuthResponseAction,
        AuthResponseContext as EndpointAuthResponseContext, AuthRetryReason, AuthStepPolicy,
        AuthUsage, AuthUsageId, BasicAuth, BasicCredential, BearerAuth, CertificateAuth,
        ClientCertificate, CredentialContext, CredentialId, CredentialLease, CredentialMaterial,
        CredentialProvider, CredentialRefreshReason, CredentialSlot, HeaderAuth, InvalidateReason,
        NoAuth, NoAuthController, NoAuthState, QueryAuth, SecretCredential, StaticApiKeyProvider,
        StaticBasicProvider, StaticBearerProvider, TransportAuth, UseCredential,
        UseCredentialState,
    };
    #[cfg(feature = "json")]
    pub use crate::auth::OAuth2ClientCredentialsProvider;
    pub use crate::cache::{CacheKey, CacheStore, NoopCacheStore, default_cache_key};
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{NoContent, text::Text};
    pub use crate::debug::{DebugLevel, DebugSink, NoopDebugSink, StderrDebugSink};
    pub use crate::endpoint::Endpoint;
    pub use crate::error::{ApiClientError, ErrorContext, FxError};
    pub use crate::inflight::{
        InflightPolicy, InflightRegistry, NoopInflightPolicy, RequestKey, SafeMethodInflightPolicy,
    };
    pub use crate::pagination::PaginatedEndpoint;
    pub use crate::pagination::{
        Caps, CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        ProgressKey, Stop,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicyPatch};
    pub use crate::rate_limit::{
        NoopRateLimiter, RateLimitContext, RateLimitPermit, RateLimitResponseContext, RateLimiter,
    };
    pub use crate::request::{PaginatedRequest, PendingRequest};
    pub use crate::retry::{NoRetryPolicy, RetryContext, RetryDecision, RetryOutcome, RetryPolicy};
    pub use crate::runtime_hooks::{
        HookMeta, NoopRuntimeHooks, PostResponseHookContext, PreSendHookContext, RuntimeHooks,
        TransportErrorHookContext,
    };
    pub use crate::runtime_state::ClientRuntimeState;
    pub use crate::secret::SecretString;
    pub use crate::timeout::TimeoutOverride;
    pub use crate::transport::{DecodedResponse, RequestMeta};
    pub use crate::transport::{ReqwestTransport, Transport};
    pub use crate::types::{HostLabelSource, HostParts as HostMap, HostSpec, RouteParts, UrlPath};
}
