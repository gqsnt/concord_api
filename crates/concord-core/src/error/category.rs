/// Stable top-level error classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    /// Client or request configuration is invalid.
    Configuration,
    /// A request codec failed.
    RequestEncoding,
    /// A request body could not be produced or transmitted.
    RequestBody,
    /// Required authentication material was unavailable.
    AuthenticationUnavailable,
    /// An authentication provider failed.
    AuthenticationProvider,
    /// A final authentication challenge rejected the request.
    AuthenticationRejected,
    /// Client-side rate-limit processing failed.
    RateLimit,
    /// A request timed out.
    Timeout,
    /// A connection could not be established.
    Connect,
    /// Another native transport failure occurred.
    Transport,
    /// A terminal non-success HTTP status was received.
    HttpStatus,
    /// An accepted response body could not be read.
    ResponseBody,
    /// An accepted response body could not be decoded.
    ResponseDecoding,
    /// An accepted response violated its declared contract.
    ResponseContract,
    /// Pagination processing failed.
    Pagination,
    /// An internal invariant failed safely.
    Internal,
}

/// Stable stage at which a managed operation failed.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureStage {
    /// Inert values or a prepared operation could not be constructed.
    Preparation,
    /// A local prerequisite failed before native dispatch.
    BeforeDispatch,
    /// Native dispatch began and no response head established a narrower fact.
    DuringDispatchOrUnknown,
    /// A response head was received but its status was rejected.
    RejectedResponse,
    /// An accepted response body could not be acquired completely.
    ResponseAcquisition,
    /// Acquired response data could not be converted to the declared output.
    ResponseConversion,
}

/// What Concord can truthfully establish about dispatch for a failed outcome.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchKnowledge {
    /// Native dispatch conclusively did not begin.
    NotDispatched,
    /// Dispatch began or the transport cannot prove that it did not.
    PossiblyDispatched,
    /// A response head was received.
    ResponseObserved,
}

/// Stable authentication failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthErrorKind {
    /// Required authentication material was unavailable.
    Unavailable,
    /// Initial acquisition failed.
    Acquisition,
    /// Refresh failed.
    Refresh,
    /// The server rejected the credential.
    Rejected,
    /// Authentication was configured inconsistently.
    InvalidConfiguration,
    /// A provider request exceeded its finite limit.
    RequestTooLarge,
    /// A provider response exceeded its finite limit.
    ResponseTooLarge,
    /// Credential material or its encoded native representation exceeded a
    /// core-owned authentication limit.
    MaterialTooLarge,
    /// Authentication failed an internal invariant.
    Internal,
}

/// Stable rate-limit failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitErrorKind {
    /// Rate limiting was configured inconsistently.
    InvalidConfiguration,
    /// A rate-limit key was invalid.
    InvalidKey,
    /// Bounded rate-limit state capacity was exhausted without discarding
    /// active history. This condition is recoverable.
    StateCapacity,
    /// Rate-limit authority could not be acquired.
    Acquire,
    /// Rate limiting failed an internal invariant.
    Internal,
}

/// Stable pagination failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaginationErrorKind {
    /// A requested page limit was invalid.
    InvalidLimit,
    /// A pagination controller failed.
    Controller,
    /// A controller declared invalid or excessive static metadata interests.
    InvalidControllerDescription,
    /// Declared page metadata exceeded its bounded retained-byte ceiling.
    PageMetadataCeilingExceeded,
    /// A declared continuation header was malformed or incomplete.
    InvalidContinuation,
    /// A continuation attempted to leave the fixed logical origin.
    CrossOriginContinuation,
    /// Pagination arithmetic overflowed.
    Overflow,
    /// Pagination prepared a request already seen in the operation.
    RepeatedRequest,
    /// `CompleteWithin` reached its page limit while continuation remained.
    CompletionLimitExceeded,
    /// The configured maximum page count was exceeded.
    PageCeilingExceeded,
    /// The hard aggregate visible-attempt ceiling was exhausted.
    AttemptCeilingExceeded,
    /// The configured maximum collected-item count was exceeded.
    CollectedItemCeilingExceeded,
    /// The configured aggregate accepted pagination-response byte limit was exceeded.
    CollectedResponseByteCeilingExceeded,
    /// The configured complete encoded pagination-identity byte limit was exceeded.
    IdentityStorageCeilingExceeded,
}

/// Stable client-construction failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientBuildErrorKind {
    /// A required generated input was absent.
    MissingInput,
    /// A configured built-in credential violated a finite material limit.
    InvalidCredential,
    /// The generated client's base URL was invalid.
    InvalidBaseUrl,
    /// HTTPS-only policy rejected the generated client's fixed HTTP base.
    HttpsRequired,
    /// The requested TLS capability was unavailable.
    TlsUnavailable,
    /// The explicit proxy could not be installed by the native transport.
    InvalidProxy,
    /// A configured root certificate was structurally invalid or could not be parsed.
    InvalidRootCertificate,
    /// A configured client identity was structurally invalid or could not be parsed.
    InvalidClientIdentity,
    /// The private native HTTP client could not be built.
    HttpClientBuild,
    /// Runtime configuration was invalid.
    RuntimeConfiguration,
    /// A required rate limiter was unavailable.
    LimiterUnavailable,
    /// Construction failed an internal invariant.
    Internal,
}

/// Safe category for one missing generated client-builder input.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientInputKind {
    /// A required public client value.
    Value,
    /// A required protected client secret.
    Secret,
    /// A required custom credential provider.
    CredentialProvider,
}

/// Stable reusable configuration failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationErrorKind {
    /// A scalar configuration value was invalid.
    InvalidValue,
    /// A configured route was invalid.
    InvalidRoute,
    /// A configured header was invalid.
    InvalidHeader,
    /// Authentication configuration was invalid.
    InvalidAuthentication,
    /// Rate-limit configuration was invalid.
    InvalidRateLimit,
    /// A proxy origin was invalid or unsafe.
    InvalidProxy,
    /// Configuration failed an internal invariant.
    Internal,
}
