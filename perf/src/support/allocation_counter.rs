use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationSnapshot {
    pub alloc_calls: usize,
    pub dealloc_calls: usize,
    pub bytes_allocated: usize,
    pub bytes_deallocated: usize,
}

#[derive(Debug)]
pub struct CountingAllocator<A> {
    inner: A,
}

impl<A> CountingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static BYTES_DEALLOCATED: AtomicUsize = AtomicUsize::new(0);

pub fn reset() {
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    DEALLOC_CALLS.store(0, Ordering::Relaxed);
    BYTES_ALLOCATED.store(0, Ordering::Relaxed);
    BYTES_DEALLOCATED.store(0, Ordering::Relaxed);
}

pub fn snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
        dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
        bytes_allocated: BYTES_ALLOCATED.load(Ordering::Relaxed),
        bytes_deallocated: BYTES_DEALLOCATED.load(Ordering::Relaxed),
    }
}

pub fn reset_allocation_counts() {
    reset();
}

pub fn snapshot_allocation_counts() -> AllocationSnapshot {
    snapshot()
}

#[allow(dead_code)]
pub type DefaultCountingAllocator = CountingAllocator<System>;

unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = {
            // SAFETY: this wrapper forwards directly to the wrapped allocator.
            unsafe { self.inner.alloc(layout) }
        };
        if !ptr.is_null() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES_ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = {
            // SAFETY: this wrapper forwards directly to the wrapped allocator.
            unsafe { self.inner.alloc_zeroed(layout) }
        };
        if !ptr.is_null() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES_ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES_DEALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: this wrapper forwards directly to the wrapped allocator.
        unsafe { self.inner.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = {
            // SAFETY: this wrapper forwards directly to the wrapped allocator.
            unsafe { self.inner.realloc(ptr, layout, new_size) }
        };
        if !new_ptr.is_null() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES_ALLOCATED.fetch_add(new_size, Ordering::Relaxed);
            BYTES_DEALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        new_ptr
    }
}
