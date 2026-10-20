//! Aggregate traversal accounting shared with the managed page executor.
//!
//! One meter belongs to one traversal. The executor records facts into it
//! synchronously at the exact lifecycle points (rate admission committed, just
//! before native handoff, response head received, every acquired body chunk),
//! so the traversal keeps truthful counts even when an in-flight page future is
//! dropped by cancellation. No lock is held across I/O: every field is atomic.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use http::HeaderValue;

use crate::body::BodyKind;

use crate::http::AcquiredByteMeter;

/// Atomic aggregate counters for one traversal.
pub(crate) struct TraversalMeter {
    acquired: AcquiredByteMeter,
    handoffs: AtomicU32,
    response_heads: AtomicU32,
    quota_admissions: AtomicU32,
    /// The page-stable body contract (kind and exact prepared Content-Type)
    /// observed when the first page's body was prepared.
    body_contract: Mutex<Option<(BodyKind, Option<HeaderValue>)>>,
}

/// A consistent-enough copy of the meter for summary construction.
///
/// Each field is individually exact at the time it is read; the traversal only
/// reads snapshots while no page fetch is concurrently running, or from the
/// synchronous drop guard after the in-flight work has been dropped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MeterSnapshot {
    pub(crate) acquired_bytes: u64,
    pub(crate) handoffs: u32,
    pub(crate) response_heads: u32,
    pub(crate) quota_admissions: u32,
}

impl TraversalMeter {
    /// Creates a meter whose acquired-byte ceiling is `byte_ceiling`.
    pub(crate) fn new(byte_ceiling: u64) -> Self {
        Self {
            acquired: AcquiredByteMeter::new(byte_ceiling),
            handoffs: AtomicU32::new(0),
            response_heads: AtomicU32::new(0),
            quota_admissions: AtomicU32::new(0),
            body_contract: Mutex::new(None),
        }
    }

    /// Records the first page's prepared body contract and checks every later
    /// page against it. A page-stable representation promises the same body
    /// kind and exact Content-Type for every page; a change is a forbidden
    /// continuation change detected before that page is dispatched. Returns
    /// `false` on a mismatch or a poisoned record (fail closed).
    pub(crate) fn check_body_contract(
        &self,
        kind: BodyKind,
        content_type: Option<&HeaderValue>,
    ) -> bool {
        let Ok(mut recorded) = self.body_contract.lock() else {
            return false;
        };
        match recorded.as_ref() {
            None => {
                *recorded = Some((kind, content_type.cloned()));
                true
            }
            Some((first_kind, first_type)) => {
                *first_kind == kind && first_type.as_ref() == content_type
            }
        }
    }

    /// Returns a handle charging acquired response-body bytes to this traversal.
    pub(crate) fn acquired_bytes(&self) -> AcquiredByteMeter {
        self.acquired.clone()
    }

    /// Records one application attempt immediately before native handoff.
    pub(crate) fn record_handoff(&self) {
        saturating_increment(&self.handoffs);
    }

    /// Records one received application response head.
    pub(crate) fn record_response_head(&self) {
        saturating_increment(&self.response_heads);
    }

    /// Records one committed rate-limit admission.
    pub(crate) fn record_quota_admission(&self) {
        saturating_increment(&self.quota_admissions);
    }

    /// Returns whether acquired bytes crossed the traversal byte ceiling.
    pub(crate) fn bytes_exceeded(&self) -> bool {
        self.acquired.exceeded()
    }

    pub(crate) fn snapshot(&self) -> MeterSnapshot {
        MeterSnapshot {
            acquired_bytes: self.acquired.total(),
            handoffs: self.handoffs.load(Ordering::Acquire),
            response_heads: self.response_heads.load(Ordering::Acquire),
            quota_admissions: self.quota_admissions.load(Ordering::Acquire),
        }
    }
}

fn saturating_increment(counter: &AtomicU32) {
    let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        Some(value.saturating_add(1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_counts_saturate_and_report_the_crossing_chunk() {
        let meter = TraversalMeter::new(10);
        let bytes = meter.acquired_bytes();
        assert!(bytes.record(6));
        assert!(bytes.record(4));
        assert!(!meter.bytes_exceeded());
        assert!(!bytes.record(1), "first excess byte crosses the ceiling");
        assert!(meter.bytes_exceeded());
        meter.record_handoff();
        meter.record_handoff();
        meter.record_response_head();
        meter.record_quota_admission();
        assert_eq!(
            meter.snapshot(),
            MeterSnapshot {
                acquired_bytes: 11,
                handoffs: 2,
                response_heads: 1,
                quota_admissions: 1,
            }
        );
        let saturated = AtomicU32::new(u32::MAX);
        saturating_increment(&saturated);
        assert_eq!(saturated.load(Ordering::Acquire), u32::MAX);
        let huge = TraversalMeter::new(u64::MAX);
        assert!(huge.acquired_bytes().record(usize::MAX));
        assert!(huge.acquired_bytes().record(usize::MAX));
        assert_eq!(huge.snapshot().acquired_bytes, u64::MAX);
    }
}
