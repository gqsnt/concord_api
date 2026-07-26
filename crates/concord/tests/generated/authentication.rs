use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::auth::{
    AccessToken, AuthError, AuthErrorKind, BoxFuture, CredentialMaterial, CredentialProvider,
    CredentialRequest, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, SecretCredential,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::error::{ClientBuildErrorKind, ClientInputKind, ErrorCategory};
use concord::http::SafeProxy;
use concord::{Bytes, ClientBuildError, SecretString};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::server::{ResponseScript, ScriptedServer};
use concord_test_support::tls::TlsServer;

pub struct SessionToken(SecretString);

fn secret(value: impl AsRef<str>) -> SecretString {
    SecretString::try_new(value).expect("bounded test protected string")
}

impl CredentialMaterial for SessionToken {}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken(<redacted>)")
    }
}

pub struct SessionCodec;

impl DecodeBody for SessionCodec {
    type Output = SessionToken;

    const ACCEPT: Option<&'static str> = Some("application/session");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let value = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(SessionToken(secret(value)))
    }
}

pub struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken(secret("provider-token"))) })
    }
}

pub struct DroppingSessionProvider {
    drops: Arc<AtomicUsize>,
}

impl Drop for DroppingSessionProvider {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl CredentialProvider for DroppingSessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken(secret("selected-token"))) })
    }
}

pub struct BearerProvider;

impl CredentialProvider for BearerProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async {
            Ok(AccessToken::try_new("provider-bearer-token")
                .expect("bounded provider access token"))
        })
    }
}

api! {
    client AuthApi {
        base "http://authentication.example.test/";
        vars {
            tenant: String;
        }
        credentials {
            secret api_secret: String;
            secret bearer_secret: String;
            secret username: String;
            secret password: String;
            credential application_key = api_key(secret.api_secret);
            credential static_bearer = bearer(secret.bearer_secret);
            credential login = basic(secret.username, secret.password);
            credential rotating_bearer = custom crate::BearerProvider;
            credential acquired = endpoint login;
            credential session = custom crate::SessionProvider;
        }
    }

    GET login() -> crate::SessionCodec;
    GET api_key_endpoint() -> Bytes {
        auth header "X-Api-Key" = application_key;
    }
    GET bearer_endpoint() -> Bytes {
        auth bearer static_bearer;
    }
    GET basic_endpoint() -> Bytes {
        auth basic login;
    }
    GET provider_bearer_endpoint() -> Bytes {
        auth bearer rotating_bearer challenge unauthorized;
    }
    scope nested() {
        auth header "X-Session" = session challenge never;
        GET inherited() -> Bytes;
        GET replaced() -> Bytes {
            auth replace query "token" = acquired challenge never;
        }
        GET public() -> Bytes {
            auth off;
        }
    }
}

api! {
    client SharedSecretApi {
        base "http://shared-secret.example.test/";
        credentials {
            secret shared: String;
            credential key = api_key(secret.shared);
            credential token = bearer(secret.shared);
        }
    }

    GET keyed() -> Bytes {
        auth header "X-Api-Key" = key;
    }
    GET bearer() -> Bytes {
        auth bearer token;
    }
}

api! {
    client CredentialProofApi {
        base "https://localhost/";
        credentials {
            secret api_secret: String;
            credential application_key = api_key(secret.api_secret);
            credential session = custom crate::SessionProvider;
        }
    }

    GET api_key_endpoint() -> Bytes {
        auth header "X-Api-Key" = application_key;
    }
    GET session_endpoint() -> Bytes {
        auth header "X-Session" = session challenge never;
    }
}

#[test]
fn generated_authentication_surface_has_ordered_inputs_and_source_specific_handles() {
    let client = auth_builder().build().expect("generated auth client");
    let credentials = client.credentials();
    let _ = credentials.application_key().credential();
    let _ = credentials.static_bearer().credential();
    let _ = credentials.login().credential();
    let _ = credentials.rotating_bearer().credential();
    let _ = credentials.acquired().credential();
    let _ = credentials.session().credential();
    assert!(!credentials.acquired().is_set());
    assert!(!credentials.session().is_set());

    let _pending = client.login().store_as_acquired();
    let _ = client.nested().inherited();
    let _ = client.nested().replaced();
    let _ = client.nested().public();
}

fn assert_missing_input(
    result: Result<AuthApi, ClientBuildError>,
    field: &'static str,
    input_kind: ClientInputKind,
) {
    let error = result.err().expect("required client input is absent");
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(error.input_field(), Some(field));
    assert_eq!(error.input_kind(), Some(input_kind));
}

#[test]
fn every_required_client_input_has_focused_safe_builder_metadata() {
    assert_missing_input(AuthApi::builder().build(), "tenant", ClientInputKind::Value);
    assert_missing_input(
        AuthApi::builder().set_tenant("tenant".to_owned()).build(),
        "api_secret",
        ClientInputKind::Secret,
    );
    let protected_error = AuthApi::builder()
        .set_tenant("tenant".to_owned())
        .api_secret(secret("first-secret-sentinel-9aa1"))
        .build()
        .err()
        .expect("bearer secret is absent");
    assert_eq!(protected_error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(protected_error.input_field(), Some("bearer_secret"));
    assert_eq!(protected_error.input_kind(), Some(ClientInputKind::Secret));
    for rendered in [protected_error.to_string(), format!("{protected_error:?}")] {
        assert!(!rendered.contains("first-secret-sentinel-9aa1"));
    }
    assert_missing_input(
        AuthApi::builder()
            .set_tenant("tenant".to_owned())
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer"))
            .build(),
        "username",
        ClientInputKind::Secret,
    );
    assert_missing_input(
        AuthApi::builder()
            .set_tenant("tenant".to_owned())
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer"))
            .username(secret("user"))
            .build(),
        "password",
        ClientInputKind::Secret,
    );
    assert_missing_input(
        AuthApi::builder()
            .set_tenant("tenant".to_owned())
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer"))
            .username(secret("user"))
            .password(secret("password"))
            .build(),
        "rotating_bearer_provider",
        ClientInputKind::CredentialProvider,
    );
    let error = AuthApi::builder()
        .set_tenant("tenant".to_owned())
        .api_secret(secret("api"))
        .bearer_secret(secret("bearer"))
        .username(secret("user"))
        .password(secret("last-secret-sentinel-4b71"))
        .rotating_bearer_provider(BearerProvider)
        .build()
        .err()
        .expect("session provider is absent");
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(error.input_field(), Some("session_provider"));
    assert_eq!(
        error.input_kind(),
        Some(ClientInputKind::CredentialProvider)
    );
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains("last-secret-sentinel-4b71"));
    }
}

fn assert_invalid_credential(
    result: Result<AuthApi, ClientBuildError>,
    field: &'static str,
    sentinel: &str,
) {
    let error = result.err().expect("credential input must be rejected");
    assert_eq!(error.kind(), ClientBuildErrorKind::InvalidCredential);
    assert_eq!(error.input_field(), Some(field));
    assert_eq!(error.input_kind(), Some(ClientInputKind::Secret));
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains(sentinel));
    }
}

#[test]
fn static_built_in_credentials_validate_exact_limits_during_client_build() {
    auth_builder()
        .api_secret(secret("a".repeat(MAX_CREDENTIAL_SECRET_BYTES)))
        .build()
        .expect("exact API-key material limit");
    auth_builder()
        .bearer_secret(secret("b".repeat(MAX_CREDENTIAL_SECRET_BYTES)))
        .build()
        .expect("exact access-token material limit");
    auth_builder()
        .username(secret("u".repeat(MAX_BASIC_USERNAME_BYTES)))
        .password(secret("p".repeat(MAX_BASIC_PASSWORD_BYTES)))
        .build()
        .expect("exact Basic component limits");
}

#[test]
fn static_built_in_credential_first_excess_fails_build_with_safe_field_metadata() {
    const FIRST_EXCESS_SENTINEL: &str = "\u{7f}";

    assert_invalid_credential(
        auth_builder()
            .api_secret(secret(format!(
                "{}{FIRST_EXCESS_SENTINEL}",
                "a".repeat(MAX_CREDENTIAL_SECRET_BYTES)
            )))
            .build(),
        "api_secret",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .bearer_secret(secret(format!(
                "{}{FIRST_EXCESS_SENTINEL}",
                "b".repeat(MAX_CREDENTIAL_SECRET_BYTES)
            )))
            .build(),
        "bearer_secret",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .username(secret(format!(
                "{}{FIRST_EXCESS_SENTINEL}",
                "u".repeat(MAX_BASIC_USERNAME_BYTES)
            )))
            .build(),
        "username",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .password(secret(format!(
                "{}{FIRST_EXCESS_SENTINEL}",
                "p".repeat(MAX_BASIC_PASSWORD_BYTES)
            )))
            .build(),
        "password",
        FIRST_EXCESS_SENTINEL,
    );
}

#[test]
fn one_declared_secret_can_feed_multiple_credentials_through_shared_clones() {
    SharedSecretApi::builder()
        .shared(secret("one-shared-protected-value"))
        .build()
        .expect("one bounded secret can feed multiple built-in sources");
}

#[tokio::test]
async fn non_clone_custom_material_compiles_and_executes_through_generated_storage() {
    let server = TlsServer::start().expect("non-Clone material TLS server");
    server
        .queue_response(200, b"non-clone".to_vec())
        .expect("non-Clone material response");
    let tunnel = ConnectProxy::start(server.address()).expect("non-Clone CONNECT tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("safe CONNECT proxy");
    let certificate = server.certificate_pem();
    let client = credential_proof_builder()
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .build()
        .expect("non-Clone material client");

    client
        .session_endpoint()
        .await
        .expect("non-Clone provider material request");

    server
        .wait_for_requests(1, Duration::from_secs(10))
        .expect("non-Clone material request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("x-session"), Some("provider-token"));
}

fn auth_builder() -> AuthApiBuilder {
    AuthApi::builder()
        .set_tenant("tenant".to_owned())
        .api_secret(secret("api-secret"))
        .bearer_secret(secret("bearer-secret"))
        .username(secret("user"))
        .password(secret("password"))
        .rotating_bearer_provider(BearerProvider)
        .session_provider(SessionProvider)
}

fn credential_proof_builder() -> CredentialProofApiBuilder {
    CredentialProofApi::builder()
        .api_secret(secret("proof-api-key"))
        .session_provider(SessionProvider)
}

#[test]
fn repeated_provider_setters_replace_in_call_order() {
    let replaced_drops = Arc::new(AtomicUsize::new(0));
    let selected_drops = Arc::new(AtomicUsize::new(0));
    let builder = AuthApi::builder()
        .set_tenant("tenant".to_owned())
        .api_secret(secret("api-secret"))
        .bearer_secret(secret("bearer-secret"))
        .username(secret("user"))
        .password(secret("password"))
        .rotating_bearer_provider(BearerProvider)
        .session_provider(DroppingSessionProvider {
            drops: Arc::clone(&replaced_drops),
        })
        .session_provider(DroppingSessionProvider {
            drops: Arc::clone(&selected_drops),
        });
    assert_eq!(replaced_drops.load(Ordering::SeqCst), 1);
    assert_eq!(selected_drops.load(Ordering::SeqCst), 0);

    let client = builder.build().expect("provider replacement client");
    assert_eq!(selected_drops.load(Ordering::SeqCst), 0);
    drop(client);
    assert_eq!(selected_drops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn generated_handles_share_clone_state_keep_independent_builds_and_replace_atomically() {
    let client = auth_builder().build().expect("first client");
    let clone = client.clone();
    clone
        .credentials()
        .acquired()
        .set(SessionToken(secret("endpoint-token")))
        .await
        .expect("endpoint set");
    assert!(client.credentials().acquired().is_set());
    client
        .credentials()
        .acquired()
        .clear()
        .await
        .expect("endpoint clear");
    assert!(!clone.credentials().acquired().is_set());

    let independent = auth_builder().build().expect("independent client");
    clone
        .credentials()
        .acquired()
        .set(SessionToken(secret("clone-only")))
        .await
        .expect("clone set");
    assert!(client.credentials().acquired().is_set());
    assert!(!independent.credentials().acquired().is_set());

    client
        .credentials()
        .application_key()
        .replace("replacement-api-key")
        .await
        .expect("complete API-key replacement");
    client
        .credentials()
        .static_bearer()
        .replace("replacement-bearer")
        .await
        .expect("complete bearer replacement");
    client
        .credentials()
        .login()
        .replace("replacement-user", "replacement-password")
        .await
        .expect("atomic Basic replacement");
    client
        .credentials()
        .rotating_bearer()
        .replace_provider(BearerProvider)
        .await
        .expect("generic bearer provider replacement");
    client
        .credentials()
        .session()
        .clear()
        .await
        .expect("custom provider clear");
    assert!(!client.credentials().session().is_set());
}

#[tokio::test]
async fn credential_replace_exact_limit_succeeds() {
    let client = credential_proof_builder()
        .build()
        .expect("static credential client");
    client
        .credentials()
        .application_key()
        .replace("x".repeat(MAX_CREDENTIAL_SECRET_BYTES))
        .await
        .expect("exact API-key replacement limit");
}

#[tokio::test]
async fn failed_static_replacement_preserves_previous_material_and_returns_safe_error() {
    const INITIAL_KEY: &str = "preserved-static-api-key";
    const FIRST_EXCESS_SENTINEL: &str = "\u{7f}";

    let server = TlsServer::start().expect("replacement preservation TLS server");
    server
        .queue_response(200, b"before".to_vec())
        .expect("before replacement response");
    server
        .queue_response(200, b"after".to_vec())
        .expect("after replacement response");
    let tunnel = ConnectProxy::start(server.address()).expect("replacement CONNECT tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("safe CONNECT proxy");
    let certificate = server.certificate_pem();
    let client = credential_proof_builder()
        .api_secret(secret(INITIAL_KEY))
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .build()
        .expect("proxied authentication client");

    client
        .api_key_endpoint()
        .await
        .expect("request before failed replacement");
    let excessive = format!(
        "{}{FIRST_EXCESS_SENTINEL}",
        "x".repeat(MAX_CREDENTIAL_SECRET_BYTES)
    );
    let error = client
        .credentials()
        .application_key()
        .replace(&excessive)
        .await
        .expect_err("first excess replacement");
    assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);
    assert!(!error.to_string().contains(FIRST_EXCESS_SENTINEL));
    assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));
    client
        .api_key_endpoint()
        .await
        .expect("request after failed replacement");

    server
        .wait_for_requests(2, Duration::from_secs(10))
        .expect("replacement preservation requests");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-api-key"), Some(INITIAL_KEY));
    assert_eq!(requests[1].header("x-api-key"), Some(INITIAL_KEY));
}

#[tokio::test]
async fn endpoint_backed_acquisition_uses_normal_endpoint_execution_and_preserves_on_failure() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"acquired-session".to_vec())
            .with_header("Content-Type", "application/session"),
        ResponseScript::status(500, b"terminal".to_vec()),
    ])
    .expect("endpoint credential server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = auth_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("proxied auth client");

    client
        .login()
        .store_as_acquired()
        .await
        .expect("normal endpoint acquisition");
    assert!(client.credentials().acquired().is_set());

    client
        .credentials()
        .acquired()
        .set(SessionToken(secret("preserved-existing")))
        .await
        .expect("existing endpoint material");
    let error = client
        .login()
        .store_as_acquired()
        .await
        .expect_err("terminal endpoint failure");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(client.credentials().acquired().is_set());
}
