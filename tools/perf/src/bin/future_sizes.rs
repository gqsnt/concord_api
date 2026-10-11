//! Deterministic execution-layout probe (V3-O07, r3 layout slice): the size
//! of the boxed operation future of one managed execution and the
//! client-thread allocation count and bytes of one warm execution.
//!
//! No timing. Both numbers are deterministic for a given source tree,
//! toolchain and target: the boxed future's size is the `size_of` of its
//! coroutine state (read with `size_of_val` through the `dyn Future`), and
//! the allocated bytes of a warm keep-alive execution include every boxed
//! coroutine state of the execution (operation future, buffered
//! acquisition future) plus fixed-size buffers.
//!
//! Like `head_cost`, this file is dialect-neutral: it uses only calls that
//! exist unchanged in Concord v2.1 and v3 (`builder`, `configure_http`,
//! direct endpoint methods and `IntoFuture`) and includes the same
//! `src/head_cost/declaration.rs`, so it can be rerun against v2.1 (with
//! `declaration_v2_1.rs.txt` copied over the declaration), any v3 commit and
//! the corrected tree with the same host, toolchain and `--release` profile.

#![forbid(unsafe_code)]

use std::future::IntoFuture;
use std::hint::black_box;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;

use concord::http::SafeProxy;

include!("../head_cost/declaration.rs");

const WARMUP: usize = 50;
const BATCH: usize = 200;

/// A keep-alive loopback responder on its own threads (not counted): every
/// request receives one fixed empty `200 OK` in a single write.
fn start_responder() -> io::Result<SocketAddr> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    thread::Builder::new()
        .name("future-sizes-responder".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                thread::spawn(move || serve(stream));
            }
        })?;
    Ok(address)
}

fn serve(mut stream: TcpStream) {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = start_responder()?;
    let proxy = SafeProxy::parse(format!("http://{address}"))?;
    let exec = ExecApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;

    // The boxed operation future of `IntoFuture` (never polled here).
    let future = exec.bounded().into_future();
    let operation_future = std::mem::size_of_val(&*future);
    drop(future);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for _ in 0..WARMUP {
        black_box(runtime.block_on(exec.bounded().into_future())?);
    }
    let info = allocation_counter::measure(|| {
        for _ in 0..BATCH {
            black_box(
                runtime
                    .block_on(exec.bounded().into_future())
                    .expect("loopback execution"),
            );
        }
    });

    println!(
        "concord-perf {} future-sizes probe (warmup {WARMUP}, batch {BATCH})",
        env!("CARGO_PKG_VERSION")
    );
    println!("boxed operation future (IntoFuture, ExecApi::bounded): {operation_future} B");
    println!(
        "managed execution, client thread: {:.2} allocations/op, {:.2} bytes/op",
        info.count_total as f64 / BATCH as f64,
        info.bytes_total as f64 / BATCH as f64,
    );
    Ok(())
}
