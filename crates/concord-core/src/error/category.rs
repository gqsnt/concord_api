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
    /// An application runtime hook failed.
    Hook,
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
    /// A received response head could not be observed.
    Observe,
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
    /// Pagination arithmetic overflowed.
    Overflow,
    /// Pagination prepared a request already seen in the operation.
    RepeatedRequest,
    /// A configured hard cap was exceeded.
    HardCapExceeded,
    /// The configured maximum page count was exceeded.
    PageCeilingExceeded,
    /// The configured maximum collected-item count was exceeded.
    CollectedItemCeilingExceeded,
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
