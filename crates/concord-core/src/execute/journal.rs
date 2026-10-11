//! The bounded operation journal of one direct request, page or credential
//! storage execution (EXEC-01, EXEC-02, CONTROL-03, EXEC-04).
//!
//! The journal is the only authority for the dispatch facts attached to an
//! escaping [`ApiError`]. It records the operation identity, parent traversal
//! identity, endpoint/method, the current pipeline phase, the last visible
//! attempt, the last observed response status, the aggregate dispatch
//! knowledge and the work counts. Facts are monotone: once a response head is
//! recorded, later body failures or a later attempt's connection failure can
//! never clear it.
//!
//! **Provisional handoff token.** When an application request is handed to
//! the native transport the journal holds a provisional token. Only a
//! core-native positive proof of no application submission settles it as
//! `NotDispatched`; a drop, timeout, cancellation or ambiguous transport
//! failure settles it as `PossiblyDispatched`; a received head settles it as
//! `ResponseObserved`. Provisional knowledge is never exposed as an aggregate
//! fact and then downgraded.
//!
//! **Completion and drop guard.** [`OperationJournal::complete`] fixes the
//! result, contextualizes an error and only then emits the final
//! observation, which therefore cannot change the result (a slow final
//! observer delays delivery; it does not alter it). If the operation future is
//! dropped before completion, the synchronous `Drop` implementation settles
//! any outstanding handoff token conservatively and emits exactly one
//! `Cancelled` completion with the best known bounded facts. No detached task
//! or queue is ever created.

use std::time::Duration;

use http::{Method, StatusCode};

use crate::client::ClientCore;
use crate::endpoint::EndpointId;
use crate::error::{
    ApiError, AttemptFacts, DispatchKnowledge, ErrorJournal, FailureStage, OperationCounts,
    ProviderAcquisitionFacts,
};
use crate::hooks::{
    AttemptInfo, CompletionOutcome, DebugLevel, OperationEvent, OperationFinished, RequestTarget,
};
use crate::operation::{OperationDeadline, OperationId};

use super::ExecutionContext;

pub(crate) struct OperationJournal {
    core: ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    endpoint: EndpointId,
    method: Method,
    target: RequestTarget,
    debug: DebugLevel,
    started: tokio::time::Instant,
    phase: FailureStage,
    aggregate: DispatchKnowledge,
    last_attempt: Option<AttemptFacts>,
    handoff_pending: bool,
    last_status: Option<StatusCode>,
    counts: OperationCounts,
    recovered_authentication: bool,
    /// The latest credential-provider acquisition this operation led or
    /// joined. It is a distinct, linked identity with its own journal.
    provider: Option<ProviderAcquisitionFacts>,
    finished: bool,
}

impl OperationJournal {
    #[allow(
        clippy::too_many_arguments,
        reason = "the journal captures the exact operation identity facts once"
    )]
    pub(crate) fn new(
        core: ClientCore,
        operation_id: OperationId,
        context: ExecutionContext,
        endpoint: EndpointId,
        method: Method,
        target: RequestTarget,
        debug: DebugLevel,
        started: tokio::time::Instant,
    ) -> Self {
        Self {
            core,
            operation_id,
            context,
            endpoint,
            method,
            target,
            debug,
            started,
            phase: FailureStage::Preparation,
            aggregate: DispatchKnowledge::NotDispatched,
            last_attempt: None,
            handoff_pending: false,
            last_status: None,
            counts: OperationCounts::default(),
            recovered_authentication: false,
            provider: None,
            finished: false,
        }
    }

    pub(crate) const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    /// Enters a pipeline phase; failures fixed while in it report this stage
    /// unless the failing code names a more precise one.
    pub(crate) fn enter(&mut self, phase: FailureStage) {
        self.phase = phase;
    }

    pub(crate) fn set_target(&mut self, target: RequestTarget) {
        self.target = target;
    }

    /// Starts a new visible attempt. Its own dispatch knowledge starts at
    /// `NotDispatched`; the aggregate is untouched.
    pub(crate) fn begin_attempt(&mut self, attempt: AttemptInfo) {
        self.settle_pending();
        self.last_attempt = Some(AttemptFacts::new(attempt));
        self.counts.attempts_started = self.counts.attempts_started.saturating_add(1);
        if attempt.authentication_recovery {
            self.counts.authentication_recoveries =
                self.counts.authentication_recoveries.saturating_add(1);
        } else if attempt.index > 0 {
            self.counts.policy_retries = self.counts.policy_retries.saturating_add(1);
        }
    }

    pub(crate) fn record_admission(&mut self) {
        self.counts.quota_admissions = self.counts.quota_admissions.saturating_add(1);
    }

    pub(crate) fn record_freshness_restart(&mut self) {
        self.counts.freshness_restarts = self.counts.freshness_restarts.saturating_add(1);
    }

    pub(crate) fn record_recovered_authentication(&mut self) {
        self.recovered_authentication = true;
    }

    /// Links the settled facts of a provider acquisition this operation led
    /// or joined (EXEC-01). The latest linked acquisition wins; an absent
    /// value leaves an earlier link untouched.
    pub(crate) fn record_provider(&mut self, facts: Option<ProviderAcquisitionFacts>) {
        if facts.is_some() {
            self.provider = facts;
        }
    }

    pub(crate) const fn provider_acquisition(&self) -> Option<ProviderAcquisitionFacts> {
        self.provider
    }

    /// Records the native handoff of the current attempt and holds the
    /// provisional dispatch token.
    pub(crate) fn handoff(&mut self) {
        self.counts.application_handoffs = self.counts.application_handoffs.saturating_add(1);
        self.handoff_pending = true;
        self.phase = FailureStage::DuringDispatchOrUnknown;
    }

    /// Settles the provisional token after a native failure. `proven` is set
    /// only by core-owned transport code holding positive proof that nothing
    /// was submitted.
    pub(crate) fn settle_transport_failure(&mut self, proven_not_submitted: bool) {
        if !self.handoff_pending {
            return;
        }
        self.handoff_pending = false;
        let knowledge = if proven_not_submitted {
            DispatchKnowledge::NotDispatched
        } else {
            DispatchKnowledge::PossiblyDispatched
        };
        self.settle(knowledge);
    }

    /// Irrevocably records a received response head for the current attempt.
    pub(crate) fn record_head(&mut self, status: StatusCode) {
        self.handoff_pending = false;
        self.last_attempt = self.last_attempt.map(|facts| facts.observed(status));
        self.aggregate = DispatchKnowledge::ResponseObserved;
        self.last_status = Some(status);
    }

    fn settle(&mut self, knowledge: DispatchKnowledge) {
        self.last_attempt = self.last_attempt.map(|facts| facts.settle(knowledge));
        self.aggregate = self.aggregate.join(knowledge);
    }

    /// A handoff whose outcome is unknown (timeout, cancellation, drop)
    /// settles as `PossiblyDispatched`.
    fn settle_pending(&mut self) {
        if self.handoff_pending {
            self.handoff_pending = false;
            self.settle(DispatchKnowledge::PossiblyDispatched);
        }
    }

    pub(crate) fn aggregate(&self) -> DispatchKnowledge {
        if self.handoff_pending {
            self.aggregate.join(DispatchKnowledge::PossiblyDispatched)
        } else {
            self.aggregate
        }
    }

    pub(crate) fn last_attempt_knowledge(&self) -> DispatchKnowledge {
        let knowledge = self
            .last_attempt
            .map_or(DispatchKnowledge::NotDispatched, |facts| {
                facts.dispatch_knowledge()
            });
        if self.handoff_pending {
            knowledge.join(DispatchKnowledge::PossiblyDispatched)
        } else {
            knowledge
        }
    }

    pub(crate) const fn counts(&self) -> OperationCounts {
        self.counts
    }

    pub(crate) const fn attempts_started(&self) -> u32 {
        self.counts.attempts_started
    }

    pub(crate) const fn policy_retries(&self) -> u32 {
        self.counts.policy_retries
    }

    pub(crate) const fn recovered_authentication(&self) -> bool {
        self.recovered_authentication
    }

    pub(crate) const fn last_status(&self) -> Option<StatusCode> {
        self.last_status
    }

    /// Returns the journal context for an error fixed at `stage`.
    pub(crate) fn error_journal(&self, stage: FailureStage) -> ErrorJournal {
        ErrorJournal {
            operation_id: Some(self.operation_id),
            parent_operation_id: self.context.parent_operation_id,
            page_index: Some(self.context.page_index),
            stage,
            aggregate: self.aggregate(),
            last_attempt: self.last_attempt.map(|facts| {
                if self.handoff_pending {
                    facts.settle(DispatchKnowledge::PossiblyDispatched)
                } else {
                    facts
                }
            }),
            last_status: self.last_status,
            counts: Some(self.counts),
            provider: self.provider,
        }
    }

    /// Settles every outstanding fact and attaches the journal to `error`.
    pub(crate) fn contextualize(&mut self, error: ApiError) -> ApiError {
        self.settle_pending();
        error.with_journal(self.error_journal(self.phase))
    }

    /// Fixes the result, contextualizes an error, then emits the final
    /// observation. Observation runs after fixation and cannot change the
    /// result; observer panics are contained by the hook boundary.
    pub(crate) fn complete<T>(
        &mut self,
        result: Result<T, ApiError>,
        success_outcome: CompletionOutcome,
    ) -> Result<T, ApiError> {
        let result = result.map_err(|error| {
            if error.is_contextualized() {
                error
            } else {
                self.contextualize(error)
            }
        });
        let outcome = match &result {
            Ok(_) => success_outcome,
            Err(error) => CompletionOutcome::from_error_category(error.category()),
        };
        self.emit(outcome, self.elapsed());
        result
    }

    fn elapsed(&self) -> Duration {
        OperationDeadline::now().saturating_duration_since(self.started)
    }

    fn emit(&mut self, outcome: CompletionOutcome, elapsed: Duration) {
        if self.finished {
            return;
        }
        self.finished = true;
        let event = OperationFinished {
            operation_id: self.operation_id,
            parent_operation_id: self.context.parent_operation_id,
            endpoint: self.endpoint.as_str(),
            method: &self.method,
            target: self.target,
            page_index: self.context.page_index,
            attempts_started: self.counts.attempts_started,
            policy_retries: self.counts.policy_retries,
            recovered_authentication: self.recovered_authentication,
            elapsed,
            final_status: self.last_status,
            outcome,
        };
        self.core.debug_dispatcher(self.debug).completion(&event);
        crate::hooks::observe(
            self.core.operation_observer().as_ref(),
            OperationEvent::OperationFinished(event),
        );
    }
}

/// Synchronous drop guard: a started operation dropped before completion
/// records the best bounded known facts and emits one `Cancelled` completion.
/// An outstanding handoff token settles as `PossiblyDispatched`. Owned
/// transport work has already been released by dropping the operation future;
/// nothing is spawned.
impl Drop for OperationJournal {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.settle_pending();
        let elapsed = self.elapsed();
        self.emit(CompletionOutcome::Cancelled, elapsed);
    }
}
