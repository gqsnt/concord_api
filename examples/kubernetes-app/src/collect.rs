//! One consistent collection read: the SDK's chunk factory driven by
//! `concord_pagination::sequential`, with the expired-snapshot policy made explicit.
//!
//! Policy (application code, stated here):
//!
//! - Chunks are fetched strictly one after the other (the next `continue` token exists only after
//!   the previous chunk); at most [`ListPolicy::max_chunks`] chunks per chain.
//! - A `410 Expired` stops the chain. With [`OnExpired::Report`] the outcome is
//!   [`ListOutcome::Expired`]: the items collected are an **incomplete prefix** of the expired
//!   snapshot and the caller must restart for a complete consistent list. With
//!   [`OnExpired::Restart`] the prefix is **discarded** (a [`RestartBoundary`] records how much)
//!   and a new chain starts from the first chunk at a new snapshot; restarts are bounded. The
//!   inconsistent-continuation token a `410` may offer is never used.
//! - Any other failure ends the read with [`ListOutcome::Failed`]: the error and the resume key
//!   (the chunk that failed) are kept apart; nothing is retried.

use std::num::{NonZeroU32, NonZeroUsize};

use concord_example_kubernetes_sdk::ResourceVersion;
use concord_example_kubernetes_sdk::list::{Continuation, ListError, ListOperation, ListPager};
use concord_pagination::{Budget, Outcome, Resume, sequential};


/// How a collection is read.
#[derive(Clone, Copy, Debug)]
pub struct ListPolicy {
    /// `limit` per chunk.
    pub chunk_size: NonZeroU32,
    /// The most chunks one chain may fetch (the pagination budget, charged per request).
    pub max_chunks: NonZeroUsize,
    /// What a `410 Expired` does.
    pub on_expired: OnExpired,
}

impl ListPolicy {
    /// `chunk_size` items per chunk, at most `max_chunks` chunks, restart at most once on
    /// expiry.
    pub fn new(chunk_size: NonZeroU32, max_chunks: NonZeroUsize) -> Self {
        Self {
            chunk_size,
            max_chunks,
            on_expired: OnExpired::Restart { max_restarts: 1 },
        }
    }
}

/// What a `410 Expired` (expired continue token, unavailable snapshot) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnExpired {
    /// Stop and report [`ListOutcome::Expired`].
    Report,
    /// Discard the prefix and start a new chain, at most `max_restarts` times; then report.
    Restart {
        /// The most restarts.
        max_restarts: u32,
    },
}

/// An explicit reset: a chain whose snapshot expired was abandoned and its items discarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartBoundary {
    /// The snapshot of the abandoned chain (`None` if its first chunk failed).
    pub expired_snapshot: Option<ResourceVersion>,
    /// Chunks of the abandoned chain that had been received.
    pub chunks_before: u64,
    /// Items of the abandoned chain that were discarded.
    pub discarded_items: usize,
    /// Whether the `410` answered a continued request (an expired token) rather than the first.
    pub during_continuation: bool,
    /// Whether the `410` offered an inconsistent-continuation token (never used).
    pub inconsistent_token_offered: bool,
}

/// How a collection read ended.
#[derive(Debug)]
#[non_exhaustive]
pub enum ListOutcome {
    /// The provider's last chunk was received: `items` is the complete collection at
    /// [`Completion::snapshot`].
    Complete,
    /// The chunk budget ran out while a next chunk existed: `items` is a consistent prefix;
    /// `resume` continues it (while the token is valid).
    Bounded {
        /// The next chunk's position.
        resume: Resume<Continuation>,
    },
    /// The snapshot expired (`410`) and the policy allowed no (further) restart: `items` is an
    /// incomplete prefix; a complete consistent list must start again from the first chunk.
    Expired {
        /// The boundary the restart would cross.
        boundary: RestartBoundary,
    },
    /// Another failure (API status, transport, decode, inconsistent snapshot).
    Failed {
        /// The error, as the SDK reported it.
        error: ListError,
        /// The chunk that failed (`Start` for the first).
        resume: Resume<Continuation>,
    },
}

impl ListOutcome {
    /// Whether the whole collection was read.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Completion and resource-version metadata of one collection read.
#[derive(Debug)]
pub struct Completion {
    /// How it ended.
    pub outcome: ListOutcome,
    /// The collection snapshot (`metadata.resourceVersion`) of the final chain.
    pub snapshot: Option<ResourceVersion>,
    /// Chunks received in the final chain.
    pub chunks: u64,
    /// `remainingItemCount` of each chunk of the final chain, as reported (estimates; usually
    /// absent with selectors and on the last chunk).
    pub remaining_item_counts: Vec<Option<i64>>,
    /// Chains abandoned because their snapshot expired, oldest first.
    pub restarts: Vec<RestartBoundary>,
    /// Requests sent (every chain, including failed and abandoned chunks).
    pub requests: u64,
}

/// The items and completion of one collection read.
#[derive(Debug)]
pub struct Collected<T> {
    /// The objects, in server order (complete only when the outcome is
    /// [`ListOutcome::Complete`]).
    pub items: Vec<T>,
    /// How the read ended.
    pub completion: Completion,
}

/// Reads the collection `template` lists (with its selectors) in chunks, from `from`.
///
/// Resuming (`Resume::At(key)`) continues a chain that an earlier call reported as
/// [`ListOutcome::Bounded`]; the snapshot check still applies.
pub async fn collect_list<O: ListOperation>(
    template: O,
    policy: ListPolicy,
    from: Resume<Continuation>,
) -> Collected<O::Item> {
    let pager = ListPager::new(template, policy.chunk_size);
    let mut restarts = Vec::new();
    let mut requests = 0;
    let mut from = from;
    loop {
        let resumed = from.key().map(|k| k.snapshot().clone());
        let mut chunks = sequential(from, Budget::new(policy.max_chunks), |key| pager.fetch(key));
        let mut items = Vec::new();
        let mut counts = Vec::new();
        let mut snapshot = resumed;
        let mut failure = None;
        while let Some(page) = chunks.next().await {
            match page {
                Ok(page) => {
                    snapshot = page.meta.snapshot.clone().or(snapshot);
                    counts.push(page.meta.remaining_item_count);
                    items.extend(page.items);
                }
                Err(error) => failure = Some(error),
            }
        }
        let report = chunks.finish();
        requests += report.progress.admitted;
        let received = report.progress.emitted;
        let completion = |outcome, items| Collected {
            items,
            completion: Completion {
                outcome,
                snapshot: snapshot.clone(),
                chunks: received,
                remaining_item_counts: counts.clone(),
                restarts: restarts.clone(),
                requests,
            },
        };
        match (report.outcome, failure) {
            (Outcome::Exhausted, None) => return completion(ListOutcome::Complete, items),
            (_, Some(ListError::Expired(api))) => {
                let boundary = RestartBoundary {
                    expired_snapshot: snapshot.clone(),
                    chunks_before: received,
                    discarded_items: items.len(),
                    during_continuation: matches!(report.failed, Some(Resume::At(_))),
                    inconsistent_token_offered: api.offers_inconsistent_continue(),
                };
                let allowed = match policy.on_expired {
                    OnExpired::Report => 0,
                    OnExpired::Restart { max_restarts } => max_restarts,
                };
                if restarts.len() as u32 >= allowed {
                    return completion(ListOutcome::Expired { boundary }, items);
                }
                restarts.push(boundary);
                from = Resume::Start;
            }
            (_, Some(error)) => {
                let resume = report.failed.unwrap_or(Resume::Start);
                return completion(ListOutcome::Failed { error, resume }, items);
            }
            // `Bounded` (the chain is never finished early here, so not `Stopped`): the first
            // chunk not received is the resume point.
            (_, None) => {
                let outcome = match report.resume {
                    Some(resume) => ListOutcome::Bounded { resume },
                    None => ListOutcome::Complete,
                };
                return completion(outcome, items);
            }
        }
    }
}
