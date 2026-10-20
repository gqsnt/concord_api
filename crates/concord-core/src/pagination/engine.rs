//! The single sequential traversal state machine behind every page terminal.
//!
//! One [`Traversal`] owns the originating request (core binding, captured
//! controls and the page-stable endpoint template), the controller state, the
//! cycle detector and the aggregate accounting of one traversal. It fetches at
//! most one page at a time and never prefetches: the next page is requested only
//! when a terminal asks for it. All terminals (`Pages`, `CollectPrefix`,
//! `CollectAll`) drive the same machine through the object-safe
//! [`ErasedTraversal`] so that their public types only mention the item type.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use http::{Method, StatusCode};

use crate::body::PageRequestRepresentation;
use crate::client::ClientCore;
use crate::endpoint::{AcceptedStatus, Effect, EndpointId};
use crate::error::{
    ApiError, ConfigurationErrorKind, DispatchKnowledge, ErrorCategory, ErrorJournal, FailureStage,
    PaginationErrorKind,
};
use crate::execute::{ExecutedPage, execute_pagination_page};
use crate::hooks::{BoxFuture, CompletionOutcome, OperationEvent, PaginationFinished};
use crate::operation::OperationDeadline;
use crate::request::{
    HeadBuilder, PreparedHead, Request, RequestMeta, RequestOptions, prepare_pagination_page,
};
use crate::response::{ApiResponse, Buffered, ExecutionMeta, ResponseRepresentation};

use super::bridge::{PageItem, PageableEndpoint};
use super::identity::CanonicalIdentity;
use super::limits::{CycleDetector, MAX_PAGE_ATTEMPTS};
use super::meter::{MeterSnapshot, TraversalMeter};
use super::summary::{ContinuationKnowledge, StopReason, TraversalSummary, join_dispatch};
use super::terminals::Page;
use super::{PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationError};

/// Slot through which a synchronous drop guard publishes the final summary of
/// a traversal whose owner can no longer ask it (for example after an
/// in-flight `next_page` future was dropped).
pub(super) type DropReport = Arc<Mutex<Option<TraversalSummary>>>;

/// How the terminal consumes the traversal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TraversalMode {
    /// `pages` and `collect_prefix`: a reached bound is an explicit prefix.
    Incremental,
    /// `collect_all`: a reached bound with a known continuation is a
    /// completion-limit error, never a success.
    Complete,
}

/// Result of one erased fetch: the traversal is handed back to its owner.
pub(super) struct Fetched<Item> {
    pub(super) traversal: Box<dyn ErasedTraversal<Item>>,
    pub(super) result: Result<Option<Page<Item>>, ApiError>,
}

/// Object-safe view of a traversal that hides endpoint/controller generics.
pub(super) trait ErasedTraversal<Item>: Send {
    /// Moves the traversal into one page fetch. Dropping the returned future
    /// after it was polled cancels the traversal through the drop guard.
    fn fetch(self: Box<Self>) -> BoxFuture<'static, Fetched<Item>>;

    /// Returns the current value-free summary.
    fn summary(&self) -> TraversalSummary;

    fn checkpoint(&self) -> Result<super::Checkpoint, super::CheckpointError>;

    /// Records an application stop unless already terminal and returns the
    /// final summary.
    fn finish(&mut self) -> TraversalSummary;

    /// Builds a contextualized internal error (with the current summary) for
    /// a terminal violating a traversal invariant.
    fn invariant_error(&self) -> ApiError;
}

/// Fixed request facts a continuation must never change.
struct FixedFacts {
    endpoint: EndpointId,
    method: Method,
    effect: Effect,
    timeout: Option<Duration>,
    accepted_status: AcceptedStatus,
    origin: url::Origin,
    rate_plan: Vec<u8>,
}

impl FixedFacts {
    fn from_head(head: &PreparedHead) -> Self {
        Self {
            endpoint: head.endpoint_id().clone(),
            method: head.method().clone(),
            effect: head.effect(),
            timeout: head.timeout(),
            accepted_status: head.accepted_status().clone(),
            origin: head.logical_url().origin(),
            rate_plan: rate_plan_fingerprint(head),
        }
    }

    fn check(&self, head: &PreparedHead) -> Result<(), PaginationErrorKind> {
        if head.logical_url().origin() != self.origin {
            return Err(PaginationErrorKind::CrossOriginContinuation);
        }
        if *head.endpoint_id() != self.endpoint
            || *head.method() != self.method
            || head.effect() != self.effect
            || head.timeout() != self.timeout
            || *head.accepted_status() != self.accepted_status
            || rate_plan_fingerprint(head) != self.rate_plan
        {
            return Err(PaginationErrorKind::ForbiddenContinuationChange);
        }
        Ok(())
    }
}

/// Canonical length-prefixed encoding of the effective rate-limit plan: policy
/// identities, ordered protected key parts, costs and windows.
fn rate_plan_fingerprint(head: &PreparedHead) -> Vec<u8> {
    fn field(output: &mut Vec<u8>, bytes: &[u8]) {
        output.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        output.extend_from_slice(bytes);
    }
    let mut output = Vec::new();
    for policy in head.rate_limit_plan().policies() {
        field(&mut output, policy.id().as_str().as_bytes());
        for part in policy.key().parts() {
            field(&mut output, part.name().as_bytes());
            field(&mut output, part.value());
        }
        output.extend_from_slice(&policy.cost().get().to_be_bytes());
        for window in policy.windows() {
            output.extend_from_slice(&window.capacity().get().to_be_bytes());
            output.extend_from_slice(&window.duration().as_nanos().to_be_bytes());
        }
        output.push(0xff);
    }
    output
}

struct TraversalLimits {
    max_items: usize,
    identity_bytes: usize,
    max_handoffs: u32,
    send_cap_binding: bool,
    replay_cap: Option<u32>,
}

enum Phase<E>
where
    E: PageableEndpoint,
{
    Unstarted,
    Ready {
        controller: E::Controller,
        endpoint: E,
    },
    Terminal,
}

/// An error that ends the traversal, with the stop reason it records.
struct Stop {
    error: ApiError,
    reason: StopReason,
}

impl Stop {
    const fn failed(error: ApiError) -> Self {
        Self {
            error,
            reason: StopReason::Failed,
        }
    }
}

impl From<ApiError> for Stop {
    fn from(error: ApiError) -> Self {
        Self::failed(error)
    }
}

pub(super) struct Traversal<E>
where
    E: PageableEndpoint,
{
    core: ClientCore,
    options: RequestOptions,
    meta: RequestMeta,
    template: E,
    mode: TraversalMode,
    phase: Phase<E>,
    summary: TraversalSummary,
    limits: TraversalLimits,
    meter: Arc<TraversalMeter>,
    detector: CycleDetector,
    fixed: Option<FixedFacts>,
    deadline: Option<OperationDeadline>,
    started_at: Option<tokio::time::Instant>,
    final_status: Option<StatusCode>,
    policy_retries: u32,
    recovered_authentication: bool,
    // Current fetch/decision correlation is independent of received-page count.
    active_page_index: u32,
    in_flight: Option<MeterSnapshot>,
    observed: bool,
    drop_report: DropReport,
    checkpoint_position: Option<(Vec<u8>, u32)>,
    restored: Option<E::Controller>,
    page_index_base: u32,
}

type Item<E> = PageItem<E>;

impl<E> Traversal<E>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    /// Captures an inert traversal. No shaping, controller start, identity
    /// allocation, body, credential, quota or transport work happens here.
    pub(super) fn new(
        request: Request<E>,
        requested_page_bound: u32,
        mode: TraversalMode,
        drop_report: DropReport,
    ) -> Self {
        let Request {
            core,
            endpoint,
            options,
        } = request;
        let meta = endpoint.request_meta();
        let effective_page_bound = requested_page_bound.min(core.pagination_max_pages().get());
        let caps = options.controls().caps();
        let send_cap = caps.send_cap();
        let aggregate_max_sends = core.aggregate_max_sends().get();
        let max_handoffs = send_cap.map_or(aggregate_max_sends, |cap| cap.min(aggregate_max_sends));
        let byte_ceiling = caps.response_byte_cap().map_or(
            core.pagination_total_response_body_limit().get(),
            |cap| {
                cap.get()
                    .min(core.pagination_total_response_body_limit().get())
            },
        );
        let limits = TraversalLimits {
            max_items: core.pagination_max_items().get(),
            identity_bytes: core.pagination_identity_limit().get(),
            max_handoffs,
            send_cap_binding: send_cap.is_some_and(|cap| cap <= aggregate_max_sends),
            replay_cap: caps.replay_cap(),
        };
        Self {
            meter: Arc::new(TraversalMeter::new(
                u64::try_from(byte_ceiling).unwrap_or(u64::MAX),
            )),
            core,
            options,
            meta,
            template: endpoint,
            mode,
            phase: Phase::Unstarted,
            summary: TraversalSummary::unstarted(requested_page_bound, effective_page_bound),
            limits,
            detector: CycleDetector::default(),
            fixed: None,
            deadline: None,
            started_at: None,
            final_status: None,
            policy_retries: 0,
            recovered_authentication: false,
            active_page_index: 0,
            in_flight: None,
            observed: false,
            drop_report,
            checkpoint_position: None,
            restored: None,
            page_index_base: 0,
        }
    }

    pub(super) fn zero_bound(&mut self) {
        self.phase = Phase::Terminal;
        self.summary.stop_reason = StopReason::PageBound;
    }

    pub(super) fn resume(
        &mut self,
        checkpoint: &super::Checkpoint,
    ) -> Result<(), super::CheckpointError>
    where
        E::Controller: super::ResumableController<E::Output>,
    {
        use super::{CheckpointError as Error, CheckpointErrorKind as Kind};
        if self.meta.endpoint_id().as_str() != checkpoint.endpoint {
            return Err(Error::new(Kind::EndpointMismatch));
        }
        if std::any::type_name::<E::Controller>() != checkpoint.controller {
            return Err(Error::new(Kind::ControllerMismatch));
        }
        if self.template.checkpoint_sensitive_cursor() {
            return Err(Error::new(Kind::SensitiveCursor));
        }
        let fingerprint = super::identity::deterministic_digest(&self.template.checkpoint_input()?);
        if fingerprint != checkpoint.fingerprint {
            return Err(Error::new(Kind::InputMismatch));
        }
        let codec = super::CheckpointCodec::<E::Controller, E::Output>::new();
        let controller = codec
            .restore(&checkpoint.state)
            .map_err(|_| Error::new(Kind::Malformed))?;
        self.template.apply_cursor(controller.cursor());
        self.page_index_base = checkpoint.next_index;
        self.active_page_index = checkpoint.next_index;
        self.restored = Some(controller);
        self.checkpoint_position = Some((checkpoint.state.clone(), checkpoint.next_index));
        Ok(())
    }

    fn save_checkpoint(&self) -> Result<super::Checkpoint, super::CheckpointError> {
        use super::{CheckpointError as Error, CheckpointErrorKind as Kind};
        let codec = self
            .template
            .checkpoint_codec()
            .ok_or_else(|| Error::new(Kind::NotResumable))?;
        if self.template.checkpoint_sensitive_cursor() {
            return Err(Error::new(Kind::SensitiveCursor));
        }
        if matches!(
            self.summary.stop_reason,
            StopReason::Exhausted | StopReason::Application | StopReason::Cancelled
        ) {
            return Err(Error::new(Kind::Finished));
        }
        let (state, next_index) = match &self.checkpoint_position {
            Some((state, index)) => (state.clone(), *index),
            None if matches!(self.phase, Phase::Unstarted) || self.summary.pages_received == 0 => {
                let controller = <E::Controller as PaginationController<E::Output>>::start(
                    self.template.controller_init(),
                )
                .map_err(|_| Error::new(Kind::Malformed))?;
                (codec.save(&controller), self.page_index_base)
            }
            None => return Err(Error::new(Kind::Finished)),
        };
        super::Checkpoint {
            endpoint: self.meta.endpoint_id().as_str().to_owned(),
            controller: std::any::type_name::<E::Controller>().to_owned(),
            fingerprint: super::identity::deterministic_digest(&self.template.checkpoint_input()?),
            state,
            next_index,
        }
        .validate_size()
    }

    async fn fetch_next(&mut self) -> Result<Option<Page<Item<E>>>, ApiError> {
        match self.phase {
            Phase::Terminal => return Ok(None),
            Phase::Unstarted => {
                if self.options.controls().is_cancelled() {
                    // Observed before any work: nothing was started, so no
                    // identity is allocated and nothing is observed.
                    self.phase = Phase::Terminal;
                    self.summary.stop_reason = StopReason::Cancelled;
                    let error = self.pagination_error(
                        PaginationErrorKind::Cancelled,
                        FailureStage::BeforeDispatch,
                    );
                    return Err(error.with_traversal_summary(self.snapshot()));
                }
                if let Err(stop) = self.start() {
                    return Err(self.terminate(stop));
                }
            }
            Phase::Ready { .. } => {}
        }
        match self.fetch_ready().await {
            Ok(page) => Ok(page),
            Err(stop) => Err(self.terminate(stop)),
        }
    }

    /// First poll of a nonzero traversal: operation identity, the single
    /// aggregate deadline, eligibility, controller start and the initial
    /// cursor identity check. Performs only bounded public preparation.
    fn start(&mut self) -> Result<(), Stop> {
        let started = OperationDeadline::now();
        let operation_id = self
            .core
            .allocate_operation_id(self.meta.endpoint_id(), self.meta.method())?;
        self.summary.operation_id = Some(operation_id);
        self.summary.stop_reason = StopReason::Active;
        self.started_at = Some(started);
        let timeout = self.options.resolve_timeout(
            self.meta
                .resolve_inherited_timeout(self.core.default_runtime_endpoint_timeout()),
        );
        let deadline = OperationDeadline::new(started, timeout).map_err(|_| {
            ApiError::configuration(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidValue,
            )
        })?;
        self.deadline = Some(deadline);

        if self.meta.effect() != Effect::Safe {
            return Err(self.decision_stop(PaginationErrorKind::IneligibleEndpoint));
        }
        // A zero send budget stops before any unrelated preparation.
        if self.limits.max_handoffs == 0 {
            return Err(self.decision_stop(self.handoff_exhaustion_kind()));
        }

        self.check_deadline(FailureStage::Preparation)?;
        let controller = match self.restored.take() {
            Some(controller) => Ok(controller),
            None => <E::Controller as PaginationController<E::Output>>::start(
                self.template.controller_init(),
            ),
        };
        self.check_deadline(FailureStage::Preparation)?;
        let controller = controller.map_err(|error| self.pagination_stop(error))?;

        if let Some(codec) = self.template.checkpoint_codec() {
            self.checkpoint_position = Some((codec.save(&controller), self.page_index_base));
        }

        let current_head = self.public_head(&self.template)?;
        let current_identity = self.identity(&current_head)?;
        self.check_deadline(FailureStage::Preparation)?;
        let cursor = <E::Controller as PaginationController<E::Output>>::cursor(&controller);
        self.check_deadline(FailureStage::Preparation)?;
        let mut applied = self.template.page_clone();
        applied.apply_cursor(cursor);
        let applied_head = self.public_head(&applied)?;
        let applied_identity = self.identity(&applied_head)?;
        drop(applied);
        if applied_identity != current_identity {
            return Err(self.decision_stop(PaginationErrorKind::InvalidInitialCursor));
        }

        self.fixed = Some(FixedFacts::from_head(&current_head));
        self.retain_identity(current_identity)?;
        self.phase = Phase::Ready {
            controller,
            endpoint: self.template.page_clone(),
        };
        Ok(())
    }

    async fn fetch_ready(&mut self) -> Result<Option<Page<Item<E>>>, Stop> {
        let Phase::Ready {
            controller,
            endpoint,
        } = std::mem::replace(&mut self.phase, Phase::Terminal)
        else {
            return Ok(None);
        };
        let (Some(operation_id), Some(deadline)) = (self.summary.operation_id, self.deadline)
        else {
            return Err(Stop::failed(self.internal_error()));
        };
        let page_index = self
            .page_index_base
            .checked_add(self.summary.pages_received)
            .ok_or_else(|| self.decision_stop(PaginationErrorKind::Overflow))?;
        self.active_page_index = page_index;

        self.check_deadline(FailureStage::BeforeDispatch)?;
        if self.options.controls().is_cancelled() {
            return Err(self.cancelled_stop(FailureStage::BeforeDispatch));
        }
        let attempt_ceiling = self.attempt_ceiling()?;
        let page = prepare_pagination_page(Request {
            core: self.core.clone(),
            endpoint,
            options: self.options.clone(),
        })?;

        // At most one page fetch is active; it shares the aggregate deadline,
        // budgets and meter. Continuation is not a retry: it is a new child
        // page operation with its own identity and admission.
        self.in_flight = Some(self.meter.snapshot());
        let execution = execute_pagination_page(
            page,
            operation_id,
            page_index,
            deadline,
            attempt_ceiling,
            Arc::clone(&self.meter),
        );
        let cancelled = self.options.controls().cancelled();
        let executed = tokio::select! {
            biased;
            () = cancelled => None,
            executed = execution => Some(executed),
        };
        let before = self.in_flight.take().unwrap_or_default();
        let Some(executed) = executed else {
            self.account_interrupted_page(before);
            return Err(self.cancelled_stop(FailureStage::DuringDispatchOrUnknown));
        };
        let executed = match executed {
            Ok(executed) => executed,
            Err(error) => {
                self.account_interrupted_page(before);
                return Err(Stop::failed(error));
            }
        };
        self.account_page(&executed, before);
        // Preserve endpoint status/challenge classification despite failed capture.
        // The shared meter still records the acquired bytes in the summary.
        let fixed_rejection = executed.result.as_ref().err().is_some_and(|error| {
            matches!(
                error.category(),
                ErrorCategory::HttpStatus | ErrorCategory::AuthenticationRejected
            )
        });
        if self.meter.bytes_exceeded() && !fixed_rejection {
            return Err(
                self.decision_stop(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
            );
        }
        let page_operation_id = executed.operation_id;
        let response = executed.result?;

        self.summary.pages_received = self.checked_increment(self.summary.pages_received)?;
        self.summary.continuation = ContinuationKnowledge::Unknown;
        let metadata = fixed_metadata(&response);
        let page_response = PageResponse::from_accepted(
            response,
            <E::Controller as PaginationController<E::Output>>::DESCRIPTION,
            operation_id,
            page_operation_id,
            page_index,
        )
        .map_err(|error| self.pagination_stop(error))?;

        let remaining_items = self
            .limits
            .max_items
            .checked_sub(self.summary.items_delivered)
            .ok_or_else(|| self.decision_stop(PaginationErrorKind::Overflow))?;
        self.check_deadline(FailureStage::PaginationDecision)?;
        let step = <E::Controller as PaginationController<E::Output>>::advance(
            controller,
            page_response,
            PageDecisionLimits::new(remaining_items),
        );
        self.check_deadline(FailureStage::PaginationDecision)?;
        let (items, next) = match step.map_err(|error| self.pagination_stop(error))? {
            PageStep::Done { items } => (items, None),
            PageStep::Continue { next, items } => (items, Some(next)),
        };
        if items.len() > remaining_items {
            return Err(self.decision_stop(PaginationErrorKind::CollectedItemCeilingExceeded));
        }

        match next {
            None => {
                self.checkpoint_position = None;
                self.summary.continuation = ContinuationKnowledge::Exhausted;
                self.summary.stop_reason = StopReason::Exhausted;
            }
            Some(next) => {
                // The continuation is validated before this page is yielded,
                // even when the bound has just been reached.
                let endpoint = self.validate_continuation(&next)?;
                if let Some(codec) = self.template.checkpoint_codec() {
                    let index = page_index
                        .checked_add(1)
                        .ok_or_else(|| self.decision_stop(PaginationErrorKind::Overflow))?;
                    self.checkpoint_position = Some((codec.save(&next), index));
                }
                self.summary.continuation = ContinuationKnowledge::NextRequestKnown;
                if self.summary.pages_received >= self.summary.effective_page_bound {
                    self.summary.stop_reason = StopReason::PageBound;
                    if self.mode == TraversalMode::Complete {
                        return Err(Stop {
                            error: self
                                .decision_error(PaginationErrorKind::CompletionLimitExceeded),
                            reason: StopReason::PageBound,
                        });
                    }
                } else {
                    self.phase = Phase::Ready {
                        controller: next,
                        endpoint,
                    };
                }
            }
        }

        let delivered_items = self
            .summary
            .items_delivered
            .checked_add(items.len())
            .ok_or_else(|| self.decision_stop(PaginationErrorKind::Overflow))?;
        self.summary.pages_delivered = self.checked_increment(self.summary.pages_delivered)?;
        self.summary.items_delivered = delivered_items;
        if self.summary.is_terminal() {
            self.observe(CompletionOutcome::Success);
        }
        Ok(Some(Page::new(page_index, items, metadata)))
    }

    /// Derives, shapes and validates the next request before the current page
    /// is delivered: public shaping, fixed-fact invariants, and cycle
    /// rejection over full protected identity vectors.
    fn validate_continuation(&mut self, next: &E::Controller) -> Result<E, Stop> {
        self.check_deadline(FailureStage::PaginationDecision)?;
        let cursor = <E::Controller as PaginationController<E::Output>>::cursor(next);
        self.check_deadline(FailureStage::PaginationDecision)?;
        let mut endpoint = self.template.page_clone();
        endpoint.apply_cursor(cursor);
        if endpoint.page_body_present() != self.template.page_body_present() {
            return Err(self.decision_stop(PaginationErrorKind::ForbiddenContinuationChange));
        }
        let head = self.public_head(&endpoint)?;
        if let Some(fixed) = &self.fixed {
            if let Err(kind) = fixed.check(&head) {
                return Err(self.decision_stop(kind));
            }
        }
        // The representation/body-presence contract is part of progress
        // identity. A cursor maps only ordinary endpoint slots (never the
        // body), so the captured page-stable body keeps its contract by
        // construction; the executor verifies the prepared kind and exact
        // Content-Type of every page against the first page before that page
        // is dispatched. No body bytes are prepared for a future page here.
        let identity = self.identity(&head)?;
        self.retain_identity(identity)?;
        Ok(endpoint)
    }

    /// Bounded public preparation of one request head: selection, rendering,
    /// structural auth/rate validation and origin preflight. Never runs body,
    /// credential-provider or limiter work.
    fn public_head(&self, endpoint: &E) -> Result<PreparedHead, Stop> {
        let meta = endpoint.request_meta();
        let builder = HeadBuilder::new(
            self.core.base_url(),
            meta.clone(),
            &self.options,
            self.core.default_debug(),
            self.core.default_runtime_endpoint_timeout(),
        );
        let mut head = endpoint.prepare_head(builder)?;
        head.validate_auth_context(self.core.credential_context())?;
        if head.endpoint_id() != meta.endpoint_id() || head.method() != meta.method() {
            return Err(Stop::failed(self.internal_error()));
        }
        head.validate_rate_limit_key_limit(self.core.rate_limit_key_limit())?;
        head.apply_media_headers(
            <E::Response as ResponseRepresentation<E::Output>>::ACCEPT,
            None,
            false,
        )?;
        head.validate_auth()?;
        if head.auth().is_empty() {
            self.core
                .preflight_url(head.endpoint_id(), head.method(), head.logical_url())?;
        } else {
            self.core.preflight_protected_url(
                head.endpoint_id(),
                head.method(),
                head.logical_url(),
            )?;
        }
        Ok(head)
    }

    fn identity(&self, head: &PreparedHead) -> Result<CanonicalIdentity, Stop> {
        let remaining = self
            .detector
            .remaining_identity_bytes(self.limits.identity_bytes)
            .map_err(|error| self.pagination_stop(error))?;
        CanonicalIdentity::from_request_head_with_limit(
            head.endpoint(),
            head.method(),
            head.logical_url(),
            head.public_headers(),
            remaining,
        )
        .map_err(|error| self.pagination_stop(error))
    }

    fn retain_identity(&mut self, identity: CanonicalIdentity) -> Result<(), Stop> {
        let digest = self
            .detector
            .preflight(&identity, self.limits.identity_bytes)
            .map_err(|error| self.pagination_stop(error))?;
        self.detector
            .retain(identity, digest)
            .map_err(|error| self.pagination_stop(error))
    }

    /// Visible attempts permitted for the next page from the remaining
    /// aggregate send (handoff) and replay budgets. Neither resets per page.
    fn attempt_ceiling(&self) -> Result<u32, Stop> {
        let used = self.meter.snapshot().handoffs;
        let remaining_sends = self.limits.max_handoffs.saturating_sub(used);
        if remaining_sends == 0 {
            return Err(self.decision_stop(self.handoff_exhaustion_kind()));
        }
        let mut ceiling = remaining_sends.min(MAX_PAGE_ATTEMPTS);
        if let Some(cap) = self.limits.replay_cap {
            let remaining_replays = cap.saturating_sub(self.summary.automatic_replays);
            ceiling = ceiling.min(remaining_replays.saturating_add(1));
        }
        Ok(ceiling)
    }

    const fn handoff_exhaustion_kind(&self) -> PaginationErrorKind {
        if self.limits.send_cap_binding {
            PaginationErrorKind::SendCapExhausted
        } else {
            PaginationErrorKind::AttemptCeilingExceeded
        }
    }

    fn account_page(&mut self, executed: &ExecutedPage<E::Output>, before: MeterSnapshot) {
        let after = self.meter.snapshot();
        let attempts = executed.attempts_started;
        self.summary.application_attempts =
            self.summary.application_attempts.saturating_add(attempts);
        self.summary.automatic_replays = self
            .summary
            .automatic_replays
            .saturating_add(attempts.saturating_sub(1));
        self.policy_retries = self.policy_retries.saturating_add(executed.policy_retries);
        self.recovered_authentication |= executed.recovered_authentication;
        if executed.final_status.is_some() {
            self.final_status = executed.final_status;
        }
        let heads = after.response_heads.saturating_sub(before.response_heads);
        let handoffs = after.handoffs.saturating_sub(before.handoffs);
        let knowledge = match &executed.result {
            Ok(_) => DispatchKnowledge::ResponseObserved,
            Err(_) if heads > 0 => DispatchKnowledge::ResponseObserved,
            // An earlier handed-off attempt of this page is not described by
            // the final error; never let it claim a stronger no-send fact.
            Err(error) if handoffs > 1 => join_dispatch(
                DispatchKnowledge::PossiblyDispatched,
                error.dispatch_knowledge(),
            ),
            Err(error) => error.dispatch_knowledge(),
        };
        self.summary.dispatch_knowledge = join_dispatch(self.summary.dispatch_knowledge, knowledge);
    }

    fn checked_increment(&self, value: u32) -> Result<u32, Stop> {
        value
            .checked_add(1)
            .ok_or_else(|| self.decision_stop(PaginationErrorKind::Overflow))
    }

    fn check_deadline(&self, stage: FailureStage) -> Result<(), Stop> {
        match self.deadline {
            Some(deadline) if deadline.check().is_err() => Err(Stop::failed(self.contextualize(
                ApiError::classified(
                    self.meta.endpoint_id().clone(),
                    self.meta.method().clone(),
                    ErrorCategory::Timeout,
                ),
                stage,
            ))),
            _ => Ok(()),
        }
    }

    fn pagination_error(&self, kind: PaginationErrorKind, stage: FailureStage) -> ApiError {
        self.contextualize(
            ApiError::pagination(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                PaginationError::new(kind),
            ),
            stage,
        )
    }

    /// Settles the traversal's aggregate journal onto a traversal-level
    /// error: the traversal identity (when started), the stage, the aggregate
    /// dispatch knowledge over every page and the last observed status.
    fn contextualize(&self, error: ApiError, stage: FailureStage) -> ApiError {
        error.with_journal(ErrorJournal {
            operation_id: self.summary.operation_id,
            parent_operation_id: None,
            page_index: self.summary.operation_id.map(|_| self.active_page_index),
            stage,
            aggregate: self.summary.dispatch_knowledge,
            last_attempt: None,
            last_status: self.final_status,
            counts: None,
            provider: None,
        })
    }

    fn decision_error(&self, kind: PaginationErrorKind) -> ApiError {
        self.pagination_error(kind, FailureStage::PaginationDecision)
    }

    fn decision_stop(&self, kind: PaginationErrorKind) -> Stop {
        Stop::failed(self.decision_error(kind))
    }

    fn pagination_stop(&self, error: PaginationError) -> Stop {
        Stop::failed(self.contextualize(
            ApiError::pagination(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                error,
            ),
            FailureStage::PaginationDecision,
        ))
    }

    fn cancelled_stop(&self, stage: FailureStage) -> Stop {
        Stop {
            error: self.pagination_error(PaginationErrorKind::Cancelled, stage),
            reason: StopReason::Cancelled,
        }
    }

    fn internal_error(&self) -> ApiError {
        self.contextualize(
            ApiError::classified(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                ErrorCategory::Internal,
            ),
            FailureStage::Preparation,
        )
    }

    /// Records a terminal error state, attaches the fixed summary and emits the
    /// aggregate observation once. The error is returned exactly once; later
    /// fetches return `Ok(None)`.
    fn terminate(&mut self, stop: Stop) -> ApiError {
        let Stop { error, reason } = stop;
        // A page failure keeps its own page journal (page operation, parent
        // traversal, last attempt); any other failure is settled by the
        // traversal journal. The summary joins the aggregate knowledge.
        let error = if error.is_contextualized() {
            error
        } else {
            let stage = error.stage_hint().unwrap_or(FailureStage::Preparation);
            self.contextualize(error, stage)
        };
        self.phase = Phase::Terminal;
        self.in_flight = None;
        self.summary.stop_reason = reason;
        let outcome = if reason == StopReason::Cancelled {
            CompletionOutcome::Cancelled
        } else {
            CompletionOutcome::from_error_category(error.category())
        };
        self.observe(outcome);
        error.with_traversal_summary(self.snapshot())
    }
}

impl<E> Traversal<E>
where
    E: PageableEndpoint,
{
    fn snapshot(&self) -> TraversalSummary {
        let mut summary = self.summary.clone();
        let meter = self.meter.snapshot();
        summary.acquired_response_bytes = meter.acquired_bytes;
        summary.quota_admissions = meter.quota_admissions;
        summary.retained_identity_bytes = self.detector.retained_identity_bytes();
        summary
    }

    /// Emits the aggregate completion observation once for a started traversal.
    fn observe(&mut self, outcome: CompletionOutcome) {
        let Some(operation_id) = self.summary.operation_id else {
            return;
        };
        if self.observed {
            return;
        }
        self.observed = true;
        let elapsed = self.started_at.map_or(Duration::ZERO, |started| {
            OperationDeadline::now().saturating_duration_since(started)
        });
        let event = PaginationFinished {
            operation_id,
            endpoint: self.meta.endpoint(),
            method: self.meta.method(),
            pages_completed: self.summary.pages_received,
            items_collected: self.summary.items_delivered,
            attempts_started: self.summary.application_attempts,
            policy_retries: self.policy_retries,
            recovered_authentication: self.recovered_authentication,
            elapsed,
            final_status: self.final_status,
            outcome,
            stop_reason: self.summary.stop_reason,
        };
        self.core
            .debug_dispatcher(self.options.resolve_debug(self.core.default_debug()))
            .pagination_completion(&event);
        crate::hooks::observe(
            self.core.operation_observer().as_ref(),
            OperationEvent::PaginationFinished(event),
        );
    }

    /// Conservative accounting for a page whose execution did not report back
    /// (cancelled in flight, or failed before its page lifecycle started),
    /// using only what the meter proves: an attempt with rate admission,
    /// handoff or response evidence counts, and dispatch knowledge is the
    /// strongest fact the meter establishes.
    fn account_interrupted_page(&mut self, before: MeterSnapshot) {
        let after = self.meter.snapshot();
        let heads = after.response_heads.saturating_sub(before.response_heads);
        let handoffs = after.handoffs.saturating_sub(before.handoffs);
        let admissions = after
            .quota_admissions
            .saturating_sub(before.quota_admissions);
        if handoffs == 0 && admissions == 0 && heads == 0 {
            return;
        }
        let attempts = handoffs.max(1);
        self.summary.application_attempts =
            self.summary.application_attempts.saturating_add(attempts);
        self.summary.automatic_replays = self
            .summary
            .automatic_replays
            .saturating_add(attempts.saturating_sub(1));
        let knowledge = if heads > 0 {
            DispatchKnowledge::ResponseObserved
        } else if handoffs > 0 {
            DispatchKnowledge::PossiblyDispatched
        } else {
            DispatchKnowledge::NotDispatched
        };
        self.summary.dispatch_knowledge = join_dispatch(self.summary.dispatch_knowledge, knowledge);
    }

    fn cancel_on_drop(&mut self) {
        if self.summary.operation_id.is_none() || self.summary.is_terminal() {
            return;
        }
        if let Some(before) = self.in_flight.take() {
            // The in-flight page work has already been dropped with the fetch
            // future; account for what the meter proves it did.
            self.account_interrupted_page(before);
        }
        self.phase = Phase::Terminal;
        self.summary.stop_reason = StopReason::Cancelled;
        self.observe(CompletionOutcome::Cancelled);
    }
}

/// Synchronous drop guard: a started, non-terminal traversal that is dropped
/// (with or without an in-flight page) records `Cancelled`, never EOF or
/// rollback. It releases owned work and starts no detached cleanup.
impl<E> Drop for Traversal<E>
where
    E: PageableEndpoint,
{
    fn drop(&mut self) {
        let was_live = self.summary.operation_id.is_some() && !self.summary.is_terminal();
        self.cancel_on_drop();
        if was_live {
            let summary = self.snapshot();
            *self
                .drop_report
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(summary);
        }
    }
}

impl<E> ErasedTraversal<Item<E>> for Traversal<E>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    fn fetch(self: Box<Self>) -> BoxFuture<'static, Fetched<Item<E>>> {
        Box::pin(async move {
            let mut traversal = self;
            let result = traversal.fetch_next().await;
            Fetched { traversal, result }
        })
    }

    fn summary(&self) -> TraversalSummary {
        self.snapshot()
    }

    fn checkpoint(&self) -> Result<super::Checkpoint, super::CheckpointError> {
        self.save_checkpoint()
    }

    fn finish(&mut self) -> TraversalSummary {
        if !self.summary.is_terminal() {
            self.phase = Phase::Terminal;
            self.summary.stop_reason = StopReason::Application;
            // An unstarted traversal has no identity and emits nothing.
            self.observe(CompletionOutcome::Success);
        }
        self.snapshot()
    }

    fn invariant_error(&self) -> ApiError {
        self.internal_error()
            .with_traversal_summary(self.snapshot())
    }
}

/// Fixed snapshot of one page's execution metadata.
fn fixed_metadata<T>(response: &ApiResponse<T>) -> ExecutionMeta {
    let metadata = response.metadata();
    ExecutionMeta::new(
        metadata.endpoint_id().clone(),
        metadata.method().clone(),
        metadata.page_index(),
        metadata.attempts_started(),
        metadata.policy_retries(),
        metadata.recovered_authentication(),
    )
    .with_provider_acquisition(metadata.provider_acquisition())
}
