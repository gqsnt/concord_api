//! The user-facing Concord contract compiler and managed HTTP runtime.
//!
//! Applications depend on this crate to declare APIs with [`api`], use the
//! generated Rust surface, and access semantic runtime extension contracts.
//! Ordinary default features include the bounded process-local rate-limit
//! policy exposed as `rate_limit::LocalRateLimiter`.
//!
//! Generated construction and navigation use declaration-domain method names.
//! Public value mutation uses `set_*`; absence-capable values additionally use
//! `clear_*`, and values with declaration defaults use `reset_*`. These
//! capabilities are independent, so a defaulted `Option<T>` exposes all three.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Declares a generated Concord client and its typed endpoints.
///
/// Generated constructors and navigation retain declaration-domain names.
/// Public value mutations use `set_*`, with `clear_*` for absence-capable
/// values and `reset_*` for values with declaration defaults. A value may have
/// both capabilities.
#[macro_export]
macro_rules! api {
    ($($declaration:tt)*) => {
        $crate::__concord_api! {
            @concord_facade($crate)
            $($declaration)*
        }
    };
}

#[doc(hidden)]
pub use concord_macros::__api as __concord_api;

pub use concord_core::PageLimit;
pub use concord_core::contracts::{
    ApiError, ApiResponse, Bytes, ClientBuildError, NoContent, SecretString, Text,
};
#[cfg(feature = "json")]
pub use concord_extras::codec::Json;

/// Credential material, provider contracts, and enabled first-party providers.
pub mod auth {
    pub use concord_core::contracts::{AccessToken, ApiKey, BasicCredential, SecretString};
    pub use concord_core::contracts::{
        Auth, AuthError, AuthErrorKind, BoxFuture, ChallengePolicy, CredentialHandle,
        CredentialMaterial, CredentialMaterialError, CredentialMaterialErrorKind,
        CredentialProvider, CredentialReason, CredentialRef, CredentialRequest,
        MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_PLACEMENTS, MAX_AUTH_QUERY_VALUE_BYTES,
        MAX_AUTHORIZATION_SCHEME_BYTES, MAX_AUTHORIZATION_VALUE_BYTES, MAX_BASIC_PASSWORD_BYTES,
        MAX_BASIC_USERNAME_BYTES, MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES,
        ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
        ProviderRetry, RecoveryMode, SecretCredential,
    };
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
    pub use concord_core::contracts::{
        BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, MAX_EMPTY_REQUEST_CHUNKS,
        PageRequestBodySpec, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
        RequestContentType, Stream, StreamBody,
    };
    #[cfg(feature = "multipart")]
    pub use concord_core::contracts::{
        MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
        MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
        MAX_MULTIPART_PARTS, Multipart, MultipartBody, MultipartFactory, MultipartPart,
    };
}

/// Buffered codec contracts, primitives, and enabled ecosystem codecs.
pub mod codec {
    pub use concord_core::codec::{
        CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
        MediaType, OctetStream, Text,
    };
    pub use concord_core::contracts::{Bytes, BytesMut};
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
}

/// Explicit endpoint values and typed pending request contracts.
pub mod endpoint {
    pub use concord_core::contracts::{
        EndpointCapabilities, EndpointDescription, EndpointId, MAX_ENDPOINT_ID_BYTES,
        MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
        MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS,
        MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, ResponseAcquisition,
        RuntimeEndpoint, RuntimeEndpointBuilder, SafeRequestPlan, TimeoutSelection,
    };
    pub use concord_core::{Request, ResponseBytesRequest, ResponseRequest};
}

/// Safe runtime, configuration, lifecycle, and decoding errors.
pub mod error {
    pub use concord_core::contracts::{
        ApiError, AuthError, AuthErrorKind, BodyError, BodyErrorKind, ClientBuildError,
        ClientBuildErrorKind, ClientInputKind, CodecError, CodecErrorKind, ConfigurationError,
        ConfigurationErrorKind, ErrorCategory, HookError, HttpResponseFailure, PaginationError,
        PaginationErrorKind, RateLimitError, RateLimitErrorKind, ResponseBodyError,
        ResponseBodyErrorKind, StatusBody, StatusBodyDecodeError, StatusBodyState,
    };
}

/// Sanitized lifecycle observation contracts and completion events.
pub mod hooks {
    pub use concord_core::contracts::{
        AttemptInfo, BeforeSend, BodyKind, BoxFuture, CompletionEvent, CompletionOutcome,
        DebugLevel, PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks, SanitizedHeaderMap,
        SanitizedHeaderValue, SanitizedHeaders,
    };
}

/// HTTP value types and safe managed-transport configuration.
pub mod http {
    pub use concord_core::contracts::{
        HeaderMap, HeaderName, HeaderValue, HttpConfigBuilder, MAX_IDENTITY_PEM_BYTES,
        MAX_PROXY_URL_BYTES, MAX_ROOT_CERTIFICATE_BYTES, MAX_ROOT_CERTIFICATES,
        MAX_ROOT_CERTIFICATES_TOTAL_BYTES, Method, SafeProxy, StatusCode, Url, Version,
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
///
/// A declaration's lexical location selects applicability; optional ordered
/// `by [...]` dimensions select bucket partitioning. With the `rate-limit`
/// feature, `LocalRateLimiter` and `RetryAfterObserver` are available here and
/// the local limiter is installed by default for generated clients.
///
/// Source-derived rule IDs are deterministic within one generated client
/// contract, not globally unique across unrelated [`api!`](crate::api)
/// expansions. Sharing one limiter across unrelated generated contracts
/// requires compatible rule namespaces.
pub mod rate_limit {
    pub use concord_core::contracts::{BoxFuture, RateLimitErrorKind};
    pub use concord_core::rate_limit::{
        Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_ID_BYTES,
        MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE, NoopRateLimiter,
        RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver,
        RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead,
        RateLimitRule, RateLimitWindow, RateLimiter,
    };
    #[cfg(feature = "rate-limit")]
    pub use concord_extras::rate_limit::{LocalRateLimiter, RetryAfterObserver};
}

/// Common imports for declarations and ordinary generated-client use.
pub mod prelude {
    #[cfg(feature = "json")]
    pub use crate::Json;
    #[cfg(feature = "multipart")]
    pub use crate::body::Multipart;
    pub use crate::body::Stream;
    pub use crate::codec::OctetStream;
    #[cfg(feature = "csv")]
    pub use crate::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use crate::records::NdJson;
    #[cfg(feature = "records")]
    pub use crate::records::Records;
    pub use crate::{
        ApiError, ApiResponse, Bytes, ClientBuildError, NoContent, PageLimit, SecretString, Text,
        api,
    };
}

/// Generic bounded record-stream contracts and enabled concrete decoders.
#[cfg(feature = "records")]
pub mod records {
    pub use concord_core::records::{
        RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
    };
    #[cfg(feature = "ndjson")]
    pub use concord_extras::records::NdJson;
    #[cfg(feature = "csv")]
    pub use concord_extras::records::{
        CommaDelimiter, Csv, CsvDelimiter, CsvHeaderMode, HasHeader, NoHeader, SemicolonDelimiter,
        TabDelimiter,
    };
}

/// Bounded response specifications, acquisition modes, and response metadata.
pub mod response {
    pub use concord_core::contracts::{
        ApiResponse, Buffered, ByteStream, Discard, ExecutionMeta, MAX_EMPTY_RESPONSE_CHUNKS,
        NoContent, PageResponseSpec, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec,
        ResponseBytesMode, ResponseHead, ResponseMode, Streaming,
    };
}

/// Concord-managed application retry policy and classifier contracts.
pub mod retry {
    pub use concord_core::contracts::{
        MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryClassifier, RetryDecision,
        RetryFailure, RetryPolicy, RetryPolicyBuildError, RetryPolicyBuildErrorKind,
        RetryPolicyBuilder,
    };
}

/// Immutable client-runtime configuration and safe operation identity.
pub mod runtime {
    pub use concord_core::contracts::{OperationId, RuntimeConfigBuilder};
}

/// Generated-code bridge. This is not a user extension surface.
#[doc(hidden)]
pub mod __private {
    #[doc(hidden)]
    pub use concord_core::__private::response_bytes_request;
    #[doc(hidden)]
    pub use concord_core::__private::{
        ApiError, ApiResponse, Auth, ClientBuildError, ClientCore, ClientInputKind, DebugLevel,
        EmptyBody, EndpointCapabilities, EndpointDescription, EndpointId, EndpointSpec,
        GENERATED_CONTRACT_VERSION, GeneratedCredential, HeadBuilder, HeaderName,
        MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
        MAX_REQUEST_URL_BYTES, Method, PageRequestBodySpec, PageResponseSpec, PaginatedEndpoint,
        PreparedHead, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitRule, RateLimitWindow,
        RequestBodyCategory, RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestMeta,
        RequestOptions, ResponseAcquisition, ResponseBodySpec, SafeRequestPlan, TimeoutOverride,
        TimeoutSelection, Url, assert_generated_contract, basic_credential_input_error,
        build_client_core, credential_input_error, credential_store_error, default_http_config,
        endpoint_id_from_static, inspect_endpoint, map_endpoint, missing_input_error,
        paginate_request, request, response_request, safe_request_plan,
    };
    /// Creates the façade-owned generated-client runtime baseline.
    #[doc(hidden)]
    #[must_use]
    pub fn default_runtime_config() -> concord_core::contracts::RuntimeConfigBuilder {
        let runtime = concord_core::__private::default_runtime_config();
        #[cfg(feature = "rate-limit")]
        {
            let limiter: std::sync::Arc<dyn concord_core::rate_limit::RateLimiter> =
                std::sync::Arc::new(concord_extras::rate_limit::LocalRateLimiter::new());
            concord_core::__private::install_generated_default_limiter(runtime, limiter)
        }
        #[cfg(not(feature = "rate-limit"))]
        {
            runtime
        }
    }

    /// Applies generated rate-limit requirements to the selected runtime.
    #[doc(hidden)]
    pub fn prepare_generated_rate_limits(
        runtime: concord_core::contracts::RuntimeConfigBuilder,
        requires_limiter: bool,
    ) -> Result<concord_core::contracts::RuntimeConfigBuilder, ClientBuildError> {
        concord_core::__private::prepare_generated_rate_limits(runtime, requires_limiter)
    }
}
