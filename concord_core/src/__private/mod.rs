//! Concord's current generated-code integration contract.
//!
//! This surface is public only so macro expansions can refer to it across
//! crate boundaries. It is generated-only, unstable implementation
//! integration. It is intentionally not a transport, middleware, runtime
//! configuration, or general reflection API.

/// Semantic identity for the current Reqwest-native generated contract.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct ReqwestNativeGeneratedContract(());

/// Compatibility value referenced by every generated API module.
#[doc(hidden)]
pub const GENERATED_API_COMPATIBILITY: ReqwestNativeGeneratedContract =
    ReqwestNativeGeneratedContract(());

/// Validate the generated integration contract during constant evaluation.
#[doc(hidden)]
pub const fn assert_macro_core_compatibility(_: ReqwestNativeGeneratedContract) {}

/// Opaque typed provider binding used by generated client contexts.
///
/// This adapter only associates a generated credential identifier with an
/// existing core-owned provider slot. Authentication execution and mutable
/// cache state remain outside this generated integration module.
#[doc(hidden)]
pub use crate::auth::{
    AuthChallengeMode, AuthPreparationMode, AuthProvenance, AuthProviderBinding, AuthUsageId,
    CredentialId,
};

/// Opaque generated association between one provider and core-owned credential state.
#[doc(hidden)]
pub struct GeneratedCredentialBinding<Cx, P>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    state: crate::auth::CredentialProviderState<Cx, P>,
}

impl<Cx, P> GeneratedCredentialBinding<Cx, P>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    #[doc(hidden)]
    pub fn new(provider: P) -> Self {
        Self {
            state: crate::auth::CredentialProviderState::new(provider),
        }
    }

    #[doc(hidden)]
    pub fn new_result(
        id: crate::auth::CredentialId,
        provider: Result<P, crate::auth::AuthError>,
    ) -> Self {
        Self {
            state: crate::auth::CredentialProviderState::new_result(id, provider),
        }
    }

    #[doc(hidden)]
    pub fn secret_binding(
        &self,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> AuthProviderBinding<'_, Cx>
    where
        P::Credential: crate::auth::SecretCredential,
    {
        self.state.secret_binding(preparation, challenge)
    }

    #[doc(hidden)]
    pub fn basic_binding(
        &self,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> AuthProviderBinding<'_, Cx>
    where
        P: crate::auth::CredentialProvider<Cx, Credential = crate::auth::BasicCredential>,
    {
        self.state.basic_binding(preparation, challenge)
    }

    #[doc(hidden)]
    pub async fn set_manual(&self, value: P::Credential) -> Result<(), crate::auth::AuthError> {
        self.state.set_manual(value).await
    }

    #[doc(hidden)]
    pub async fn clear_manual(&self) -> Result<(), crate::auth::AuthError> {
        self.state.clear_manual().await
    }

    #[doc(hidden)]
    pub async fn has_value(&self) -> bool {
        self.state.has_value().await
    }
}

/// Obtain generated client-auth configuration for one short mutation.
#[doc(hidden)]
pub fn generated_auth_write<T>(
    lock: &std::sync::RwLock<T>,
) -> Result<std::sync::RwLockWriteGuard<'_, T>, crate::auth::AuthError> {
    crate::auth::write_auth_lock(lock, "generated auth configuration lock poisoned")
}

/// Install a generated rate-limit response policy without exposing the
/// concrete limiter implementation to generated source.
#[doc(hidden)]
pub fn generated_rate_limiter<P>(policy: P) -> std::sync::Arc<dyn crate::rate_limit::RateLimiter>
where
    P: crate::rate_limit::RateLimitResponsePolicy,
{
    std::sync::Arc::new(
        crate::rate_limit::GovernorRateLimiter::new()
            .with_response_policy(std::sync::Arc::new(policy)),
    )
}

#[doc(hidden)]
pub use crate::retry_mode::{ApiOriginDescriptor, FixedOriginDescriptor, OriginScheme};

/// Static origin relationship for one endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointOriginDescriptor {
    Fixed(FixedOriginDescriptor),
    Dynamic,
}

/// HTTP method identity stored without a runtime request value.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
}

impl HttpMethod {
    /// Adapt descriptor metadata to the current runtime method type.
    #[doc(hidden)]
    pub fn as_http_method(self) -> http::Method {
        match self {
            Self::Get => http::Method::GET,
            Self::Post => http::Method::POST,
            Self::Put => http::Method::PUT,
            Self::Delete => http::Method::DELETE,
            Self::Head => http::Method::HEAD,
            Self::Options => http::Method::OPTIONS,
            Self::Patch => http::Method::PATCH,
        }
    }
}

/// Request body contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestBodyDescriptor {
    None,
    Buffered { codec: &'static str },
    Streaming { media: &'static str },
    Multipart,
}

/// Static request metadata; it never contains a body instance.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestDescriptor {
    pub body: RequestBodyDescriptor,
}

/// Response contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseFormatDescriptor {
    Buffered { codec: &'static str },
    Bytes,
    NoContent,
    Streaming { media: &'static str },
}

/// Static response metadata; response processing remains in the current
/// runtime pipeline.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseDescriptor {
    pub format: ResponseFormatDescriptor,
}

/// One secret-free authentication requirement identity.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthRequirementDescriptor {
    pub credential: &'static str,
    pub usage_id: &'static str,
}

/// Static authentication metadata. Providers, credentials, and caches are
/// deliberately absent.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthDescriptor {
    pub requirements: &'static [AuthRequirementDescriptor],
}

/// Pagination facts known before runtime execution.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaginationDescriptor {
    pub can_change_origin: bool,
}

/// Static endpoint descriptor emitted once for every generated endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointDescriptor {
    pub name: &'static str,
    pub api_name: &'static str,
    pub method: HttpMethod,
    pub origin: EndpointOriginDescriptor,
    pub request: RequestDescriptor,
    pub response: ResponseDescriptor,
    pub auth: AuthDescriptor,
    pub pagination: Option<PaginationDescriptor>,
}

/// Static API descriptor emitted once for every generated API.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiDescriptor {
    pub name: &'static str,
    pub origin: ApiOriginDescriptor,
    pub endpoints: &'static [&'static EndpointDescriptor],
}

#[cfg(feature = "json")]
#[doc(hidden)]
pub use crate::auth::OAuth2ClientCredentialsProvider;
#[doc(hidden)]
pub use crate::auth::{
    AuthChallengePolicy, AuthPlacement, AuthPlan, AuthRequirement, CredentialRef,
    ManualCredentialProvider, NoAuthState, StaticApiKeyProvider, StaticBasicProvider,
    StaticBearerProvider,
};
#[doc(hidden)]
pub use crate::codec::{
    BodyCodec, CodecError, ContentType, DecodeContext, Decodes, EncodeContext, EncodedBody,
    Encodes, Format, FormatType, ResponseCodec,
};
#[doc(hidden)]
pub use crate::endpoint::{
    ClientPlanContext, EndpointMeta, EndpointPlan, IntoEndpointPlan, PaginatedEndpoint,
    PaginationMarker, RequestOverrides, RequestPlan, RequestPlanView, ResolvedRoute, ResponsePlan,
    ResponseTerminalEndpoint, ReusableEndpoint,
};
#[doc(hidden)]
pub use crate::error::ErrorContext;
#[cfg(feature = "multipart")]
#[doc(hidden)]
pub use crate::io::MultipartRequest;
#[doc(hidden)]
pub use crate::io::{
    BufferedResponse, BytesResponse, EncodedRequest, NoContentResponse, NoRequestBody,
    PreparedBody, PreparedRequestEntity, RawStreamRequest, RawStreamResponse, RequestEntity,
    ResponseEntity, ResponseEntityCapabilities, ResponseEntityPlan, ResponseEntityWithMeta,
};
#[doc(hidden)]
pub use crate::pagination::{
    Control, CursorPagination, EndpointPagination, HasNextCursor, OffsetLimitPagination,
    PageAdvance, PageApply, PageDecision, PageItems, PagedPagination, PaginateBinding,
    PaginationCaps, PaginationRuntime, PaginationRuntimeAdapter, PaginationTermination,
    ProgressKey,
};
#[doc(hidden)]
pub use crate::policy::{Policy, PolicyLayer, PolicySnapshot, ResolvedPolicy};
#[doc(hidden)]
pub use crate::rate_limit::{
    RateLimitBucketUse, RateLimitKey, RateLimitKeyPart, RateLimitPlan, RateLimitWindow,
};
#[doc(hidden)]
pub use crate::types::HostLabelSource;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_method_adapter_is_metadata_only() {
        assert_eq!(HttpMethod::Get.as_http_method(), http::Method::GET);
        const _: () = assert_macro_core_compatibility(GENERATED_API_COMPATIBILITY);
    }

    #[test]
    fn generated_surface_contains_no_auth_engine_or_mutable_cache_implementation() {
        let source = include_str!("mod.rs");
        for forbidden in [
            concat!("Credential", "SlotState"),
            concat!("get_or_", "refresh"),
            concat!("invalidate_", "generation"),
            concat!("AuthHttp", "Executor"),
            concat!("Req", "westClient"),
            concat!("req", "west::"),
            concat!("Trans", "port"),
            concat!("Dyn", "Body"),
        ] {
            assert!(
                !source.contains(forbidden),
                "__private contained {forbidden}"
            );
        }
    }
}
