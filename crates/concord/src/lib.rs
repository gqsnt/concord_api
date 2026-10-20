//! The user-facing Concord contract compiler and managed HTTP runtime.
//!
//! Applications depend on this crate to declare APIs with [`api`], use the
//! generated Rust surface, and access semantic runtime extension contracts.
//! Ordinary default features include the bounded process-local rate-limit
//! policy exposed as `rate_limit::LocalRateLimiter`.
//!
//! Generated construction and navigation use declaration-domain method names.
//! Value controls follow the declared shape, never Rust type spelling:
//! `set_*` selects a payload (`impl IntoValue<T>`), `set_*_if_some` takes an
//! exact `Option<T>` whose `None` means "no new opinion", optional (`?`) values
//! add the exact slot replacement `replace_*(Option<T>)` and `omit_*`, and
//! initialized values add `reset_*`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Declares a generated Concord client and its typed endpoints.
///
/// Generated constructors and navigation retain declaration-domain names.
/// `set_*` selects a payload through the closed `IntoValue<T>` conversions;
/// optional values add the exact `replace_*(Option<T>)` and `omit_*`,
/// `set_*_if_some(Option<T>)` keeps the current selection for `None`, and
/// initialized values add `reset_*`. See the generated method docs of each
/// client for its exact controls.
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

#[cfg(feature = "records")]
pub use concord_core::contracts::ItemStream;
pub use concord_core::contracts::{
    ApiError, ApiResponse, AtomEncoder, AtomError, AtomErrorKind, Binary, BoundedAtomWriter,
    ByteStream, Bytes, CallOptions, CancellationToken, ClientBuildDiagnostic, ClientBuildError,
    ControlError, ControlSummary, Discard, DispatchKnowledge, Empty, ExecutionMeta, IntoValue,
    OperationId, OrderedValues, RequestAtom, RequestControls, RequestStream, SecretString,
    StoreOutcome, Text,
};
pub use concord_extras::atom::DisplayAtom;

/// Explicit scalar protocol adapters.
pub mod atom {
    pub use concord_core::contracts::{
        AtomEncoder, AtomError, AtomErrorKind, BoundedAtomWriter, OrderedValues, RequestAtom,
    };
    pub use concord_extras::atom::DisplayAtom;
}
#[cfg(feature = "json")]
pub use concord_extras::codec::Json;

/// Credential material, provider contracts, and enabled first-party providers.
pub mod auth {
    pub use concord_core::contracts::{AccessToken, ApiKey, BasicCredential, SecretString};
    pub use concord_core::contracts::{
        Auth, AuthError, AuthErrorKind, AuthTarget, BoxFuture, ChallengePolicy, CredentialHandle,
        CredentialMaterial, CredentialMaterialError, CredentialMaterialErrorKind,
        CredentialProvider, CredentialReason, CredentialRef, CredentialRequest,
        MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_PLACEMENTS, MAX_AUTH_QUERY_VALUE_BYTES,
        MAX_AUTHORIZATION_SCHEME_BYTES, MAX_AUTHORIZATION_VALUE_BYTES, MAX_BASIC_PASSWORD_BYTES,
        MAX_BASIC_USERNAME_BYTES, MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES,
        ProviderBody, ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse,
        ProviderRetry, RecoveryMode, SecretCredential,
    };
    pub use concord_core::contracts::{
        ChallengeRecovery, CredentialVersion, MAX_CREDENTIAL_FLIGHT_WAITERS,
        MAX_PROVIDER_ORIGIN_BYTES, MAX_PROVIDER_ORIGINS, MAX_PROVIDER_SENDS, ProviderOrigin,
        ProviderOriginError, ProviderOriginErrorKind, StoreOutcome, StoreRequest,
    };
    #[cfg(feature = "oauth2-client")]
    pub use concord_extras::auth::{
        MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES, MAX_OAUTH2_CLIENT_ID_BYTES,
        MAX_OAUTH2_CLIENT_SECRET_BYTES, MAX_OAUTH2_SCOPE_BYTES, MAX_OAUTH2_TOKEN_URL_BYTES,
        OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError,
        OAuth2ClientCredentialsBuildErrorKind, OAuth2ClientCredentialsBuilder,
    };
}

/// Request representations, the one prepared body, factories, and bounded
/// streaming primitives.
pub mod body {
    pub use concord_core::body::BodyDescription;
    pub use concord_core::contracts::{
        BodyError, BodyErrorKind, BodyFactory, BodyKind, LengthHint, MAX_BODY_MEDIA_HINT_BYTES,
        MAX_EMPTY_REQUEST_CHUNKS, MAX_REQUEST_CONTENT_TYPE_BYTES, PageRequestRepresentation,
        PreparedBody, RequestBodyContext, RequestRepresentation, RequestStream,
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
        Binary, CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
        Text,
    };
    pub use concord_core::contracts::{Bytes, BytesMut};
    #[cfg(feature = "json")]
    pub use concord_extras::codec::Json;
    pub use concord_extras::codec::NotFound;
}

/// Explicit endpoint values and typed pending request contracts.
pub mod endpoint {
    pub use concord_core::contracts::{
        AbsentStatus, AcceptedStatus, Effect, EffectSource, EndpointCapabilities,
        EndpointDescription, EndpointId, MAX_ENDPOINT_ID_BYTES, MAX_FORMATTED_PUBLIC_VALUE_BYTES,
        MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
        MAX_REQUEST_URL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS, MAX_RUNTIME_ENDPOINT_METHOD_BYTES,
        MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, RequestPlan, ResponseAcquisition,
        StatusSource, TimeoutSelection,
    };
    pub use concord_core::contracts::{
        BoundRuntimeRequest, MAX_RUNTIME_KEY_BYTES, RuntimeAuthDisposition, RuntimeBindingFacts,
        RuntimeEndpoint, RuntimeEndpointBuilder, RuntimeFamily, RuntimeFamilyMode,
        RuntimeOperation, RuntimeOperationFact, RuntimeRouteMode,
    };
    pub use concord_core::{AcceptedBytesRequest, Request, ResponseRequest, StoreRequest};
}

/// Complete, value-free effective contract descriptions (§18).
///
/// `Client::contract()` returns a `&'static ContractDescription` without an
/// executable client, secret or network. Descriptions are immutable,
/// `Send + Sync` and built once; constructing one never evaluates defaults,
/// converters or encoders, and sensitive literals are held only as redacted
/// markers. The versioned JSON format `concord.contract/3`, semantic diffing
/// and the `contract export|check|diff` tooling live in the `concord-inspect`
/// companion crate so this facade carries no JSON dependency.
pub mod inspection {
    pub use concord_core::inspection::*;
}

/// Sanitized runtime, configuration, lifecycle, and decoding errors.
pub mod error {
    pub use concord_core::contracts::{
        ApiError, AttemptFacts, AuthError, AuthErrorKind, BodyError, BodyErrorKind,
        ClientBuildDiagnostic, ClientBuildError, ClientBuildErrorKind, ClientInputKind, CodecError,
        CodecErrorKind, ConfigurationError, ConfigurationErrorKind, DispatchKnowledge,
        ErrorCategory, FailureStage, HttpResponseFailure, OperationCounts, PaginationError,
        PaginationErrorKind, ProviderAcquisitionFacts, ProviderAcquisitionOutcome, RateLimitError,
        RateLimitErrorKind, ResponseBodyError, ResponseBodyErrorKind, StatusBody,
        StatusBodyDecodeError, StatusBodyDecodeErrorKind, StatusBodyState,
    };
}

/// Read-only bounded and sanitized operational observation.
pub mod observation {
    pub use concord_core::contracts::{
        AttemptInfo, AttemptStarted, BodyKind, CompletionOutcome, DebugLevel, OperationEvent,
        OperationFinished, OperationObserver, PaginationFinished, ProviderAcquisitionFinished,
        RequestTarget, ResponseHeadObserved, SanitizedHeaderMap, SanitizedHeaderValue,
        SanitizedHeaders, StreamFinished, StreamKind, StreamOutcome,
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

/// Pagination contracts, traversal terminals and summaries, and built-in
/// controllers.
pub mod pagination {
    pub use concord_core::pagination::{
        Checkpoint, CheckpointError, CheckpointErrorKind, CollectAll, CollectPrefix,
        ContinuationKnowledge, DEFAULT_TRAVERSAL_MAX_HANDOFFS, MAX_PAGE_ATTEMPTS,
        MAX_PAGINATION_HEADER_BYTES, MAX_PAGINATION_HEADER_INTERESTS, Page, PageCorrelation,
        PageDecisionLimits, PageResponse, PageStep, Pages, PaginationController,
        PaginationDescription, PaginationError, PaginationErrorKind, Prefix, ResumableController,
        StopReason, TraversalSummary,
    };
    pub use concord_extras::pagination::{
        CursorInit, CursorPagination, HasNextCursor, LinkCursorPagination, NEXT_CURSOR_HEADER,
        NextHeaderPagination, OffsetInit, OffsetPagination, PageInit, PagePagination,
    };
}

/// Core rate-limit contracts and the enabled first-party implementation.
///
/// A declaration's lexical location selects applicability; ordered `key [...]`
/// dimensions select bucket partitioning. With the `rate-limit` feature,
/// `LocalRateLimiter` is available here. Any generated
/// client with an effective policy still requires explicit limiter selection.
///
/// Policy IDs are authored stable identities. Sharing one limiter across
/// generated contracts therefore requires intentionally compatible policy
/// namespaces.
pub mod rate_limit {
    pub use concord_core::contracts::{BoxFuture, RateLimitErrorKind};
    pub use concord_core::rate_limit::{
        CanonicalKeyWriter, CooldownOrigin, DEFAULT_RATE_LIMIT_KEY_BYTES,
        DEFAULT_RATE_SCHEMA_REGISTRATIONS, ENDPOINT_LABEL, EncodedKeyPart, Enforcement,
        LimiterCapability, MAX_RATE_KEY_ENCODING_ID_BYTES, MAX_RATE_LIMIT_ID_BYTES,
        MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_POLICIES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
        MAX_RATE_LIMITER_NAMESPACE_BYTES, NoopRateLimiter, ORIGIN_LABEL, RATE_KEY_FRAMING_VERSION,
        RateDimensionKind, RateDimensionSchema, RateKeyEncoder, RateKeyPart, RateLimitDeclaration,
        RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit,
        RateLimitPlan, RateLimitPolicy, RateLimitRequest, RateLimitResponse, RateLimitWindow,
        RateLimiter, RatePlanSummary, RatePolicySchema, RatePolicySummary, RateWindowSchema,
        UTF8_ENCODING_ID, encode_canonical, encode_canonical_with, is_valid_encoding_id,
    };
    #[cfg(feature = "rate-limit")]
    pub use concord_extras::rate_limit::LocalRateLimiter;
}

/// Common imports for declarations and ordinary generated-client use.
pub mod prelude {
    #[cfg(feature = "records")]
    pub use crate::ItemStream;
    #[cfg(feature = "json")]
    pub use crate::Json;
    #[cfg(feature = "multipart")]
    pub use crate::body::Multipart;
    pub use crate::body::RequestStream;
    pub use crate::codec::Binary;
    #[cfg(feature = "csv")]
    pub use crate::records::Csv;
    #[cfg(feature = "ndjson")]
    pub use crate::records::NdJson;
    pub use crate::{
        ApiError, ApiResponse, ByteStream, Bytes, ClientBuildError, Discard, Empty, SecretString,
        Text, api,
    };
}

/// Generic bounded record-stream contracts and enabled concrete decoders.
#[cfg(feature = "records")]
pub mod records {
    pub use concord_core::records::{
        ItemBatches, ItemStream, RecordDecoder, RecordError, RecordErrorKind,
    };
    #[doc(hidden)]
    pub use concord_core::records::{decode_complete_records, validate_record_decoder};
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
        AcceptedBytesDriver, ApiResponse, BodyDisposition, Buffered, ByteStream, Discard,
        DiscardDriver, Empty, ExecutionMeta, MAX_ACCEPT_BYTES, MAX_EMPTY_RESPONSE_CHUNKS,
        OptionalOutcome, ResponseBodyError, ResponseBodyErrorKind, ResponseDriver, ResponseHead,
        ResponseRepresentation, StreamErrorKind, StreamState, StreamSummary, Streaming,
    };
    pub use concord_extras::codec::NotFound;
}

/// Concord-managed application retry policy and classifier contracts.
pub mod retry {
    pub use concord_core::contracts::{
        MAX_POLICY_RETRIES_PER_OPERATION, MAX_VISIBLE_ATTEMPTS_PER_OPERATION, RetryAttempt,
        RetryBackoff, RetryBackoffError, RetryClassifier, RetryDecision, RetryFailure, RetryJitter,
        RetryPolicy, RetryPolicyBuildError, RetryPolicyBuildErrorKind, RetryPolicyBuilder,
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
    pub use concord_core::__private::*;
    /// Creates the façade-owned generated-client runtime baseline.
    #[doc(hidden)]
    #[must_use]
    pub fn default_runtime_config() -> concord_core::contracts::RuntimeConfigBuilder {
        concord_core::__private::default_runtime_config()
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
