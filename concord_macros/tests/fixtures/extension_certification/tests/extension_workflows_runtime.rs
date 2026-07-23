#![forbid(unsafe_code)]

use std::io;
use std::sync::Arc;
use std::time::Duration;

use runtime::advanced::SafeProxy;
use runtime::prelude::{Bytes, SecretString};
use concord_test_support::tls::TlsServer;
use concord_fixture_extension_certification::{
    DirectExtensionProvider, ExtensionHooks, ExtensionLimiter, SecureExtensionApi,
    extension_runtime_probe_counts, reset_extension_runtime_probes,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const DEADLOCK_BOUND: Duration = Duration::from_secs(10);
const CREDENTIAL_SENTINEL: &str = "extension-runtime-credential-sentinel-5ce18a";
const MAX_CONNECT_HEAD_BYTES: usize = 8 * 1024;

async fn run_connect_proxy(listener: TcpListener, tls_address: std::net::SocketAddr) -> io::Result<String> {
    let (mut downstream, _) = timeout(DEADLOCK_BOUND, listener.accept())
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "proxy accept timed out"))??;
    let mut head = Vec::new();
    let mut chunk = [0_u8; 512];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = downstream.read(&mut chunk).await?;
        if read == 0 || head.len() + read > MAX_CONNECT_HEAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "proxy CONNECT head was incomplete or excessive",
            ));
        }
        head.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8(head)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "CONNECT head was not UTF-8"))?;
    let mut upstream = TcpStream::connect(tls_address).await?;
    downstream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await?;
    Ok(head)
}

#[tokio::test]
async fn downstream_credential_limiter_and_hooks_execute_through_generated_runtime() {
    let tls = TlsServer::start().expect("trusted application TLS fixture");
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("ephemeral extension proxy");
    let address = listener.local_addr().expect("extension proxy address");
    let server = tokio::spawn(run_connect_proxy(listener, tls.address()));
    let proxy = SafeProxy::parse(format!("http://{address}")).expect("loopback proxy URL");
    let certificate = tls.certificate_pem();

    reset_extension_runtime_probes();
    let client = SecureExtensionApi::builder()
        .session_provider(DirectExtensionProvider::new(SecretString::new(
            CREDENTIAL_SENTINEL,
        )))
        .http(move |http| http.proxy(proxy).add_root_certificate_pem(certificate))
        .runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .hooks(Arc::new(ExtensionHooks))
        })
        .build()
        .expect("public-only extension runtime client");
    let output = client
        .protected()
        .await
        .expect("generated protected extension request");
    assert_eq!(output, Bytes::from_static(b"ok"));

    tls.wait_for_requests(1, DEADLOCK_BOUND)
        .expect("TLS application request");
    let requests = tls.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), "/");
    assert_eq!(requests[0].header("x-session"), Some(CREDENTIAL_SENTINEL));

    drop(client);
    let connect = timeout(DEADLOCK_BOUND, server)
        .await
        .expect("extension proxy task timed out")
        .expect("extension proxy task")
        .expect("extension proxy result");
    assert!(connect.starts_with("CONNECT localhost:443 HTTP/1.1"));
    assert_eq!(extension_runtime_probe_counts(), (1, 1, 1, 1, 1, 1));

    println!(
        "CONCORD_DOWNSTREAM_EXTENSION_RUNTIME_OK provider=1 limiter_acquire=1 limiter_observe=1 hook_before=1 hook_response=1 hook_complete=1"
    );
}
