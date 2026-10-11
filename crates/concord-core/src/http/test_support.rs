use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use h2::Reason;
use tokio::net::TcpListener;

const WAIT: Duration = Duration::from_secs(3);

pub(crate) struct H2RefusedServer {
    address: SocketAddr,
    physical_attempts: Arc<AtomicUsize>,
    worker: tokio::task::JoinHandle<()>,
}

impl H2RefusedServer {
    pub(crate) async fn start(refusals: usize, expected_attempts: usize) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("HTTP/2 retry listener should bind");
        let address = listener.local_addr().expect("HTTP/2 address should exist");
        let physical_attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&physical_attempts);
        let worker = tokio::spawn(async move {
            let (stream, _) = tokio::time::timeout(WAIT, listener.accept())
                .await
                .expect("HTTP/2 client should connect")
                .expect("HTTP/2 accept should succeed");
            let mut connection = h2::server::handshake(stream)
                .await
                .expect("HTTP/2 handshake should succeed");
            while let Some(request) = tokio::time::timeout(WAIT, connection.accept())
                .await
                .expect("HTTP/2 request should arrive")
            {
                let (_, mut respond) = request.expect("HTTP/2 request should parse");
                let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt <= refusals {
                    respond.send_reset(Reason::REFUSED_STREAM);
                } else {
                    let response = http::Response::builder()
                        .status(200)
                        .body(())
                        .expect("HTTP/2 response should build");
                    respond
                        .send_response(response, true)
                        .expect("HTTP/2 response should send");
                }
                if attempt >= expected_attempts {
                    // Any unexpected extra physical try is still counted.
                    if let Ok(Some(Ok((_, mut respond)))) =
                        tokio::time::timeout(Duration::from_millis(500), connection.accept()).await
                    {
                        counter.fetch_add(1, Ordering::SeqCst);
                        respond.send_reset(Reason::REFUSED_STREAM);
                    }
                    break;
                }
            }
        });
        Self {
            address,
            physical_attempts,
            worker,
        }
    }

    pub(crate) fn base_url(&self) -> String {
        format!("http://{}/", self.address)
    }

    pub(crate) fn url(&self) -> String {
        format!("http://{}/retry", self.address)
    }

    pub(crate) fn physical_attempts(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.physical_attempts)
    }

    pub(crate) async fn finish(self) {
        tokio::time::timeout(WAIT, self.worker)
            .await
            .expect("HTTP/2 worker should finish")
            .expect("HTTP/2 worker should not panic");
    }
}
