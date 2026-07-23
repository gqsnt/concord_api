//! The user-facing Concord contract compiler and managed HTTP runtime.
//!
//! Applications depend on this crate to declare APIs with [`api`], use the
//! generated Rust surface, and access semantic runtime extension contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use concord_macros::api;

#[cfg(feature = "dangerous-dev-tools")]
#[doc(hidden)]
pub use concord_core::__development;
#[cfg(feature = "multipart")]
pub use concord_core::prelude::Multipart;
pub use concord_core::prelude::{
    AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
    ClientBuildError, ConfigurationError, DebugLevel, DecodedResponse, ErrorCategory, NoContent,
    OctetStream, RequestBody, SecretString, Stream, StreamBody, Text,
};
pub use concord_core::{PageLimit, PaginatedRequest, Request, ResponseRequest};
#[cfg(feature = "json")]
pub use concord_extras::codec::Json;
pub use concord_extras::pagination::{CursorPagination, OffsetPagination, PagePagination};

/// Buffered codec contracts, primitives, and enabled ecosystem codecs.
pub mod codec {
    pub use concord_core::codec::{
        CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream,
        Text,
    };
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
}

/// Generic pagination contracts and built-in controller policies.
pub mod pagination {
    pub use concord_core::pagination::{
        PageLimit, PaginatedRequest, PaginationController, PaginationError, PaginationErrorKind,
    };
    pub use concord_extras::pagination::{
        CursorPagination, HasNextCursor, OffsetPagination, PagePagination,
    };
}

/// Core rate-limit contracts and the enabled first-party implementation.
pub mod rate_limit {
    pub use concord_core::rate_limit::{
        Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, NoopRateLimiter, RateLimitError,
        RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver, RateLimitPermit,
        RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimitRule,
        RateLimitWindow, RateLimiter,
    };
    #[cfg(feature = "rate-limit-governor")]
    pub use concord_extras::rate_limit::{DefaultRateLimitObserver, GovernorRateLimiter};
}

/// Common imports for declarations and ordinary generated-client use.
pub mod prelude {
    pub use concord_macros::api;

    #[cfg(feature = "multipart")]
    pub use concord_core::prelude::Multipart;
    pub use concord_core::prelude::{
        AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
        ClientBuildError, ConfigurationError, DebugLevel, DecodedResponse, ErrorCategory,
        NoContent, OctetStream, PageLimit, PaginatedRequest, Request, RequestBody, ResponseRequest,
        SecretString, Stream, StreamBody, Text,
    };
    #[cfg(feature = "records")]
    pub use concord_core::prelude::{RecordBatches, RecordError, RecordStream, Records};
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
    #[cfg(feature = "csv")]
    pub use concord_extras::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use concord_extras::records::NdJson;
}

/// Stable advanced semantic contracts and runtime configuration types.
pub mod advanced {
    pub use concord_core::advanced::{
        ApiError, ApiResponse, Auth, AuthError, AuthErrorKind, BeforeSend, BodyError,
        BodyErrorKind, BodyFactory, BodyKind, BoxFuture, Buffered, ByteStream, Bytes, BytesMut,
        ChallengePolicy, ClientBuildError, ClientBuildErrorKind, CodecError, CompletionEvent,
        CompletionOutcome, ConfigurationError, ConfigurationErrorKind, Cooldown, CooldownScope,
        CredentialHandle, CredentialMaterial, CredentialProvider, CredentialReason, CredentialRef,
        CredentialRequest, DebugLevel, DecodeBody, DecodeContext, DecodedResponse, Discard,
        EncodeBody, EncodeContext, ErrorCategory, ExecutionMeta, HeaderMap, HeaderName,
        HeaderValue, HookError, HttpConfigBuilder, HttpResponseFailure, LengthHint, MediaType,
        Method, NoContent, NoopRateLimiter, OctetStream, OperationId, PageResponseSpec,
        PaginationCompletionEvent, PaginationController, PaginationError, PaginationErrorKind,
        PreparedEndpoint, PreparedEndpointBuilder, ProviderBody, ProviderBodyFactory, ProviderHttp,
        ProviderRequest, ProviderResponse, ProviderRetry, RateLimitError, RateLimitErrorKind,
        RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver, RateLimitPermit,
        RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimitRule,
        RateLimitWindow, RateLimiter, RecoveryMode, RequestBody, RequestBodyContext,
        RequestBodyMetadata, RequestBodySpec, RequestContentType, ResponseBodyError,
        ResponseBodyErrorKind, ResponseBodySpec, ResponseHead, ResponseHeadEvent, ResponseMode,
        RuntimeConfigBuilder, RuntimeHooks, SafeProxy, SanitizedHeaderMap, SanitizedHeaderValue,
        SanitizedHeaders, SecretCredential, StatusBody, StatusBodyDecodeError, StatusBodyState,
        StatusCode, Stream, StreamBody, Streaming, Text, Url, Version,
    };
    #[cfg(feature = "multipart")]
    pub use concord_core::advanced::{Multipart, MultipartBody, MultipartFactory, MultipartPart};
    #[cfg(feature = "records")]
    pub use concord_core::advanced::{
        RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
    };
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
    pub use concord_extras::pagination::{
        CursorPagination, HasNextCursor, OffsetPagination, PagePagination,
    };
    #[cfg(feature = "rate-limit-governor")]
    pub use concord_extras::rate_limit::{DefaultRateLimitObserver, GovernorRateLimiter};
    #[cfg(feature = "csv")]
    pub use concord_extras::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use concord_extras::records::NdJson;
}

/// Generic bounded record-stream contracts and enabled concrete decoders.
#[cfg(feature = "records")]
pub mod records {
    pub use concord_core::records::{
        RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
    };
    #[cfg(feature = "csv")]
    pub use concord_extras::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use concord_extras::records::NdJson;
}

/// Explicitly feature-gated capabilities with reduced ordinary safety guarantees.
#[cfg(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response"))]
pub mod dangerous {
    #[cfg(feature = "dangerous-custom-retry")]
    pub use concord_core::dangerous::ReqwestRetryBuilder;
    #[cfg(feature = "dangerous-raw-response")]
    pub use concord_core::dangerous::{RawResponse, RawResponseRequest};
}

/// Generated-code bridge. This is not a user extension surface.
#[doc(hidden)]
pub mod __private {
    #[doc(hidden)]
    pub use concord_core::__private::{
        ApiError, ApiResponse, Auth, ClientBuildError, ClientCore, DebugLevel, EmptyBody,
        EndpointSpec, GENERATED_CONTRACT_VERSION, GeneratedCredential, HeadBuilder, HeaderName,
        Method, PageResponseSpec, PaginatedEndpoint, PreparedHead, RateLimitId, RateLimitKey,
        RateLimitKeyPart, RateLimitRule, RateLimitWindow, RawResponseEndpoint, RequestBodyContext,
        RequestBodyMetadata, RequestBodySpec, RequestMeta, RequestOptions, ResponseBodySpec,
        TimeoutOverride, Url, assert_generated_contract, build_client_core, credential_store_error,
        default_http_config, default_http_config_with_custom_retry,
        duplicate_http_configuration_error, duplicate_runtime_configuration_error,
        inspect_endpoint, map_endpoint, missing_input_error, oauth2_client_credentials,
        oauth2_client_credentials_for_replace, paginate_request, request,
    };

    /// Produces a focused diagnostic for the transitional built-in OAuth2 syntax.
    #[doc(hidden)]
    pub const fn assert_oauth2_client_feature() {
        #[cfg(not(feature = "oauth2-client"))]
        panic!("Concord `oauth2_client` requires the `concord/oauth2-client` feature");
    }

    /// Produces a focused const-evaluation diagnostic when multipart is unavailable.
    #[doc(hidden)]
    pub const fn assert_multipart_feature() {
        #[cfg(not(feature = "multipart"))]
        panic!("Concord multipart DSL requires the `concord/multipart` feature");
    }

    /// Produces a focused diagnostic when the legacy custom-retry DSL is unavailable.
    #[doc(hidden)]
    pub const fn assert_custom_retry_feature() {
        #[cfg(not(feature = "dangerous-custom-retry"))]
        panic!("Concord `retry custom PATH` requires the `concord/dangerous-custom-retry` feature");
    }

    /// Creates the façade-owned generated-client runtime baseline.
    #[doc(hidden)]
    #[must_use]
    pub fn default_runtime_config() -> concord_core::advanced::RuntimeConfigBuilder {
        let runtime = concord_core::__private::default_runtime_config();
        #[cfg(feature = "rate-limit-governor")]
        {
            let limiter: std::sync::Arc<dyn concord_core::rate_limit::RateLimiter> =
                std::sync::Arc::new(concord_extras::rate_limit::GovernorRateLimiter::new());
            return concord_core::__private::install_generated_default_limiter(runtime, limiter);
        }
        #[cfg(not(feature = "rate-limit-governor"))]
        {
            runtime
        }
    }

    /// Applies generated rate-limit requirements to the selected runtime.
    #[doc(hidden)]
    pub fn prepare_generated_rate_limits(
        runtime: concord_core::advanced::RuntimeConfigBuilder,
        requires_limiter: bool,
    ) -> Result<concord_core::advanced::RuntimeConfigBuilder, ClientBuildError> {
        concord_core::__private::prepare_generated_rate_limits(runtime, requires_limiter)
    }

    /// Applies generated rate limits with a declaration-selected observer.
    #[doc(hidden)]
    pub fn prepare_generated_rate_limits_with_observer<O>(
        runtime: concord_core::advanced::RuntimeConfigBuilder,
        requires_limiter: bool,
    ) -> Result<concord_core::advanced::RuntimeConfigBuilder, ClientBuildError>
    where
        O: Default + concord_core::advanced::RateLimitObserver,
    {
        #[cfg(feature = "rate-limit-governor")]
        let runtime =
            concord_core::__private::install_generated_default_limiter_with(runtime, || {
                let limiter: std::sync::Arc<dyn concord_core::rate_limit::RateLimiter> =
                    std::sync::Arc::new(
                        concord_extras::rate_limit::GovernorRateLimiter::new()
                            .observer(std::sync::Arc::new(O::default())),
                    );
                limiter
            });
        #[cfg(not(feature = "rate-limit-governor"))]
        let _ = std::marker::PhantomData::<O>;
        concord_core::__private::prepare_generated_rate_limits(runtime, requires_limiter)
    }
}
