#![cfg(all(feature = "oauth2-client", feature = "default-tls"))]

use concord::auth::OAuth2ClientCredentials;
use concord::http::SafeProxy;
use concord::{Binary, Bytes, api};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;
use std::time::Duration;

api! {
    client R7OAuth {
        base "https://localhost/";
        credentials { credential oauth = custom OAuth2ClientCredentials; }
        auth { bearer oauth; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

#[tokio::test]
async fn r7_oauth_rejection_delay_prevents_sequential_acquisitions_and_allows_recovery() {
    for status in [429, 503] {
        let server = TlsServer::start().unwrap();
        server
            .queue_response_with_headers(status, &[("Retry-After", "1")], b"private-provider-body")
            .unwrap();
        let proxy = ConnectProxy::start(server.address()).unwrap();
        let proxy_url = SafeProxy::parse(proxy.url()).unwrap();
        let certificate = server.certificate_pem().to_owned();
        let provider = OAuth2ClientCredentials::builder("https://localhost/token")
            .client_id("private-client-id")
            .client_secret("private-client-secret")
            .build()
            .unwrap();
        let client = R7OAuth::builder()
            .oauth(provider)
            .configure_http(move |http| {
                http.http1_only()
                    .proxy(proxy_url)
                    .add_root_certificate_pem(&certificate)
            })
            .build()
            .unwrap();
        let first = client.protected().await.unwrap_err();
        assert_eq!(
            first.category(),
            concord::error::ErrorCategory::AuthenticationProvider
        );
        let second = client.protected().await.unwrap_err();
        assert_eq!(
            second.category(),
            concord::error::ErrorCategory::AuthenticationProvider
        );
        assert_eq!(
            server.requests().len(),
            1,
            "server delay must prevent a second helper exchange"
        );
        for error in [&first, &second] {
            let formatted = format!("{error} {error:?}");
            for sentinel in [
                "private-provider-body",
                "private-client-id",
                "private-client-secret",
            ] {
                assert!(!formatted.contains(sentinel));
            }
        }
        tokio::time::sleep(Duration::from_millis(1100)).await;
        server
            .queue_response(
                200,
                br#"{"access_token":"recovered-token","token_type":"bearer","expires_in":30}"#,
            )
            .unwrap();
        server.queue_response(200, b"recovered").unwrap();
        assert_eq!(
            client.protected().await.unwrap(),
            Bytes::from_static(b"recovered")
        );
        assert_eq!(
            server.requests().len(),
            3,
            "one recovery exchange and one application handoff"
        );
    }
}
