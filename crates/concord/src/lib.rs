//! The user-facing Concord contract compiler and managed HTTP runtime.
//!
//! Applications depend on this crate to declare APIs with [`api`], use the
//! generated Rust surface, and access semantic runtime extension contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Declares a generated Concord client and its typed endpoints.
#[cfg(feature = "dangerous-raw-response")]
#[macro_export]
macro_rules! api {
    ($($declaration:tt)*) => {
        $crate::__concord_api! {
            @concord_capabilities(raw_response)
            @concord_facade($crate)
            $($declaration)*
        }
    };
}

/// Declares a generated Concord client and its typed endpoints.
#[cfg(not(feature = "dangerous-raw-response"))]
#[macro_export]
macro_rules! api {
    ($($declaration:tt)*) => {
        $crate::__concord_api! {
            @concord_capabilities()
            @concord_facade($crate)
            $($declaration)*
        }
    };
}

#[doc(hidden)]
pub use concord_macros::__api as __concord_api;

#[cfg(feature = "multipart")]
pub use concord_core::prelude::Multipart;
pub use concord_core::prelude::{
    AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
    ClientBuildError, ClientInputKind, ConfigurationError, DebugLevel, ErrorCategory, NoContent,
    OctetStream, RequestBody, SecretString, Stream, StreamBody, Text,
};
pub use concord_core::{PageLimit, PaginatedRequest, Request, ResponseRequest};
#[cfg(feature = "json")]
pub use concord_extras::codec::Json;
pub use concord_extras::pagination::{CursorPagination, OffsetPagination, PagePagination};

/// Credential material, provider contracts, and enabled first-party providers.
pub mod auth {
    pub use concord_core::advanced::{
        Auth, AuthError, AuthErrorKind, BoxFuture, ChallengePolicy, CredentialHandle,
        CredentialMaterial, CredentialProvider, CredentialReason, CredentialRef, CredentialRequest,
        MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_QUERY_VALUE_BYTES, MAX_AUTHORIZATION_VALUE_BYTES,
        MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES, MAX_CREDENTIAL_SECRET_BYTES,
        ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
        ProviderRetry, RecoveryMode, SecretCredential,
    };
    pub use concord_core::prelude::{AccessToken, ApiKey, BasicCredential, SecretString};
    #[cfg(feature = "oauth2-client")]
    pub use concord_extras::auth::{
        MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES, MAX_OAUTH2_CLIENT_ID_BYTES,
        MAX_OAUTH2_CLIENT_SECRET_BYTES, MAX_OAUTH2_SCOPE_BYTES, MAX_OAUTH2_TOKEN_URL_BYTES,
        OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError,
        OAuth2ClientCredentialsBuildErrorKind, OAuth2ClientCredentialsBuilder,
    };
}

/// Request-body specifications, factories, metadata, and bounded streaming primitives.
pub mod body {
    pub use concord_core::advanced::{
        BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, RequestBody,
        RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestContentType, Stream,
        StreamBody,
    };
    #[cfg(feature = "multipart")]
    pub use concord_core::advanced::{
        MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
        MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
        MAX_MULTIPART_PARTS, Multipart, MultipartBody, MultipartFactory, MultipartPart,
    };
}

/// Buffered codec contracts, primitives, and enabled ecosystem codecs.
pub mod codec {
    pub use concord_core::advanced::{Bytes, BytesMut};
    pub use concord_core::codec::{
        CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream,
        Text,
    };
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
}

/// Explicit endpoint values and typed pending request contracts.
pub mod endpoint {
    pub use concord_core::advanced::{
        EndpointCapabilities, EndpointDescription, EndpointId, MAX_ENDPOINT_ID_BYTES,
        MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS,
        MAX_RUNTIME_ENDPOINT_COMPONENTS, MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory,
        ResponseAcquisition, RuntimeEndpoint, RuntimeEndpointBuilder, SafeRequestPlan,
        TimeoutSelection,
    };
    pub use concord_core::{PaginatedRequest, Request, ResponseRequest};
}

/// Safe runtime, configuration, lifecycle, and decoding errors.
pub mod error {
    pub use concord_core::advanced::{
        ApiError, AuthError, AuthErrorKind, BodyError, BodyErrorKind, ClientBuildError,
        ClientBuildErrorKind, ClientInputKind, CodecError, ConfigurationError,
        ConfigurationErrorKind, ErrorCategory, HookError, HttpResponseFailure, PaginationError,
        PaginationErrorKind, RateLimitError, RateLimitErrorKind, ResponseBodyError,
        ResponseBodyErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
    };
}

/// Sanitized lifecycle observation contracts and completion events.
pub mod hooks {
    pub use concord_core::advanced::{
        BeforeSend, BodyKind, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
        PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks, SanitizedHeaderMap,
        SanitizedHeaderValue, SanitizedHeaders,
    };
}

/// HTTP value types and safe managed-transport configuration.
pub mod http {
    pub use concord_core::advanced::{
        HeaderMap, HeaderName, HeaderValue, HttpConfigBuilder, Method, SafeProxy, StatusCode, Url,
        Version,
    };
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
    pub use concord_core::advanced::{BoxFuture, RateLimitErrorKind};
    pub use concord_core::rate_limit::{
        Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_ID_BYTES,
        MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE, NoopRateLimiter,
        RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver,
        RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead,
        RateLimitRule, RateLimitWindow, RateLimiter,
    };
    #[cfg(feature = "rate-limit-governor")]
    pub use concord_extras::rate_limit::{DefaultRateLimitObserver, GovernorRateLimiter};
}

/// Common imports for declarations and ordinary generated-client use.
pub mod prelude {
    pub use crate::api;

    #[cfg(feature = "multipart")]
    pub use concord_core::prelude::Multipart;
    pub use concord_core::prelude::{
        AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, ByteStream, Bytes,
        ClientBuildError, ClientInputKind, ConfigurationError, DebugLevel, ErrorCategory,
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
        ChallengePolicy, ClientBuildError, ClientBuildErrorKind, ClientInputKind, CodecError,
        CompletionEvent, CompletionOutcome, ConfigurationError, ConfigurationErrorKind, Cooldown,
        CooldownScope, CredentialHandle, CredentialMaterial, CredentialProvider, CredentialReason,
        CredentialRef, CredentialRequest, DebugLevel, DecodeBody, DecodeContext, Discard,
        EncodeBody, EncodeContext, EndpointCapabilities, EndpointDescription, EndpointId,
        ErrorCategory, ExecutionMeta, HeaderMap, HeaderName, HeaderValue, HookError,
        HttpConfigBuilder, HttpResponseFailure, LengthHint, MAX_ENDPOINT_ID_BYTES,
        MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES,
        MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE,
        MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS, MAX_RUNTIME_ENDPOINT_COMPONENTS,
        MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, MediaType, Method, NoContent, NoopRateLimiter,
        OctetStream, OperationId, PageResponseSpec, PaginationCompletionEvent,
        PaginationController, PaginationError, PaginationErrorKind, ProviderBody,
        ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse, ProviderRetry,
        RateLimitError, RateLimitErrorKind, RateLimitId, RateLimitKey, RateLimitKeyPart,
        RateLimitObserver, RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse,
        RateLimitResponseHead, RateLimitRule, RateLimitWindow, RateLimiter, RecoveryMode,
        RequestBody, RequestBodyCategory, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
        RequestContentType, ResponseAcquisition, ResponseBodyError, ResponseBodyErrorKind,
        ResponseBodySpec, ResponseHead, ResponseHeadEvent, ResponseMode, RuntimeConfigBuilder,
        RuntimeEndpoint, RuntimeEndpointBuilder, RuntimeHooks, SafeProxy, SafeRequestPlan,
        SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders, SecretCredential, StatusBody,
        StatusBodyDecodeError, StatusBodyState, StatusCode, Stream, StreamBody, Streaming, Text,
        TimeoutSelection, Url, Version,
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

/// Bounded response specifications, acquisition modes, and response metadata.
pub mod response {
    pub use concord_core::advanced::{
        ApiResponse, Buffered, ByteStream, Discard, ExecutionMeta, NoContent, PageResponseSpec,
        ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead, ResponseMode,
        Streaming,
    };
}

/// Immutable client-runtime configuration and safe operation identity.
pub mod runtime {
    pub use concord_core::advanced::{OperationId, RuntimeConfigBuilder};
}

/// Explicitly feature-gated capabilities with reduced ordinary safety guarantees.
#[cfg(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response"))]
pub mod dangerous {
    #[cfg(feature = "dangerous-custom-retry")]
    pub use concord_core::dangerous::ReqwestRetryBuilder;
    #[cfg(feature = "dangerous-raw-response")]
    pub use concord_core::dangerous::{RawResponse, RawResponseMode, RawResponseRequest};
}

/// Generated-code bridge. This is not a user extension surface.
#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "dangerous-raw-response")]
    #[doc(hidden)]
    pub use concord_core::__private::raw_response_request;
    #[doc(hidden)]
    pub use concord_core::__private::{
        ApiError, ApiResponse, Auth, ClientBuildError, ClientCore, ClientInputKind, DebugLevel,
        EmptyBody, EndpointCapabilities, EndpointDescription, EndpointId, EndpointSpec,
        GENERATED_CONTRACT_VERSION, GeneratedCredential, HeadBuilder, HeaderName, Method,
        PageResponseSpec, PaginatedEndpoint, PreparedHead, RateLimitId, RateLimitKey,
        RateLimitKeyPart, RateLimitRule, RateLimitWindow, RawResponseEndpoint, RequestBodyCategory,
        RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestMeta, RequestOptions,
        ResponseAcquisition, ResponseBodySpec, SafeRequestPlan, TimeoutOverride, TimeoutSelection,
        Url, assert_generated_contract, build_client_core, credential_store_error,
        default_http_config, endpoint_id_from_static, inspect_endpoint, map_endpoint,
        missing_input_error, paginate_request, request, response_request, safe_request_plan,
    };
    /// Creates the façade-owned generated-client runtime baseline.
    #[doc(hidden)]
    #[must_use]
    pub fn default_runtime_config() -> concord_core::advanced::RuntimeConfigBuilder {
        let runtime = concord_core::__private::default_runtime_config();
        #[cfg(feature = "rate-limit-governor")]
        {
            let limiter: std::sync::Arc<dyn concord_core::rate_limit::RateLimiter> =
                std::sync::Arc::new(concord_extras::rate_limit::GovernorRateLimiter::new());
            concord_core::__private::install_generated_default_limiter(runtime, limiter)
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
}
