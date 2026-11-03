//! Track H: heap allocations per `prepare()` by header value route. Every endpoint sends the same
//! four header names, so the header-map allocations are the same for all of them; the differences
//! are the payloads. A native value built with `HeaderValue::from_static` (or a literal template,
//! lowered to a static) needs no payload allocation; a shared native value is reused by reference
//! count; text is copied into a new `HeaderValue`; a formatted template allocates its `String`
//! too. Run with `--nocapture` to see the table (counts are per `prepare()`, steady state).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use concord::reqwest::{self, header::HeaderValue};

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; only calls are counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size(), Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

concord::api! {
    /// One endpoint per header route, four header values each (same names everywhere).
    pub client Routes(
        t1: String, t2: String, t3: String, t4: String,
        i1: u64, i2: u64, i3: u64, i4: u64,
    ) {
        base "http://routes.invalid";
        GET none();
        GET text() {
            headers { "x-a" = client.t1; "x-b" = client.t2; "x-c" = client.t3; "x-d" = client.t4; }
        }
        GET integer() {
            headers { "x-a" = client.i1; "x-b" = client.i2; "x-c" = client.i3; "x-d" = client.i4; }
        }
        GET literal() {
            headers { "x-a" = "application/json"; "x-b" = "en"; "x-c" = "gzip"; "x-d" = "7"; }
        }
        GET formatted() {
            headers {
                "x-a" = "{client.t1}"; "x-b" = "{client.t2}";
                "x-c" = "{client.t3}"; "x-d" = "{client.t4}";
            }
        }
    }
}

/// (allocations, bytes) of one `prepare()` (+ drop), minimum over `runs` after a warm-up.
fn measure(mut prepare: impl FnMut()) -> (usize, usize) {
    for _ in 0..3 {
        prepare();
    }
    (0..50)
        .map(|_| {
            let (a, b) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
            prepare();
            (ALLOCS.load(Relaxed) - a, BYTES.load(Relaxed) - b)
        })
        .min()
        .unwrap()
}

fn routes(_native: [HeaderValue; 4]) -> Routes {
    let text = |s: &str| s.to_owned();
    Routes::new(
        reqwest::Client::new(),
        RoutesConfig {
            t1: text("application/json"),
            t2: text("en"),
            t3: text("gzip"),
            t4: text("7"),
            i1: 1,
            i2: 22,
            i3: 333,
            i4: 4444,
        },
    )
}

#[test]
fn header_payload_allocations_by_route() {
    let statics = routes([
        HeaderValue::from_static("application/json"),
        HeaderValue::from_static("en"),
        HeaderValue::from_static("gzip"),
        HeaderValue::from_static("7"),
    ]);
    let shared = routes([
        HeaderValue::from_bytes(b"application/json").unwrap(),
        HeaderValue::from_bytes(b"en").unwrap(),
        HeaderValue::from_bytes(b"gzip").unwrap(),
        HeaderValue::from_bytes(b"caf\xe9").unwrap(),
    ]);
    let none = measure(|| drop(statics.none().prepare().unwrap()));
    let rows = [
        (
            "literal",
            measure(|| drop(statics.literal().prepare().unwrap())),
        ),
        (
            "integer",
            measure(|| drop(statics.integer().prepare().unwrap())),
        ),
        (
            "text String",
            measure(|| drop(statics.text().prepare().unwrap())),
        ),
        (
            "formatted template",
            measure(|| drop(statics.formatted().prepare().unwrap())),
        ),
    ];
    println!(
        "prepare() without headers: {} allocations, {} bytes",
        none.0, none.1
    );
    println!(
        "{:<20} {:>12} {:>8} {:>16}",
        "route (4 values)", "allocations", "bytes", "vs literal"
    );
    let base = rows[0].1;
    for (name, (a, b)) in rows {
        println!(
            "{name:<20} {a:>12} {b:>8} {:>+16}",
            a as isize - base.0 as isize
        );
    }
    let _ = &shared;
}
