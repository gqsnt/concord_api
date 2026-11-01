//! HTTPS loopback cases that need reqwest's TLS stack (feature `reqwest-defaults`).

mod support;

use std::error::Error as _;
use std::sync::Arc;

use concord::{ErrorKind, reqwest};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CA: &[u8] = include_bytes!("support/tls/ca.pem");
const CERT: &[u8] = include_bytes!("support/tls/cert.pem");
const KEY: &[u8] = include_bytes!("support/tls/key.pem");
const SECRET: &str = "S3CR3T-v@lue/x";
const SECRET_QUERY: &str = "S3CR3T-v%40lue%2Fx";

/// An HTTPS server that answers every request with `302` to `http://…` echoing the query and,
/// as `token=`, the `authorization` header (servers can build redirect targets from anything).
async fn https_redirect_to_http() -> u16 {
    let config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from_pem_slice(CERT).unwrap()],
            PrivateKeyDer::from_pem_slice(KEY).unwrap(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&head);
                let target = text.split(' ').nth(1).unwrap_or("/");
                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
                let mut location =
                    reqwest::Url::parse(&format!("http://127.0.0.1:{port}/next?{query}")).unwrap();
                let auth = text
                    .split("\r\n")
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                    .map(|(_, value)| value.trim().to_owned());
                if let Some(auth) = auth {
                    location.query_pairs_mut().append_pair("token", &auth);
                }
                let reply = format!(
                    "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
                let _ = tls.write_all(reply.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    port
}

concord::api! {
    /// TLS fixture.
    pub client Secure(#[sensitive] query("sig") sig: String) {
        base "https://localhost";
        GET ping();
        GET public(query("q") q: String);
    }
}

fn https_only_client() -> reqwest::Client {
    reqwest::Client::builder()
        .https_only(true)
        .tls_certs_only([reqwest::Certificate::from_pem(CA).unwrap()])
        .build()
        .unwrap()
}

fn chain(error: Option<&(dyn std::error::Error + 'static)>) -> String {
    let mut out = String::new();
    let mut source = error;
    while let Some(s) = source {
        out.push_str(&format!(" | {s} {s:?}"));
        source = s.source();
    }
    out
}

/// C4-R-008: an `https_only` client refuses the downgrade redirect; reqwest nests an error that
/// carries the redirect target (query included). Concord's `Display`, `Debug` and `source()`
/// chain must not show the sensitive query value; the native chain (baseline) does.
#[tokio::test]
async fn c4_r_008_redirect_error_chain_does_not_leak() {
    let port = https_redirect_to_http().await;
    let api = Secure::new(https_only_client(), SECRET)
        .with_base_url(format!("https://localhost:{port}"))
        .unwrap();

    let error = api.ping().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    let shown = format!("{error} {error:?}{}", chain(error.source()));
    assert!(
        !shown.contains(SECRET_QUERY) && !shown.contains("S3CR3T"),
        "{shown}"
    );

    let native = chain(
        error
            .transport()
            .map(|e| e as &(dyn std::error::Error + 'static)),
    );
    assert!(
        native.contains(SECRET_QUERY),
        "baseline changed (no nested URL): {native}"
    );
}

concord::api! {
    /// Header-only secret: no sensitive URL part at all.
    pub client HeaderOnly(#[sensitive] header("authorization") token: String) {
        base "https://localhost";
        GET ping();
    }
}

/// C4-R-008 (residual, final Codex review): a server may copy a sensitive *header* into the
/// redirect target; the nested reqwest error then holds it. Concord errors expose no `source()`,
/// so nothing reaches ordinary error reporting; the native error (explicit accessor) does.
#[tokio::test]
async fn c4_r_008_header_reflected_into_redirect_does_not_leak() {
    const HEADER_SECRET: &str = "Bearer hdr-S3CR3T/+=";
    let port = https_redirect_to_http().await;
    let api = HeaderOnly::new(https_only_client(), HEADER_SECRET)
        .with_base_url(format!("https://localhost:{port}"))
        .unwrap();

    let error = api.ping().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.source().is_none());
    let shown = format!("{error} {error:?}{}", chain(error.source()));
    assert!(!shown.contains("S3CR3T"), "{shown}");

    let native = chain(
        error
            .transport()
            .map(|e| e as &(dyn std::error::Error + 'static)),
    );
    assert!(
        native.contains("hdr-S3CR3T"),
        "baseline changed (no reflected header): {native}"
    );
}
