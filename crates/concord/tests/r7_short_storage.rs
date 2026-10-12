#![cfg(all(feature = "json", feature = "default-tls"))]

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::http::SafeProxy;
use concord::{Binary, Bytes, Json, SecretString, api};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;
use serde::Deserialize;
use std::time::{Duration, Instant};

pub struct StoredToken {
    secret: SecretString,
    expires: Instant,
}
impl CredentialMaterial for StoredToken {
    fn expires_at(&self) -> Option<Instant> {
        Some(self.expires)
    }
}
impl SecretCredential for StoredToken {
    fn expose_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}
impl<'de> Deserialize<'de> for StoredToken {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self {
            secret: SecretString::try_new(value).map_err(serde::de::Error::custom)?,
            expires: Instant::now() + Duration::from_secs(30),
        })
    }
}

api! {
    client R7Stored {
        base "https://localhost/";
        credentials { credential session = endpoint login; }
        POST login() -> StoredToken via Json;
        GET protected() -> Bytes via Binary { auth { bearer session; } }
    }
}

#[tokio::test]
async fn r7_public_endpoint_storage_of_a_30_second_token_executes_a_protected_request() {
    let server = TlsServer::start().unwrap();
    server
        .queue_response_with_headers(
            200,
            &[("Content-Type", "application/json")],
            br#""stored-short-token""#,
        )
        .unwrap();
    server.queue_response(200, b"protected").unwrap();
    let relay = ConnectProxy::start(server.address()).unwrap();
    let proxy = SafeProxy::parse(relay.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let api = R7Stored::builder()
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(&certificate)
        })
        .build()
        .unwrap();
    assert!(api.login().store_as_session().await.unwrap().is_installed());
    assert_eq!(
        api.protected().await.unwrap(),
        Bytes::from_static(b"protected")
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        server.requests()[1].header("authorization"),
        Some("Bearer stored-short-token")
    );
}
