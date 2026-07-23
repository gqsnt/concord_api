use std::fmt;

use concord_core::advanced::{
    AuthError, CredentialMaterial, CredentialProvider, CredentialRequest, DecodeBody,
    DecodeContext, ErrorCategory, SafeProxy, SecretCredential,
};
use concord_core::prelude::{AccessToken, Bytes, SecretString};
use concord_macros::api;
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

    fn decode(
        body: Bytes,
        _context: DecodeContext<'_>,
    ) -> Result<Self::Output, concord_core::advanced::CodecError> {
        let value = String::from_utf8(body.to_vec())
            .map_err(|_| concord_core::advanced::CodecError::new("safe session decode"))?;
        Ok(SessionToken(SecretString::new(value)))
    }
}

pub struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> concord_core::advanced::BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken(SecretString::new("provider-token"))) })
    }
}

pub struct BearerProvider;

impl CredentialProvider for BearerProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> concord_core::advanced::BoxFuture<'a, Result<Self::Material, AuthError>> {
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
            credential acquired = endpoint Login;
            credential session = custom crate::SessionProvider;
        }
    }

    GET Login() -> crate::SessionCodec;
    GET ApiKeyEndpoint() -> Bytes {
        auth header "X-Api-Key" = application_key;
    }
    GET BearerEndpoint() -> Bytes {
        auth bearer static_bearer;
    }
    GET BasicEndpoint() -> Bytes {
        auth basic login;
    }
    GET ProviderBearerEndpoint() -> Bytes {
        auth bearer rotating_bearer challenge unauthorized;
    }
    scope Nested() {
        auth header "X-Session" = session challenge never;
        GET Inherited() -> Bytes;
        GET Replaced() -> Bytes {
            auth replace query "token" = acquired challenge never;
        }
        GET Public() -> Bytes {
            auth off;
        }
    }
}

#[test]
fn generated_authentication_surface_has_ordered_inputs_and_source_specific_handles() {
    use auth_api::prelude::*;

    let client = AuthApi::new(
        "tenant".to_owned(),
        "api-secret",
        "bearer-secret",
        "user",
        "password",
        BearerProvider,
        SessionProvider,
    )
    .expect("generated auth client");
    let credentials = client.credentials();
    let _ = credentials.application_key().credential();
    let _ = credentials.static_bearer().credential();
    let _ = credentials.login().credential();
    let _ = credentials.rotating_bearer().credential();
    let _ = credentials.acquired().credential();
    let _ = credentials.session().credential();
    assert!(!credentials.acquired().is_set());
    assert!(!credentials.session().is_set());

    let _pending = client.login().acquire_as_acquired();
    let _ = client.nested().inherited();
    let _ = client.nested().replaced();
    let _ = client.nested().public();
}

fn auth_builder() -> AuthApiBuilder {
    AuthApi::builder()
        .tenant("tenant".to_owned())
        .api_secret("api-secret")
        .bearer_secret("bearer-secret")
        .username("user")
        .password("password")
        .rotating_bearer_provider(BearerProvider)
        .session_provider(SessionProvider)
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
    use auth_api::prelude::*;

    let server = ScriptedServer::start([
        ResponseScript::ok(b"acquired-session".to_vec())
            .with_header("Content-Type", "application/session"),
        ResponseScript::status(500, b"terminal".to_vec()),
    ])
    .expect("endpoint credential server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = auth_builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("proxied auth client");

    client
        .login()
        .acquire_as_acquired()
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
        .acquire_as_acquired()
        .await
        .expect_err("terminal endpoint failure");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(client.credentials().acquired().is_set());
}
