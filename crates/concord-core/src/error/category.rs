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
    /// An attached [`CancellationToken`](crate::contracts::CancellationToken)
    /// stopped the operation. Local work stopped; the error's dispatch
    /// knowledge states what may already have reached the server.
    Cancelled,
    /// The caller's send cap (`CallOptions::cap_sends`) left no application
    /// handoff for this operation. A zero send cap produces this error before
    /// any body, credential, rate or transport work.
    SendCapExhausted,
}

/// Stable stage at which a managed operation failed.
///
/// Stages follow the managed pipeline in order. The stage names where the
/// failure was fixed; it is not a claim about dispatch, which is reported
/// separately by [`DispatchKnowledge`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureStage {
    /// Inert values, the public request head or the one prepared request body
    /// could not be constructed (pipeline phases 0–3).
    Preparation,
    /// Declared credential material could not be obtained before an attempt
    /// (pipeline phase 4).
    CredentialPreflight,
    /// Conjunctive quota admission failed (pipeline phase 5).
    RateAdmission,
    /// Another local prerequisite failed after admission and before native
    /// handoff: final credential freshness, native framing or placement
    /// (pipeline phase 6 and the local part of phase 7).
    BeforeDispatch,
    /// Native dispatch began and no response head established a narrower fact.
    DuringDispatchOrUnknown,
    /// A response head was received but its status was rejected.
    RejectedResponse,
    /// An accepted response body could not be acquired completely.
    ResponseAcquisition,
    /// Acquired response data could not be converted to the declared output.
    ResponseConversion,
    /// A traversal rejected a completely received page or its proposed
    /// continuation (controller failure, invalid or cyclic continuation,
    /// traversal budget or completion limit).
    PaginationDecision,
    /// A remote response was received and decoded completely, but local
    /// credential installation failed. Contact and status facts are retained.
    CredentialInstall,
    /// Consumption of an already acquired managed stream failed after the
    /// operation handed continuation ownership to the stream.
    StreamConsumption,
}

/// What Concord can truthfully establish about dispatch for a failed outcome.
///
/// The variants are ordered `NotDispatched < PossiblyDispatched <
/// ResponseObserved`. An operation's aggregate knowledge is the monotone join
/// over all of its attempts: it never regresses once a stronger fact is
/// settled.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchKnowledge {
    /// Native dispatch conclusively did not begin: either no application
    /// request was ever handed to the transport, or core holds a positive
    /// native proof that the handed-off request was not submitted. An error
    /// category (for example `Connect`) alone never establishes this fact.
    NotDispatched,
    /// Dispatch began or the transport cannot prove that it did not.
    PossiblyDispatched,
    /// A response head was received.
    ResponseObserved,
}

impl DispatchKnowledge {
    const fn rank(self) -> u8 {
        match self {
            Self::NotDispatched => 0,
            Self::PossiblyDispatched => 1,
            Self::ResponseObserved => 2,
        }
    }

    /// Returns the monotone join (the stronger of the two facts).
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Returns whether this knowledge admits that an application request may
    /// have reached the server (anything other than
    /// [`Self::NotDispatched`]). It is not evidence of a business effect.
    #[must_use]
    pub const fn may_have_reached_server(self) -> bool {
        !matches!(self, Self::NotDispatched)
    }
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
    /// A shared limiter found an incompatible active schema (or unverified
    /// legacy state) under the same policy ID and failed closed before any
    /// state use.
    SchemaConflict,
}

/// Stable pagination failure classification.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaginationErrorKind {
    /// A pagination controller failed.
    Controller,
    /// The controller's initial cursor, applied to a copy of the selected
    /// cursor slots, did not reproduce the current request's canonical public
    /// identity. Raised before the first dispatch.
    InvalidInitialCursor,
    /// The bound endpoint is not pageable (its effective effect is not safe).
    IneligibleEndpoint,
    /// A continuation attempted to change a fixed request fact other than the
    /// cursor-owned slots (effect, timeout, accepted status, rate-limit plan or
    /// page-stable body contract).
    ForbiddenContinuationChange,
    /// The caller send cap left no application handoff for the next page.
    SendCapExhausted,
    /// An attached cancellation signal terminated the traversal.
    Cancelled,
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
    /// A proposed continuation repeats a request already seen in the
    /// traversal (an immediate repeat or a longer cycle).
    RepeatedRequest,
    /// `collect_all` reached its effective page bound while the controller
    /// still offered a valid continuation. No partial success is returned.
    CompletionLimitExceeded,
    /// The aggregate application-handoff ceiling of the traversal was exhausted.
    AttemptCeilingExceeded,
    /// The traversal delivered-item ceiling would be exceeded by a complete
    /// page batch. Batches are never truncated to fit.
    CollectedItemCeilingExceeded,
    /// The aggregate response bytes acquired across every application attempt
    /// of the traversal (accepted pages, rejected statuses, retry responses and
    /// failed partial acquisitions) exceeded the traversal byte ceiling.
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
    /// A native HTTP configuration selection was invalid before construction.
    HttpConfiguration,
    /// Runtime configuration was invalid.
    RuntimeConfiguration,
    /// A required rate limiter was unavailable.
    LimiterUnavailable,
    /// A credential provider declared helper origins that exceed the
    /// configured capacity or include a cleartext origin.
    InvalidProviderOrigins,
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
