use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::api;
use concord::auth::{
    AccessToken, AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    SecretCredential,
};
use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::error::{ClientBuildErrorKind, ClientInputKind, ErrorCategory};
use concord::http::SafeProxy;
use concord::{Bytes, ClientBuildError, SecretString};
use concord_test_support::server::{ResponseScript, ScriptedServer};

#[derive(Clone)]
pub struct SessionToken(SecretString);

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
        let value =
            String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("safe session decode"))?;
        Ok(SessionToken(SecretString::new(value)))
    }
}

pub struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken(SecretString::new("provider-token"))) })
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
        Box::pin(async { Ok(SessionToken(SecretString::new("selected-token"))) })
    }
}

pub struct BearerProvider;

impl CredentialProvider for BearerProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(AccessToken::new("provider-bearer-token")) })
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
    assert_eq!(error.missing_field(), Some(field));
    assert_eq!(error.missing_input_kind(), Some(input_kind));
}

#[test]
fn every_required_client_input_has_focused_safe_builder_metadata() {
    assert_missing_input(AuthApi::builder().build(), "tenant", ClientInputKind::Value);
    assert_missing_input(
        AuthApi::builder().tenant("tenant".to_owned()).build(),
        "api_secret",
        ClientInputKind::Secret,
    );
    let protected_error = AuthApi::builder()
        .tenant("tenant".to_owned())
        .api_secret(SecretString::new("first-secret-sentinel-9aa1"))
        .build()
        .err()
        .expect("bearer secret is absent");
    assert_eq!(protected_error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(protected_error.missing_field(), Some("bearer_secret"));
    assert_eq!(
        protected_error.missing_input_kind(),
        Some(ClientInputKind::Secret)
    );
    for rendered in [protected_error.to_string(), format!("{protected_error:?}")] {
        assert!(!rendered.contains("first-secret-sentinel-9aa1"));
    }
    assert_missing_input(
        AuthApi::builder()
            .tenant("tenant".to_owned())
            .api_secret(SecretString::new("api"))
            .bearer_secret(SecretString::new("bearer"))
            .build(),
        "username",
        ClientInputKind::Secret,
    );
    assert_missing_input(
        AuthApi::builder()
            .tenant("tenant".to_owned())
            .api_secret(SecretString::new("api"))
            .bearer_secret(SecretString::new("bearer"))
            .username(SecretString::new("user"))
            .build(),
        "password",
        ClientInputKind::Secret,
    );
    assert_missing_input(
        AuthApi::builder()
            .tenant("tenant".to_owned())
            .api_secret(SecretString::new("api"))
            .bearer_secret(SecretString::new("bearer"))
            .username(SecretString::new("user"))
            .password(SecretString::new("password"))
            .build(),
        "rotating_bearer_provider",
        ClientInputKind::CredentialProvider,
    );
    let error = AuthApi::builder()
        .tenant("tenant".to_owned())
        .api_secret(SecretString::new("api"))
        .bearer_secret(SecretString::new("bearer"))
        .username(SecretString::new("user"))
        .password(SecretString::new("last-secret-sentinel-4b71"))
        .rotating_bearer_provider(BearerProvider)
        .build()
        .err()
        .expect("session provider is absent");
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(error.missing_field(), Some("session_provider"));
    assert_eq!(
        error.missing_input_kind(),
        Some(ClientInputKind::CredentialProvider)
    );
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains("last-secret-sentinel-4b71"));
    }
}

fn auth_builder() -> AuthApiBuilder {
    AuthApi::builder()
        .tenant("tenant".to_owned())
        .api_secret(SecretString::new("api-secret"))
        .bearer_secret(SecretString::new("bearer-secret"))
        .username(SecretString::new("user"))
        .password(SecretString::new("password"))
        .rotating_bearer_provider(BearerProvider)
        .session_provider(SessionProvider)
}

#[test]
fn repeated_provider_setters_replace_in_call_order() {
    let replaced_drops = Arc::new(AtomicUsize::new(0));
    let selected_drops = Arc::new(AtomicUsize::new(0));
    let builder = AuthApi::builder()
        .tenant("tenant".to_owned())
        .api_secret(SecretString::new("api-secret"))
        .bearer_secret(SecretString::new("bearer-secret"))
        .username(SecretString::new("user"))
        .password(SecretString::new("password"))
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
        .set(SessionToken(SecretString::new("endpoint-token")))
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
        .set(SessionToken(SecretString::new("clone-only")))
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
        .set(SessionToken(SecretString::new("preserved-existing")))
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
