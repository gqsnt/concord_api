//! One statically typed production engine for generated requests.
//!
//! Every direct request, traversal page and credential-storage execution runs
//! through [`execute_finalized_operation`] (direct requests and pages through
//! its identity form [`execute_prepared_operation`]), which owns one
//! [`OperationJournal`](journal::OperationJournal). The journal is the only
//! authority for the dispatch facts attached to an escaping [`ApiError`]
//! (EXEC-01/EXEC-02), counts every application handoff against the caller's
//! send cap and every automatic replay against the replay cap (CONTROL-02),
//! and emits the final observation only after the complete result —
//! including a local finalization effect such as credential installation
//! (§11.4) — is fixed (EXEC-04).

use std::future::IntoFuture;
use std::num::NonZeroUsize;

use http::{Method, StatusCode};

use crate::auth::PreparedAuthPlan;
use crate::body::{
    BodyKind, ManagedBody, Replayability, RequestBodyContext, RequestRepresentation,
};
use crate::client::ClientCore;
use crate::controls::{CallOptions, RequestControlState};
use crate::endpoint::{EndpointId, ResponseAcquisition};
use crate::error::{
    ApiError, ConfigurationErrorKind, DispatchKnowledge, ErrorCategory, ErrorJournal, FailureStage,
};
use crate::hooks::{AttemptInfo, AttemptStarted, BoxFuture, OperationEvent, ResponseHeadObserved};
use crate::http::provider::{ProviderHttp, ProviderLink, ProviderScope};
use crate::operation::{OperationDeadline, OperationId};
use crate::rate_limit::{RateLimitRequest, RateLimitResponse};
use crate::request::pending_erasure::ErasedOperation;
use crate::request::{
    AcceptedBytesRequest, EndpointSpec, OperationPreparation, PreparationFailure, PreparedHead,
    PreparedOperation, PreparedPaginationPage, Request, ResponseRequest, prepare_operation,
};
use crate::response::{
    AcceptedBytesSelection, ApiResponse, BodyDisposition, DeclaredResponse, ExecutionMeta,
    ExpectedAbsence, ResponseDriver, ResponseHead, ResponseInput, ResponseRepresentation,
    ResponseSelection, body_disposition, completion_outcome, execute_response,
};
use crate::retry::{
    RetryAttempt, RetryDecision, RetryFailure, RetryPolicy, ServerDelayHint, parse_server_delay,
};

mod journal;

use journal::OperationJournal;

/// Hard aggregate visible-attempt ceiling shared by direct and paginated operations.
pub const MAX_VISIBLE_ATTEMPTS_PER_OPERATION: u32 = 4_096;

/// Maximum phase-6 credential-freshness restarts per attempt. Together with
/// the initial admission this bounds committed quota admissions to three per
/// allowed application handoff.
pub(crate) const MAX_FRESHNESS_RESTARTS: u32 = 2;

#[doc(hidden)]
impl<E> IntoFuture for Request<E>
where
    E: EndpointSpec,
{
    type Output = Result<E::Output, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(
            execute_request_as::<E, DeclaredResponse<E::Response>, _, _>(
                self,
                ApiResponse::into_value,
            ),
        )
    }
}

impl<O> IntoFuture for ResponseRequest<O>
where
    O: Send + 'static,
{
    type Output = Result<ApiResponse<O>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.start()
    }
}

impl IntoFuture for AcceptedBytesRequest {
    type Output = Result<ApiResponse<bytes::Bytes>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.start()
    }
}

struct TypedResponse<E> {
    request: Request<E>,
}

struct TypedAcceptedBytes<E> {
    request: Request<E>,
}

fn declared_timeout<E: EndpointSpec>(request: &Request<E>) -> Option<std::time::Duration> {
    request
        .endpoint
        .request_meta()
        .resolve_inherited_timeout(request.core.default_runtime_endpoint_timeout())
}

impl<E> ErasedOperation<ApiResponse<E::Output>> for TypedResponse<E>
where
    E: EndpointSpec,
{
    fn options(&self) -> &crate::request::RequestOptions {
        &self.request.options
    }

    fn options_mut(&mut self) -> &mut crate::request::RequestOptions {
        &mut self.request.options
    }

    fn declared_timeout(&self) -> Option<std::time::Duration> {
        declared_timeout(&self.request)
    }

    fn start(self: Box<Self>) -> BoxFuture<'static, Result<ApiResponse<E::Output>, ApiError>> {
        Box::pin(
            execute_request_as::<E, DeclaredResponse<E::Response>, _, _>(
                self.request,
                std::convert::identity,
            ),
        )
    }
}

impl<E> ErasedOperation<ApiResponse<bytes::Bytes>> for TypedAcceptedBytes<E>
where
    E: EndpointSpec,
    <E::Response as ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    fn options(&self) -> &crate::request::RequestOptions {
        &self.request.options
    }

    fn options_mut(&mut self) -> &mut crate::request::RequestOptions {
        &mut self.request.options
    }

    fn declared_timeout(&self) -> Option<std::time::Duration> {
        declared_timeout(&self.request)
    }

    fn start(self: Box<Self>) -> BoxFuture<'static, Result<ApiResponse<bytes::Bytes>, ApiError>> {
        Box::pin(execute_request_as::<
            E,
            AcceptedBytesSelection<E::Response>,
            _,
            _,
        >(self.request, std::convert::identity))
    }
}

/// Wraps one bound request as its `.response()` terminal.
pub(crate) fn response_request<E>(request: Request<E>) -> ResponseRequest<E::Output>
where
    E: EndpointSpec,
{
    ResponseRequest {
        inner: Box::new(TypedResponse { request }),
    }
}

/// Wraps one bound request as its `.accepted_bytes()` terminal. Only
/// buffered and discard drivers qualify (§10.4).
pub(crate) fn accepted_bytes_request<E>(request: Request<E>) -> AcceptedBytesRequest
where
    E: EndpointSpec,
    <E::Response as ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    AcceptedBytesRequest {
        inner: Box::new(TypedAcceptedBytes { request }),
    }
}

#[cfg(test)]
impl<E> Request<E>
where
    E: EndpointSpec,
    <E::Response as ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    /// Test-only construction of the capability terminal used by generated
    /// endpoint request types.
    #[must_use]
    pub(crate) fn accepted_bytes(self) -> AcceptedBytesRequest {
        accepted_bytes_request(self)
    }
}

/// Executes one direct request under selection `S` and maps its fixed
/// response with `output`.
///
/// Every terminal boxes this future directly: `output` (a function item, so
/// zero-sized) replaces a wrapping `async` block, which would store the
/// by-value `Request` a second time in its own state machine.
async fn execute_request_as<E, S, T, M>(request: Request<E>, output: M) -> Result<T, ApiError>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
    M: FnOnce(ApiResponse<S::Output>) -> T,
{
    // The prepared operation moves into the operation future inside the
    // match arm, so this state machine does not also keep a (moved-out)
    // `PreparedOperation` slot alive across the operation's `.await`.
    let operation = match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => {
            let attempt_ceiling = direct_attempt_ceiling(prepared.head.call_caps());
            execute_prepared_operation::<E, S>(prepared, ExecutionContext::DIRECT, attempt_ceiling)
        }
        OperationPreparation::Failed(failure) => {
            return complete_preparation_failure(failure);
        }
    };
    operation.await.result.map(output)
}

/// Visible-attempt ceiling of one direct request: the per-request hard
/// maximum (initial + at most 16 policy retries + at most one authentication
/// recovery) intersected with the caller's send cap and with the replay cap
/// plus the initial submission. Freshness restarts are not attempts.
pub(crate) fn direct_attempt_ceiling(caps: &CallOptions) -> u32 {
    let mut ceiling = crate::pagination::MAX_PAGE_ATTEMPTS;
    if let Some(sends) = caps.send_cap() {
        ceiling = ceiling.min(sends);
    }
    if let Some(replays) = caps.replay_cap() {
        ceiling = ceiling.min(replays.saturating_add(1));
    }
    ceiling
}

#[derive(Clone, Copy)]
pub(crate) struct ExecutionContext {
    pub(crate) parent_operation_id: Option<OperationId>,
    pub(crate) page_index: u32,
}

impl ExecutionContext {
    const DIRECT: Self = Self {
        parent_operation_id: None,
        page_index: 0,
    };

    const fn page(parent_operation_id: OperationId, page_index: u32) -> Self {
        Self {
            parent_operation_id: Some(parent_operation_id),
            page_index,
        }
    }
}

/// The fixed result of one journaled operation and its settled facts.
pub(crate) struct Executed<R> {
    pub(crate) operation_id: OperationId,
    pub(crate) result: Result<R, ApiError>,
    pub(crate) final_status: Option<StatusCode>,
    pub(crate) attempts_started: u32,
    pub(crate) policy_retries: u32,
    pub(crate) recovered_authentication: bool,
}

/// The fixed result of one direct request or traversal page.
pub(crate) type ExecutedPage<T> = Executed<ApiResponse<T>>;

/// The local finalization window of one operation: after its exchange
/// produced an accepted value and **before** its result is fixed and finally
/// observed (§11.4, EXEC-04).
///
/// A finalization step runs synchronously inside the operation's journal and
/// cancellation scope. It performs local work that belongs to the operation's
/// result (credential installation), so the common caps apply through it and
/// its failures are contextualized by the same journal: the same operation
/// identity, retained response status, attempt and dispatch facts, and the
/// stage it entered. The single `OperationFinished` observation is emitted
/// only after the finalization step fixed the complete result.
pub(crate) struct Finalization<'a> {
    journal: &'a mut OperationJournal,
    deadline: OperationDeadline,
    controls: Option<&'a RequestControlState>,
    endpoint_id: &'a EndpointId,
    method: &'a Method,
}

impl Finalization<'_> {
    /// Enters `stage` and decides the operation's common caps before a local
    /// effect: an attached cancellation signal observed now, then the
    /// absolute deadline. A failure is contextualized at `stage` when the
    /// result is fixed.
    pub(crate) fn decide_caps(&mut self, stage: FailureStage) -> Result<(), ApiError> {
        self.journal.enter(stage);
        if self.controls.is_some_and(RequestControlState::is_cancelled) {
            return Err(cancelled_error(self.endpoint_id, self.method));
        }
        check_deadline(self.deadline, self.endpoint_id, self.method)
    }

    /// The validated operational endpoint ID of the executed request (AUTH-05).
    pub(crate) fn endpoint_id(&self) -> &EndpointId {
        self.endpoint_id
    }

    /// The application method of the executed request.
    pub(crate) fn method(&self) -> &Method {
        self.method
    }
}

/// Executes one traversal page as a child operation of the aggregate.
///
/// `attempt_ceiling` bounds this page's visible attempts (initial plus policy
/// and authentication replays) and is derived by the traversal from its
/// remaining aggregate send/replay budgets. `meter` receives the page's rate
/// admissions, handoffs, response heads and every acquired body byte. The
/// traversal owns cancellation of a started page fetch.
pub(crate) async fn execute_pagination_page<E>(
    page: PreparedPaginationPage<E>,
    parent_operation_id: OperationId,
    page_index: u32,
    deadline: OperationDeadline,
    attempt_ceiling: u32,
    meter: std::sync::Arc<crate::pagination::TraversalMeter>,
) -> Result<ExecutedPage<E::Output>, ApiError>
where
    E: EndpointSpec,
{
    let PreparedPaginationPage {
        core,
        endpoint,
        head,
    } = page;
    let page_journal = |operation_id, stage| ErrorJournal {
        operation_id,
        parent_operation_id: Some(parent_operation_id),
        page_index: Some(page_index),
        ..ErrorJournal::before_operation(stage)
    };
    let operation_id = core
        .allocate_operation_id(head.endpoint_id(), head.method())
        .map_err(|error| error.with_journal(page_journal(None, FailureStage::Preparation)))?;
    check_deadline(deadline, head.endpoint_id(), head.method()).map_err(|error| {
        error.with_journal(page_journal(
            Some(operation_id),
            FailureStage::BeforeDispatch,
        ))
    })?;
    let context = ExecutionContext::page(parent_operation_id, page_index);
    Ok(
        execute_prepared_operation::<E, DeclaredResponse<E::Response>>(
            PreparedOperation {
                core,
                endpoint,
                operation_id,
                deadline,
                head,
                absence: None,
                meter: Some(meter),
                controls: None,
            },
            context,
            attempt_ceiling,
        )
        .await,
    )
}

/// Runs one prepared operation to a fixed result under its journal.
///
/// Attached cancellation signals are raced against the whole pipeline. When a
/// signal wins, the in-flight work future is dropped (releasing owned
/// transport work) and the operation fails with a `Cancelled` error whose
/// dispatch knowledge is what the journal proves at that moment: a pending
/// handoff settles as `PossiblyDispatched`, a received head as
/// `ResponseObserved`. Cancellation never retracts a handed-off request,
/// refunds committed quota or erases contact evidence.
///
/// A plain function returning the finalized-operation future with the
/// identity finalizer, not an `async fn` wrapper: a wrapper would store its
/// arguments a second time in its own state machine.
pub(crate) fn execute_prepared_operation<E, S>(
    prepared: PreparedOperation<E>,
    context: ExecutionContext,
    attempt_ceiling: u32,
) -> impl Future<Output = ExecutedPage<S::Output>> + Send
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
{
    execute_finalized_operation::<E, S, _, _>(
        prepared,
        context,
        attempt_ceiling,
        |response, _finalization| Ok(response),
    )
}

/// Runs one prepared operation, then its synchronous local `finalize` step,
/// to one fixed result under the same journal (§11.4, EXEC-04).
///
/// `finalize` receives the accepted response inside the operation's
/// cancellation scope and before the journal completes: it decides the common
/// caps ([`Finalization::decide_caps`]) and performs its local effect, and
/// its value or error is the operation's result. Only then is the result
/// fixed and the single final observation emitted, so a final observer can
/// neither precede nor change a local effect such as credential installation.
pub(crate) async fn execute_finalized_operation<E, S, F, R>(
    prepared: PreparedOperation<E>,
    context: ExecutionContext,
    attempt_ceiling: u32,
    finalize: F,
) -> Executed<R>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
    F: FnOnce(ApiResponse<S::Output>, &mut Finalization<'_>) -> Result<R, ApiError> + Send,
{
    let PreparedOperation {
        core,
        endpoint,
        operation_id,
        deadline,
        mut head,
        absence,
        meter,
        controls,
    } = prepared;
    let endpoint_id = head.endpoint_id().clone();
    let method = head.method().clone();
    let mut journal = OperationJournal::new(
        core.clone(),
        operation_id,
        context,
        endpoint_id.clone(),
        method.clone(),
        head.request_target(),
        head.debug(),
        deadline.started(),
    );
    let retry_policy = core.retry_policy().cloned();
    let controls = controls.filter(|controls| !controls.cancellation().is_empty());
    // EXEC-01: provider acquisitions led or joined by this operation's
    // credential work link their settled journal facts here. The link
    // outlives the work future, so a leader cancelled with it still links
    // its cancelled provider journal before the result is fixed.
    let provider_link = ProviderLink::default();
    let outcome = {
        let journal = &mut journal;
        let controls = controls.as_ref();
        let (endpoint_id, method) = (&endpoint_id, &method);
        let work = async {
            let (response, _response_body_bytes) = run_operation::<E, S>(
                OperationRun {
                    core: &core,
                    head: &mut head,
                    meter: meter.as_deref(),
                    retry_policy: retry_policy.as_ref(),
                    deadline,
                    context,
                    attempt_ceiling,
                    controls,
                    provider_link: &provider_link,
                },
                endpoint,
                absence,
                journal,
            )
            .await?;
            finalize(
                response,
                &mut Finalization {
                    journal,
                    deadline,
                    controls,
                    endpoint_id,
                    method,
                },
            )
        };
        match controls {
            Some(controls) => {
                let cancelled = controls.cancelled();
                tokio::select! {
                    biased;
                    () = cancelled => Err(cancelled_error(endpoint_id, method)),
                    result = work => result,
                }
            }
            None => work.await,
        }
    };
    journal.set_target(head.request_target());
    journal.record_provider(provider_link.take());
    // EXEC-04: the complete result (including any finalization effect) is
    // fixed here, and only then observed.
    let result = journal.complete(outcome, completion_outcome::<E::Response, E::Output, S>());
    Executed {
        operation_id,
        result,
        final_status: journal.last_status(),
        attempts_started: journal.attempts_started(),
        policy_retries: journal.policy_retries(),
        recovered_authentication: journal.recovered_authentication(),
    }
}

struct OperationRun<'a> {
    core: &'a ClientCore,
    head: &'a mut PreparedHead,
    meter: Option<&'a crate::pagination::TraversalMeter>,
    retry_policy: Option<&'a RetryPolicy>,
    deadline: OperationDeadline,
    context: ExecutionContext,
    attempt_ceiling: u32,
    controls: Option<&'a RequestControlState>,
    provider_link: &'a ProviderLink,
}

/// Immutable per-attempt environment.
#[derive(Clone, Copy)]
struct AttemptEnv<'a> {
    core: &'a ClientCore,
    head: &'a PreparedHead,
    meter: Option<&'a crate::pagination::TraversalMeter>,
    retry_policy: Option<&'a RetryPolicy>,
    deadline: OperationDeadline,
    context: ExecutionContext,
    attempt_ceiling: u32,
    controls: Option<&'a RequestControlState>,
    operation_id: OperationId,
    provider_link: &'a ProviderLink,
}

#[allow(
    clippy::too_many_lines,
    reason = "the normative pipeline phases are kept in one visible order"
)]
async fn run_operation<E, S>(
    run: OperationRun<'_>,
    endpoint: E,
    absence: Option<ExpectedAbsence<E::Output, E::Response>>,
    journal: &mut OperationJournal,
) -> Result<(ApiResponse<S::Output>, usize), ApiError>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
{
    let OperationRun {
        core,
        head,
        meter,
        retry_policy,
        deadline,
        context,
        attempt_ceiling,
        controls,
        provider_link,
    } = run;
    let operation_id = journal.operation_id();
    journal.enter(FailureStage::Preparation);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    if absence.is_some() && !S::SUPPORTS_ABSENCE {
        // Head-only absence cannot combine with the accepted-bytes terminal.
        return Err(ApiError::configuration(
            head.endpoint_id().clone(),
            head.method().clone(),
            ConfigurationErrorKind::InvalidValue,
        ));
    }
    // Phase 3: prepare the one body before any credential or limiter work.
    let mut prepared_body = prepare_request_body::<E>(core, deadline, head, endpoint.into_body())?;
    journal.set_target(head.request_target());
    let head: &PreparedHead = head;
    if let Some(meter) = meter {
        if !meter.check_body_contract(prepared_body.kind(), prepared_body.content_type()) {
            return Err(ApiError::pagination(
                head.endpoint_id().clone(),
                head.method().clone(),
                crate::pagination::PaginationError::new(
                    crate::error::PaginationErrorKind::ForbiddenContinuationChange,
                ),
            )
            .with_stage(
                FailureStage::PaginationDecision,
                DispatchKnowledge::NotDispatched,
            ));
        }
    }
    let env = AttemptEnv {
        core,
        head,
        meter,
        retry_policy,
        deadline,
        context,
        attempt_ceiling,
        controls,
        operation_id,
        provider_link,
    };

    let mut attempt_info = AttemptInfo::INITIAL;
    journal.begin_attempt(attempt_info);
    observe_attempt_started(
        core,
        operation_id,
        context,
        deadline,
        head,
        BodyFacts::of(&prepared_body),
        attempt_info,
    )?;

    // Phase 4: credential preflight.
    journal.enter(FailureStage::CredentialPreflight);
    let mut prepared_auth =
        run_auth(journal, env, head.auth().preflight(env.provider_http())).await?;
    let outcome = loop {
        let attempt_body = BodyFacts::of(&prepared_body);
        let attempt = match send_attempt(
            env,
            &mut prepared_body,
            attempt_body,
            prepared_auth,
            attempt_info,
            journal,
        )
        .await
        {
            Ok(attempt) => attempt,
            Err(error) => {
                // Only a native failure after handoff is a classifier input.
                let failure = match error.category() {
                    ErrorCategory::Connect => Some(RetryFailure::Connect),
                    ErrorCategory::Transport => Some(RetryFailure::Transport),
                    _ => None,
                };
                if let Some(failure) = failure {
                    if let Some(next) = policy_retry_after(
                        env,
                        journal,
                        attempt_info,
                        failure,
                        prepared_body.replayability(),
                        None,
                    )
                    .await?
                    {
                        attempt_info = next;
                        prepared_auth =
                            prepare_policy_retry(env, journal, &mut prepared_body, attempt_info)
                                .await?;
                        continue;
                    }
                }
                return Err(error);
            }
        };
        let status = attempt.response.status();
        if head.meta().etag_revalidation() && status == StatusCode::NOT_MODIFIED {
            let entity = attempt
                .revalidation_key
                .filter(|_| attempt.conditional_sent)
                .and_then(|key| core.revalidation_cache().get(key))
                .ok_or_else(|| {
                    ApiError::classified(
                        head.endpoint_id().clone(),
                        head.method().clone(),
                        ErrorCategory::ResponseContract,
                    )
                    .with_stage(
                        FailureStage::ResponseConversion,
                        DispatchKnowledge::ResponseObserved,
                    )
                })?;
            let response = crate::http::application::ApplicationResponse::from_entity(
                entity,
                head.endpoint_id().clone(),
                head.method().clone(),
            );
            break AttemptOutcome::Accepted(response, None);
        }

        if let Some(challenge_index) = attempt.applied_auth.challenge_index(status) {
            journal.enter(FailureStage::RejectedResponse);
            // RETRY-02: at most one recovery replay, only for a provider
            // source whose mode authorizes refresh, and only when the
            // effect/body/cap gates all permit resubmission.
            let can_recover = journal.counts().authentication_recoveries == 0
                && recovery_replay_permitted(
                    head,
                    prepared_body.replayability(),
                    journal.attempts_started(),
                    attempt_ceiling,
                )
                && attempt
                    .applied_auth
                    .challenge_can_recover(challenge_index)
                    .map_err(|error| auth_api_error(head, error))?;
            if !can_recover {
                attempt
                    .applied_auth
                    .apply_terminal_challenge(challenge_index)
                    .map_err(|error| auth_api_error(head, error))?;
                let failure = capture_terminal_status(env, journal, attempt).await;
                return Err(ApiError::authentication_rejected(failure));
            }

            // A response selected for recovery is dropped without polling its body.
            let AttemptResponse {
                response,
                applied_auth,
                ..
            } = attempt;
            drop(response);
            let next_index = journal.attempts_started();
            attempt_info = AttemptInfo {
                index: next_index,
                policy_retry_index: journal.policy_retries(),
                authentication_recovery: true,
            };
            journal.begin_attempt(attempt_info);
            if let Err(error) = observe_attempt_started(
                core,
                operation_id,
                context,
                deadline,
                head,
                BodyFacts::of(&prepared_body),
                attempt_info,
            ) {
                let _ = applied_auth.apply_terminal_challenge(challenge_index);
                return Err(error);
            }
            journal.enter(FailureStage::BeforeDispatch);
            if let Err(error) =
                prepared_body.reconstruct_for_next_attempt(core.request_body_limit())
            {
                drop(error);
                let _ = applied_auth.apply_terminal_challenge(challenge_index);
                return Err(ApiError::classified(
                    head.endpoint_id().clone(),
                    head.method().clone(),
                    ErrorCategory::RequestBody,
                ));
            }
            if let Err(error) = check_deadline(deadline, head.endpoint_id(), head.method()) {
                let _ = applied_auth.apply_terminal_challenge(challenge_index);
                return Err(error);
            }
            journal.enter(FailureStage::CredentialPreflight);
            prepared_auth = match run_auth(
                journal,
                env,
                applied_auth.recover_attributed(challenge_index, env.provider_http()),
            )
            .await
            {
                Ok(Some(plan)) => plan,
                Ok(None) => {
                    // An authorized refresh that yields no usable material
                    // never permits a replay.
                    let _ = applied_auth.apply_terminal_challenge(challenge_index);
                    return Err(auth_api_error(
                        head,
                        crate::auth::AuthError::new(crate::error::AuthErrorKind::Rejected),
                    ));
                }
                Err(error) => {
                    let _ = applied_auth.apply_terminal_challenge(challenge_index);
                    return Err(error);
                }
            };
            journal.record_recovered_authentication();
            continue;
        }

        // Head-only expected absence is resolved after challenge processing
        // and rate feedback (recorded in `send_attempt`) and before
        // accepted-head validation or any body poll.
        if absence
            .as_ref()
            .is_some_and(|absence| absence.contains(status))
        {
            break AttemptOutcome::Absent(attempt.response);
        }

        if head.accepted_status().contains(status) {
            break AttemptOutcome::Accepted(attempt.response, attempt.revalidation_key);
        }

        journal.enter(FailureStage::RejectedResponse);
        let failure = RetryFailure::RejectedStatus {
            status,
            server_delay: attempt.server_delay.map(ServerDelayHint::delay),
        };
        if let Some(next) = policy_retry_after(
            env,
            journal,
            attempt_info,
            failure,
            prepared_body.replayability(),
            attempt.server_delay,
        )
        .await?
        {
            // A rejected response selected for retry is dropped without
            // polling its body.
            drop(attempt);
            attempt_info = next;
            prepared_auth =
                prepare_policy_retry(env, journal, &mut prepared_body, attempt_info).await?;
            continue;
        }

        let failure = capture_terminal_status(env, journal, attempt).await;
        return Err(ApiError::from_http_response(failure));
    };

    let execution_meta = |journal: &OperationJournal| {
        ExecutionMeta::new(
            head.endpoint_id().clone(),
            head.method().clone(),
            context.page_index,
            journal.attempts_started(),
            journal.policy_retries(),
            journal.recovered_authentication(),
        )
        .with_provider_acquisition(journal.provider_acquisition())
    };
    let (response, revalidation_key) = match outcome {
        AttemptOutcome::Accepted(response, key) => (response, key),
        AttemptOutcome::Absent(response) => {
            let status = response.status();
            let version = response.version();
            let headers = response.headers().clone();
            // Drop the native body without polling or draining it: an
            // irrelevant huge, truncated or stalled body is never read. The
            // outcome is fixed by the observed head alone, so no later time
            // or body check can turn it into another result.
            drop(response);
            let output = absence
                .as_ref()
                .and_then(S::absent_output)
                .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
            return Ok((
                ApiResponse::new(
                    head.logical_url().clone(),
                    status,
                    version,
                    headers,
                    execution_meta(journal),
                    output,
                )
                .with_body_disposition(BodyDisposition::DiscardedUnread),
                0,
            ));
        }
    };

    // Accepted branch: acquisition and conversion failures are terminal and
    // never retried, whatever a classifier would recommend.
    journal.enter(FailureStage::ResponseAcquisition);
    let mut response = response;
    if let Some(key) = revalidation_key.filter(|_| response.status() == StatusCode::OK) {
        let tag = response.headers().get(http::header::ETAG).cloned();
        let no_store = response
            .headers()
            .get_all(http::header::CACHE_CONTROL)
            .iter()
            .any(|value| {
                value
                    .as_bytes()
                    .split(|byte| *byte == b',')
                    .any(|directive| {
                        let name = directive
                            .split(|byte| *byte == b'=')
                            .next()
                            .unwrap_or_default();
                        name.trim_ascii().eq_ignore_ascii_case(b"no-store")
                    })
            });
        if let Some(tag) = tag.filter(|_| !no_store) {
            response.capture_revalidation(core.clone(), key, tag);
        } else {
            // A replacing response invalidates the previous entity even if
            // acquiring or decoding the new response subsequently fails.
            core.revalidation_cache().store(key, None);
        }
    }
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();
    let content_length = response.content_length();
    let metadata = execution_meta(journal);
    let response_head = ResponseHead {
        endpoint: head.endpoint(),
        method: head.method(),
        url: head.logical_url(),
        status,
        version,
        headers: &headers,
        content_length,
        metadata: &metadata,
    };
    let response_cap = head.call_caps().response_byte_cap();
    // Stream handoff (STREAM-01/02): a streaming driver receives the original
    // correlation, the runtime stream ceilings narrowed by the call caps, the
    // whole-operation cap instant and the request's cancellation signals.
    // After handoff this executor performs no policy retry, authentication
    // recovery or native replay.
    // Select by the actual driver: accepted-bytes overrides and wrapper
    // representations need not use the declared endpoint acquisition mode.
    let stream = (S::Driver::ACQUISITION == ResponseAcquisition::Streaming).then(|| {
        crate::response::StreamContext::for_accepted_response(
            core.operation_observer().clone(),
            head,
            core.stream_limits(),
            deadline,
            operation_id,
            context.parent_operation_id,
            attempt_info,
            context.page_index,
            status,
        )
    });
    let stream_handoff = stream.as_ref().map(|stream| stream.handoff.clone());
    let stream_limit = stream.as_ref().map_or_else(
        || narrow_limit(core.stream_limits().bytes, response_cap),
        |stream| stream.limits.bytes,
    );
    let (output, response_body_bytes) = execute_response::<E::Response, E::Output, S>(
        ResponseInput {
            response,
            limit: narrow_limit(core.response_body_limit(), response_cap),
            stream_limit,
            deadline,
            endpoint: head.endpoint_id().clone(),
            method: head.method().clone(),
            stream,
        },
        &response_head,
    )
    .await?;
    if let Some(handoff) = stream_handoff {
        handoff.mark_delivered();
    }
    Ok((
        ApiResponse::new(
            head.logical_url().clone(),
            status,
            version,
            headers,
            metadata,
            output,
        )
        .with_body_disposition(body_disposition::<E::Response, E::Output, S>()),
        response_body_bytes,
    ))
}

/// A caller response-byte cap only narrows a configured acquisition ceiling.
fn narrow_limit(limit: NonZeroUsize, cap: Option<NonZeroUsize>) -> NonZeroUsize {
    cap.map_or(limit, |cap| limit.min(cap))
}

/// Captures the terminal rejected status body of the final attempt under the
/// deadline and the (possibly caller-narrowed) status-capture ceiling.
async fn capture_terminal_status(
    env: AttemptEnv<'_>,
    journal: &OperationJournal,
    attempt: AttemptResponse,
) -> crate::error::HttpResponseFailure {
    let head = env.head;
    let version = attempt.response.version();
    env.core
        .application_http()
        .terminal_status_managed(
            head.endpoint_id().clone(),
            head.method().clone(),
            attempt.response,
            attempt.server_delay.map(ServerDelayHint::delay),
            env.deadline.expires_at(),
            head.call_caps().response_byte_cap(),
        )
        .await
        .with_decode_facts(
            head.logical_url().clone(),
            version,
            ExecutionMeta::new(
                head.endpoint_id().clone(),
                head.method().clone(),
                env.context.page_index,
                journal.attempts_started(),
                journal.policy_retries(),
                journal.recovered_authentication(),
            )
            .with_provider_acquisition(journal.provider_acquisition()),
        )
}

/// RETRY-02 authentication-recovery replay authority, checked before any
/// provider refresh is requested.
///
/// A replay requires a safe or idempotent effect (a non-idempotent operation
/// that received a response is never resubmitted automatically, because a
/// challenge status alone does not prove the write had no effect), a
/// mechanically replayable body, and a remaining visible attempt under the
/// ceiling (which already intersects the caller's send and replay caps, and
/// for a page the traversal's remaining aggregate budgets). The replay itself
/// still passes a new rate admission and the deadline.
fn recovery_replay_permitted(
    head: &PreparedHead,
    replayability: Replayability,
    attempts_started: u32,
    attempt_ceiling: u32,
) -> bool {
    head.effect() != crate::endpoint::Effect::NonIdempotent
        && replayability != Replayability::OneShot
        && attempts_started < attempt_ceiling
}

/// Classifies a failed attempt and, when every RETRY-01 gate passes, sleeps
/// the required wait and reserves shared retry-load budget. Returns the next
/// attempt's identity when a policy replay was committed.
async fn policy_retry_after(
    env: AttemptEnv<'_>,
    journal: &mut OperationJournal,
    attempt: AttemptInfo,
    failure: RetryFailure,
    body_replayability: Replayability,
    server_delay: Option<ServerDelayHint>,
) -> Result<Option<AttemptInfo>, ApiError> {
    let Some(policy) = env.retry_policy else {
        return Ok(None);
    };
    let head = env.head;
    check_deadline(env.deadline, head.endpoint_id(), head.method())?;
    let decision = policy.classify(RetryAttempt::new(
        head.endpoint(),
        head.method(),
        head.classifier_host(),
        env.context.page_index,
        attempt,
        journal.policy_retries(),
        failure,
        journal.last_attempt_knowledge(),
    ));
    check_deadline(env.deadline, head.endpoint_id(), head.method())?;
    if decision != RetryDecision::Retry {
        return Ok(None);
    }
    admit_policy_retry(PolicyRetryAdmission {
        policy,
        operation_id: journal.operation_id(),
        env,
        body_replayability,
        dispatch_knowledge: journal.aggregate(),
        server_delay,
        attempts_started: journal.attempts_started(),
        policy_retries: journal.policy_retries(),
    })
    .await
}

struct PolicyRetryAdmission<'a> {
    policy: &'a RetryPolicy,
    operation_id: OperationId,
    env: AttemptEnv<'a>,
    body_replayability: Replayability,
    dispatch_knowledge: DispatchKnowledge,
    server_delay: Option<ServerDelayHint>,
    attempts_started: u32,
    policy_retries: u32,
}

/// RETRY-01 conjunctive policy-replay authority (after classifier `Retry`):
///
/// 1. settled no dispatch, or a safe/idempotent effect;
/// 2. a remaining per-request/page policy count;
/// 3. a mechanically replayable body;
/// 4. a remaining visible attempt under the ceiling (aggregate send and
///    replay caps);
/// 5. a finite server minimum: an overlarge hint forbids retry, and the
///    untruncated remaining minimum must be within willingness;
/// 6. the wait `max(backoff, server_not_before_remaining)` (never their sum)
///    must end before the deadline;
/// 7. shared retry-load budget, reserved after the wait immediately before
///    committing (cancellation while sleeping consumes no reservation).
///
/// The committed attempt then passes a new rate admission in
/// `send_attempt`.
async fn admit_policy_retry(
    admission: PolicyRetryAdmission<'_>,
) -> Result<Option<AttemptInfo>, ApiError> {
    let PolicyRetryAdmission {
        policy,
        operation_id,
        env,
        body_replayability,
        dispatch_knowledge,
        server_delay,
        attempts_started,
        policy_retries,
    } = admission;
    let head = env.head;
    if head.effect() == crate::endpoint::Effect::NonIdempotent
        && dispatch_knowledge != DispatchKnowledge::NotDispatched
    {
        return Ok(None);
    }
    if policy_retries >= policy.max_retries_per_operation() {
        return Ok(None);
    }
    if body_replayability == Replayability::OneShot {
        return Ok(None);
    }

    let next_policy_retries = policy_retries
        .checked_add(1)
        .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
    let next_attempts_started = attempts_started
        .checked_add(1)
        .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
    if next_attempts_started > env.attempt_ceiling {
        return Ok(None);
    }

    let server_minimum = match server_delay {
        Some(hint) if hint.forbids_retry() => return Ok(None),
        Some(hint) => Some(hint.remaining_at(OperationDeadline::now())),
        None => None,
    };
    let entropy = operation_id.retry_entropy(next_policy_retries, env.context.page_index);
    let Some(delay) = policy.retry_delay(next_policy_retries, entropy, server_minimum) else {
        return Ok(None);
    };
    if env
        .deadline
        .remaining()
        .is_some_and(|remaining| delay >= remaining)
    {
        return Ok(None);
    }
    if !delay.is_zero() {
        env.deadline
            .run(tokio::time::sleep(delay))
            .await
            .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?;
    }
    check_deadline(env.deadline, head.endpoint_id(), head.method())?;
    if !policy.reserve_retry(head.logical_url()) {
        return Ok(None);
    }

    Ok(Some(AttemptInfo {
        index: attempts_started,
        policy_retry_index: next_policy_retries,
        authentication_recovery: false,
    }))
}

/// Starts the committed policy-retry attempt: observation, body
/// reconstruction and a fresh credential preflight.
async fn prepare_policy_retry(
    env: AttemptEnv<'_>,
    journal: &mut OperationJournal,
    prepared_body: &mut ManagedBody,
    attempt: AttemptInfo,
) -> Result<PreparedAuthPlan, ApiError> {
    let head = env.head;
    journal.begin_attempt(attempt);
    observe_attempt_started(
        env.core,
        journal.operation_id(),
        env.context,
        env.deadline,
        head,
        BodyFacts::of(prepared_body),
        attempt,
    )?;
    journal.enter(FailureStage::BeforeDispatch);
    prepared_body
        .reconstruct_for_next_attempt(env.core.request_body_limit())
        .map_err(|error| {
            drop(error);
            ApiError::classified(
                head.endpoint_id().clone(),
                head.method().clone(),
                ErrorCategory::RequestBody,
            )
        })?;
    check_deadline(env.deadline, head.endpoint_id(), head.method())?;
    journal.enter(FailureStage::CredentialPreflight);
    run_auth(journal, env, head.auth().preflight(env.provider_http())).await
}

/// Completes a phase-0–2 preparation failure of a started operation: the
/// error is contextualized (`Preparation`, `NotDispatched`, operation
/// identity) and the final observation is emitted once.
pub(crate) fn complete_preparation_failure<T>(failure: PreparationFailure) -> Result<T, ApiError> {
    let PreparationFailure {
        core,
        operation_id,
        meta,
        target,
        debug,
        started,
        error,
    } = failure;
    let mut journal = OperationJournal::new(
        core,
        operation_id,
        ExecutionContext::DIRECT,
        meta.endpoint_id().clone(),
        meta.method().clone(),
        target,
        debug,
        started,
    );
    journal.enter(FailureStage::Preparation);
    journal.complete(Err(error), crate::hooks::CompletionOutcome::Success)
}

fn observe_attempt_started(
    core: &ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &PreparedHead,
    body: BodyFacts,
    attempt: AttemptInfo,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let event = attempt_started_event(operation_id, context, head, body, attempt);
    crate::hooks::observe(
        core.operation_observer().as_ref(),
        OperationEvent::AttemptStarted(event),
    );
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    Ok(())
}

fn observe_response_head(
    core: &ClientCore,
    deadline: OperationDeadline,
    endpoint_id: &EndpointId,
    event: ResponseHeadObserved<'_>,
) -> Result<(), ApiError> {
    let method = event.method.clone();
    // This is factual notification of an already received head. A synchronous
    // earlier callback cannot erase it when it consumes the remaining time.
    crate::hooks::observe(
        core.operation_observer().as_ref(),
        OperationEvent::ResponseHeadObserved(event),
    );
    check_deadline(deadline, endpoint_id, &method)
}

fn attempt_started_event<'a>(
    operation_id: OperationId,
    context: ExecutionContext,
    head: &'a PreparedHead,
    body: BodyFacts,
    attempt: AttemptInfo,
) -> AttemptStarted<'a> {
    AttemptStarted {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        target: head.request_target(),
        page_index: context.page_index,
        attempt,
        body_kind: body.kind,
        known_body_length: body.known_length,
    }
}

struct AttemptResponse {
    response: crate::http::application::ApplicationResponse,
    applied_auth: PreparedAuthPlan,
    server_delay: Option<ServerDelayHint>,
    revalidation_key: Option<crate::revalidation::CacheKey>,
    conditional_sent: bool,
}

/// The selected result branch of the final attempt.
enum AttemptOutcome {
    /// A payload-success status on the ordinary accepted branch.
    Accepted(
        crate::http::application::ApplicationResponse,
        Option<crate::revalidation::CacheKey>,
    ),
    /// A declared head-only expected-absence status.
    Absent(crate::http::application::ApplicationResponse),
}

/// Structural body facts reported to observers and transport evidence.
#[derive(Clone, Copy)]
struct BodyFacts {
    kind: BodyKind,
    known_length: Option<u64>,
}

impl BodyFacts {
    fn of(body: &ManagedBody) -> Self {
        Self {
            kind: body.kind(),
            known_length: body.known_length(),
        }
    }
}

/// Phase 3: prepares the one request body for this operation or page.
///
/// The representation runs once, under the deadline, after public head
/// selection and before credential acquisition or rate admission. Known
/// length is checked against the configured limit and the prepared body's
/// exact `Content-Type` becomes the head's only body media header. An invalid
/// body therefore fails before any provider or limiter work.
fn prepare_request_body<E>(
    core: &ClientCore,
    deadline: OperationDeadline,
    head: &mut PreparedHead,
    input: E::Input,
) -> Result<ManagedBody, ApiError>
where
    E: EndpointSpec,
{
    let endpoint = head.endpoint_id().clone();
    let method = head.method().clone();
    check_deadline(deadline, &endpoint, &method)?;
    let prepared = <E::Body as RequestRepresentation<E::Input>>::prepare(
        input,
        RequestBodyContext::new(
            &endpoint,
            &method,
            core.request_body_limit(),
            deadline.remaining(),
        ),
    );
    check_deadline(deadline, &endpoint, &method)?;
    let managed = prepared
        .and_then(|body| body.into_managed(core.request_body_limit()))
        .map_err(|error| request_body_error(head, error))?;
    check_deadline(deadline, &endpoint, &method)?;
    head.apply_body_content_type(managed.content_type())?;
    Ok(managed)
}

/// Phases 5–10 of one visible attempt: rate admission, final credential
/// freshness (with at most [`MAX_FRESHNESS_RESTARTS`] restarts), native
/// framing and placement, the send-cap and cancellation checks, native
/// handoff under the provisional dispatch token, and response-head facts.
async fn send_attempt(
    env: AttemptEnv<'_>,
    prepared_body: &mut ManagedBody,
    body: BodyFacts,
    prepared_auth: PreparedAuthPlan,
    attempt: AttemptInfo,
    journal: &mut OperationJournal,
) -> Result<AttemptResponse, ApiError> {
    let AttemptEnv {
        core,
        head,
        meter,
        retry_policy,
        deadline,
        context,
        attempt_ceiling,
        controls,
        operation_id: _,
        provider_link: _,
    } = env;
    journal.enter(FailureStage::BeforeDispatch);
    let native_body = prepared_body
        .prepare_native(core.request_body_limit())
        .map_err(|error| {
            drop(error);
            ApiError::classified(
                head.endpoint_id().clone(),
                head.method().clone(),
                ErrorCategory::RequestBody,
            )
        })?;
    check_deadline(deadline, head.endpoint_id(), head.method())?;

    // Phases 5–6: conjunctive admission, then the final freshness check. A
    // changed credential restarts admission at most twice; admissions already
    // committed stay consumed, the deadline is never reset and no attempt or
    // handoff is counted for a restart.
    let mut candidate = prepared_auth;
    let mut restarts = 0_u32;
    let (applied_auth, finalized_auth) = loop {
        journal.enter(FailureStage::RateAdmission);
        acquire_rate_limit_authority(core, context, attempt, deadline, head).await?;
        journal.record_admission();
        if let Some(meter) = meter {
            meter.record_quota_admission();
        }
        journal.enter(FailureStage::BeforeDispatch);
        let (applied, finalized) =
            run_auth(journal, env, candidate.finalize(env.provider_http())).await?;
        if applied
            .applies_same_material(&candidate)
            .map_err(|error| auth_api_error(head, error))?
        {
            break (applied, finalized);
        }
        drop(finalized);
        if restarts >= MAX_FRESHNESS_RESTARTS {
            // Repeated churn: a credential-unavailable failure whose journal
            // carries the admission and contact counts.
            return Err(auth_api_error(
                head,
                crate::auth::AuthError::new(crate::error::AuthErrorKind::Unavailable),
            ));
        }
        restarts = restarts.saturating_add(1);
        journal.record_freshness_restart();
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        candidate = applied;
    };

    let mut request = core.application_http().build_request(
        head.endpoint_id(),
        head.method(),
        head.logical_url(),
        head.public_headers(),
        native_body,
    )?;
    let request_event = attempt_started_event(journal.operation_id(), context, head, body, attempt);
    core.debug_dispatcher(head.debug())
        .request_ready(&request_event);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    finalized_auth.materialize(&mut request, head.endpoint_id(), head.method())?;
    let revalidation_key = head
        .meta()
        .etag_revalidation()
        .then(|| request.revalidation_key(core.revalidation_cache(), head.endpoint()));
    let mut conditional_sent = false;
    if let Some(entity) = revalidation_key.and_then(|key| core.revalidation_cache().get(key)) {
        request.insert_auth_header(http::header::IF_NONE_MATCH, entity.tag);
        conditional_sent = true;
    }
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    // CONTROL-03: a cancellation signal observed before handoff prevents it.
    if controls.is_some_and(RequestControlState::is_cancelled) {
        return Err(cancelled_error(head.endpoint_id(), head.method()));
    }
    // CONTROL-02: the send cap (and for a page the traversal's remaining
    // aggregate budget) counts every application handoff.
    if journal.counts().application_handoffs >= attempt_ceiling {
        return Err(send_cap_error(head.endpoint_id(), head.method()));
    }
    if attempt.index == 0 {
        if let Some(policy) = retry_policy {
            policy.record_initial_dispatch(head.logical_url());
        }
    }

    // Phase 7: native handoff under the provisional dispatch token.
    if let Some(meter) = meter {
        meter.record_handoff();
    }
    journal.handoff();
    let execution = core.application_http().execute(
        head.endpoint_id(),
        head.method(),
        request,
        head.observed_url(),
        head.public_headers(),
        head.pagination_sensitive_headers(),
        body.kind,
        body.known_length,
    );
    let mut received_head = None;
    let execution = async {
        let mut response = execution.await?;
        // Facts transfer synchronously with native ownership, before the outer
        // deadline's post-check can discard an otherwise ready response.
        let status = response.status();
        journal.record_head(status);
        journal.enter(if head.accepted_status().contains(status) {
            FailureStage::ResponseAcquisition
        } else {
            FailureStage::RejectedResponse
        });
        if let Some(meter) = meter {
            meter.record_response_head();
            response.meter_acquired_bytes(meter.acquired_bytes());
        }
        let arrived_at = OperationDeadline::now();
        let response_wall_time = core.response_wall_time();
        let server_delay = parse_server_delay(response.headers(), response_wall_time, arrived_at);
        let response_event = ResponseHeadObserved {
            operation_id: journal.operation_id(),
            parent_operation_id: context.parent_operation_id,
            endpoint: head.endpoint(),
            method: head.method(),
            target: head.request_target(),
            page_index: context.page_index,
            attempt,
            status,
            content_length: parsed_content_length(response.headers()),
            server_delay: server_delay.map(ServerDelayHint::delay),
        };
        received_head = Some((server_delay, response_event));
        Ok::<_, crate::http::application::DispatchFailure>(response)
    };
    let response = match deadline.run(execution).await {
        Err(_) => {
            // Output remains abandoned. This emits neutral evidence of a head
            // actually received, never limiter feedback/retry/challenge/body work.
            if let Some((_, response_event)) = received_head.take() {
                core.debug_dispatcher(head.debug())
                    .response_head(&response_event);
                crate::hooks::observe(
                    core.operation_observer().as_ref(),
                    OperationEvent::ResponseHeadObserved(response_event),
                );
            }
            return Err(timeout_error(head.endpoint_id(), head.method()));
        }
        Ok(Err(failure)) => {
            journal.settle_transport_failure(failure.proven_not_submitted);
            return Err(failure.error);
        }
        Ok(Ok(response)) => response,
    };
    let (server_delay, response_event) = received_head
        .take()
        .expect("a returned native response has recorded head facts");
    let status = response_event.status;
    core.debug_dispatcher(head.debug())
        .response_head(&response_event);
    if deadline.check().is_ok() {
        observe_rate_limit_response(core, context, attempt, head, status, server_delay);
    }
    observe_response_head(core, deadline, head.endpoint_id(), response_event)?;
    Ok(AttemptResponse {
        response,
        applied_auth,
        server_delay,
        revalidation_key,
        conditional_sent,
    })
}

impl<'a> AttemptEnv<'a> {
    /// The provider capability for this operation's credential work: bounded
    /// by the remaining deadline and scoped to this operation, so provider
    /// acquisitions it leads name it as their parent (EXEC-01).
    fn provider_http(&self) -> ProviderHttp<'a> {
        self.core
            .provider_http(Some(self.deadline))
            .controlled(self.controls)
            .scoped(ProviderScope::new(
                self.operation_id,
                self.core.operation_observer().as_ref(),
                self.provider_link,
            ))
    }
}

/// Runs credential work under the operation deadline, then links every
/// provider acquisition it led or joined into the operation journal, whether
/// the work succeeded, failed or ran out of time.
async fn run_auth<T>(
    journal: &mut OperationJournal,
    env: AttemptEnv<'_>,
    future: impl std::future::Future<Output = Result<T, crate::auth::AuthError>>,
) -> Result<T, ApiError> {
    let head = env.head;
    let result = env.deadline.run(future).await;
    journal.record_provider(env.provider_link.take());
    // A synchronous provider/material callback may cancel during this poll.
    // The outer select cannot observe that until polling returns; preserve
    // cancellation as the operation result after the provider has retired its
    // flight and linked the truthful provider journal.
    if env.controls.is_some_and(RequestControlState::is_cancelled) {
        return Err(cancelled_error(head.endpoint_id(), head.method()));
    }
    result
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?
        .map_err(|error| auth_api_error(head, error))
}

fn auth_api_error(head: &PreparedHead, error: crate::auth::AuthError) -> ApiError {
    ApiError::authentication(head.endpoint_id().clone(), head.method().clone(), error)
}

fn request_body_error(head: &PreparedHead, error: crate::body::BodyError) -> ApiError {
    let category = if error.kind() == crate::body::BodyErrorKind::Producer {
        ErrorCategory::RequestEncoding
    } else {
        ErrorCategory::RequestBody
    };
    drop(error);
    ApiError::classified(head.endpoint_id().clone(), head.method().clone(), category)
}

async fn acquire_rate_limit_authority(
    core: &ClientCore,
    context: ExecutionContext,
    attempt: AttemptInfo,
    deadline: OperationDeadline,
    head: &PreparedHead,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let request = RateLimitRequest {
        cooldown_origin: head.cooldown_origin(),
        endpoint: head.endpoint(),
        method: head.method(),
        logical_origin: head.rate_limit_logical_origin(),
        page_index: context.page_index,
        attempt,
        plan: head.rate_limit_plan(),
        capability: core.rate_limiter_capability(),
    };
    let future = core.rate_limiter().acquire(request);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?
        .map(|_permit| ())
        .map_err(|error| {
            ApiError::rate_limit(head.endpoint_id().clone(), head.method().clone(), error)
        })
}

fn observe_rate_limit_response(
    core: &ClientCore,
    context: ExecutionContext,
    attempt: AttemptInfo,
    head: &PreparedHead,
    status: StatusCode,
    server_delay: Option<ServerDelayHint>,
) {
    let response = RateLimitResponse {
        request: RateLimitRequest {
            cooldown_origin: head.cooldown_origin(),
            endpoint: head.endpoint(),
            method: head.method(),
            logical_origin: head.rate_limit_logical_origin(),
            page_index: context.page_index,
            attempt,
            plan: head.rate_limit_plan(),
            capability: core.rate_limiter_capability(),
        },
        status,
        server_delay: server_delay.map(ServerDelayHint::delay),
        cooldown_cap: core.rate_limit_cooldown_cap(),
    };
    core.rate_limiter().observe(response);
}

fn parsed_content_length(headers: &http::HeaderMap) -> Option<u64> {
    let value = headers.get(http::header::CONTENT_LENGTH)?.to_str().ok()?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.bytes().try_fold(0_u64, |length, byte| {
        length.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
    })
}

fn check_deadline(
    deadline: OperationDeadline,
    endpoint: &EndpointId,
    method: &Method,
) -> Result<(), ApiError> {
    deadline
        .check()
        .map_err(|_| timeout_error(endpoint, method))
}

fn timeout_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout)
}

pub(crate) fn cancelled_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Cancelled)
}

pub(crate) fn send_cap_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(
        endpoint.clone(),
        method.clone(),
        ErrorCategory::SendCapExhausted,
    )
}

fn internal_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Internal)
}

mod store;

pub use store::StoreRequest;
pub(crate) use store::store_request;

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod tests;
