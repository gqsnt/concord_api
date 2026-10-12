#![cfg(all(feature = "oauth2-client", feature = "default-tls"))]

use concord::auth::OAuth2ClientCredentials;
use concord::error::AuthErrorKind;
use concord::http::SafeProxy;
use concord::{Binary, Bytes, api};
use concord_core::contracts::{Method, ProviderRequest, StatusCode, Url};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;
use std::time::Duration;

api! {
    client R8OAuth {
        timeout 10s;
        base "https://localhost/";
        credentials { credential oauth = custom OAuth2ClientCredentials; }
        auth { bearer oauth; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

fn client(server: &TlsServer, limit: usize) -> (R8OAuth, ConnectProxy) {
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let proxy_url = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let provider = OAuth2ClientCredentials::builder("https://localhost/token")
        .client_id("r8-private-client-id")
        .client_secret("r8-private-client-secret")
        .build()
        .unwrap();
    let client = R8OAuth::builder()
        .oauth(provider)
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy_url)
                .add_root_certificate_pem(&certificate)
        })
        .configure_runtime(move |runtime| {
            runtime
                .provider_response_limit(limit)
                .provider_timeout(Duration::from_secs(5))
        })
        .build()
        .unwrap();
    (client, proxy)
}

fn safe(error: &concord::error::ApiError) {
    let formatted = format!("{error} {error:?}");
    for secret in [
        "r8-private-client-id",
        "r8-private-client-secret",
        "r8-private-body",
        "r8-private-token",
    ] {
        assert!(!formatted.contains(secret));
    }
}

struct R8TimeoutProvider([concord::auth::ProviderOrigin; 1]);
impl concord::auth::CredentialProvider for R8TimeoutProvider {
    type Material = concord::auth::AccessToken;
    fn acquire<'a>(
        &'a self,
        context: concord::auth::CredentialRequest<'a>,
    ) -> concord::auth::BoxFuture<'a, Result<Self::Material, concord::auth::AuthError>> {
        Box::pin(async move {
            context
                .http
                .send(
                    ProviderRequest::new(
                        Method::POST,
                        Url::parse("https://localhost/token").unwrap(),
                    )
                    .timeout(Duration::from_millis(200)),
                )
                .await?;
            Err(concord::auth::AuthError::new(AuthErrorKind::Acquisition))
        })
    }
    fn http_origins(&self) -> &[concord::auth::ProviderOrigin] {
        &self.0
    }
}

api! {
    client R8Timeout {
        timeout 10s;
        base "https://localhost/";
        credentials { credential token = custom R8TimeoutProvider; }
        auth { bearer token; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

#[tokio::test]
async fn r8_returned_tls_body_timeout_preserves_rejection_journal_and_cooldown() {
    let server = TlsServer::start().unwrap();
    server.queue_wire_response(vec![
        (Duration::ZERO, b"HTTP/1.1 429 Rejected\r\nRetry-After: 10\r\nContent-Length: 100\r\nConnection: close\r\n\r\n".to_vec()),
        (Duration::from_millis(500), b"r8-private-body".to_vec()),
    ]).unwrap();
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let proxy_url = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let client = R8Timeout::builder()
        .token(R8TimeoutProvider([concord::auth::ProviderOrigin::parse(
            "https://localhost",
        )
        .unwrap()]))
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy_url)
                .add_root_certificate_pem(&certificate)
        })
        .configure_runtime(|runtime| runtime.provider_timeout(Duration::from_secs(5)))
        .build()
        .unwrap();
    for _ in 0..2 {
        let error = client.protected().await.unwrap_err();
        safe(&error);
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
        let facts = error.provider_acquisition().unwrap();
        assert_eq!(facts.last_status(), Some(StatusCode::TOO_MANY_REQUESTS));
        assert_eq!(facts.helper_sends(), 1);
        assert_eq!(
            facts.outcome(),
            concord::error::ProviderAcquisitionOutcome::Failed
        );
        assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    }
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn r8_rejected_known_length_excess_retains_cooldown() {
    for status in [429, 503] {
        let server = TlsServer::start().unwrap();
        for _ in 0..2 {
            server
                .queue_response_with_headers(status, &[("Retry-After", "10")], b"r8-private-body")
                .unwrap();
        }
        let (client, _proxy) = client(&server, 4);
        for _ in 0..2 {
            let error = client.protected().await.unwrap_err();
            safe(&error);
            assert_eq!(error.auth_kind(), Some(AuthErrorKind::ResponseTooLarge));
            assert_eq!(
                error
                    .provider_acquisition()
                    .unwrap()
                    .last_status()
                    .unwrap()
                    .as_u16(),
                status
            );
        }
        println!(
            "R8 known excess status={status} helper_count={}",
            server.requests().len()
        );
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn r8_body_error_cooldown_expires_and_recovers_naturally() {
    let server = TlsServer::start().unwrap();
    server
        .queue_response_with_headers(503, &[("Retry-After", "1")], vec![b'x'; 129])
        .unwrap();
    server
        .queue_response(
            200,
            br#"{"access_token":"r8-private-token","token_type":"bearer","expires_in":120}"#,
        )
        .unwrap();
    server.queue_response(200, b"ok").unwrap();
    let (client, _proxy) = client(&server, 128);
    for _ in 0..2 {
        assert_eq!(
            client.protected().await.unwrap_err().auth_kind(),
            Some(AuthErrorKind::ResponseTooLarge)
        );
    }
    assert_eq!(server.requests().len(), 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(client.protected().await.unwrap(), Bytes::from_static(b"ok"));
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn r8_rejected_chunked_excess_and_truncated_body_retain_cooldown() {
    for (framing, kind) in [
        (
            "Transfer-Encoding: chunked\r\n\r\nf\r\nr8-private-body\r\n0\r\n\r\n",
            AuthErrorKind::ResponseTooLarge,
        ),
        (
            "Content-Length: 100\r\n\r\nr8-private-body",
            AuthErrorKind::Acquisition,
        ),
    ] {
        let server = TlsServer::start().unwrap();
        for _ in 0..2 {
            server
                .queue_wire_response(vec![(
                    Duration::ZERO,
                    format!(
                        "HTTP/1.1 429 Rejected\r\nConnection: close\r\nRetry-After: 10\r\n{framing}"
                    )
                    .into_bytes(),
                )])
                .unwrap();
        }
        let (client, _proxy) = client(
            &server,
            if kind == AuthErrorKind::ResponseTooLarge {
                4
            } else {
                1000
            },
        );
        for index in 0..2 {
            let error = client.protected().await.unwrap_err();
            safe(&error);
            println!(
                "R8 wire index={index} expected={kind:?} actual={:?} helpers={} head={:?}",
                error.auth_kind(),
                server.requests().len(),
                error.provider_acquisition().unwrap().last_status()
            );
            assert_eq!(error.auth_kind(), Some(kind));
        }
        println!(
            "R8 body failure kind={kind:?} helper_count={}",
            server.requests().len()
        );
        assert_eq!(server.requests().len(), 1);
    }
}

async fn delayed_ttl(seconds: u64) -> (bool, usize) {
    let server = TlsServer::start().unwrap();
    let body = format!(
        r#"{{"access_token":"r8-private-token","token_type":"bearer","expires_in":{seconds}}}"#
    );
    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
    let split = body.len() / 2;
    // Real std monotonic elapsed time, unpaused Tokio; the head and partial JSON
    // precede a delay longer than the short token's declared TTL.
    for _ in 0..4 {
        server
            .queue_wire_response(vec![
                (
                    Duration::ZERO,
                    [head.as_bytes(), &body.as_bytes()[..split]].concat(),
                ),
                (
                    Duration::from_millis(1200),
                    body.as_bytes()[split..].to_vec(),
                ),
            ])
            .unwrap();
    }
    let (client, _proxy) = client(&server, 1024);
    let result = client.protected().await;
    let requests = server.requests();
    let applications = requests
        .iter()
        .filter(|request| request.target() == "/protected")
        .count();
    println!(
        "R8 delayed TTL={seconds} ok={} helpers={} applications={applications}",
        result.is_ok(),
        requests.len() - applications
    );
    (result.is_ok(), applications)
}

async fn delayed_positive() -> (bool, usize) {
    let server = TlsServer::start().unwrap();
    let body = br#"{"access_token":"r8-private-token","token_type":"bearer","expires_in":120}"#;
    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
    server
        .queue_wire_response(vec![
            (Duration::ZERO, [head.as_bytes(), &body[..20]].concat()),
            (Duration::from_millis(1200), body[20..].to_vec()),
        ])
        .unwrap();
    server.queue_response(200, b"ok").unwrap();
    let (client, _proxy) = client(&server, 1024);
    let result = client.protected().await;
    let applications = server
        .requests()
        .iter()
        .filter(|r| r.target() == "/protected")
        .count();
    println!(
        "R8 delayed TTL=120 ok={} applications={applications}",
        result.is_ok()
    );
    (result.is_ok(), applications)
}

#[tokio::test]
async fn r8_elapsed_tls_acquisition_consumes_short_ttl() {
    assert_eq!(delayed_ttl(1).await, (false, 0));
}

#[tokio::test]
async fn r8_delayed_long_ttl_control() {
    assert_eq!(delayed_positive().await, (true, 1));
}

async fn token_type(body: &'static [u8], valid: bool) {
    let server = TlsServer::start().unwrap();
    server.queue_response(200, body).unwrap();
    server
        .queue_response(200, if valid { b"ok".as_slice() } else { body })
        .unwrap();
    let (client, _proxy) = client(&server, 1024);
    let result = client.protected().await;
    let applications = server
        .requests()
        .iter()
        .filter(|r| r.target() == "/protected")
        .count();
    println!(
        "R8 token type expected_valid={valid} ok={} applications={applications}",
        result.is_ok()
    );
    assert_eq!(result.is_ok(), valid);
    assert_eq!(applications, usize::from(valid));
    if let Err(error) = result {
        safe(&error);
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
        // A repeated malformed response must cause another helper exchange,
        // never reuse an installed token or issue an application request.
        assert_eq!(
            client.protected().await.unwrap_err().auth_kind(),
            Some(AuthErrorKind::Acquisition)
        );
        assert_eq!(server.requests().len(), 2);
        assert!(server.requests().iter().all(|r| r.target() == "/token"));
    }
}

#[tokio::test]
async fn r8_missing_token_type_is_rejected() {
    token_type(br#"{"access_token":"r8-private-token"}"#, false).await;
}

#[tokio::test]
async fn r8_token_type_controls_and_optional_expiration() {
    for body in [
        br#"{"access_token":"r8-private-token","token_type":null}"#.as_slice(),
        br#"{"access_token":"r8-private-token","token_type":7}"#,
        br#"{"access_token":"r8-private-token","token_type":""}"#,
        br#"{"access_token":"r8-private-token","token_type":"mac"}"#,
    ] {
        token_type(body, false).await;
    }
    for body in [
        br#"{"access_token":"r8-private-token","token_type":"bearer"}"#.as_slice(),
        br#"{"access_token":"r8-private-token","token_type":"BeArEr","unknown":7,"expires_in":120}"#,
    ] { token_type(body, true).await; }
}
