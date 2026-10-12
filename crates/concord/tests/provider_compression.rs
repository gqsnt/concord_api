#![cfg(all(
    feature = "default-tls",
    feature = "oauth2-client",
    feature = "gzip",
    feature = "brotli",
    feature = "deflate"
))]

use concord::auth::{
    AccessToken, AuthError, CredentialProvider, CredentialRequest, OAuth2ClientCredentials,
    ProviderOrigin,
};
use concord::error::{
    ApiError, AuthErrorKind, DispatchKnowledge, FailureStage, ProviderAcquisitionOutcome,
};
use concord::http::SafeProxy;
use concord::{Binary, Bytes, api};
use concord_core::contracts::{BoxFuture, Method, ProviderRequest, Url};
use concord_test_support::{proxy::ConnectProxy, tls::TlsServer};
use std::time::Duration;

include!("support/provider_compression_fixtures.rs");

api! {
    client ProviderCompressionOAuth {
        base "https://localhost/";
        timeout 10s;
        credentials { credential token = custom OAuth2ClientCredentials; }
        auth { bearer token; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

struct BoundedBodyProvider {
    origins: [ProviderOrigin; 1],
    expected_bytes: usize,
    request_timeout: Option<Duration>,
}
impl CredentialProvider for BoundedBodyProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        Box::pin(async move {
            let mut request =
                ProviderRequest::new(Method::POST, Url::parse("https://localhost/token").unwrap());
            if let Some(timeout) = self.request_timeout {
                request = request.timeout(timeout);
            }
            let response = context.http.send(request).await?;
            if !response.status().is_success() {
                let mut error = AuthError::new(AuthErrorKind::Acquisition);
                if let Some(delay) = response.retry_after() {
                    error = error.with_retry_after(delay);
                }
                return Err(error);
            }
            if response.body().len() != self.expected_bytes
                || response.headers().contains_key("content-encoding")
            {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            AccessToken::try_new("bounded-provider-fixture-token").map_err(AuthError::from)
        })
    }
    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

api! {
    client ProviderCompressionBounded {
        base "https://localhost/";
        timeout 10s;
        credentials { credential token = custom BoundedBodyProvider; }
        auth { bearer token; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

fn oauth(server: &TlsServer, disable: bool) -> (ProviderCompressionOAuth, ConnectProxy) {
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let routing = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let provider = OAuth2ClientCredentials::builder("https://localhost/token")
        .client_id("provider-compression-fixture-client")
        .client_secret("provider-compression-fixture-secret")
        .build()
        .unwrap();
    let client = ProviderCompressionOAuth::builder()
        .token(provider)
        .configure_http(move |http| {
            let http = http
                .http1_only()
                .proxy(routing)
                .add_root_certificate_pem(&certificate);
            if disable { http.disable_gzip() } else { http }
        })
        .configure_runtime(|runtime| {
            runtime
                .provider_response_limit(TOKEN_DECODED_BYTES + 16)
                .provider_timeout(Duration::from_secs(5))
        })
        .build()
        .unwrap();
    (client, proxy)
}

fn bounded(
    server: &TlsServer,
    limit: usize,
    request_timeout: Option<Duration>,
    provider_timeout: Duration,
) -> (ProviderCompressionBounded, ConnectProxy) {
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let routing = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let provider = BoundedBodyProvider {
        origins: [ProviderOrigin::parse("https://localhost").unwrap()],
        expected_bytes: 4096,
        request_timeout,
    };
    let client = ProviderCompressionBounded::builder()
        .token(provider)
        .configure_http(move |http| {
            http.http1_only()
                .proxy(routing)
                .add_root_certificate_pem(&certificate)
        })
        .configure_runtime(move |runtime| {
            runtime
                .provider_response_limit(limit)
                .provider_timeout(provider_timeout)
        })
        .build()
        .unwrap();
    (client, proxy)
}

fn failure(error: &ApiError, status: u16, kind: AuthErrorKind) {
    assert_eq!(error.auth_kind(), Some(kind));
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    let facts = error.provider_acquisition().unwrap();
    assert_eq!(facts.last_status().unwrap().as_u16(), status);
    assert_eq!(facts.helper_sends(), 1);
    assert_eq!(
        facts.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Failed);
    // A retained cooldown can link the earlier acquisition into a newer
    // application operation; only the original leader is its parent.
    assert!(facts.parent_operation_id().is_some());
    let formatted = format!("{error} {error:?}");
    for sentinel in [
        "provider-compression-fixture-secret",
        "provider-compression-fixture-token",
        "bounded-provider-fixture-token",
    ] {
        assert!(!formatted.contains(sentinel));
    }
}

fn stalled_rejection(server: &TlsServer) {
    for _ in 0..2 {
        let mut head = format!("HTTP/1.1 429 Rejected\r\nContent-Encoding: GZip\r\nRetry-After: 10\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", CAP_GZIP.len()).into_bytes();
        head.extend_from_slice(&CAP_GZIP[..5]);
        server
            .queue_wire_response(vec![
                (Duration::ZERO, head),
                (Duration::from_millis(1600), CAP_GZIP[5..].to_vec()),
            ])
            .unwrap();
    }
}

#[tokio::test]
async fn provider_compression_mixed_case_oauth_success_has_exact_material_and_journal() {
    for (coding, encoded) in [
        ("GZip", TOKEN_GZIP),
        ("BR", TOKEN_BROTLI),
        ("Deflate", TOKEN_DEFLATE),
    ] {
        let server = TlsServer::start().unwrap();
        server
            .queue_response_with_headers(200, &[("Content-Encoding", coding)], encoded)
            .unwrap();
        server.queue_response(200, b"ok").unwrap();
        let (client, _proxy) = oauth(&server, false);
        let response = client
            .protected()
            .response()
            .await
            .expect("supported mixed-case provider response decodes");
        assert_eq!(response.value(), &Bytes::from_static(b"ok"));
        assert_eq!(response.url().as_str(), "https://localhost/protected");
        let facts = response.metadata().provider_acquisition().unwrap();
        assert_eq!(facts.last_status().unwrap().as_u16(), 200);
        assert_eq!(facts.helper_sends(), 1);
        assert_eq!(
            facts.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );
        assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Installed);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].target(), "/token");
        assert_eq!(requests[1].target(), "/protected");
        assert_eq!(
            requests[1].header("authorization"),
            Some("Bearer provider-compression-fixture-token")
        );
        println!(
            "provider-compression coding={coding} token_status=200 helper_sends=1 application_sends=1 installed=true"
        );
    }
}

#[tokio::test]
async fn provider_compression_trusted_gzip_disable_keeps_encoded_bytes_unaccepted() {
    let server = TlsServer::start().unwrap();
    for _ in 0..2 {
        server
            .queue_response_with_headers(200, &[("Content-Encoding", "GZip")], TOKEN_GZIP)
            .unwrap();
    }
    let (client, _proxy) = oauth(&server, true);
    for _ in 0..2 {
        failure(
            &client.protected().await.unwrap_err(),
            200,
            AuthErrorKind::Acquisition,
        );
    }
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn provider_compression_decoded_body_cap_exact_positive_and_first_excess() {
    let server = TlsServer::start().unwrap();
    server
        .queue_response_with_headers(200, &[("Content-Encoding", "GZip")], CAP_GZIP)
        .unwrap();
    server.queue_response(200, b"ok").unwrap();
    let (client, _proxy) = bounded(&server, 4096, None, Duration::from_secs(5));
    assert_eq!(client.protected().await.unwrap(), Bytes::from_static(b"ok"));
    assert_eq!(server.requests().len(), 2);
    let server = TlsServer::start().unwrap();
    for _ in 0..2 {
        server
            .queue_response_with_headers(200, &[("Content-Encoding", "GZip")], CAP_GZIP)
            .unwrap();
    }
    let (client, _proxy) = bounded(&server, 4095, None, Duration::from_secs(5));
    for _ in 0..2 {
        failure(
            &client.protected().await.unwrap_err(),
            200,
            AuthErrorKind::ResponseTooLarge,
        );
    }
    assert_eq!(
        server.requests().len(),
        2,
        "accepted head grants no cooldown"
    );
}

#[tokio::test]
async fn provider_compression_rejected_decoded_cap_preserves_returned_error_cooldown() {
    let server = TlsServer::start().unwrap();
    for _ in 0..2 {
        server
            .queue_response_with_headers(
                429,
                &[("Content-Encoding", "GZip"), ("Retry-After", "10")],
                CAP_GZIP,
            )
            .unwrap();
    }
    let (client, _proxy) = bounded(&server, 4095, None, Duration::from_secs(5));
    let first = client.protected().await.unwrap_err();
    let second = client.protected().await.unwrap_err();
    failure(&first, 429, AuthErrorKind::ResponseTooLarge);
    failure(&second, 429, AuthErrorKind::ResponseTooLarge);
    assert_eq!(first.provider_acquisition(), second.provider_acquisition());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn provider_compression_returned_request_timeout_keeps_head_and_cooldown() {
    let server = TlsServer::start().unwrap();
    stalled_rejection(&server);
    let (client, _proxy) = bounded(
        &server,
        4096,
        Some(Duration::from_millis(700)),
        Duration::from_secs(5),
    );
    let first = client.protected().await.unwrap_err();
    let second = client.protected().await.unwrap_err();
    failure(&first, 429, AuthErrorKind::Acquisition);
    failure(&second, 429, AuthErrorKind::Acquisition);
    assert_eq!(first.provider_acquisition(), second.provider_acquisition());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn provider_compression_abandoned_provider_ceiling_preserves_head_without_cooldown() {
    let server = TlsServer::start().unwrap();
    stalled_rejection(&server);
    let (client, _proxy) = bounded(&server, 4096, None, Duration::from_millis(700));
    let first = client.protected().await.unwrap_err();
    let second = client.protected().await.unwrap_err();
    failure(&first, 429, AuthErrorKind::Acquisition);
    failure(&second, 429, AuthErrorKind::Acquisition);
    assert_ne!(
        first.provider_acquisition().unwrap().operation_id(),
        second.provider_acquisition().unwrap().operation_id()
    );
    assert_eq!(
        server.requests().len(),
        2,
        "abandoned inner work does not synthesize cooldown"
    );
}
