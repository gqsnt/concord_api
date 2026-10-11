//! Unstable integration surface for Concord-generated Rust.

#[doc(hidden)]
pub const GENERATED_CONTRACT_VERSION: u32 = crate::generated_contract::VERSION;

/// Verifies the semantic generated-code contract during const evaluation.
#[doc(hidden)]
pub const fn assert_generated_contract(expected: u32) {
    assert!(
        GENERATED_CONTRACT_VERSION == expected,
        "concord macro/core contract mismatch"
    );
}

#[doc(hidden)]
pub use http::{HeaderName, Method};
#[doc(hidden)]
pub use url::Url;

#[doc(hidden)]
pub use crate::atom::{AtomEncoder, BoundedAtomWriter, OrderedValues, RequestAtom};
#[doc(hidden)]
pub use crate::auth::Auth;
#[doc(hidden)]
pub use crate::body::{
    BodyDescription, EmptyBody, OptionalRequestRepresentation, PageRequestRepresentation,
    PreparedBody, RequestBodyContext, RequestRepresentation,
};
#[doc(hidden)]
pub use crate::client::ClientCore;
#[doc(hidden)]
pub use crate::controls::{AnyCancelled, RequestControlState};
#[doc(hidden)]
pub use crate::endpoint::{
    AbsentStatus, AcceptedStatus, Effect, EffectSource, EndpointCapabilities, EndpointDescription,
    EndpointId, RequestBodyCategory, RequestPlan, ResponseAcquisition, StatusSource,
    TimeoutSelection,
};
#[doc(hidden)]
pub use crate::error::{ApiError, ClientBuildError, ClientInputKind};
#[doc(hidden)]
pub use crate::hooks::DebugLevel;
#[doc(hidden)]
pub use crate::limits::{
    MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
    MAX_REQUEST_URL_BYTES,
};
#[doc(hidden)]
pub use crate::pagination::{
    ControllerCursor, ControllerInit, PageItem, PageableEndpoint, collect_all_request,
    collect_prefix_request, pages_request,
};
#[doc(hidden)]
pub use crate::rate_limit::{
    RateKeyEncoder, RateKeyPart, RateLimitDeclaration, RateLimitId, RateLimitKey, RateLimitKeyPart,
    RateLimitPolicy, RateLimitWindow,
};
#[doc(hidden)]
pub use crate::request::{
    EndpointSpec, HeadBuilder, HeadEmissions, NO_EMISSION, OccurrenceSink, OwnerProjection,
    PatchFamily, PatchKind, PatchOp, PreparedHead, ProgramSegment, ProgramVisitor, RequestMeta,
    RequestOptions, TimeoutOverride, apply_head_program, surviving_keys, visit_extended_program,
};
#[doc(hidden)]
pub use crate::response::{
    ApiResponse, Buffered, ExpectedAbsence, HeadAbsence, OptionalOutcome, ResponseRepresentation,
};
#[doc(hidden)]
pub use crate::runtime::defaults::{
    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS, DEFAULT_PAGINATION_IDENTITY_BYTES,
    DEFAULT_PAGINATION_MAX_ITEMS, DEFAULT_PAGINATION_MAX_PAGES,
    DEFAULT_PAGINATION_TOTAL_RESPONSE_BYTES, DEFAULT_PROVIDER_MAX_ORIGINS,
    DEFAULT_PROVIDER_MAX_SENDS, DEFAULT_PROVIDER_REQUEST_BYTES, DEFAULT_PROVIDER_RESPONSE_BYTES,
    DEFAULT_PROVIDER_TIMEOUT_SECONDS, DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
    DEFAULT_RATE_LIMIT_KEY_BYTES, DEFAULT_REQUEST_BODY_BYTES, DEFAULT_RESPONSE_BODY_BYTES,
    DEFAULT_STATUS_BODY_BYTES, DEFAULT_STREAM_FRAME_BYTES, DEFAULT_STREAM_IDLE_SECONDS,
    DEFAULT_STREAM_LIFETIME_SECONDS, DEFAULT_STREAM_MAX_RECORDS, DEFAULT_STREAM_RESPONSE_BYTES,
};

/// Constructs the absent value of a head-only expected-absence result.
///
/// Implemented only through the sealed [`OptionalOutcome`] relationship, so
/// it is available for `Option<T>` (and aliases of it) and nothing else.
/// Core itself constructs absence on a declared status; generated code only
/// needs this for type assertions and documentation examples.
#[doc(hidden)]
#[must_use]
pub fn absent_outcome<O>() -> O
where
    O: OptionalOutcome,
{
    crate::response::absent_outcome::<O>()
}

/// Wraps one present value into a head-only expected-absence result.
#[doc(hidden)]
#[must_use]
pub fn present_outcome<O>(value: O::Present) -> O
where
    O: OptionalOutcome,
{
    crate::response::present_outcome::<O>(value)
}

/// Compile-time validation of a representation's static `Accept` constant.
///
/// Generated code emits, once per endpoint response representation,
/// `const _: () = __private::assert_response_accept(<R as ResponseRepresentation<O>>::ACCEPT);`
/// so an invalid value (for example one containing a line break) is a
/// compile error rather than a runtime surprise.
///
/// # Panics
///
/// Panics — at compile time in a constant — when the value is not a valid
/// HTTP `Accept` value.
#[doc(hidden)]
pub const fn assert_response_accept(accept: Option<&'static str>) {
    if let Some(accept) = accept {
        assert!(
            crate::response::accept_is_valid(accept),
            "invalid static response representation Accept value"
        );
    }
}

/// Runtime validation of a representation's static `Accept` constant at
/// client build.
///
/// Generated builders may call this once per endpoint response
/// representation; it runs no representation code.
#[doc(hidden)]
pub fn validate_response_accept<R, Output>() -> Result<(), ClientBuildError>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    match R::ACCEPT {
        Some(accept) if !crate::response::accept_is_valid(accept) => Err(ClientBuildError::new(
            crate::error::ClientBuildErrorKind::RuntimeConfiguration,
        )),
        _ => Ok(()),
    }
}

/// Creates one allocation-free, compile-time-validated generated endpoint ID.
#[doc(hidden)]
#[must_use]
pub const fn endpoint_id_from_static(value: &'static str) -> EndpointId {
    EndpointId::from_static(value)
}

/// Generated-private owner for one credential source selection.
///
/// One value is created per declared credential when the generated client is
/// built and shared by every clone of that client. Every method is
/// synchronous and local: none performs network work, revokes a remote
/// session or recalls already dispatched requests. No lock is held across
/// application code; poisoning or counter exhaustion fails closed with a
/// value-free [`AuthError`](crate::auth::AuthError).
///
/// Bridge for the macro:
///
/// * construction: [`Self::static_value`], [`Self::endpoint`],
///   [`Self::provider`] / [`Self::provider_arc`] (fallible: the provider's
///   declared helper origins are validated at build);
/// * placements: [`Self::credential`];
/// * `client.credentials().clear_<name>()?` → [`Self::clear`];
/// * `client.credentials().replace_<name>(material)?` → [`Self::replace`]
///   (or [`Self::replace_result`] for built-in fallible constructors);
/// * `store_as_<name>()` → [`store_request`].
#[doc(hidden)]
pub struct GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    selector: std::sync::Arc<crate::auth::SlotSelector<M>>,
}

impl<M> Clone for GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    fn clone(&self) -> Self {
        Self {
            selector: std::sync::Arc::clone(&self.selector),
        }
    }
}

impl<M> GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    /// Creates a static source whose initial material is installed version 1.
    #[doc(hidden)]
    pub fn static_value(core: &ClientCore, value: M) -> Self {
        Self {
            selector: crate::auth::SlotSelector::static_value_in_context(
                value,
                core.auth_failure_cooldown_cap(),
                core.credential_context().clone(),
            ),
        }
    }

    /// Creates an initially empty endpoint-backed source.
    #[doc(hidden)]
    pub fn endpoint(core: &ClientCore) -> Self {
        Self {
            selector: crate::auth::SlotSelector::endpoint_in_context(
                core.auth_failure_cooldown_cap(),
                core.credential_context().clone(),
            ),
        }
    }

    /// Creates a provider source, validating the provider's declared helper
    /// origins against the client's configured capacity. `field` names the
    /// generated provider input for the build error.
    #[doc(hidden)]
    pub fn provider<P>(
        core: &ClientCore,
        field: &'static str,
        provider: P,
    ) -> Result<Self, ClientBuildError>
    where
        P: crate::auth::CredentialProvider<Material = M>,
    {
        crate::auth::SlotSelector::provider_in_context(
            provider,
            core.auth_failure_cooldown_cap(),
            core.credential_context().clone(),
            core.provider_max_origins(),
        )
        .map(|selector| Self { selector })
        .map_err(|_: crate::auth::OriginSetError| ClientBuildError::invalid_provider_origins(field))
    }

    /// Shared-provider form of [`Self::provider`].
    #[doc(hidden)]
    pub fn provider_arc(
        core: &ClientCore,
        field: &'static str,
        provider: std::sync::Arc<dyn crate::auth::CredentialProvider<Material = M>>,
    ) -> Result<Self, ClientBuildError> {
        crate::auth::SlotSelector::provider_arc_in_context(
            provider,
            core.auth_failure_cooldown_cap(),
            core.credential_context().clone(),
            core.provider_max_origins(),
        )
        .map(|selector| Self { selector })
        .map_err(|_: crate::auth::OriginSetError| ClientBuildError::invalid_provider_origins(field))
    }

    #[doc(hidden)]
    pub fn credential(&self) -> crate::auth::CredentialRef<M> {
        crate::auth::CredentialRef::new(std::sync::Arc::clone(&self.selector))
    }

    #[doc(hidden)]
    pub fn endpoint_handle(&self) -> crate::auth::CredentialHandle<M> {
        crate::auth::CredentialHandle::new(std::sync::Arc::clone(&self.selector))
    }

    /// Manual local clear: advances the installation intent even when empty,
    /// removes ready material and supersedes in-flight acquisition.
    #[doc(hidden)]
    pub fn clear(&self) -> Result<(), crate::auth::AuthError> {
        self.selector.clear()
    }

    /// Manual local replacement: advances the installation intent and
    /// installs `value` as a new version.
    #[doc(hidden)]
    pub fn replace(
        &self,
        value: M,
    ) -> Result<crate::auth::CredentialVersion, crate::auth::AuthError> {
        self.selector.replace(value)
    }

    /// [`Self::replace`] for a built-in fallible material constructor; a
    /// construction error leaves the current material and version unchanged.
    #[doc(hidden)]
    pub fn replace_result(
        &self,
        value: Result<M, crate::auth::CredentialMaterialError>,
    ) -> Result<crate::auth::CredentialVersion, crate::auth::AuthError> {
        self.selector.replace_result(value)
    }

    /// Complete provider replacement. The replacement's declared origins are
    /// validated against the client's capacity; pending intents of the old
    /// provider are superseded.
    #[doc(hidden)]
    pub fn replace_provider<P>(&self, provider: P) -> Result<(), crate::auth::AuthError>
    where
        P: crate::auth::CredentialProvider<Material = M>,
    {
        self.selector.replace_provider(provider)
    }

    #[doc(hidden)]
    pub fn is_set(&self) -> bool {
        self.selector.is_set()
    }

    /// Returns the currently installed, unrejected version, if any.
    #[doc(hidden)]
    pub fn installed_version(&self) -> Option<crate::auth::CredentialVersion> {
        self.selector.installed_version()
    }
}

/// Wraps one bound source-endpoint request as the `store_as_<credential>()`
/// terminal for `credential`.
///
/// The request keeps every control it already carries; the terminal executes
/// it through the ordinary managed executor on first poll. The endpoint's
/// declared output must be exactly the credential material type.
#[doc(hidden)]
#[must_use]
pub fn store_request<E, M>(
    request: crate::Request<E>,
    credential: &GeneratedCredential<M>,
) -> crate::StoreRequest
where
    E: EndpointSpec<Output = M>,
    M: crate::auth::CredentialMaterial,
{
    crate::execute::store_request(request, std::sync::Arc::clone(&credential.selector))
}

/// Borrows a generated endpoint through the opaque request wrapper without
/// starting execution.
#[doc(hidden)]
pub fn inspect_endpoint<E, T>(request: &crate::Request<E>, inspect: impl FnOnce(&E) -> T) -> T {
    inspect(&request.endpoint)
}

/// Creates the ordinary generated-client HTTP configuration baseline.
#[doc(hidden)]
#[must_use]
pub fn default_http_config() -> crate::http::HttpConfigBuilder {
    crate::http::HttpConfigBuilder::new()
}

/// Creates the ordinary generated-client runtime configuration baseline.
#[doc(hidden)]
#[must_use]
pub fn default_runtime_config() -> crate::runtime::RuntimeConfigBuilder {
    crate::runtime::RuntimeConfigBuilder::new()
}

/// Internal generated-builder selection state.
///
/// `UseDeclaration` means that `build` must either evaluate the declaration
/// default or report a missing required value. `Set` preserves the exact
/// effective value, including an outer Concord-owned `Option` when present.
#[doc(hidden)]
pub enum Override<T> {
    /// Select declaration behavior at the next build boundary.
    UseDeclaration,
    /// Use one caller-selected effective value.
    Set(T),
}

/// Applies generated static rate-limit requirements without constructing a
/// custom observer.
#[doc(hidden)]
pub fn prepare_generated_rate_limits(
    runtime: crate::runtime::RuntimeConfigBuilder,
    requires_limiter: bool,
) -> Result<crate::runtime::RuntimeConfigBuilder, ClientBuildError> {
    runtime.prepare_generated_rate_limits(requires_limiter)
}

/// Builds one private shared core for generated client storage.
#[doc(hidden)]
pub fn build_client_core(
    fixed_base: &str,
    http: crate::http::HttpConfigBuilder,
    runtime: crate::runtime::RuntimeConfigBuilder,
) -> Result<ClientCore, ClientBuildError> {
    ClientCore::build(fixed_base, http, runtime)
}

/// Produces a safe generated-builder error for one missing required input.
#[doc(hidden)]
pub const fn missing_input_error(
    field: &'static str,
    input_kind: ClientInputKind,
) -> ClientBuildError {
    ClientBuildError::missing_input(field, input_kind)
}

/// Produces one safe generated-builder error accumulating every missing
/// required input (values, secrets, providers) in declaration order.
#[doc(hidden)]
pub fn missing_inputs_error(missing: Vec<(&'static str, ClientInputKind)>) -> ClientBuildError {
    ClientBuildError::missing_inputs(missing)
}

/// Produces a safe generated-builder error for one invalid single-string credential input.
#[doc(hidden)]
pub fn credential_input_error(
    field: &'static str,
    _error: crate::auth::CredentialMaterialError,
) -> ClientBuildError {
    ClientBuildError::invalid_credential(field)
}

/// Produces a safe generated-builder error for one invalid Basic credential input.
#[doc(hidden)]
pub fn basic_credential_input_error(
    username_field: &'static str,
    password_field: &'static str,
    error: crate::auth::CredentialMaterialError,
) -> ClientBuildError {
    let field = match error.kind() {
        crate::auth::CredentialMaterialErrorKind::BasicPasswordTooLarge => password_field,
        crate::auth::CredentialMaterialErrorKind::SecretStringTooLarge
        | crate::auth::CredentialMaterialErrorKind::CredentialSecretTooLarge
        | crate::auth::CredentialMaterialErrorKind::BasicUsernameTooLarge
        | crate::auth::CredentialMaterialErrorKind::BasicUsernameContainsColon => username_field,
    };
    ClientBuildError::invalid_credential(field)
}

/// Creates one side-effect-free stable pending request wrapper.
#[doc(hidden)]
#[must_use]
pub fn request<E>(core: &ClientCore, endpoint: E) -> crate::Request<E> {
    crate::Request::new(core.clone(), endpoint)
}

/// Creates one inert unbound-intent plan through real public-head preparation.
///
/// Planning never dispatches (EXEC-03): every failure reports
/// `FailureStage::Preparation`, `DispatchKnowledge::NotDispatched` and no
/// operation identity.
#[doc(hidden)]
pub fn plan_unbound_request(
    core: &ClientCore,
    description: &EndpointDescription,
    response_accept: Option<&'static str>,
    prepare_head: impl FnOnce(HeadBuilder) -> Result<PreparedHead, ApiError>,
) -> Result<RequestPlan, ApiError> {
    plan_unbound_request_inner(core, description, response_accept, prepare_head)
        .map_err(ApiError::before_operation)
}

fn plan_unbound_request_inner(
    core: &ClientCore,
    description: &EndpointDescription,
    response_accept: Option<&'static str>,
    prepare_head: impl FnOnce(HeadBuilder) -> Result<PreparedHead, ApiError>,
) -> Result<RequestPlan, ApiError> {
    let meta = RequestMeta::from_description(description);
    let builder = HeadBuilder::new(
        core.base_url(),
        meta,
        &RequestOptions::new(),
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    );
    let mut head = prepare_head(builder)?;
    head.validate_auth_context(core.credential_context())?;
    if head.endpoint_id() != description.endpoint_id() || head.method() != description.method() {
        return Err(ApiError::classified(
            description.endpoint_id().clone(),
            description.method().clone(),
            crate::error::ErrorCategory::Internal,
        ));
    }
    head.validate_rate_limit_key_limit(core.rate_limit_key_limit())?;
    // Planning never prepares a body (the exact Content-Type is deferred to
    // preparation), but the response representation's static Accept constant
    // is part of every finalized head and consumes the same aggregate budget
    // as execution.
    head.apply_media_headers(response_accept, None, false)?;
    head.validate_auth()?;
    if head.auth().is_empty() {
        core.preflight_url(head.endpoint_id(), head.method(), head.logical_url())?;
    } else {
        core.preflight_protected_url(head.endpoint_id(), head.method(), head.logical_url())?;
    }
    let public_headers = crate::redaction::SanitizedHeaderMap::from_headers_with_sensitive(
        head.public_headers(),
        Some(head.pagination_sensitive_headers()),
    );
    Ok(RequestPlan::new(
        description.clone(),
        std::sync::Arc::from(head.observed_url()),
        public_headers,
        head.timeout(),
        0,
        core.rate_plan_summary(head.rate_limit_plan()),
    ))
}

/// Plans one already-bound inert generated request without inspecting or
/// materializing its body and without allocating an execution lifecycle.
///
/// Planning never dispatches (EXEC-03): every failure reports
/// `FailureStage::Preparation`, `DispatchKnowledge::NotDispatched` and no
/// operation identity.
#[doc(hidden)]
pub fn request_plan<E>(
    request: &crate::Request<E>,
    description: &EndpointDescription,
) -> Result<RequestPlan, ApiError>
where
    E: EndpointSpec,
{
    request_plan_inner(request, description).map_err(ApiError::before_operation)
}

fn request_plan_inner<E>(
    request: &crate::Request<E>,
    description: &EndpointDescription,
) -> Result<RequestPlan, ApiError>
where
    E: EndpointSpec,
{
    let meta = request.endpoint.request_meta();
    let builder = HeadBuilder::new(
        request.core.base_url(),
        meta,
        &request.options,
        request.core.default_debug(),
        request.core.default_runtime_endpoint_timeout(),
    );
    let mut head = request.endpoint.prepare_head(builder)?;
    head.validate_auth_context(request.core.credential_context())?;
    head.validate_rate_limit_key_limit(request.core.rate_limit_key_limit())?;
    head.apply_media_headers(
        <E::Response as ResponseRepresentation<E::Output>>::ACCEPT,
        None,
        false,
    )?;
    head.validate_auth()?;
    if head.auth().is_empty() {
        request
            .core
            .preflight_url(head.endpoint_id(), head.method(), head.logical_url())?;
    } else {
        request.core.preflight_protected_url(
            head.endpoint_id(),
            head.method(),
            head.logical_url(),
        )?;
    }
    let public_headers = crate::redaction::SanitizedHeaderMap::from_headers_with_sensitive(
        head.public_headers(),
        Some(head.pagination_sensitive_headers()),
    );
    Ok(RequestPlan::new(
        description.clone(),
        std::sync::Arc::from(head.observed_url()),
        public_headers,
        head.timeout(),
        0,
        request.core.rate_plan_summary(head.rate_limit_plan()),
    ))
}

/// Generated named-request bridge for `request.timeout(duration)`.
///
/// Forwards to the inherent [`crate::Request::timeout`]: replaces only the
/// selected finite timeout; caps are retained. Generated wrappers may call
/// either this function or the inherent method.
#[doc(hidden)]
pub fn request_timeout<E>(
    request: crate::Request<E>,
    duration: std::time::Duration,
) -> Result<crate::Request<E>, crate::contracts::ControlError> {
    request.timeout(duration)
}

/// Generated named-request bridge for `request.reset_timeout()`
/// ([`crate::Request::reset_timeout`]); caps are retained.
#[doc(hidden)]
#[must_use]
pub fn request_reset_timeout<E>(request: crate::Request<E>) -> crate::Request<E> {
    request.reset_timeout()
}

/// Generated named-request bridge for `request.cancel_with(token)`
/// ([`crate::Request::cancel_with`]); at most eight distinct signals.
#[doc(hidden)]
pub fn request_cancel_with<E>(
    request: crate::Request<E>,
    token: crate::contracts::CancellationToken,
) -> Result<crate::Request<E>, crate::contracts::ControlError> {
    request.cancel_with(token)
}

/// Generated named-request bridge for `request.with_options(&options)`
/// ([`crate::Request::with_options`]); caps intersect componentwise.
#[doc(hidden)]
#[must_use]
pub fn request_with_options<E>(
    request: crate::Request<E>,
    options: &crate::contracts::CallOptions,
) -> crate::Request<E> {
    request.with_options(options)
}

/// Generated named-request bridge for `request.control_summary()`
/// ([`crate::Request::control_summary`]); the declared timeout comes from the
/// endpoint's effective metadata.
#[doc(hidden)]
#[must_use]
pub fn request_control_summary<E>(request: &crate::Request<E>) -> crate::contracts::ControlSummary
where
    E: EndpointSpec,
{
    request.control_summary()
}

/// Selects universal response metadata for one generated named request wrapper.
#[doc(hidden)]
#[must_use]
pub fn response_request<E>(request: crate::Request<E>) -> crate::ResponseRequest<E::Output>
where
    E: EndpointSpec,
{
    crate::execute::response_request(request)
}

/// Selects complete bounded bytes for one generated named request wrapper.
#[doc(hidden)]
#[must_use]
pub fn accepted_bytes_request<E>(request: crate::Request<E>) -> crate::AcceptedBytesRequest
where
    E: EndpointSpec,
    <E::Response as ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    crate::execute::accepted_bytes_request(request)
}

/// Consumes a pending request, maps its endpoint value, and preserves every
/// generic request-local and runtime field without starting execution.
#[doc(hidden)]
#[must_use]
pub fn map_endpoint<E, F>(request: crate::Request<E>, map: F) -> crate::Request<E>
where
    F: FnOnce(E) -> E,
{
    let crate::Request {
        core,
        endpoint,
        options,
    } = request;
    crate::Request {
        core,
        endpoint: map(endpoint),
        options,
    }
}

/// Binds one runtime endpoint at a generated owner projection
/// (`owner.bind(endpoint)`); see [`crate::RuntimeEndpoint`].
#[doc(hidden)]
pub fn bind_runtime_endpoint<R, O>(
    projection: std::sync::Arc<dyn OwnerProjection>,
    endpoint: crate::request::RuntimeEndpoint<R, O>,
) -> Result<crate::request::BoundRuntimeRequest<R, O>, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    crate::request::bind_runtime_at_owner(&projection, endpoint).map_err(ApiError::before_operation)
}

/// Plans one runtime endpoint at a generated owner projection without
/// binding it (`owner.plan_runtime(&endpoint)`).
#[doc(hidden)]
pub fn plan_runtime_endpoint<R, O>(
    projection: std::sync::Arc<dyn OwnerProjection>,
    endpoint: &crate::request::RuntimeEndpoint<R, O>,
) -> Result<RequestPlan, ApiError>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    crate::request::plan_runtime_at_owner(&projection, endpoint).map_err(ApiError::before_operation)
}

/// Builds the head-only expected absence of one generated endpoint from its
/// macro-validated status codes (`absent on 404;`).
///
/// # Panics
///
/// Panics only if the codes are not a valid final-status set, which the
/// declaration compiler rejects before emitting this call.
#[doc(hidden)]
#[must_use]
pub fn expected_absence<O, R>(codes: &'static [u16]) -> ExpectedAbsence<O, HeadAbsence<R>>
where
    O: OptionalOutcome,
    R: ResponseRepresentation<O::Present, Driver = Buffered>,
{
    let statuses = AbsentStatus::codes(
        codes
            .iter()
            .map(|code| http::StatusCode::from_u16(*code).expect("macro-validated status")),
    )
    .expect("macro-validated absence statuses");
    ExpectedAbsence::new(statuses)
}

/// Derives one generated endpoint's runtime description from its client's
/// built contract (the single source of meaning for executor facts).
///
/// # Panics
///
/// Panics with a contract-mismatch message if the contract has no endpoint
/// with this operational ID, which indicates a macro/core version mismatch.
#[doc(hidden)]
#[must_use]
pub fn endpoint_description(
    contract: &'static crate::inspection::ContractDescription,
    endpoint_id: &str,
) -> EndpointDescription {
    EndpointDescription::from_contract(contract, endpoint_id)
        .expect("concord macro/core contract mismatch: generated endpoint missing from contract")
}
