use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_test_support::server::{ResponseScript, ScriptedServer};
use concord_test_support::tls::TlsServer;

const AUTH_MARKER: &str = "Probe inert-auth-marker";

#[derive(Eq, PartialEq)]
enum AuthProbeError {
    CleartextAuthentication,
    InvalidUrl,
    Transport,
}

impl fmt::Debug for AuthProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CleartextAuthentication => "CleartextAuthentication",
            Self::InvalidUrl => "InvalidUrl",
            Self::Transport => "Transport",
        })
    }
}

impl fmt::Display for AuthProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CleartextAuthentication => "effective authentication requires HTTPS",
            Self::InvalidUrl => "invalid probe URL",
            Self::Transport => "probe transport failed",
        })
    }
}

fn base_builder() -> reqwest::ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .retry(reqwest::retry::never())
}

async fn send_endpoint<F>(
    client: &reqwest::Client,
    url: &str,
    effective_authentication: bool,
    materialize: F,
) -> Result<reqwest::StatusCode, AuthProbeError>
where
    F: FnOnce() -> String,
{
    let parsed = reqwest::Url::parse(url).map_err(|_| AuthProbeError::InvalidUrl)?;
    if effective_authentication && parsed.scheme() != "https" {
        return Err(AuthProbeError::CleartextAuthentication);
    }
    let request = client.get(parsed);
    let request = if effective_authentication {
        request.header(reqwest::header::AUTHORIZATION, materialize())
    } else {
        request
    };
    request
        .send()
        .await
        .map(|response| response.status())
        .map_err(|_| AuthProbeError::Transport)
}

fn gate_provider_url<F>(url: &str, materialize: F) -> Result<(), AuthProbeError>
where
    F: FnOnce() -> String,
{
    let parsed = reqwest::Url::parse(url).map_err(|_| AuthProbeError::InvalidUrl)?;
    if parsed.scheme() != "https" {
        return Err(AuthProbeError::CleartextAuthentication);
    }
    let material = materialize();
    drop(material);
    Ok(())
}

#[tokio::test]
async fn authenticated_http_fails_before_materialization() {
    let origin = ScriptedServer::start([ResponseScript::ok(b"unexpected".to_vec())])
        .expect("HTTP origin should start");
    let count = Arc::new(AtomicUsize::new(0));
    let materializations = Arc::clone(&count);
    let result = send_endpoint(
        &base_builder().build().expect("client should build"),
        &origin.url("/authenticated"),
        true,
        move || {
            materializations.fetch_add(1, Ordering::SeqCst);
            AUTH_MARKER.to_owned()
        },
    )
    .await;
    assert_eq!(result, Err(AuthProbeError::CleartextAuthentication));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(origin.request_count(), 0);
}

#[tokio::test]
async fn authenticated_http_localhost_has_no_exception() {
    let origin = ScriptedServer::start([ResponseScript::ok(b"unexpected".to_vec())])
        .expect("localhost HTTP origin should start");
    let count = Arc::new(AtomicUsize::new(0));
    let materializations = Arc::clone(&count);
    let url = format!("http://localhost:{}/authenticated", origin.address().port());
    let result = send_endpoint(
        &base_builder()
            .resolve("localhost", origin.address())
            .build()
            .expect("localhost client should build"),
        &url,
        true,
        move || {
            materializations.fetch_add(1, Ordering::SeqCst);
            AUTH_MARKER.to_owned()
        },
    )
    .await;
    assert_eq!(result, Err(AuthProbeError::CleartextAuthentication));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(origin.request_count(), 0);
}

#[tokio::test]
async fn unauthenticated_http_is_allowed() {
    let origin = ScriptedServer::start([ResponseScript::ok(b"allowed".to_vec())])
        .expect("unauthenticated HTTP origin should start");
    let count = Arc::new(AtomicUsize::new(0));
    let materializations = Arc::clone(&count);
    let result = send_endpoint(
        &base_builder().build().expect("client should build"),
        &origin.url("/unauthenticated"),
        false,
        move || {
            materializations.fetch_add(1, Ordering::SeqCst);
            AUTH_MARKER.to_owned()
        },
    )
    .await;
    assert_eq!(result, Ok(reqwest::StatusCode::OK));
    origin
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("unauthenticated request should arrive");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let requests = origin.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("authorization"), None);
}

#[tokio::test]
async fn credentialed_https_materializes_after_scheme_validation() {
    let server = TlsServer::start().expect("trusted TLS fixture should start");
    let certificate = reqwest::Certificate::from_der(server.certificate_der())
        .expect("test trust anchor should parse");
    let client = base_builder()
        .add_root_certificate(certificate)
        .resolve("localhost", server.address())
        .build()
        .expect("trusted TLS client should build");
    let count = Arc::new(AtomicUsize::new(0));
    let materializations = Arc::clone(&count);
    let result = send_endpoint(
        &client,
        &server.url("localhost", "/authenticated"),
        true,
        move || {
            materializations.fetch_add(1, Ordering::SeqCst);
            AUTH_MARKER.to_owned()
        },
    )
    .await;
    assert_eq!(result, Ok(reqwest::StatusCode::OK));
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("authenticated TLS request should arrive");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), "/authenticated");
    assert!(
        requests[0]
            .header("authorization")
            .is_some_and(|value| value == AUTH_MARKER)
    );
    for diagnostic in [format!("{result:?}"), AuthProbeError::Transport.to_string()] {
        assert!(!diagnostic.contains(AUTH_MARKER));
    }
}

#[test]
fn http_provider_request_fails_before_materialization() {
    for host in ["127.0.0.1", "localhost"] {
        let count = AtomicUsize::new(0);
        let result = gate_provider_url(&format!("http://{host}:1/token"), || {
            count.fetch_add(1, Ordering::SeqCst);
            AUTH_MARKER.to_owned()
        });
        assert_eq!(result, Err(AuthProbeError::CleartextAuthentication));
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn https_provider_request_passes_scheme_validation() {
    let count = AtomicUsize::new(0);
    let result = gate_provider_url("https://localhost:443/token", || {
        count.fetch_add(1, Ordering::SeqCst);
        AUTH_MARKER.to_owned()
    });
    assert_eq!(result, Ok(()));
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
