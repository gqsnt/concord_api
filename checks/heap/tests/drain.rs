//! C4-R-010: an endpoint with output `()` must drain a large response without buffering it.
//! A counting global allocator records the peak heap size while the request runs; the same body
//! fetched as `Binary` (which buffers by design) shows that the measurement discriminates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; only sizes are counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(now, Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const BODY: usize = 64 * 1024 * 1024;
static CHUNK: [u8; 64 * 1024] = [7; 64 * 1024];

/// Serves `BODY` bytes per request from a static chunk (no large allocation on the server side).
async fn serve() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let mut head = Vec::new();
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {BODY}\r\nconnection: close\r\n\r\n"
                );
                if stream.write_all(reply.as_bytes()).await.is_err() {
                    return;
                }
                for _ in 0..BODY / CHUNK.len() {
                    if stream.write_all(&CHUNK).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    addr
}

concord::api! {
    /// Drain fixture.
    pub client Big {
        base "http://big.invalid";
        GET drained() { path "big"; }
        GET buffered() -> concord::Binary { path "big"; }
    }
}

async fn peak_growth<F: std::future::Future>(f: F) -> (usize, F::Output) {
    let before = CURRENT.load(Relaxed);
    PEAK.store(before, Relaxed);
    let out = f.await;
    (PEAK.load(Relaxed).saturating_sub(before), out)
}

#[tokio::test(flavor = "current_thread")]
async fn unit_response_is_drained_not_buffered() {
    let addr = serve().await;
    let api = Big::new(concord::reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();

    let (unit_growth, result) = peak_growth(api.drained().send()).await;
    result.unwrap();
    let (binary_growth, bytes) = peak_growth(api.buffered().send()).await;
    assert_eq!(bytes.unwrap().len(), BODY);

    println!("peak heap growth: () {unit_growth} bytes, Binary {binary_growth} bytes");
    assert!(
        binary_growth >= BODY,
        "the measurement must see buffering ({binary_growth})"
    );
    assert!(
        unit_growth < 4 * 1024 * 1024,
        "`()` must not buffer the body ({unit_growth} bytes)"
    );
}
