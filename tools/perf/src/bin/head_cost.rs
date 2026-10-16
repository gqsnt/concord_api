//! Steady request-head cost probe (V3-O07): allocations and time per
//! operation for request construction, binding and bound-head planning, and
//! the client-thread allocations of one managed loopback execution.
//!
//! The harness measures current V3 construction, binding, planning and managed
//! execution. Declarations live in `src/head_cost/declaration.rs`.
//! Compare source commits using isolated checkouts and target directories.
//!
//! Method: each category first runs `WARMUP` untimed iterations so lazy
//! contract statics, pools and the keep-alive loopback connection reach a
//! steady state. Allocations are counted with `allocation-counter` (a
//! counting global allocator; current thread only, which is where planning
//! and the current-thread execution runtime run) over one batch whose
//! inputs are prepared outside the counted region. Time is the median and the
//! minimum per-operation time over `ROUNDS` batches. The counting allocator
//! is active for the timed batches in both versions alike.

#![forbid(unsafe_code)]

use std::future::IntoFuture;
use std::hint::black_box;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use concord::SecretString;
use concord::http::SafeProxy;
use concord::rate_limit::NoopRateLimiter;

include!("../head_cost/declaration.rs");

/// A minimal keep-alive loopback responder for the execution row: every
/// request receives one fixed empty `200 OK`, written with a single write on
/// a `TCP_NODELAY` socket. (The shared `concord-test-support` scripted
/// server writes a response head in several small writes without
/// `TCP_NODELAY`, which stalls every keep-alive response on a delayed ACK of
/// about 40 ms in both versions and would hide the client cost.) It runs on
/// its own threads, so none of its allocations are counted.
struct Responder {
    address: SocketAddr,
    requests: Arc<AtomicUsize>,
}

impl Responder {
    fn start() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&requests);
        thread::Builder::new()
            .name("head-cost-responder".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let counter = Arc::clone(&counter);
                    thread::spawn(move || serve(stream, &counter));
                }
            })?;
        Ok(Self { address, requests })
    }
}

fn serve(mut stream: TcpStream, requests: &AtomicUsize) {
    const RESPONSE: &[u8] =
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
    if stream.set_nodelay(true).is_err() {
        return;
    }
    let mut buffered = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        // Body-free GET requests end at the blank line.
        while let Some(end) = buffered
            .windows(4)
            .position(|window: &[u8]| window == b"\r\n\r\n")
        {
            buffered.drain(..end + 4);
            requests.fetch_add(1, Ordering::SeqCst);
            if stream.write_all(RESPONSE).is_err() {
                return;
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => buffered.extend_from_slice(&chunk[..read]),
        }
    }
}

const WARMUP: usize = 1_000;
const BATCH: usize = 2_000;
const ROUNDS: usize = 9;
const EXEC_WARMUP: usize = 50;
const EXEC_BATCH: usize = 200;
const EXEC_ROUNDS: usize = 5;

/// Batch sizes of one category.
#[derive(Clone, Copy)]
struct Sizes {
    warmup: usize,
    batch: usize,
    rounds: usize,
}

const PLANNING: Sizes = Sizes {
    warmup: WARMUP,
    batch: BATCH,
    rounds: ROUNDS,
};

const EXECUTION: Sizes = Sizes {
    warmup: EXEC_WARMUP,
    batch: EXEC_BATCH,
    rounds: EXEC_ROUNDS,
};

/// One measured category.
struct Row {
    name: &'static str,
    allocations: f64,
    bytes: f64,
    net_allocations: i64,
    median_ns: f64,
    min_ns: f64,
}

/// Measures `operation` over inputs produced by `input` outside the counted
/// and timed regions.
fn measure<S, I, O>(name: &'static str, sizes: Sizes, mut input: I, mut operation: O) -> Row
where
    I: FnMut() -> S,
    O: FnMut(S),
{
    let prepare = |input: &mut I, count: usize| (0..count).map(|_| input()).collect::<Vec<_>>();

    for item in prepare(&mut input, sizes.warmup) {
        operation(item);
    }

    let inputs = prepare(&mut input, sizes.batch);
    let info = allocation_counter::measure(|| {
        for item in inputs {
            operation(item);
        }
    });

    let mut per_operation = Vec::with_capacity(sizes.rounds);
    for _ in 0..sizes.rounds {
        let inputs = prepare(&mut input, sizes.batch);
        let started = Instant::now();
        for item in inputs {
            operation(item);
        }
        per_operation.push(started.elapsed().as_nanos() as f64 / sizes.batch as f64);
    }
    per_operation.sort_by(f64::total_cmp);

    Row {
        name,
        allocations: info.count_total as f64 / sizes.batch as f64,
        bytes: info.bytes_total as f64 / sizes.batch as f64,
        net_allocations: info.count_current,
        median_ns: per_operation[per_operation.len() / 2],
        min_ns: per_operation[0],
    }
}

fn head_client() -> Result<HeadApi, Box<dyn std::error::Error>> {
    Ok(HeadApi::builder()
        .set_tenant("tenant-a".to_owned())
        .api_secret(SecretString::try_new("head-cost-only-key")?)
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?)
}

fn tags() -> Vec<String> {
    vec!["stable".to_owned(), "featured".to_owned()]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let head = head_client()?;

    // Print the four planned heads once so measured V3 candidates can be
    // checked to prepare the same public requests.
    let show = HeadApi::unbound()
        .records("archive".to_owned())
        .show("item-1".to_owned());
    let search = head
        .records("archive".to_owned())
        .search("rust".to_owned(), tags());
    let protected = head.protected();
    let limited = head.limited();
    println!(
        "concord-perf {} head-cost probe: warmup {WARMUP}, batch {BATCH}, rounds {ROUNDS}; execution warmup {EXEC_WARMUP}, batch {EXEC_BATCH}, rounds {EXEC_ROUNDS}",
        env!("CARGO_PKG_VERSION")
    );
    for (label, plan) in [
        ("show", head.plan(&show)?),
        ("search", search.plan()?),
        ("protected", protected.plan()?),
        ("limited", limited.plan()?),
    ] {
        println!(
            "planned {label}: {} ({} public header(s))",
            plan.observed_url(),
            plan.public_headers().len()
        );
    }

    let mut rows = Vec::new();
    rows.push(measure(
        "direct request construction (scope + search)",
        PLANNING,
        || ("archive".to_owned(), "rust".to_owned(), tags()),
        |(collection, term, tags)| {
            let _ = black_box(head.records(collection).search(term, tags));
        },
    ));
    rows.push(measure(
        "unbound endpoint construction (scope + show)",
        PLANNING,
        || ("archive".to_owned(), "item-1".to_owned()),
        |(collection, id)| {
            black_box(HeadApi::unbound().records(collection).show(id));
        },
    ));
    rows.push(measure(
        "request binding (client.request)",
        PLANNING,
        || {
            HeadApi::unbound()
                .records("archive".to_owned())
                .show("item-1".to_owned())
        },
        |endpoint| {
            let _ = black_box(head.request(endpoint));
        },
    ));
    rows.push(measure(
        "bound-head plan: scope + endpoint path (client.plan)",
        PLANNING,
        || (),
        |()| {
            black_box(head.plan(&show).expect("show plan"));
        },
    ));
    rows.push(measure(
        "bound-head plan: query program (search)",
        PLANNING,
        || (),
        |()| {
            black_box(search.plan().expect("search plan"));
        },
    ));
    rows.push(measure(
        "bound-head plan: bearer placement (protected)",
        PLANNING,
        || (),
        |()| {
            black_box(protected.plan().expect("protected plan"));
        },
    ));
    rows.push(measure(
        "bound-head plan: rate policy (limited)",
        PLANNING,
        || (),
        |()| {
            black_box(limited.plan().expect("limited plan"));
        },
    ));

    let executions = EXECUTION.warmup + EXECUTION.batch * (1 + EXECUTION.rounds);
    let responder = Responder::start()?;
    let proxy = SafeProxy::parse(format!("http://{}", responder.address))?;
    let exec = ExecApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rows.push(measure(
        "managed execution, client thread (loopback, keep-alive)",
        EXECUTION,
        || (),
        |()| {
            black_box(
                runtime
                    .block_on(exec.bounded().into_future())
                    .expect("loopback execution"),
            );
        },
    ));
    assert_eq!(responder.requests.load(Ordering::SeqCst), executions);

    println!(
        "| operation | allocations/op | bytes/op | net allocations per batch | median ns/op | min ns/op |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for row in rows {
        println!(
            "| {} | {:.2} | {:.1} | {} | {:.0} | {:.0} |",
            row.name, row.allocations, row.bytes, row.net_allocations, row.median_ns, row.min_ns
        );
    }
    Ok(())
}
