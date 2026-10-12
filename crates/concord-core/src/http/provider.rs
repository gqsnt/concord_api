use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use url::Url;

#[cfg(test)]
use concord_test_support::executor::ProviderChannel;
#[cfg(test)]
use concord_test_support::script::BoundaryRequest;

use crate::auth::{AuthError, CredentialReason, ProviderOrigin};
use crate::error::{
    AuthErrorKind, DispatchKnowledge, ErrorCategory, ProviderAcquisitionFacts,
    ProviderAcquisitionOutcome, reqwest_category,
};
use crate::hooks::{OperationEvent, OperationObserver, ProviderAcquisitionFinished};
use crate::operation::{OperationDeadline, OperationId};

use super::build::TlsCapability;
use super::response_progress::{ResponseChunkError, ResponseChunkProgress, next_nonempty_chunk};

/// Private provider HTTP owner with an independent client and pool.
pub(super) struct ProviderHttpInner {
    client: reqwest::Client,
    tls: TlsCapability,
    request_limit: NonZeroUsize,
    response_limit: NonZeroUsize,
    max_sends: u32,
    timeout: Duration,
    failure_cooldown_cap: Duration,
    #[cfg(test)]
    development: Option<ProviderChannel>,
}

/// An isolated, bounded provider-HTTP capability borrowed for one credential
/// acquisition or refresh (AUTH-02).
///
/// **Authority.** Every request is checked, before any body-factory call or
/// transport work, against the exact origin set the providing credential
/// declared through
/// [`CredentialProvider::http_origins`](crate::contracts::CredentialProvider::http_origins);
/// an undeclared origin, a non-HTTPS URL or a URL carrying user information is
/// refused. A capability that is not bound to a provider acquisition grants no
/// destinations at all. The helper never applies application credential
/// placements, never follows redirects, stores no cookies and presents no
/// application client identity.
///
/// **Work and time bounds.** One acquisition may perform at most the
/// configured number of sends (4 by default, hard maximum 64) and runs under
/// `min(remaining operation deadline, provider timeout)` (30 seconds by
/// default), further narrowed by any shorter request timeout. Request and
/// response bodies are limited (1 MiB each by default). Provider response
/// acquisition suppresses empty transport chunks, applies the response-wide
/// empty-chunk ceiling, and yields cooperatively during ready-empty runs.
///
/// **Cancellation and sensitivity.** Dropping the provider future cancels
/// later polling but does not roll back bytes already sent or other completed
/// external effects. Provider URLs, headers, bodies, and responses may be
/// sensitive; applications and providers remain responsible for their own
/// logging and storage.
///
/// **Provider journal (EXEC-01).** Each acquisition or refresh is its own
/// operation, with an identity distinct from the application operation that
/// led it. A bounded, value-free provider journal records the requests this
/// helper hands to the provider transport, the last provider response status,
/// and the provider's own monotone dispatch knowledge. It links to the
/// leading application operation. The settled
/// [`ProviderAcquisitionFacts`] are reported through
/// [`ApiError::provider_acquisition`](crate::contracts::ApiError::provider_acquisition),
/// [`ExecutionMeta::provider_acquisition`](crate::contracts::ExecutionMeta::provider_acquisition)
/// and the
/// [`OperationEvent::ProviderAcquisitionFinished`](crate::contracts::OperationEvent::ProviderAcquisitionFinished)
/// observation. Because provider traffic has this separate journal, an
/// application request reported as not dispatched does not assert that no
/// auxiliary provider network activity occurred.
///
/// Providers can inspect the active semantic request-body ceiling through
/// [`ProviderHttp::request_body_limit`] without gaining access to native
/// transport state. There is no API to attach an application credential
/// placement to a provider request:
///
/// ```compile_fail
/// use concord_core::contracts::{Auth, ProviderRequest};
/// fn attach(request: ProviderRequest, placement: Auth) -> ProviderRequest {
///     request.auth(placement)
/// }
/// ```
#[derive(Clone, Copy)]
pub struct ProviderHttp<'a> {
    inner: Option<&'a ProviderHttpInner>,
    deadline: Option<OperationDeadline>,
    grant: Option<ProviderGrant<'a>>,
    scope: Option<ProviderScope<'a>>,
}

/// The exact destinations, remaining sends and journal of one provider
/// acquisition.
#[derive(Clone, Copy)]
struct ProviderGrant<'a> {
    origins: &'a [ProviderOrigin],
    sends: &'a ProviderSendBudget,
    journal: &'a ProviderJournal,
}

/// The application operation on whose behalf credential work runs: its
/// identity (the parent of any provider acquisition it leads), the client's
/// observer, and the link through which that operation's journal learns
/// about every acquisition it led or joined.
#[derive(Clone, Copy)]
pub(crate) struct ProviderScope<'a> {
    parent: OperationId,
    observer: &'a dyn OperationObserver,
    link: &'a ProviderLink,
}

impl<'a> ProviderScope<'a> {
    pub(crate) const fn new(
        parent: OperationId,
        observer: &'a dyn OperationObserver,
        link: &'a ProviderLink,
    ) -> Self {
        Self {
            parent,
            observer,
            link,
        }
    }

    pub(crate) const fn parent(&self) -> OperationId {
        self.parent
    }

    /// Records facts of an acquisition this operation led or joined.
    pub(crate) fn link(&self, facts: ProviderAcquisitionFacts) {
        self.link.record(facts);
    }

    /// Records a settled acquisition this operation led and emits its single
    /// observation.
    pub(crate) fn settle(
        &self,
        journal: &ProviderJournal,
        facts: ProviderAcquisitionFacts,
        auth_kind: Option<AuthErrorKind>,
    ) {
        self.link.record(facts);
        crate::hooks::observe(
            self.observer,
            OperationEvent::ProviderAcquisitionFinished(ProviderAcquisitionFinished {
                facts,
                auth_kind,
                elapsed: journal.elapsed(),
            }),
        );
    }
}

/// Collects the most recent provider acquisition one application operation
/// led or joined. Bounded: it holds one `Copy` fact set, and its lock is never
/// held across an await or application code.
#[derive(Default)]
pub(crate) struct ProviderLink {
    latest: Mutex<Option<ProviderAcquisitionFacts>>,
}

impl ProviderLink {
    fn record(&self, facts: ProviderAcquisitionFacts) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(facts);
    }

    /// Takes the facts recorded since the last take.
    pub(crate) fn take(&self) -> Option<ProviderAcquisitionFacts> {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// Bounded journal of one provider acquisition's helper traffic (EXEC-01).
///
/// Every fact is monotone and value-free: a saturating handoff count, the
/// last provider response status and the provider's own dispatch knowledge
/// (`NotDispatched < PossiblyDispatched < ResponseObserved`). A handoff is
/// recorded immediately before the native provider send, so a request refused
/// before transport never counts.
pub(crate) struct ProviderJournal {
    operation_id: OperationId,
    parent_operation_id: Option<OperationId>,
    reason: CredentialReason,
    started: tokio::time::Instant,
    handoffs: AtomicU32,
    last_status: AtomicU16,
    knowledge: AtomicU8,
}

impl ProviderJournal {
    const NOT_DISPATCHED: u8 = 0;
    const POSSIBLY_DISPATCHED: u8 = 1;
    const RESPONSE_OBSERVED: u8 = 2;

    pub(crate) fn new(
        operation_id: OperationId,
        parent_operation_id: Option<OperationId>,
        reason: CredentialReason,
    ) -> Self {
        Self {
            operation_id,
            parent_operation_id,
            reason,
            started: OperationDeadline::now(),
            handoffs: AtomicU32::new(0),
            last_status: AtomicU16::new(0),
            knowledge: AtomicU8::new(Self::NOT_DISPATCHED),
        }
    }

    /// Records one helper request handed to the native provider transport.
    /// Until a head arrives its outcome is unknown, so provider knowledge
    /// rises to at least `PossiblyDispatched`.
    fn record_handoff(&self) {
        let _ = self
            .handoffs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                Some(count.saturating_add(1))
            });
        self.knowledge
            .fetch_max(Self::POSSIBLY_DISPATCHED, Ordering::AcqRel);
    }

    /// Irrevocably records a received provider response head.
    fn record_head(&self, status: StatusCode) {
        self.last_status.store(status.as_u16(), Ordering::Release);
        self.knowledge
            .fetch_max(Self::RESPONSE_OBSERVED, Ordering::AcqRel);
    }

    fn elapsed(&self) -> Duration {
        OperationDeadline::now().saturating_duration_since(self.started)
    }

    /// Snapshots the journal with the acquisition's settled outcome.
    pub(crate) fn facts(&self, outcome: ProviderAcquisitionOutcome) -> ProviderAcquisitionFacts {
        let knowledge = match self.knowledge.load(Ordering::Acquire) {
            Self::NOT_DISPATCHED => DispatchKnowledge::NotDispatched,
            Self::POSSIBLY_DISPATCHED => DispatchKnowledge::PossiblyDispatched,
            _ => DispatchKnowledge::ResponseObserved,
        };
        ProviderAcquisitionFacts::new(
            self.operation_id,
            self.parent_operation_id,
            self.reason,
            self.handoffs.load(Ordering::Acquire),
            StatusCode::from_u16(self.last_status.load(Ordering::Acquire)).ok(),
            knowledge,
            outcome,
        )
    }
}

/// Per-acquisition provider-helper send budget, shared by every copy of one
/// bound [`ProviderHttp`].
pub(crate) struct ProviderSendBudget {
    remaining: AtomicU32,
}

impl ProviderSendBudget {
    pub(crate) const fn new(maximum: u32) -> Self {
        Self {
            remaining: AtomicU32::new(maximum),
        }
    }

    /// Creates one fresh budget with the capability's configured maximum.
    pub(crate) fn for_http(http: &ProviderHttp<'_>) -> Self {
        Self::new(http.inner.map_or(0, |inner| inner.max_sends))
    }

    fn try_take(&self) -> bool {
        self.remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
    }

    fn remaining(&self) -> u32 {
        self.remaining.load(Ordering::Acquire)
    }
}

/// One bounded provider HTTP request.
pub struct ProviderRequest {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: ProviderBody,
    timeout: Option<Duration>,
    response_limit: Option<NonZeroUsize>,
}

impl ProviderRequest {
    /// Creates an empty provider request.
    #[must_use]
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: HeaderMap::new(),
            body: ProviderBody::Empty,
            timeout: None,
            response_limit: None,
        }
    }

    /// Inserts or replaces one provider request header.
    #[must_use]
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Replaces the provider request body.
    #[must_use]
    pub fn body(mut self, body: ProviderBody) -> Self {
        self.body = body;
        self
    }

    /// Sets the request-local provider timeout.
    #[must_use]
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Reduces the finite provider response limit for this request.
    #[must_use]
    pub fn response_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.response_limit = Some(bytes);
        self
    }
}

/// A replay-stable body for one provider HTTP operation.
pub enum ProviderBody {
    /// No request body.
    Empty,
    /// Complete retained bytes.
    Bytes(Bytes),
    /// A synchronous factory invoked once for this provider operation.
    Factory(Arc<dyn ProviderBodyFactory>),
}

/// Produces complete provider request bytes.
///
/// `create` runs once for each provider send that carries a
/// [`ProviderBody::Factory`], after the destination passed the provider's
/// origin and HTTPS checks and before any bytes are sent.
///
/// # Extension contract
///
/// * **Ownership.** The factory is shared behind an `Arc` by the provider
///   request; the returned bytes are owned by that one send.
/// * **Concurrency.** Calls for independent provider operations may overlap.
/// * **Cancellation.** The synchronous callback cannot be preempted; its bytes
///   are checked against the provider limit and operation deadline after
///   return. Cancellation prevents later work only.
/// * **Input sensitivity.** Inputs, output and factory state may be
///   sensitive; implementations own logging and storage.
/// * **Errors.** A returned [`AuthError`] fails that provider send and is
///   returned to the provider; output above the provider request limit fails
///   with `AuthErrorKind::RequestTooLarge` before dispatch.
/// * **Resource obligations.** Implementations must bound CPU, memory,
///   retries, maps, queues, spawned tasks, metric cardinality and external
///   I/O; Concord does not automatically bound custom factory state.
/// * **Negative capabilities.** A factory cannot choose the destination,
///   method or headers of the provider request, and cannot reach the
///   application client or its credential stores.
pub trait ProviderBodyFactory: Send + Sync + 'static {
    /// Creates the complete bytes once for one provider operation.
    fn create(&self) -> Result<Bytes, AuthError>;
}

/// A complete bounded provider response, including non-success statuses.
///
/// Empty transport chunks have already been suppressed and bounded before this
/// value is made available to a provider.
pub struct ProviderResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    server_delay: Option<crate::retry::ServerDelayHint>,
    failure_cooldown_cap: Duration,
}

impl ProviderResponse {
    /// Returns the provider response status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the actual provider response headers.
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Validated server rejection delay still remaining from response-head
    /// arrival, capped by the runtime's authentication failure cooldown.
    /// Delta seconds and HTTP dates share core's standard parser; malformed
    /// or duplicate fields yield no hint. This performs no retry or admission.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        self.server_delay.map(|hint| {
            hint.bounded_remaining_at(OperationDeadline::now(), self.failure_cooldown_cap)
        })
    }

    /// Borrows the complete bounded provider response body.
    #[must_use]
    pub const fn body(&self) -> &Bytes {
        &self.body
    }

    /// Extracts the complete bounded provider response body.
    #[must_use]
    pub fn into_body(self) -> Bytes {
        self.body
    }
}

impl<'a> ProviderHttp<'a> {
    const fn new(inner: &'a ProviderHttpInner, deadline: Option<OperationDeadline>) -> Self {
        Self {
            inner: Some(inner),
            deadline,
            grant: None,
            scope: None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn unavailable_for_test() -> ProviderHttp<'static> {
        ProviderHttp {
            inner: None,
            deadline: None,
            grant: None,
            scope: None,
        }
    }

    /// Attributes credential work done with this capability to one
    /// application operation: provider acquisitions it leads name that
    /// operation as their parent, report to its observer and link their facts
    /// into its journal.
    pub(crate) const fn scoped(mut self, scope: ProviderScope<'a>) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Returns the application operation this capability works for, if any.
    pub(crate) const fn scope(&self) -> Option<ProviderScope<'a>> {
        self.scope
    }

    /// Binds this capability to one provider acquisition: exactly `origins`
    /// become reachable, sends draw from `sends`, helper traffic is recorded
    /// in `journal`, and the deadline becomes
    /// `min(remaining operation deadline, provider timeout)` fixed now.
    ///
    /// Returns the bound capability and the acquisition deadline that also
    /// bounds the provider future as a whole. The bound capability carries no
    /// application scope.
    pub(crate) fn for_acquisition<'b>(
        self,
        origins: &'b [ProviderOrigin],
        sends: &'b ProviderSendBudget,
        journal: &'b ProviderJournal,
    ) -> (ProviderHttp<'b>, Option<OperationDeadline>)
    where
        'a: 'b,
    {
        let deadline = match self.inner {
            Some(inner) => {
                let limit = self
                    .deadline
                    .and_then(OperationDeadline::remaining)
                    .map_or(inner.timeout, |remaining| remaining.min(inner.timeout));
                OperationDeadline::new(OperationDeadline::now(), Some(limit))
                    .ok()
                    .or(self.deadline)
            }
            None => self.deadline,
        };
        (
            ProviderHttp {
                inner: self.inner,
                deadline,
                grant: Some(ProviderGrant {
                    origins,
                    sends,
                    journal,
                }),
                scope: None,
            },
            deadline,
        )
    }

    /// Returns the exact origins this capability may reach. An unbound
    /// capability reaches none.
    #[must_use]
    pub fn allowed_origins(&self) -> &'a [ProviderOrigin] {
        self.grant.map_or(&[], |grant| grant.origins)
    }

    /// Returns how many helper sends remain for this acquisition.
    #[must_use]
    pub fn remaining_sends(&self) -> u32 {
        self.grant.map_or(0, |grant| grant.sends.remaining())
    }

    /// Returns the active finite request-body byte ceiling.
    ///
    /// Built-in providers can use this semantic bound to reject encoded output
    /// before allocating it. The value exposes no native client, pool, TLS, or
    /// request-builder state.
    #[must_use]
    pub const fn request_body_limit(&self) -> NonZeroUsize {
        match self.inner {
            Some(inner) => inner.request_limit,
            None => NonZeroUsize::MIN,
        }
    }

    /// Sends one bounded request through the isolated provider client.
    ///
    /// The URL's exact origin must be declared by the providing credential;
    /// otherwise the request fails with `InvalidConfiguration` before any
    /// body-factory call or transport work. Each call consumes one of the
    /// acquisition's helper sends; when none remain the request fails with
    /// `Acquisition` before transport. Errors are value-free.
    pub async fn send(self, request: ProviderRequest) -> Result<ProviderResponse, AuthError> {
        let Some(inner) = self.inner else {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        };
        if request.url.scheme() != "https"
            || inner.tls == TlsCapability::Unavailable
            || !request.url.username().is_empty()
            || request.url.password().is_some()
        {
            return Err(AuthError::new(AuthErrorKind::InvalidConfiguration));
        }
        let Some(grant) = self.grant.filter(|grant| {
            grant
                .origins
                .iter()
                .any(|origin| origin.matches(&request.url))
        }) else {
            return Err(AuthError::with_safe_message(
                AuthErrorKind::InvalidConfiguration,
                "provider HTTP origin is not declared by this credential provider",
            ));
        };
        if !grant.sends.try_take() {
            return Err(AuthError::with_safe_message(
                AuthErrorKind::Acquisition,
                "provider HTTP send limit reached for this acquisition",
            ));
        }

        let future = inner.send_bounded(request, self.deadline, grant.journal);
        match self.deadline {
            Some(deadline) => deadline
                .run(future)
                .await
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?,
            None => future.await,
        }
    }
}

impl ProviderHttpInner {
    pub(super) const fn new(
        client: reqwest::Client,
        tls: TlsCapability,
        request_limit: NonZeroUsize,
        response_limit: NonZeroUsize,
        max_sends: u32,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            tls,
            request_limit,
            response_limit,
            max_sends,
            timeout,
            failure_cooldown_cap: Duration::from_secs(
                crate::runtime::defaults::DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS,
            ),
            #[cfg(test)]
            development: None,
        }
    }

    pub(super) fn with_failure_cooldown_cap(mut self, cap: Duration) -> Self {
        self.failure_cooldown_cap = cap;
        self
    }

    #[cfg(test)]
    pub(super) fn with_development(mut self, development: ProviderChannel) -> Self {
        self.development = Some(development);
        self
    }

    #[cfg(test)]
    pub(super) const fn test_runtime_limits(&self) -> (NonZeroUsize, NonZeroUsize) {
        (self.request_limit, self.response_limit)
    }

    #[cfg(test)]
    pub(super) fn prepare(
        &self,
        method: Method,
        url: &str,
        known_body_length: Option<usize>,
    ) -> Result<reqwest::RequestBuilder, ErrorCategory> {
        let url = url::Url::parse(url).map_err(|_| ErrorCategory::Configuration)?;
        if url.scheme() != "https" || self.tls == TlsCapability::Unavailable {
            return Err(ErrorCategory::Configuration);
        }
        if known_body_length.is_some_and(|length| length > self.request_limit.get()) {
            return Err(ErrorCategory::RequestBody);
        }
        Ok(self.client.request(method, url))
    }

    pub(super) const fn capability(&self, deadline: Option<OperationDeadline>) -> ProviderHttp<'_> {
        ProviderHttp::new(self, deadline)
    }

    async fn send_bounded(
        &self,
        request: ProviderRequest,
        deadline: Option<OperationDeadline>,
        journal: &ProviderJournal,
    ) -> Result<ProviderResponse, AuthError> {
        let ProviderRequest {
            method,
            url,
            headers,
            body,
            timeout,
            response_limit,
        } = request;

        // Scheme/TLS validation occurs in ProviderHttp::send before this match,
        // so an unsafe URL cannot invoke application code in a body factory.
        let body = match body {
            ProviderBody::Empty => None,
            ProviderBody::Bytes(bytes) => Some(bytes),
            ProviderBody::Factory(factory) => Some(factory.create()?),
        };
        if body
            .as_ref()
            .is_some_and(|bytes| bytes.len() > self.request_limit.get())
        {
            return Err(AuthError::new(AuthErrorKind::RequestTooLarge));
        }

        let effective_response_limit = response_limit
            .map(NonZeroUsize::get)
            .unwrap_or(self.response_limit.get())
            .min(self.response_limit.get());
        let effective_timeout = match (timeout, deadline.and_then(OperationDeadline::remaining)) {
            (Some(request), Some(parent)) => Some(request.min(parent)),
            (Some(request), None) => Some(request),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        };
        #[cfg(test)]
        let native_url = url.as_str().to_owned();

        let mut builder = self
            .client
            .request(method.clone(), url)
            .headers(headers.clone());
        if let Some(body) = &body {
            builder = builder.body(body.clone());
        }

        let operation = async {
            #[cfg(test)]
            let response = {
                use concord_test_support::script::BodyCategory;
                let boundary = super::development::boundary_request(
                    "credential-provider",
                    method.clone(),
                    None,
                    &headers,
                    if body.is_some() {
                        BodyCategory::Bytes
                    } else {
                        BodyCategory::Empty
                    },
                    body.as_ref().map(Bytes::len),
                    body.clone(),
                )
                .__with_native(native_url, &headers);
                journal.record_handoff();
                self.execute_at_boundary(builder, boundary)
                    .await
                    .map_err(auth_http_error)?
            };
            #[cfg(not(test))]
            let response = {
                journal.record_handoff();
                builder
                    .send()
                    .await
                    .map_err(|error| auth_http_error(reqwest_category(error, false)))?
            };
            journal.record_head(response.status());

            if response
                .content_length()
                .is_some_and(|length| length > effective_response_limit as u64)
            {
                return Err(AuthError::new(AuthErrorKind::ResponseTooLarge));
            }
            collect_provider_response(
                response,
                effective_response_limit,
                self.failure_cooldown_cap,
            )
            .await
        };
        match effective_timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?,
            None => operation.await,
        }
    }

    #[cfg(test)]
    fn validate_response(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ErrorCategory> {
        if response
            .content_length()
            .is_some_and(|length| length > self.response_limit.get() as u64)
        {
            return Err(ErrorCategory::ResponseBody);
        }
        Ok(response)
    }

    #[cfg(test)]
    pub(super) async fn execute_at_boundary(
        &self,
        request: reqwest::RequestBuilder,
        boundary: BoundaryRequest,
    ) -> Result<reqwest::Response, ErrorCategory> {
        let response = if let Some(development) = &self.development {
            drop(request);
            development
                .__execute(boundary)
                .await
                .map(super::development::scripted_response)
                .map_err(|failure| super::development::failure_category(failure.kind()))?
        } else {
            request
                .send()
                .await
                .map_err(|error| reqwest_category(error, false))?
        };
        self.validate_response(response)
    }
}

/// Test-only: sends through a capability bound to exactly the request's own
/// origin with a fresh default send budget, as a provider that declared that
/// origin would.
#[cfg(test)]
pub(crate) async fn send_declared_for_test(
    http: ProviderHttp<'_>,
    request: ProviderRequest,
) -> Result<ProviderResponse, AuthError> {
    let origins = [ProviderOrigin::from_url(&request.url)
        .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?];
    let budget = ProviderSendBudget::for_http(&http);
    let journal = ProviderJournal::for_test();
    let (bound, _) = http.for_acquisition(&origins, &budget, &journal);
    bound.send(request).await
}

#[cfg(test)]
impl ProviderJournal {
    /// A journal for provider-helper tests that run outside any acquisition.
    pub(crate) fn for_test() -> Self {
        Self::new(
            OperationId::allocate().expect("test provider operation ID"),
            None,
            CredentialReason::Missing,
        )
    }
}

async fn collect_provider_response(
    mut response: reqwest::Response,
    limit: usize,
    failure_cooldown_cap: Duration,
) -> Result<ProviderResponse, AuthError> {
    let status = response.status();
    let headers = response.headers().clone();
    let server_delay = crate::retry::parse_server_delay(
        &headers,
        std::time::SystemTime::now(),
        OperationDeadline::now(),
    );
    let mut collected = BytesMut::new();
    let mut progress = ResponseChunkProgress::default();
    loop {
        let chunk = next_nonempty_chunk(&mut response, &mut progress)
            .await
            .map_err(|error| match error {
                ResponseChunkError::Acquisition => AuthError::new(AuthErrorKind::Acquisition),
                ResponseChunkError::LimitExceeded => {
                    AuthError::new(AuthErrorKind::ResponseTooLarge)
                }
            })?;
        let Some(chunk) = chunk else {
            return Ok(ProviderResponse {
                status,
                headers,
                body: collected.freeze(),
                server_delay,
                failure_cooldown_cap,
            });
        };
        let next = collected
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| AuthError::new(AuthErrorKind::ResponseTooLarge))?;
        if next > limit {
            return Err(AuthError::new(AuthErrorKind::ResponseTooLarge));
        }
        collected.extend_from_slice(&chunk);
    }
}

fn auth_http_error(category: ErrorCategory) -> AuthError {
    let kind = match category {
        ErrorCategory::RequestBody => AuthErrorKind::RequestTooLarge,
        ErrorCategory::ResponseBody => AuthErrorKind::ResponseTooLarge,
        ErrorCategory::Configuration => AuthErrorKind::InvalidConfiguration,
        _ => AuthErrorKind::Acquisition,
    };
    AuthError::new(kind)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};

    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;
    use concord_test_support::script::ScriptedResponse;
    use futures_core::Stream;

    use super::*;
    use crate::http::response_progress::test_support::{empty_chunks, response as native_response};
    use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};

    struct InfiniteEmptyBody {
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
        fairness_reached: Arc<tokio::sync::Notify>,
    }

    impl Stream for InfiniteEmptyBody {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let polls = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            if polls == MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
                self.fairness_reached.notify_one();
            }
            Poll::Ready(Some(Ok(Bytes::new())))
        }
    }

    impl Drop for InfiniteEmptyBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn infinite_empty_response(
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        fairness_reached: &Arc<tokio::sync::Notify>,
    ) -> reqwest::Response {
        let body = reqwest::Body::wrap_stream(InfiniteEmptyBody {
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
            fairness_reached: Arc::clone(fairness_reached),
        });
        http::Response::new(body).into()
    }

    struct CountingFactory {
        calls: Arc<AtomicUsize>,
        bytes: Bytes,
    }

    impl ProviderBodyFactory for CountingFactory {
        fn create(&self) -> Result<Bytes, AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.bytes.clone())
        }
    }

    fn inner(
        executor: &DeterministicExecutor,
        request_limit: usize,
        response_limit: usize,
        tls: TlsCapability,
    ) -> ProviderHttpInner {
        let _ = rustls::crypto::ring::default_provider().install_default();
        ProviderHttpInner::new(
            reqwest::Client::new(),
            tls,
            NonZeroUsize::new(request_limit).expect("request limit"),
            NonZeroUsize::new(response_limit).expect("response limit"),
            4,
            Duration::from_secs(30),
        )
        .with_development(executor.provider())
    }

    /// Sends through a capability bound to the request's own origin, as a
    /// provider that declared exactly that origin would.
    async fn send_declared(
        inner: &ProviderHttpInner,
        deadline: Option<OperationDeadline>,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, AuthError> {
        send_declared_for_test(inner.capability(deadline), request).await
    }

    fn declared(origin: &str) -> [ProviderOrigin; 1] {
        [ProviderOrigin::parse(origin).expect("declared test origin")]
    }

    #[tokio::test]
    async fn undeclared_origins_are_rejected_before_factory_or_transport() {
        const HOST_SENTINEL: &str = "undeclared-host-sentinel";
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let origins = declared("https://provider.example.test");
        let budget = ProviderSendBudget::new(4);
        let journal = ProviderJournal::for_test();
        let (http, _) = provider
            .capability(None)
            .for_acquisition(&origins, &budget, &journal);
        let factory = || {
            ProviderBody::Factory(Arc::new(CountingFactory {
                calls: Arc::clone(&calls),
                bytes: Bytes::from_static(b"provider-body"),
            }))
        };
        for url in [
            "https://undeclared-host-sentinel.test/token",
            "https://provider.example.test:8443/token",
            "https://sub.provider.example.test/token",
            "https://user:secret@provider.example.test/token",
        ] {
            let request =
                ProviderRequest::new(Method::POST, Url::parse(url).expect("URL")).body(factory());
            let error = error(http.send(request).await);
            assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration, "{url}");
            let rendered = format!("{error} {error:?}");
            assert!(!rendered.contains(HOST_SENTINEL));
            assert!(!rendered.contains("secret"));
        }

        // A capability that is not bound to a provider acquisition reaches
        // no destination at all, even a syntactically valid HTTPS one.
        let unbound = provider.capability(None);
        assert!(unbound.allowed_origins().is_empty());
        assert_eq!(unbound.remaining_sends(), 0);
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/token").expect("URL"),
        )
        .body(factory());
        assert_eq!(
            error(unbound.send(request).await).kind(),
            AuthErrorKind::InvalidConfiguration
        );

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
        assert_eq!(
            http.remaining_sends(),
            4,
            "denied requests consume no sends"
        );
        assert_eq!(http.allowed_origins(), &origins);
        // EXEC-01: refused requests never reach the provider journal.
        let facts = journal.facts(ProviderAcquisitionOutcome::Failed);
        assert_eq!(facts.helper_sends(), 0);
        assert_eq!(facts.last_status(), None);
        assert_eq!(facts.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    }

    #[tokio::test]
    async fn helper_send_budget_is_per_acquisition_and_enforced_before_transport() {
        let executor = DeterministicExecutor::new();
        for _ in 0..3 {
            executor
                .provider()
                .queue_response(ScriptedResponse::success(Bytes::from_static(b"ok")))
                .expect("provider response");
        }
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let origins = declared("https://provider.example.test");
        let request = || {
            ProviderRequest::new(
                Method::GET,
                Url::parse("https://provider.example.test/token").expect("URL"),
            )
        };
        let budget = ProviderSendBudget::new(2);
        let journal = ProviderJournal::for_test();
        let (http, _) = provider
            .capability(None)
            .for_acquisition(&origins, &budget, &journal);
        http.send(request()).await.expect("first send");
        assert_eq!(http.remaining_sends(), 1);
        http.send(request()).await.expect("second send");
        let exhausted = error(http.send(request()).await);
        assert_eq!(exhausted.kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 2);
        // EXEC-01: the provider journal counts the two handed-off helper
        // requests (not the one refused by the budget) and their last head.
        let facts = journal.facts(ProviderAcquisitionOutcome::Installed);
        assert_eq!(facts.helper_sends(), 2);
        assert_eq!(facts.last_status(), Some(StatusCode::OK));
        assert_eq!(
            facts.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );

        // A later acquisition receives a fresh configured budget and journal.
        let fresh = ProviderSendBudget::for_http(&provider.capability(None));
        let later_journal = ProviderJournal::for_test();
        let (later, _) =
            provider
                .capability(None)
                .for_acquisition(&origins, &fresh, &later_journal);
        assert_eq!(later.remaining_sends(), 4);
        later.send(request()).await.expect("fresh acquisition send");
        assert_eq!(executor.provider().request_count().expect("count"), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn acquisition_deadline_is_min_of_operation_remaining_and_provider_timeout() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .provider()
            .queue_gate(gate.clone())
            .expect("provider gate");
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
            .expect("provider response");
        let _ = rustls::crypto::ring::default_provider().install_default();
        let provider = ProviderHttpInner::new(
            reqwest::Client::new(),
            TlsCapability::Available,
            NonZeroUsize::new(64).expect("limit"),
            NonZeroUsize::new(64).expect("limit"),
            4,
            Duration::from_secs(5),
        )
        .with_development(executor.provider());
        let origins = declared("https://provider.example.test");
        let budget = ProviderSendBudget::new(4);
        let journal = ProviderJournal::for_test();

        // Without an operation deadline the provider ceiling still applies.
        let (_, ceiling) = provider
            .capability(None)
            .for_acquisition(&origins, &budget, &journal);
        assert_eq!(
            ceiling.and_then(OperationDeadline::remaining),
            Some(Duration::from_secs(5))
        );
        // A shorter remaining operation deadline wins.
        let short =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(2)))
                .expect("short parent");
        let (_, narrowed) = provider
            .capability(Some(short))
            .for_acquisition(&origins, &budget, &journal);
        assert_eq!(
            narrowed.and_then(OperationDeadline::remaining),
            Some(Duration::from_secs(2))
        );

        let parent =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(30)))
                .expect("parent deadline");
        let (http, _) = provider
            .capability(Some(parent))
            .for_acquisition(&origins, &budget, &journal);
        let send = http.send(ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/slow").expect("URL"),
        ));
        tokio::pin!(send);
        tokio::select! {
            () = gate.wait_until_reached() => {}
            result = &mut send => panic!("provider operation ended before its gate: {}", result.is_ok()),
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(error(send.await).kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
    }

    fn error<T>(result: Result<T, AuthError>) -> AuthError {
        match result {
            Ok(_) => panic!("expected provider HTTP failure"),
            Err(error) => error,
        }
    }

    #[test]
    fn provider_http_exposes_the_active_request_body_limit() {
        let executor = DeterministicExecutor::new();
        let provider = inner(&executor, 37, 64, TlsCapability::Available);
        assert_eq!(provider.capability(None).request_body_limit().get(), 37);
    }

    #[tokio::test]
    async fn non_https_and_missing_tls_fail_before_factory_or_native_execution() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let factory = || {
            ProviderBody::Factory(Arc::new(CountingFactory {
                calls: Arc::clone(&calls),
                bytes: Bytes::from_static(b"protected-provider-body"),
            }))
        };
        let available = inner(&executor, 64, 64, TlsCapability::Available);
        let cleartext = ProviderRequest::new(
            Method::POST,
            Url::parse("http://provider.example.test/token").expect("URL"),
        )
        .body(factory());
        assert_eq!(
            error(send_declared(&available, None, cleartext).await).kind(),
            AuthErrorKind::InvalidConfiguration
        );

        let unavailable = inner(&executor, 64, 64, TlsCapability::Unavailable);
        let protected = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/token").expect("URL"),
        )
        .body(factory());
        assert_eq!(
            error(send_declared(&unavailable, None, protected).await).kind(),
            AuthErrorKind::InvalidConfiguration
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test]
    async fn request_limit_runs_factory_once_and_stops_before_provider_channel() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 3, 64, TlsCapability::Available);
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/token").expect("URL"),
        )
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"four"),
        })));
        assert_eq!(
            error(send_declared(&provider, None, request).await).kind(),
            AuthErrorKind::RequestTooLarge
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
    }

    #[tokio::test]
    async fn successful_provider_operation_invokes_body_factory_exactly_once() {
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_expected_response(
                concord_test_support::script::ExpectedRequest::new()
                    .method(Method::POST)
                    .body(Bytes::from_static(b"factory-body")),
                ScriptedResponse::success(Bytes::from_static(b"ok")),
            )
            .expect("provider response");
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/factory").expect("URL"),
        )
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"factory-body"),
        })));
        let response = send_declared(&provider, None, request)
            .await
            .expect("provider operation");
        assert_eq!(response.body(), &Bytes::from_static(b"ok"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
    }

    #[tokio::test]
    async fn provider_owns_non_success_status_headers_and_bounded_body() {
        let executor = DeterministicExecutor::new();
        let mut headers = HeaderMap::new();
        headers.insert("x-provider", HeaderValue::from_static("present"));
        executor
            .provider()
            .queue_response(
                ScriptedResponse::status(
                    StatusCode::TOO_MANY_REQUESTS,
                    Bytes::from_static(b"safe"),
                )
                .headers(headers),
            )
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let response = send_declared(
            &provider,
            None,
            ProviderRequest::new(
                Method::GET,
                Url::parse("https://provider.example.test/status").expect("URL"),
            ),
        )
        .await
        .expect("non-success belongs to provider");
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get("x-provider"),
            Some(&HeaderValue::from_static("present"))
        );
        assert_eq!(response.body(), &Bytes::from_static(b"safe"));
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn r7_provider_delay_is_validated_bounded_and_anchored_before_body_consumption() {
        let wall = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let observed = OperationDeadline::now();
        for (fields, expected) in [
            (vec!["10"], Some(Duration::from_secs(5))),
            (vec!["0"], Some(Duration::ZERO)),
            (vec!["bogus"], None),
            (vec!["-1"], None),
            (vec!["10", "10"], None),
            (
                vec!["999999999999999999999999999999"],
                Some(Duration::from_secs(5)),
            ),
        ] {
            let mut headers = HeaderMap::new();
            for field in fields {
                headers.append("retry-after", HeaderValue::from_str(field).unwrap());
            }
            let response = ProviderResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
                body: Bytes::new(),
                server_delay: crate::retry::parse_server_delay(&headers, wall, observed),
                headers,
                failure_cooldown_cap: Duration::from_secs(5),
            };
            assert_eq!(response.retry_after(), expected);
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            HeaderValue::from_str(&httpdate::fmt_http_date(wall + Duration::from_secs(10)))
                .unwrap(),
        );
        let response = ProviderResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: Bytes::new(),
            server_delay: crate::retry::parse_server_delay(&headers, wall, observed),
            headers,
            failure_cooldown_cap: Duration::from_secs(5),
        };
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(response.retry_after(), Some(Duration::from_secs(3)));
        tokio::time::advance(Duration::from_secs(3)).await;
        assert_eq!(response.retry_after(), Some(Duration::ZERO));
    }

    #[tokio::test]
    async fn provider_response_suppresses_empty_chunks() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let response = collect_provider_response(
            native_response(
                [
                    Ok(Bytes::new()),
                    Ok(Bytes::from_static(b"provider")),
                    Ok(Bytes::new()),
                ],
                &polls,
                &dropped,
            ),
            64,
            Duration::from_secs(60),
        )
        .await
        .expect("provider response");

        assert_eq!(response.body(), &Bytes::from_static(b"provider"));
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn provider_response_yields_executor_after_ready_empty_budget() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
        chunks.push(Ok(Bytes::from_static(b"deferred")));
        let mut collection = Box::pin(collect_provider_response(
            native_response(chunks, &polls, &dropped),
            64,
            Duration::from_secs(60),
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(matches!(
            collection.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
    }

    #[tokio::test]
    async fn provider_response_first_excess_is_safe() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));

        let error = error(
            collect_provider_response(
                native_response(
                    empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
                    &polls,
                    &dropped,
                ),
                64,
                Duration::from_secs(60),
            )
            .await,
        );
        assert_eq!(error.kind(), AuthErrorKind::ResponseTooLarge);
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 1);
        assert!(dropped.load(Ordering::SeqCst));
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
        assert!(!rendered.contains("empty chunk"));
    }

    #[tokio::test(start_paused = true)]
    async fn provider_response_deadline_can_fire_during_empty_chunk_run() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let fairness_reached = Arc::new(tokio::sync::Notify::new());
        let response = infinite_empty_response(&polls, &dropped, &fairness_reached);
        let collection = tokio::spawn(async move {
            tokio::time::timeout(
                Duration::from_secs(1),
                collect_provider_response(response, 64, Duration::from_secs(60)),
            )
            .await
        });
        fairness_reached.notified().await;
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(collection.await.expect("provider collection task").is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn provider_response_error_contains_no_body_or_url() {
        const BODY_SENTINEL: &str = "provider-body-secret-61f8";
        const URL_SENTINEL: &str = "https://provider-secret.invalid/token";
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = vec![Ok(Bytes::from_static(BODY_SENTINEL.as_bytes()))];
        chunks.extend(empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1));

        let error = error(
            collect_provider_response(
                native_response(chunks, &polls, &dropped),
                64,
                Duration::from_secs(60),
            )
            .await,
        );
        let rendered = format!("{error} {error:?}");
        assert_eq!(error.kind(), AuthErrorKind::ResponseTooLarge);
        assert!(!rendered.contains(BODY_SENTINEL));
        assert!(!rendered.contains(URL_SENTINEL));
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
    }

    #[tokio::test]
    async fn known_streamed_and_request_local_response_limits_fail_closed() {
        let executor = DeterministicExecutor::new();
        let provider = inner(&executor, 64, 4, TlsCapability::Available);

        let mut known_headers = HeaderMap::new();
        known_headers.insert("content-length", HeaderValue::from_static("5"));
        executor
            .provider()
            .queue_response(
                ScriptedResponse::success(Bytes::from_static(b"12345")).headers(known_headers),
            )
            .expect("known response");
        let known = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/known").expect("URL"),
        );
        assert_eq!(
            error(send_declared(&provider, None, known).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"12345")))
            .expect("streamed response");
        let streamed = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/streamed").expect("URL"),
        );
        assert_eq!(
            error(send_declared(&provider, None, streamed).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"1234")))
            .expect("request-local response");
        let reduced = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/reduced").expect("URL"),
        )
        .response_limit(NonZeroUsize::new(3).expect("limit"));
        assert_eq!(
            error(send_declared(&provider, None, reduced).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );

        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"12345")))
            .expect("non-increasing response");
        let cannot_increase = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/capped").expect("URL"),
        )
        .response_limit(NonZeroUsize::new(64).expect("limit"));
        assert_eq!(
            error(send_declared(&provider, None, cannot_increase).await).kind(),
            AuthErrorKind::ResponseTooLarge
        );
    }

    #[tokio::test]
    async fn expired_parent_budget_prevents_factory_and_provider_execution() {
        let executor = DeterministicExecutor::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let deadline = OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::ZERO))
            .expect("deadline");
        let request = ProviderRequest::new(
            Method::POST,
            Url::parse("https://provider.example.test/deadline").expect("URL"),
        )
        .timeout(Duration::from_secs(30))
        .body(ProviderBody::Factory(Arc::new(CountingFactory {
            calls: Arc::clone(&calls),
            bytes: Bytes::from_static(b"body"),
        })));
        assert_eq!(
            error(send_declared(&provider, Some(deadline), request).await).kind(),
            AuthErrorKind::Acquisition
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.provider().request_count().expect("count"), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn earlier_request_timeout_bounds_the_complete_provider_operation() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .provider()
            .queue_gate(gate.clone())
            .expect("provider gate");
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let parent =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(30)))
                .expect("parent deadline");
        let request = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/request-timeout").expect("URL"),
        )
        .timeout(Duration::from_secs(5));
        let send = send_declared(&provider, Some(parent), request);
        tokio::pin!(send);
        tokio::select! {
            () = gate.wait_until_reached() => {}
            result = &mut send => panic!("provider operation ended before its gate: {}", result.is_ok()),
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(error(send.await).kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn earlier_parent_budget_bounds_provider_request_timeout() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .provider()
            .queue_gate(gate.clone())
            .expect("provider gate");
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
            .expect("provider response");
        let provider = inner(&executor, 64, 64, TlsCapability::Available);
        let parent =
            OperationDeadline::new(tokio::time::Instant::now(), Some(Duration::from_secs(3)))
                .expect("parent deadline");
        let request = ProviderRequest::new(
            Method::GET,
            Url::parse("https://provider.example.test/parent-timeout").expect("URL"),
        )
        .timeout(Duration::from_secs(30));
        let send = send_declared(&provider, Some(parent), request);
        tokio::pin!(send);
        tokio::select! {
            () = gate.wait_until_reached() => {}
            result = &mut send => panic!("provider operation ended before its gate: {}", result.is_ok()),
        }
        tokio::time::advance(Duration::from_secs(3)).await;
        assert_eq!(error(send.await).kind(), AuthErrorKind::Acquisition);
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }
}
