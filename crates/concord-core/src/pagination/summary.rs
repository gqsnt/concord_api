//! Owned, value-free facts about one traversal.

use crate::error::DispatchKnowledge;
use crate::operation::OperationId;

/// Why a traversal stopped, or that it has not stopped yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// No page fetch or collector has been polled yet.
    NotStarted,
    /// The traversal started and can still fetch another page.
    Active,
    /// The controller proved exhaustion (`PageStep::Done`).
    Exhausted,
    /// The effective page bound was reached. With
    /// [`ContinuationKnowledge::NextRequestKnown`] a valid next request existed;
    /// a zero bound reports this with [`ContinuationKnowledge::Unknown`].
    PageBound,
    /// The application stopped the traversal early (`Pages::finish`).
    Application,
    /// A started traversal was cancelled: a dropped started fetch or
    /// traversal, or an attached cancellation signal.
    Cancelled,
    /// A fetch, controller, continuation or budget failure ended the traversal.
    Failed,
}

/// What is known about continuing past the last received page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContinuationKnowledge {
    /// Nothing is known (nothing fetched, or the traversal stopped before a
    /// continuation could be validated).
    Unknown,
    /// A valid next request can be constructed. This does not claim that
    /// another item exists or that the dataset is snapshot-consistent.
    NextRequestKnown,
    /// The controller proved exhaustion.
    Exhausted,
}

/// Owned, cloneable facts about one traversal.
///
/// Every counter is checked and bounded by the traversal's finite ceilings.
/// No field claims how many delivered items the application actually
/// processed. The summary contains no cursor, request value, header, body or
/// credential and is safe to log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraversalSummary {
    pub(super) operation_id: Option<OperationId>,
    pub(super) stop_reason: StopReason,
    pub(super) continuation: ContinuationKnowledge,
    pub(super) requested_page_bound: u32,
    pub(super) effective_page_bound: u32,
    pub(super) pages_received: u32,
    pub(super) pages_delivered: u32,
    pub(super) items_delivered: usize,
    pub(super) application_attempts: u32,
    pub(super) automatic_replays: u32,
    pub(super) quota_admissions: u32,
    pub(super) acquired_response_bytes: u64,
    pub(super) retained_identity_bytes: usize,
    pub(super) dispatch_knowledge: DispatchKnowledge,
}

impl TraversalSummary {
    /// Summary of a traversal that has not started.
    pub(super) const fn unstarted(requested: u32, effective: u32) -> Self {
        Self {
            operation_id: None,
            stop_reason: StopReason::NotStarted,
            continuation: ContinuationKnowledge::Unknown,
            requested_page_bound: requested,
            effective_page_bound: effective,
            pages_received: 0,
            pages_delivered: 0,
            items_delivered: 0,
            application_attempts: 0,
            automatic_replays: 0,
            quota_admissions: 0,
            acquired_response_bytes: 0,
            retained_identity_bytes: 0,
            dispatch_knowledge: DispatchKnowledge::NotDispatched,
        }
    }

    /// Summary of a zero-bound traversal: no work, `PageBound`/`Unknown`.
    pub(super) const fn zero_bound() -> Self {
        let mut summary = Self::unstarted(0, 0);
        summary.stop_reason = StopReason::PageBound;
        summary
    }

    /// Returns the aggregate traversal operation identity, allocated at the
    /// first polled nonzero fetch or collector. Zero-bound and unstarted
    /// traversals have none.
    #[must_use]
    pub const fn operation_id(&self) -> Option<OperationId> {
        self.operation_id
    }

    /// Returns why the traversal stopped (or that it is not started/active).
    #[must_use]
    pub const fn stop_reason(&self) -> StopReason {
        self.stop_reason
    }

    /// Returns what is known about continuing past the last received page.
    #[must_use]
    pub const fn continuation(&self) -> ContinuationKnowledge {
        self.continuation
    }

    /// Returns the page bound the caller requested.
    #[must_use]
    pub const fn requested_page_bound(&self) -> u32 {
        self.requested_page_bound
    }

    /// Returns the enforced bound: the requested bound intersected with the
    /// client's configured page ceiling.
    #[must_use]
    pub const fn effective_page_bound(&self) -> u32 {
        self.effective_page_bound
    }

    /// Returns the number of pages completely acquired and decoded, including
    /// a page whose items were withheld by a controller or continuation error.
    #[must_use]
    pub const fn pages_received(&self) -> u32 {
        self.pages_received
    }

    /// Returns the number of pages whose items were delivered to the caller.
    #[must_use]
    pub const fn pages_delivered(&self) -> u32 {
        self.pages_delivered
    }

    /// Returns the number of items delivered to the caller.
    #[must_use]
    pub const fn items_delivered(&self) -> usize {
        self.items_delivered
    }

    /// Returns the application attempts started across all pages (initial
    /// submissions plus automatic replays). An attempt cancelled in flight is
    /// counted conservatively.
    #[must_use]
    pub const fn application_attempts(&self) -> u32 {
        self.application_attempts
    }

    /// Returns the automatic replays (policy retries and authentication
    /// recovery) across all pages. First submissions of later pages are not
    /// replays.
    #[must_use]
    pub const fn automatic_replays(&self) -> u32 {
        self.automatic_replays
    }

    /// Returns the committed rate-limit admissions across all attempts.
    #[must_use]
    pub const fn quota_admissions(&self) -> u32 {
        self.quota_admissions
    }

    /// Returns every nonempty response-body byte actually acquired for
    /// application attempts: accepted pages, captured rejected-status bodies,
    /// and bytes read before a failed acquisition. Retry responses dropped
    /// without reading contribute nothing because nothing was read.
    #[must_use]
    pub const fn acquired_response_bytes(&self) -> u64 {
        self.acquired_response_bytes
    }

    /// Returns the bytes of protected canonical request identities retained
    /// for cycle detection.
    #[must_use]
    pub const fn retained_identity_bytes(&self) -> usize {
        self.retained_identity_bytes
    }

    /// Returns the aggregate dispatch knowledge: the monotone join
    /// `NotDispatched < PossiblyDispatched < ResponseObserved` over every
    /// attempt of the traversal.
    #[must_use]
    pub const fn dispatch_knowledge(&self) -> DispatchKnowledge {
        self.dispatch_knowledge
    }

    pub(super) const fn is_terminal(&self) -> bool {
        !matches!(
            self.stop_reason,
            StopReason::NotStarted | StopReason::Active
        )
    }
}

/// Monotone join of dispatch knowledge.
pub(super) const fn join_dispatch(
    left: DispatchKnowledge,
    right: DispatchKnowledge,
) -> DispatchKnowledge {
    const fn rank(knowledge: DispatchKnowledge) -> u8 {
        match knowledge {
            DispatchKnowledge::NotDispatched => 0,
            DispatchKnowledge::PossiblyDispatched => 1,
            DispatchKnowledge::ResponseObserved => 2,
        }
    }
    if rank(right) > rank(left) {
        right
    } else {
        left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_unstarted_summaries_have_no_identity_or_work() {
        let zero = TraversalSummary::zero_bound();
        assert_eq!(zero.stop_reason(), StopReason::PageBound);
        assert_eq!(zero.continuation(), ContinuationKnowledge::Unknown);
        assert_eq!(zero.operation_id(), None);
        assert_eq!(zero.requested_page_bound(), 0);
        assert_eq!(zero.effective_page_bound(), 0);
        assert!(zero.is_terminal());

        let unstarted = TraversalSummary::unstarted(7, 5);
        assert_eq!(unstarted.stop_reason(), StopReason::NotStarted);
        assert!(!unstarted.is_terminal());
        assert_eq!(unstarted.requested_page_bound(), 7);
        assert_eq!(unstarted.effective_page_bound(), 5);
        assert_eq!(unstarted.pages_received(), 0);
        assert_eq!(unstarted.pages_delivered(), 0);
        assert_eq!(unstarted.items_delivered(), 0);
        assert_eq!(unstarted.application_attempts(), 0);
        assert_eq!(unstarted.automatic_replays(), 0);
        assert_eq!(unstarted.quota_admissions(), 0);
        assert_eq!(unstarted.acquired_response_bytes(), 0);
        assert_eq!(unstarted.retained_identity_bytes(), 0);
        assert_eq!(
            unstarted.dispatch_knowledge(),
            DispatchKnowledge::NotDispatched
        );
    }

    #[test]
    fn dispatch_join_is_monotone() {
        use DispatchKnowledge::{NotDispatched, PossiblyDispatched, ResponseObserved};
        assert_eq!(
            join_dispatch(NotDispatched, PossiblyDispatched),
            PossiblyDispatched
        );
        assert_eq!(
            join_dispatch(ResponseObserved, NotDispatched),
            ResponseObserved
        );
        assert_eq!(
            join_dispatch(PossiblyDispatched, ResponseObserved),
            ResponseObserved
        );
        assert_eq!(join_dispatch(NotDispatched, NotDispatched), NotDispatched);
    }

    #[test]
    fn scalar_enums_are_copy_and_comparable() {
        fn assert_copy_eq<T: Copy + Eq + std::fmt::Debug>() {}
        assert_copy_eq::<StopReason>();
        assert_copy_eq::<ContinuationKnowledge>();
        fn assert_owned_clone<T: Clone + Send + Sync + 'static>() {}
        assert_owned_clone::<TraversalSummary>();
    }
}
