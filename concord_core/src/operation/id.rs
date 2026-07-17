use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static GLOBAL_OPERATION_IDS: OperationIdAllocator = OperationIdAllocator::new(1);

/// Opaque correlation identity for one endpoint or page operation.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OperationId(u64);

impl OperationId {
    #[allow(dead_code, reason = "allocated by the production executor")]
    pub(crate) fn allocate() -> Result<Self, OperationIdExhausted> {
        GLOBAL_OPERATION_IDS.allocate()
    }
}

impl fmt::Debug for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OperationId(<opaque>)")
    }
}

#[derive(Debug)]
pub(crate) struct OperationIdExhausted;

struct OperationIdAllocator {
    next: AtomicU64,
}

impl OperationIdAllocator {
    const fn new(next: u64) -> Self {
        Self {
            next: AtomicU64::new(next),
        }
    }

    fn allocate(&self) -> Result<OperationId, OperationIdExhausted> {
        self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(OperationId)
            .map_err(|_| OperationIdExhausted)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier, Mutex};

    use super::*;

    #[test]
    fn debug_is_opaque_and_traits_remain_structural() {
        let first = OperationId::allocate().expect("first operation ID");
        let copied = first;
        assert_eq!(first, copied);
        assert_eq!(format!("{first:?}"), "OperationId(<opaque>)");
        let mut values = HashSet::new();
        assert!(values.insert(first));
        assert!(!values.insert(copied));
    }

    #[test]
    fn concurrent_allocations_are_distinct() {
        const WORKERS: usize = 16;
        const PER_WORKER: usize = 128;
        let barrier = Arc::new(Barrier::new(WORKERS));
        let ids = Arc::new(Mutex::new(Vec::with_capacity(WORKERS * PER_WORKER)));
        let mut workers = Vec::new();
        for _ in 0..WORKERS {
            let barrier = Arc::clone(&barrier);
            let ids = Arc::clone(&ids);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                let local = (0..PER_WORKER)
                    .map(|_| OperationId::allocate().expect("concurrent operation ID"))
                    .collect::<Vec<_>>();
                ids.lock().expect("ID collection lock").extend(local);
            }));
        }
        for worker in workers {
            worker.join().expect("ID worker must not panic");
        }
        let ids = ids.lock().expect("ID collection lock");
        assert_eq!(ids.len(), WORKERS * PER_WORKER);
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), ids.len());
    }

    #[test]
    fn exhaustion_fails_without_panicking_or_wrapping() {
        let allocator = OperationIdAllocator::new(u64::MAX - 1);
        assert_eq!(
            allocator.allocate().expect("last usable ID"),
            OperationId(u64::MAX - 1)
        );
        assert!(allocator.allocate().is_err());
        assert!(allocator.allocate().is_err());
    }
}
