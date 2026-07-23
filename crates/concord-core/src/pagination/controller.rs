use std::num::NonZeroUsize;

use super::PaginationError;

/// Deterministic application-defined continuation state for one sequential pagination operation.
///
/// Each controller value is local to one pagination operation and moves from
/// one page to the next.
/// Distinct controller values may be active in concurrent pagination operations;
/// the controller itself need not be `Sync`, and no method executes under a
/// core lock.
/// Decoded pages and continuation values may be sensitive; implementations own
/// their logging and storage and must bound CPU, memory, retries, maps, queues,
/// spawned tasks, and metric cardinality; they must be non-blocking and perform
/// no external I/O. All methods are synchronous, execute outside the
/// per-page operation deadline, and cannot be preempted by Concord. Cancellation
/// prevents later pages only, and Concord does not automatically bound custom
/// controller state or callback time.
pub trait PaginationController<Page>: Sized + Send + 'static {
    /// Validates the current state before one page request is prepared.
    fn validate(&self) -> Result<(), PaginationError> {
        Ok(())
    }

    /// Consumes the current state and derives the optional next state.
    fn next(self, page: &Page) -> Result<Option<Self>, PaginationError>;

    /// Selects exact decoded-item count termination for this state.
    fn short_page_size(&self) -> Option<NonZeroUsize> {
        None
    }
}
