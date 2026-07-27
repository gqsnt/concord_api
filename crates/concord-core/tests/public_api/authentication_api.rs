use std::error::Error as _;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord_core::contracts::{
    AccessToken, ApiKey, Auth, AuthError, AuthErrorKind, BasicCredential, BoxFuture,
    ChallengePolicy, CredentialHandle, CredentialMaterial, CredentialProvider, CredentialReason,
    CredentialRef, CredentialRequest, HeaderName, HeaderValue, Method, ProviderBody,
    ProviderBodyFactory, ProviderHttp, ProviderRequest, ProviderResponse, RecoveryMode,
    SecretCredential, SecretString, StatusCode, Url,
};

const SENTINEL: &str = "authentication-public-api-sentinel-f629cb6a8fe749ea";

struct SessionToken(SecretString);

impl CredentialMaterial for SessionToken {}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://provider.example.test/token").expect("provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/octet-stream"),
            )
            .body(ProviderBody::Bytes("request".into()))
            .timeout(Duration::from_secs(2))
            .response_limit(NonZeroUsize::new(4_096).expect("positive response limit"));
            let _response = context.http.send(request).await?;
            Ok(SessionToken(
                SecretString::try_new("provider-result").expect("bounded provider result"),
            ))
        })
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}

struct Factory;

impl ProviderBodyFactory for Factory {
    fn create(&self) -> Result<concord_core::contracts::Bytes, AuthError> {
        Ok("factory".into())
    }
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}
fn assert_clone<T: Clone>() {}
fn assert_copy<T: Copy>() {}

#[allow(dead_code)]
fn exact_handle_and_auth_signatures(
    handle: &CredentialHandle<SessionToken>,
    secret: CredentialRef<SessionToken>,
    basic: CredentialRef<BasicCredential>,
) {
    let _: CredentialRef<SessionToken> = handle.credential();
    let _: bool = handle.is_set();
    let _: Auth = Auth::bearer(secret.clone());
    let _: Auth = Auth::authorization(secret.clone());
    let _: Auth = Auth::authorization_with_scheme("Token", secret.clone())
        .expect("valid static Authorization scheme");
    let _: Auth = Auth::header(HeaderName::from_static("x-session"), secret.clone())
        .expect("valid header placement");
    let _: Auth = Auth::query("session", secret)
        .expect("valid query placement")
        .challenge(ChallengePolicy::UnauthorizedOrForbidden);
    let _: Auth = Auth::basic(basic).challenge(ChallengePolicy::Unauthorized);
    let _: ChallengePolicy = ChallengePolicy::Never;

    async fn mutate(handle: &CredentialHandle<SessionToken>) -> Result<(), AuthError> {
        handle
            .set(SessionToken(
                SecretString::try_new("replacement").expect("bounded replacement"),
            ))
            .await?;
        handle.clear().await
    }

    let _future = mutate(handle);
}

#[allow(dead_code)]
async fn exact_provider_response_surface(http: ProviderHttp<'_>) -> Result<(), AuthError> {
    let response: ProviderResponse = http
        .send(
            ProviderRequest::new(
                Method::GET,
                Url::parse("https://provider.example.test/status").expect("provider URL is valid"),
            )
            .body(ProviderBody::Factory(Arc::new(Factory))),
        )
        .await?;
    let _: StatusCode = response.status();
    let _: &concord_core::contracts::HeaderMap = response.headers();
    let _: &concord_core::contracts::Bytes = response.body();
    let _: concord_core::contracts::Bytes = response.into_body();
    Ok(())
}

#[test]
fn authentication_contracts_are_available_only_through_approved_namespaces() {
    assert_clone::<SecretString>();
    assert_clone::<AccessToken>();
    assert_clone::<ApiKey>();
    assert_clone::<BasicCredential>();
    assert_clone::<CredentialRef<SessionToken>>();
    assert_clone::<CredentialHandle<SessionToken>>();
    assert_copy::<ProviderHttp<'static>>();
    assert_send_sync_static::<SessionProvider>();
    assert_send_sync_static::<SessionToken>();

    let _: CredentialReason = CredentialReason::Missing;
    let _: CredentialReason = CredentialReason::Expired;
    let _: CredentialReason = CredentialReason::ExpiringSoon;
    let _: CredentialReason = CredentialReason::Rejected;
    let _: RecoveryMode = RecoveryMode::None;
    let _: RecoveryMode = RecoveryMode::InvalidateOnly;
    let _: RecoveryMode = RecoveryMode::Refresh;
}

#[test]
fn built_in_materials_and_errors_are_explicit_and_secret_safe() {
    let secret = SecretString::try_new(SENTINEL).expect("bounded protected string");
    assert_eq!(secret.expose_secret().len(), SENTINEL.len());
    let expiration = Instant::now() + Duration::from_secs(300);
    let token = AccessToken::try_from_secret_with_expiration(secret.clone(), expiration)
        .expect("bounded expiring access token");
    let plain_token = AccessToken::try_new(SENTINEL).expect("bounded access token");
    let key = ApiKey::try_new(SENTINEL).expect("bounded API key");
    let basic = BasicCredential::try_new(SENTINEL, SENTINEL).expect("bounded Basic credential");
    assert_eq!(token.expires_at(), Some(expiration));
    assert_eq!(plain_token.expires_at(), None);

    for diagnostic in [
        secret.to_string(),
        format!("{secret:?}"),
        token.to_string(),
        format!("{token:?}"),
        plain_token.to_string(),
        format!("{plain_token:?}"),
        key.to_string(),
        format!("{key:?}"),
        basic.to_string(),
        format!("{basic:?}"),
    ] {
        assert!(!diagnostic.contains(SENTINEL));
    }

    let error = AuthError::with_safe_message(AuthErrorKind::Acquisition, "provider unavailable")
        .with_retry_after(Duration::from_secs(7));
    assert_eq!(error.kind(), AuthErrorKind::Acquisition);
    assert_eq!(error.retry_after(), Some(Duration::from_secs(7)));
    assert!(error.source().is_none());
    assert!(!error.to_string().contains(SENTINEL));
    assert!(!format!("{error:?}").contains(SENTINEL));
}
