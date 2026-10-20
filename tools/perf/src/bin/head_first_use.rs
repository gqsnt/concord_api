//! First-use request-head cost probe (V3-O07): the time, allocation count
//! and allocated bytes of the FIRST operation of one category in a fresh
//! process, with no warm-up.
//!
//! This is the first-use companion of `head_cost`, which measures the warm
//! steady state. Both use the current V3 declarations from
//! `src/head_cost/declaration.rs`. Run each sample in a fresh process.
//!
//! Usage: `head_first_use <category>`; one process measures one category
//! (run it in a fresh process per sample). The categories:
//!
//! - `client-build`: the first `HeadApi::builder()...build()`;
//! - `direct-construction`: the first `client.records(..).search(..)`;
//! - `unbound-construction`: the first `HeadApi::unbound().records(..).show(..)`;
//! - `binding`: the first `client.request(unbound_endpoint)`;
//! - `plan-show`: the first `client.plan(&unbound_endpoint)`;
//! - `plan-search`: the first `search.plan()` of a direct request;
//! - `plan-protected`: the first `protected.plan()`;
//! - `plan-limited`: the first `limited.plan()` (rate policy);
//! - `execution`: the first managed loopback execution (`ExecApi`):
//!   `runtime.block_on(exec.bounded().into_future())` as in `head_cost`,
//!   which includes opening the loopback connection;
//! - `sequence`: all of the above in this order in one process, each one's
//!   first operation measured as it comes (later categories then see the
//!   lazy state earlier ones initialized).
//!
//! Everything a category needs (the client, the inputs, the request being
//! planned, the runtime and loopback responder) is prepared before the
//! measured region; the measured region is exactly the operation and the drop
//! of its result, as in `head_cost`. Allocations are counted with
//! `allocation-counter` (current thread only); "peak live bytes" is the
//! counter's maximum of bytes held at once during the region (allocated
//! bytes, not resident memory). Output: one line per measured category,
//! `first-use <category>: ns=<n> allocations=<n> bytes=<n> peak_live_bytes=<n> net_allocations=<n>`.

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

/// The same keep-alive loopback responder as `head_cost`: one fixed empty
/// `200 OK` per request, written with a single write on a `TCP_NODELAY`
/// socket, on its own threads (none of its allocations are counted).
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
            .name("head-first-use-responder".into())
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

const CATEGORIES: [&str; 9] = [
    "client-build",
    "direct-construction",
    "unbound-construction",
    "binding",
    "plan-show",
    "plan-search",
    "plan-protected",
    "plan-limited",
    "execution",
];

/// Measures one first operation and prints its line.
fn first(category: &str, operation: impl FnOnce()) {
    let mut nanos = 0_u128;
    let info = allocation_counter::measure(|| {
        let started = Instant::now();
        operation();
        nanos = started.elapsed().as_nanos();
    });
    println!(
        "first-use {category}: ns={nanos} allocations={} bytes={} peak_live_bytes={} net_allocations={}",
        info.count_total, info.bytes_total, info.bytes_max, info.count_current
    );
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

/// Runs one category in this process. `sequence` passes a shared client.
fn run(category: &str, shared: Option<&HeadApi>) -> Result<(), Box<dyn std::error::Error>> {
    let owned;
    let head = match (category, shared) {
        ("client-build" | "unbound-construction" | "execution", _) => None,
        (_, Some(head)) => Some(head),
        (_, None) => {
            owned = head_client()?;
            Some(&owned)
        }
    };
    match category {
        "client-build" => {
            let secret = SecretString::try_new("head-cost-only-key")?;
            let tenant = "tenant-a".to_owned();
            first(category, || {
                black_box(
                    HeadApi::builder()
                        .set_tenant(tenant)
                        .api_secret(secret)
                        .configure_runtime(|runtime| {
                            runtime.rate_limiter(Arc::new(NoopRateLimiter::new()))
                        })
                        .build()
                        .expect("client build"),
                );
            });
        }
        "direct-construction" => {
            let head = head.ok_or("client")?;
            let (collection, term, tags) = ("archive".to_owned(), "rust".to_owned(), tags());
            first(category, || {
                let _ = black_box(head.records(collection).search(term, tags));
            });
        }
        "unbound-construction" => {
            let (collection, id) = ("archive".to_owned(), "item-1".to_owned());
            first(category, || {
                black_box(HeadApi::unbound().records(collection).show(id));
            });
        }
        "binding" => {
            let head = head.ok_or("client")?;
            let endpoint = HeadApi::unbound()
                .records("archive".to_owned())
                .show("item-1".to_owned());
            first(category, || {
                let _ = black_box(head.request(endpoint));
            });
        }
        "plan-show" => {
            let head = head.ok_or("client")?;
            let show = HeadApi::unbound()
                .records("archive".to_owned())
                .show("item-1".to_owned());
            first(category, || {
                black_box(head.plan(&show).expect("show plan"));
            });
        }
        "plan-search" => {
            let head = head.ok_or("client")?;
            let search = head
                .records("archive".to_owned())
                .search("rust".to_owned(), tags());
            first(category, || {
                black_box(search.plan().expect("search plan"));
            });
        }
        "plan-protected" => {
            let protected = head.ok_or("client")?.protected();
            first(category, || {
                black_box(protected.plan().expect("protected plan"));
            });
        }
        "plan-limited" => {
            let limited = head.ok_or("client")?.limited();
            first(category, || {
                black_box(limited.plan().expect("limited plan"));
            });
        }
        "execution" => {
            let responder = Responder::start()?;
            let proxy = SafeProxy::parse(format!("http://{}", responder.address))?;
            let exec = ExecApi::builder()
                .configure_http(move |http| http.proxy(proxy))
                .build()?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            // As in `head_cost`, the measured operation includes building the
            // request and its `IntoFuture` (the boxed operation future).
            first(category, || {
                black_box(
                    runtime
                        .block_on(exec.bounded().into_future())
                        .expect("loopback execution"),
                );
            });
            assert_eq!(responder.requests.load(Ordering::SeqCst), 1);
        }
        other => return Err(format!("unknown category {other:?}").into()),
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let category = std::env::args().nth(1).ok_or_else(|| {
        format!(
            "usage: head_first_use <category>; categories: {}, sequence",
            CATEGORIES.join(", ")
        )
    })?;
    println!(
        "concord-perf {} head first-use probe: category {category}, no warm-up",
        env!("CARGO_PKG_VERSION")
    );
    if category == "sequence" {
        let mut head = None;
        for category in CATEGORIES {
            run(category, head.as_ref())?;
            if category == "client-build" {
                // The measured build's client was dropped inside the region;
                // later categories share one client built here (its lazy state
                // was already touched by the measured build).
                head = Some(head_client()?);
            }
        }
    } else {
        run(&category, None)?;
    }
    Ok(())
}
