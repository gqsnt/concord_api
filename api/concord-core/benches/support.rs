//! Shared benchmark harness; setup, allocator sampling and Criterion are separate.

use criterion::{BatchSize, Criterion};
use std::alloc::{GlobalAlloc, Layout, System};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

struct Counting;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static TRACE: AtomicBool = AtomicBool::new(false);

fn allocated() {
    if ACTIVE.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        if TRACE.load(Ordering::Relaxed) {
            ACTIVE.store(false, Ordering::Relaxed);
            // GlobalAlloc must never unwind, including diagnostic I/O failures.
            let _ = std::panic::catch_unwind(|| {
                use std::io::Write as _;
                let _ = writeln!(
                    std::io::stderr(),
                    "ALLOCATION TRACE\n{}",
                    std::backtrace::Backtrace::force_capture()
                );
            });
            ACTIVE.store(true, Ordering::Relaxed);
        }
    }
}

// SAFETY: every pointer and Layout is forwarded unchanged to System. Counting
// uses atomics. Optional diagnostic capture disables counting before re-entry
// and catches unwinds; none can escape an allocator method.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

pub fn criterion() -> Criterion {
    Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args()
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("benchmark runtime")
}

pub fn bench<S, O, F>(
    criterion: &mut Criterion,
    name: &str,
    mut setup: impl FnMut() -> S,
    mut operation: impl FnMut(S) -> F,
) -> usize
where
    F: Future<Output = O>,
{
    let runtime = runtime();
    let count = allocations(name, &mut setup, |fixture| {
        runtime.block_on(operation(fixture))
    });
    criterion.bench_function(name, |b| {
        b.iter_batched(
            &mut setup,
            |fixture| std::hint::black_box(runtime.block_on(operation(fixture))),
            BatchSize::SmallInput,
        );
    });
    count
}

pub fn allocations<S, O>(
    name: &str,
    mut setup: impl FnMut() -> S,
    mut operation: impl FnMut(S) -> O,
) -> usize {
    // Warm the runtime and lazy library state before counting. Each count uses
    // a fresh fixture; requests recorded by Script never grow across samples.
    for _ in 0..4 {
        std::hint::black_box(operation(setup()));
    }
    let mut counts = Vec::with_capacity(32);
    for sample in 0..32 {
        let fixture = setup();
        TRACE.store(
            sample == 0 && std::env::var("M5_ALLOC_TRACE").is_ok_and(|filter| name == filter),
            Ordering::Relaxed,
        );
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ACTIVE.store(true, Ordering::Relaxed);
        let output = operation(fixture);
        ACTIVE.store(false, Ordering::Relaxed);
        TRACE.store(false, Ordering::Relaxed);
        counts.push(ALLOCATIONS.load(Ordering::Relaxed));
        std::hint::black_box(output);
    }
    counts.sort_unstable();
    println!(
        "ALLOC {name} median={} min={} max={} (alloc+alloc_zeroed+realloc; setup excluded)",
        counts[16], counts[0], counts[31]
    );
    counts[16]
}
