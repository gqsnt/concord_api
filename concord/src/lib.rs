//! The user-facing Concord contract compiler and managed HTTP runtime.
//!
//! Applications depend on this crate to declare APIs with [`api`], use the
//! generated Rust surface, and access semantic runtime extension contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use concord_macros::api;

#[cfg(feature = "json")]
pub use concord_core::prelude::Json;
#[cfg(feature = "multipart")]
pub use concord_core::prelude::Multipart;
pub use concord_core::prelude::{
    AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
    ClientBuildError, ConfigurationError, DebugLevel, DecodedResponse, ErrorCategory, NoContent,
    OctetStream, RequestBody, SecretString, Stream, StreamBody, Text,
};
pub use concord_core::{PageLimit, PaginatedRequest, Request, ResponseRequest};

/// Common imports for declarations and ordinary generated-client use.
pub mod prelude {
    pub use concord_macros::api;

    #[cfg(feature = "csv")]
    pub use concord_core::prelude::Csv;
    #[cfg(feature = "json")]
    pub use concord_core::prelude::Json;
    #[cfg(feature = "multipart")]
    pub use concord_core::prelude::Multipart;
    #[cfg(feature = "ndjson")]
    pub use concord_core::prelude::NdJson;
    pub use concord_core::prelude::{
        AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
        ClientBuildError, ConfigurationError, DebugLevel, DecodedResponse, ErrorCategory,
        NoContent, OctetStream, PageLimit, PaginatedRequest, Request, RequestBody, ResponseRequest,
        SecretString, Stream, StreamBody, Text,
    };
    #[cfg(feature = "records")]
    pub use concord_core::prelude::{RecordBatches, RecordError, RecordStream, Records};
}

/// Stable advanced semantic contracts and runtime configuration types.
pub mod advanced {
    #[cfg(feature = "csv")]
    pub use concord_core::advanced::Csv;
    #[cfg(feature = "rate-limit-governor")]
    pub use concord_core::advanced::GovernorRateLimiter;
    #[cfg(feature = "json")]
    pub use concord_core::advanced::Json;
    #[cfg(feature = "ndjson")]
    pub use concord_core::advanced::NdJson;
    pub use concord_core::advanced::{
        ApiError, ApiResponse, Auth, AuthError, AuthErrorKind, BeforeSend, BodyError,
        BodyErrorKind, BodyFactory, BodyKind, BoxFuture, Buffered, ByteStream, Bytes, BytesMut,
        ChallengePolicy, ClientBuildError, ClientBuildErrorKind, CodecError, CompletionEvent,
        CompletionOutcome, ConfigurationError, ConfigurationErrorKind, Cooldown, CooldownScope,
        CredentialHandle, CredentialMaterial, CredentialProvider, CredentialReason, CredentialRef,
        CredentialRequest, CursorPagination, DebugLevel, DecodeBody, DecodeContext,
        DecodedResponse, DefaultRateLimitObserver, Discard, EncodeBody, EncodeContext,
        ErrorCategory, ExecutionMeta, HasNextCursor, HeaderMap, HeaderName, HeaderValue, HookError,
        HttpConfigBuilder, HttpResponseFailure, LengthHint, MediaType, Method, NoContent,
        NoopRateLimiter, OctetStream, OffsetPagination, OperationId, PagePagination,
        PageResponseSpec, PaginationCompletionEvent, PaginationController, PaginationError,
        PaginationErrorKind, PreparedEndpoint, PreparedEndpointBuilder, ProviderBody,
        ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse, ProviderRetry,
        RateLimitError, RateLimitErrorKind, RateLimitId, RateLimitKey, RateLimitKeyPart,
        RateLimitObserver, RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse,
        RateLimitResponseHead, RateLimitRule, RateLimitWindow, RateLimiter, RecoveryMode,
        RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestContentType,
        ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
        ResponseHeadEvent, ResponseMode, RuntimeConfigBuilder, RuntimeHooks, SafeProxy,
        SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders, SecretCredential, StatusBody,
        StatusBodyDecodeError, StatusBodyState, StatusCode, Stream, StreamBody, Streaming, Text,
        Url, Version,
    };
    #[cfg(feature = "multipart")]
    pub use concord_core::advanced::{Multipart, MultipartBody, MultipartFactory, MultipartPart};
    #[cfg(feature = "records")]
    pub use concord_core::advanced::{
        RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
    };
}

/// Generic bounded record-stream contracts and enabled concrete decoders.
#[cfg(feature = "records")]
pub mod records {
    #[cfg(feature = "csv")]
    pub use concord_core::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use concord_core::records::NdJson;
    pub use concord_core::records::{
        RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
    };
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

    /// Produces a focused const-evaluation diagnostic when JSON is unavailable.
    #[doc(hidden)]
    pub const fn assert_json_feature() {
        #[cfg(not(feature = "json"))]
        panic!("Concord JSON DSL requires the `concord/json` feature");
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
        concord_core::__private::default_runtime_config()
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
        concord_core::__private::prepare_generated_rate_limits_with_observer::<O>(
            runtime,
            requires_limiter,
        )
    }
}
