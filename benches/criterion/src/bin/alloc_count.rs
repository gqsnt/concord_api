//! Deterministic allocation counts per benchmark case (Criterion measures time only).
//!
//! `cargo run --release -p concord-bench --bin alloc-count [-- [FILTER] [--json PATH]]`
//!
//! For every case of the synchronous groups (the same scenario definitions and IDs as the
//! Criterion suite), after one warm-up call: builds N inputs, then runs the consuming stage on
//! each, counting global-allocator calls in each phase. Outputs are dropped after counting.
//! Reported per call: allocation calls (`alloc`, `alloc_zeroed`, `realloc`) and bytes requested
//! (sizes of allocations plus new sizes of reallocations; frees are not subtracted).

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use concord_bench::{Case, Fixtures, GROUPS, Runner, http_client, run_group};

struct Counting;

static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; only calls and sizes are
// counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new_size as u64, Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn snapshot() -> (u64, u64) {
    (CALLS.load(Relaxed), BYTES.load(Relaxed))
}

/// Per-call figures; `exact` is false when the total is not a multiple of the call count.
#[derive(Clone, Copy)]
struct PerCall {
    calls: f64,
    bytes: f64,
    exact: bool,
}

fn per_call(before: (u64, u64), after: (u64, u64), n: usize) -> PerCall {
    let (calls, bytes) = (after.0 - before.0, after.1 - before.1);
    let n64 = n as u64;
    PerCall {
        calls: calls as f64 / n as f64,
        bytes: bytes as f64 / n as f64,
        exact: calls % n64 == 0 && bytes % n64 == 0,
    }
}

struct Row {
    id: String,
    stage: &'static str,
    input: PerCall,
    run: PerCall,
}

struct Counter {
    group: &'static str,
    filter: Option<String>,
    rows: Vec<Row>,
}

impl Runner for Counter {
    fn case<I, O>(
        &mut self,
        case: Case,
        mut input: impl FnMut() -> I,
        mut run: impl FnMut(I) -> O,
    ) {
        let id = format!("{}/{}", self.group, case.id);
        if self
            .filter
            .as_ref()
            .is_some_and(|f| !id.contains(f.as_str()))
        {
            return;
        }
        let n = if case.large_input { 16 } else { 256 };
        drop(run(input())); // warm-up: lazy statics, first-use initialisation
        let mut inputs = Vec::with_capacity(n);
        let mut outputs = Vec::with_capacity(n);
        let a = snapshot();
        for _ in 0..n {
            inputs.push(input());
        }
        let b = snapshot();
        for i in inputs {
            outputs.push(run(i));
        }
        let c = snapshot();
        drop(outputs);
        self.rows.push(Row {
            id,
            stage: case.run_stage,
            input: per_call(a, b, n),
            run: per_call(b, c, n),
        });
    }
}

fn fmt(v: PerCall, bytes: bool) -> String {
    let x = if bytes { v.bytes } else { v.calls };
    let mark = if v.exact { "" } else { "~" };
    if x.fract() == 0.0 {
        format!("{mark}{x:.0}")
    } else {
        format!("{mark}{x:.2}")
    }
}

fn main() {
    let mut filter = None;
    let mut json_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_path = Some(args.next().expect("--json PATH")),
            // `cargo bench`-style flags are ignored so the binary tolerates common invocations.
            a if a.starts_with("--") => {}
            a => filter = Some(a.to_owned()),
        }
    }

    let fx = Fixtures::new(http_client());
    let mut counter = Counter {
        group: "",
        filter,
        rows: Vec::new(),
    };
    for group in GROUPS {
        counter.group = group;
        run_group(group, &fx, &mut counter);
    }

    let mut out = String::new();
    writeln!(
        out,
        "| case | input allocs | input bytes | stage | stage allocs | stage bytes |\n|---|---:|---:|---|---:|---:|"
    )
    .unwrap();
    for r in &counter.rows {
        writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} |",
            r.id,
            fmt(r.input, false),
            fmt(r.input, true),
            r.stage,
            fmt(r.run, false),
            fmt(r.run, true)
        )
        .unwrap();
    }
    print!("{out}");
    println!("\nPer call; `~` marks a mean that is not an exact multiple (non-deterministic).");

    if let Some(path) = json_path {
        let rows: Vec<serde_json::Value> = counter
            .rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "stage": r.stage,
                    "input_allocs": r.input.calls,
                    "input_bytes": r.input.bytes,
                    "stage_allocs": r.run.calls,
                    "stage_bytes": r.run.bytes,
                    "exact": r.input.exact && r.run.exact,
                })
            })
            .collect();
        std::fs::write(&path, serde_json::to_string_pretty(&rows).unwrap()).expect("write json");
        eprintln!("wrote {path}");
    }
}
