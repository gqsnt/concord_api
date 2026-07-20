use std::time::{Duration, Instant};

use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderValue, Method};
use url::Url;

use crate::error::{AuthErrorKind, ClientBuildError, ClientBuildErrorKind};
use crate::hooks::BoxFuture;
use crate::http::provider::{ProviderBody, ProviderRequest};

use super::{
    AccessToken, AuthError, CredentialProvider, CredentialRequest, RecoveryMode, SecretString,
};

pub(crate) struct OAuth2ClientCredentials {
    token_url: Url,
    client_id: SecretString,
    client_secret: SecretString,
    scope: Option<Box<str>>,
}

impl OAuth2ClientCredentials {
    pub(crate) fn new(
        token_url: &str,
        client_id: SecretString,
        client_secret: SecretString,
        scope: Option<&str>,
    ) -> Result<Self, ClientBuildError> {
        let token_url = Url::parse(token_url)
            .map_err(|_| ClientBuildError::new(ClientBuildErrorKind::Internal))?;
        if token_url.scheme() != "https"
            || token_url.host().is_none()
            || !token_url.username().is_empty()
            || token_url.password().is_some()
            || token_url.fragment().is_some()
        {
            return Err(ClientBuildError::new(ClientBuildErrorKind::Internal));
        }
        Ok(Self {
            token_url,
            client_id,
            client_secret,
            scope: scope.map(Into::into),
        })
    }

    async fn request(&self, context: CredentialRequest<'_>) -> Result<AccessToken, AuthError> {
        let username = form_component(self.client_id.expose_secret());
        let password = form_component(self.client_secret.expose_secret());
        let authorization = format!(
            "Basic {}",
            encode_base64(format!("{username}:{password}").as_bytes())
        );
        let authorization = HeaderValue::from_str(&authorization)
            .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?;
        let body = Bytes::from({
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("grant_type", "client_credentials");
            if let Some(scope) = &self.scope {
                form.append_pair("scope", scope);
            }
            form.finish()
        });
        let request = ProviderRequest::new(Method::POST, self.token_url.clone())
            .header(AUTHORIZATION, authorization)
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(body));
        let response = context.http.send(request).await?;
        if !response.status().is_success() {
            return Err(AuthError::new(AuthErrorKind::Acquisition));
        }
        let value: serde_json::Value =
            serde_json::from_slice(response.body()).map_err(|error| {
                drop(error);
                AuthError::new(AuthErrorKind::Acquisition)
            })?;
        let object = value
            .as_object()
            .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
        let token = object
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
        if object.get("token_type").is_some_and(|value| {
            value
                .as_str()
                .is_none_or(|token_type| !token_type.eq_ignore_ascii_case("bearer"))
        }) {
            return Err(AuthError::new(AuthErrorKind::Acquisition));
        }
        let expiration = object
            .get("expires_in")
            .map(|value| {
                let seconds = value
                    .as_u64()
                    .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
                Instant::now()
                    .checked_add(Duration::from_secs(seconds))
                    .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))
            })
            .transpose()?;
        Ok(match expiration {
            Some(expires_at) => AccessToken::with_expiration(token, expires_at),
            None => AccessToken::new(token),
        })
    }
}

impl CredentialProvider for OAuth2ClientCredentials {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(self.request(context))
    }

    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(self.request(context))
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}

fn form_component(value: &str) -> String {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("", value)
        .finish();
    encoded
        .strip_prefix('=')
        .expect("form serializer always emits the empty key separator")
        .to_owned()
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(TABLE[usize::from(first >> 2)]));
        output.push(char::from(
            TABLE[usize::from(((first & 3) << 4) | (second >> 4))],
        ));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from(((second & 15) << 2) | (third >> 6))])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(third & 63)])
        } else {
            '='
        });
    }
    output
}

#[cfg(all(test, feature = "default-tls"))]
mod tests {
    use std::error::Error;
    use std::num::NonZeroUsize;

    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::script::{CapturedHeaderValue, ExpectedRequest, ScriptedResponse};

    use crate::client::ClientCore;
    use crate::http::HttpConfigBuilder;
    use crate::runtime::RuntimeConfigBuilder;
    use http::StatusCode;

    use super::*;
    use crate::auth::{CredentialMaterial, CredentialReason, SecretCredential};

    const CLIENT_ID: &str = "oauth-client-id-sentinel-4db370ebb046";
    const CLIENT_SECRET: &str = "oauth-client-secret-sentinel-9f8b14c2e6a1";

    fn core(executor: &DeterministicExecutor, response_limit: usize) -> ClientCore {
        ClientCore::build_with_development(
            "https://application.example.test/",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new().provider_response_limit(
                NonZeroUsize::new(response_limit).expect("provider response limit"),
            ),
            executor,
        )
        .expect("test core")
    }

    fn provider(scope: Option<&str>) -> OAuth2ClientCredentials {
        OAuth2ClientCredentials::new(
            "https://auth.example.test/oauth/token",
            SecretString::new(CLIENT_ID),
            SecretString::new(CLIENT_SECRET),
            scope,
        )
        .expect("OAuth provider")
    }

    fn request(core: &ClientCore, reason: CredentialReason) -> CredentialRequest<'_> {
        CredentialRequest {
            http: core.provider_http(None),
            reason,
        }
    }

    fn auth_error<T>(result: Result<T, AuthError>) -> AuthError {
        match result {
            Ok(_) => panic!("expected OAuth authentication failure"),
            Err(error) => error,
        }
    }

    #[tokio::test]
    async fn oauth_success_uses_exact_isolated_post_basic_form_and_scope() {
        const TOKEN: &str = "oauth-result-token-41e79c";
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .native_url("https://auth.example.test/oauth/token")
                    .header(
                        AUTHORIZATION,
                        HeaderValue::from_static(
                            "Basic cGhhc2U2LW9hdXRoLWNsaWVudC1pZC00ZGIzNzBlYmIwNDY6cGhhc2U2LW9hdXRoLWNsaWVudC1zZWNyZXQtOWY4YjE0YzJlNmEx",
                        ),
                    )
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/x-www-form-urlencoded"),
                    )
                    .body(Bytes::from_static(
                        b"grant_type=client_credentials&scope=users.read+offline",
                    )),
                ScriptedResponse::success(Bytes::from_static(
                    br#"{"access_token":"oauth-result-token-41e79c","token_type":"BeArEr","expires_in":3600}"#,
                )),
            )
            .expect("OAuth exchange");
        let core = core(&executor, 4_096);
        let provider = provider(Some("users.read offline"));
        let before = Instant::now();
        let material = provider
            .acquire(request(&core, CredentialReason::Missing))
            .await
            .expect("OAuth acquisition");
        assert_eq!(material.expose_secret().len(), TOKEN.len());
        assert!(material.expires_at().is_some_and(|expires| {
            expires >= before + Duration::from_secs(3_599)
                && expires <= Instant::now() + Duration::from_secs(3_600)
        }));
        assert_eq!(executor.provider().request_count().expect("count"), 1);
        assert_eq!(executor.application().request_count().expect("count"), 0);

        let capture = executor.provider().captures().expect("captures").remove(0);
        assert!(capture.safe_url().is_none());
        assert!(capture.headers().iter().any(|header| {
            header.name() == AUTHORIZATION && header.value() == &CapturedHeaderValue::Redacted
        }));
        let rendered = format!("{capture:?}");
        for sentinel in [CLIENT_ID, CLIENT_SECRET, TOKEN] {
            assert!(!rendered.contains(sentinel));
        }
    }

    #[tokio::test]
    async fn oauth_refresh_reacquires_without_scope_or_refresh_token_grant() {
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .body(Bytes::from_static(b"grant_type=client_credentials")),
                ScriptedResponse::success(Bytes::from_static(
                    br#"{"access_token":"renewed","token_type":"bearer"}"#,
                )),
            )
            .expect("refresh exchange");
        let core = core(&executor, 4_096);
        let provider = provider(None);
        let current = AccessToken::new("challenged-current");
        let refreshed = provider
            .refresh(&current, request(&core, CredentialReason::Rejected))
            .await
            .expect("client-credentials reacquisition");
        assert_eq!(refreshed.expose_secret(), "renewed");
    }

    #[tokio::test]
    async fn oauth_basic_components_use_form_encoding_before_base64() {
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_expected_response(
                ExpectedRequest::new().header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Basic Y2xpZW50K2lkJTNBcGx1cyUyQjpzK2UlM0FjcmV0JTJC"),
                ),
                ScriptedResponse::success(Bytes::from_static(br#"{"access_token":"encoded"}"#)),
            )
            .expect("encoded OAuth exchange");
        let core = core(&executor, 4_096);
        let provider = OAuth2ClientCredentials::new(
            "https://auth.example.test/oauth/token",
            SecretString::new("client id:plus+"),
            SecretString::new("s e:cret+"),
            None,
        )
        .expect("OAuth provider");
        let token = provider
            .acquire(request(&core, CredentialReason::Missing))
            .await
            .expect("encoded OAuth acquisition");
        assert_eq!(token.expose_secret(), "encoded");
    }

    #[tokio::test]
    async fn malformed_unsafe_and_non_success_oauth_responses_are_body_safe() {
        const SENTINEL: &str = "oauth-error-body-sentinel-e83a9d";
        let cases = [
            (StatusCode::OK, br#"{}"#.as_slice()),
            (StatusCode::OK, br#"{"access_token":""}"#.as_slice()),
            (
                StatusCode::OK,
                br#"{"access_token":"x","token_type":"mac"}"#.as_slice(),
            ),
            (StatusCode::OK, br#"{"#.as_slice()),
            (
                StatusCode::OK,
                br#"{"access_token":"x","expires_in":-1}"#.as_slice(),
            ),
            (
                StatusCode::OK,
                br#"{"access_token":"x","expires_in":18446744073709551616}"#.as_slice(),
            ),
            (StatusCode::BAD_REQUEST, SENTINEL.as_bytes()),
        ];
        for (status, body) in cases {
            let executor = DeterministicExecutor::new();
            executor
                .provider()
                .queue_response(ScriptedResponse::status(
                    status,
                    Bytes::copy_from_slice(body),
                ))
                .expect("OAuth response");
            let core = core(&executor, 4_096);
            let error = auth_error(
                provider(None)
                    .acquire(request(&core, CredentialReason::Missing))
                    .await,
            );
            assert_eq!(error.kind(), AuthErrorKind::Acquisition);
            for rendered in [error.to_string(), format!("{error:?}")] {
                assert!(!rendered.contains(SENTINEL));
            }
            assert!(Error::source(&error).is_none());
        }
    }

    #[tokio::test]
    async fn oversized_oauth_response_is_reduced_to_safe_bounded_error() {
        const SENTINEL: &str = "oversized-oauth-sentinel-803ef4";
        let executor = DeterministicExecutor::new();
        executor
            .provider()
            .queue_response(ScriptedResponse::success(Bytes::from_static(
                SENTINEL.as_bytes(),
            )))
            .expect("oversized response");
        let core = core(&executor, 4);
        let error = auth_error(
            provider(None)
                .acquire(request(&core, CredentialReason::Missing))
                .await,
        );
        assert_eq!(error.kind(), AuthErrorKind::ResponseTooLarge);
        assert!(!error.to_string().contains(SENTINEL));
        assert!(!format!("{error:?}").contains(SENTINEL));
    }
}
