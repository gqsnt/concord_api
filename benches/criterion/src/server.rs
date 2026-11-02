//! Minimal keep-alive HTTP/1.1 loopback server (moved from the former `benches/perf` harness).
//!
//! Every request gets a `200 OK` JSON response written with one `write_all`: the large body when
//! the request target contains `/json`, the small S1 health document otherwise.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The small response body (~90 B); decodes as S1 `Health`.
pub const SMALL_BODY: &[u8] =
    br#"{"status":"ok","id":"1","name":"n","healthy":true,"title":"t","items":[],"next_cursor":null}"#;

fn response(body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// Binds `127.0.0.1:0` and serves on the current Tokio runtime until it shuts down. `large` is
/// returned for request targets containing `/json`.
pub async fn serve(large: &[u8]) -> SocketAddr {
    let small = Arc::new(response(SMALL_BODY));
    let large = Arc::new(response(large));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            stream.set_nodelay(true).expect("nodelay");
            let (small, large) = (small.clone(), large.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut pending = Vec::new();
                loop {
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    pending.extend_from_slice(&buf[..n]);
                    while let Some(pos) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                        let line_end = pending.windows(2).position(|w| w == b"\r\n").unwrap_or(pos);
                        let wants_large = pending[..line_end].windows(5).any(|w| w == b"/json");
                        pending.drain(..pos + 4);
                        // One write per response: separate head/body writes interact with
                        // Nagle's algorithm and delayed ACKs (~40 ms stalls).
                        let reply = if wants_large { &large } else { &small };
                        if stream.write_all(reply).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    addr
}
