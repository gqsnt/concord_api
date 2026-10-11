//! Value-free operation-journal facts attached to every error that escapes a
//! managed operation (EXEC-01, EXEC-02).

use http::StatusCode;

use super::{DispatchKnowledge, FailureStage};
use crate::auth::CredentialReason;
use crate::hooks::AttemptInfo;
use crate::operation::OperationId;

/// Value-free facts about the last application-visible attempt of an
/// operation.
///
/// These facts are deliberately separate from the operation's aggregate
/// [`DispatchKnowledge`]: an earlier attempt can have observed a response
/// (aggregate `ResponseObserved`) while the last attempt was conclusively
/// not sent (`NotDispatched`). Indexes are zero-based machine metadata.
///
/// The value contains no request or response data, credentials or native
/// transport objects and is safe to format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptFacts {
    attempt: AttemptInfo,
    dispatch_knowledge: DispatchKnowledge,
    status: Option<StatusCode>,
}

impl AttemptFacts {
    pub(crate) const fn new(attempt: AttemptInfo) -> Self {
        Self {
            attempt,
            dispatch_knowledge: DispatchKnowledge::NotDispatched,
            status: None,
        }
    }

    pub(crate) const fn settle(mut self, knowledge: DispatchKnowledge) -> Self {
        self.dispatch_knowledge = self.dispatch_knowledge.join(knowledge);
        self
    }

    pub(crate) const fn observed(mut self, status: StatusCode) -> Self {
        self.dispatch_knowledge = DispatchKnowledge::ResponseObserved;
        self.status = Some(status);
        self
    }

    /// Returns the zero-based visible attempt index within its direct
    /// request or page.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.attempt.index
    }

    /// Returns the structural identity of the attempt (index, policy-retry
    /// index and whether it was the single authentication recovery).
    #[must_use]
    pub const fn attempt(&self) -> AttemptInfo {
        self.attempt
    }

    /// Returns what core settled about dispatch for this attempt alone.
    #[must_use]
    pub const fn dispatch_knowledge(&self) -> DispatchKnowledge {
        self.dispatch_knowledge
    }

    /// Returns the status of this attempt's response head, when one was
    /// received.
    #[must_use]
    pub const fn status(&self) -> Option<StatusCode> {
        self.status
    }
}

/// Value-free work counts recorded by one operation journal.
///
/// Counts are cumulative for the operation that produced the error (a direct
/// request, a traversal page, or a credential-storage execution). Traversal
/// aggregates are reported separately by the traversal summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCounts {
    pub(crate) attempts_started: u32,
    pub(crate) application_handoffs: u32,
    pub(crate) quota_admissions: u32,
    pub(crate) policy_retries: u32,
    pub(crate) authentication_recoveries: u32,
    pub(crate) freshness_restarts: u32,
}

impl OperationCounts {
    /// Visible attempts whose before-send stage began (initial attempt,
    /// policy retries and authentication recovery).
    #[must_use]
    pub const fn attempts_started(&self) -> u32 {
        self.attempts_started
    }

    /// Application requests handed to the native transport. This is the
    /// quantity limited by `CallOptions::cap_sends`.
    #[must_use]
    pub const fn application_handoffs(&self) -> u32 {
        self.application_handoffs
    }

    /// Committed successful quota admissions. Admissions consumed before a
    /// freshness restart remain counted; they are never refunded.
    #[must_use]
    pub const fn quota_admissions(&self) -> u32 {
        self.quota_admissions
    }

    /// Policy retries committed for this operation.
    #[must_use]
    pub const fn policy_retries(&self) -> u32 {
        self.policy_retries
    }

    /// Authentication-recovery replays committed for this operation (at most
    /// one per direct request or page).
    #[must_use]
    pub const fn authentication_recoveries(&self) -> u32 {
        self.authentication_recoveries
    }

    /// Phase-6 credential freshness restarts (at most two per attempt).
    #[must_use]
    pub const fn freshness_restarts(&self) -> u32 {
        self.freshness_restarts
    }

    /// Replays counted against `CallOptions::cap_replays`: policy retries plus
    /// authentication recoveries.
    #[must_use]
    pub const fn automatic_replays(&self) -> u32 {
        self.policy_retries
            .saturating_add(self.authentication_recoveries)
    }
}

/// How one credential-provider acquisition or refresh ended.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAcquisitionOutcome {
    /// The provider returned material that core installed as a new version.
    Installed,
    /// The provider, its helper time limit, or local installation failed.
    Failed,
    /// A manual clear/replace or a later installation intent superseded the
    /// acquisition. Any material it produced was dropped uninstalled.
    Superseded,
    /// The leading application operation stopped (it was dropped, cancelled
    /// or ran out of time) while the provider was still running. Its provider
    /// work was released, and followers re-evaluate.
    Cancelled,
}

/// Value-free journal facts for one credential-provider acquisition or
/// refresh (EXEC-01, §13.7).
///
/// A provider acquisition has its own operation identity. That identity is
/// distinct from every application operation, page, traversal and attempt
/// identity. The operation that leads the acquisition is its parent. Other
/// operations can join the same acquisition as followers; they report the
/// same facts, including the same provider
/// [`operation_id`](Self::operation_id), and do not start another one.
///
/// Provider helper traffic has its own bounded journal, kept separate from
/// the application journal. Application dispatch knowledge of
/// `NotDispatched` therefore does not assert that no auxiliary provider
/// network activity occurred; [`dispatch_knowledge`](Self::dispatch_knowledge)
/// reports that separately. The counts cover only traffic through Concord's
/// isolated [`ProviderHttp`](crate::contracts::ProviderHttp) helper. A
/// provider that owns another HTTP client does I/O that Concord cannot
/// observe.
///
/// The value contains no URL, header, body, credential material or provider
/// text, so it is safe to format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderAcquisitionFacts {
    operation_id: OperationId,
    parent_operation_id: Option<OperationId>,
    reason: CredentialReason,
    helper_sends: u32,
    last_status: Option<StatusCode>,
    dispatch_knowledge: DispatchKnowledge,
    outcome: ProviderAcquisitionOutcome,
}

impl ProviderAcquisitionFacts {
    #[allow(
        clippy::too_many_arguments,
        reason = "the provider journal snapshot fixes each value-free fact once"
    )]
    pub(crate) const fn new(
        operation_id: OperationId,
        parent_operation_id: Option<OperationId>,
        reason: CredentialReason,
        helper_sends: u32,
        last_status: Option<StatusCode>,
        dispatch_knowledge: DispatchKnowledge,
        outcome: ProviderAcquisitionOutcome,
    ) -> Self {
        Self {
            operation_id,
            parent_operation_id,
            reason,
            helper_sends,
            last_status,
            dispatch_knowledge,
            outcome,
        }
    }

    /// Returns the identity of this provider acquisition. It never equals an
    /// application operation identity.
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    /// Returns the application operation (a direct request, page or storage
    /// execution) that led this acquisition. It is `None` only when provider
    /// work ran outside a managed application operation.
    #[must_use]
    pub const fn parent_operation_id(&self) -> Option<OperationId> {
        self.parent_operation_id
    }

    /// Returns why core asked the provider for material.
    #[must_use]
    pub const fn reason(&self) -> CredentialReason {
        self.reason
    }

    /// Returns the number of provider helper requests handed to the isolated
    /// provider transport. Requests refused before transport, for example for
    /// an undeclared origin, an exhausted send budget or an oversized body,
    /// are not counted.
    #[must_use]
    pub const fn helper_sends(&self) -> u32 {
        self.helper_sends
    }

    /// Returns the status of the last provider helper response head, if one
    /// was received. Any status counts, including a rejected one.
    #[must_use]
    pub const fn last_status(&self) -> Option<StatusCode> {
        self.last_status
    }

    /// Returns the provider journal's own monotone dispatch knowledge. It
    /// covers only helper traffic and is `NotDispatched` when no helper
    /// request was handed off.
    #[must_use]
    pub const fn dispatch_knowledge(&self) -> DispatchKnowledge {
        self.dispatch_knowledge
    }

    /// Returns how the acquisition ended.
    #[must_use]
    pub const fn outcome(&self) -> ProviderAcquisitionOutcome {
        self.outcome
    }
}

/// Journal context settled onto an escaping error. Crate-private: the public
/// surface is the accessor set on [`ApiError`](super::ApiError).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ErrorJournal {
    pub(crate) operation_id: Option<OperationId>,
    pub(crate) parent_operation_id: Option<OperationId>,
    pub(crate) page_index: Option<u32>,
    pub(crate) stage: FailureStage,
    pub(crate) aggregate: DispatchKnowledge,
    pub(crate) last_attempt: Option<AttemptFacts>,
    pub(crate) last_status: Option<StatusCode>,
    pub(crate) counts: Option<OperationCounts>,
    /// The latest provider acquisition this operation led or joined.
    pub(crate) provider: Option<ProviderAcquisitionFacts>,
}

impl ErrorJournal {
    /// Context for an error fixed before any operation identity existed, or
    /// during planning, which never dispatches (EXEC-03).
    pub(crate) const fn before_operation(stage: FailureStage) -> Self {
        Self {
            operation_id: None,
            parent_operation_id: None,
            page_index: None,
            stage,
            aggregate: DispatchKnowledge::NotDispatched,
            last_attempt: None,
            last_status: None,
            counts: None,
            provider: None,
        }
    }
}
