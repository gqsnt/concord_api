#![cfg(all(feature = "oauth2-client", feature = "default-tls"))]

use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::auth::{
    AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest,
    OAuth2ClientCredentials,
};
use concord::error::ErrorCategory;
use concord::rate_limit::{
    RateLimitError, RateLimitErrorKind, RateLimitPermit, RateLimitRequest, RateLimiter,
};
use concord_test_support::tls::TlsServer;

type OAuthAlias = OAuth2ClientCredentials;

api! {
    client OAuthRuntimeApi {
        base "https://application.invalid/";
        credentials {
            credential oauth = custom OAuthAlias;
        }
    }

    GET Protected() -> Bytes {
        auth bearer oauth challenge unauthorized;
        rate_limit { 1 / 1s; }
    }
}

api! {
    client ProviderPathApi {
        base "https://provider-paths.invalid/";
        credentials {
            credential aliased = custom OAuthAlias;
            credential qualified = custom concord::auth::OAuth2ClientCredentials;
        }
    }
}

struct LocalAccessTokenProvider {
    calls: Arc<AtomicUsize>,
    token: &'static str,
}

impl CredentialProvider for LocalAccessTokenProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let token = self.token;
        Box::pin(async move { Ok(AccessToken::new(token)) })
    }
}

struct FailingLimiter {
    calls: Arc<AtomicUsize>,
}

impl RateLimiter for FailingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
    }
}

fn provider(token_url: &str) -> OAuth2ClientCredentials {
    OAuth2ClientCredentials::builder(token_url)
        .client_id("client id:plus+")
        .client_secret("s e:cret+")
        .scope("users.read offline")
        .build()
        .expect("valid OAuth provider")
}

fn render_error_chain(error: &(dyn Error + 'static)) -> String {
    let mut rendered = format!("{error}\n{error:?}");
    let mut source = error.source();
    while let Some(current) = source {
        rendered.push('\n');
        rendered.push_str(&current.to_string());
        rendered.push('\n');
        rendered.push_str(&format!("{current:?}"));
        source = current.source();
    }
    rendered
}

#[tokio::test]
async fn custom_provider_paths_are_type_neutral_inert_and_replaceable() {
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let client = ProviderPathApi::builder()
        .aliased_provider(LocalAccessTokenProvider {
            calls: Arc::clone(&first_calls),
            token: "first-local-token",
        })
        .qualified_provider(LocalAccessTokenProvider {
            calls: Arc::clone(&second_calls),
            token: "second-local-token",
        })
        .build()
        .expect("provider-path client");

    assert_eq!(first_calls.load(Ordering::SeqCst), 0);
    assert_eq!(second_calls.load(Ordering::SeqCst), 0);
    assert!(!client.credentials().aliased().is_set());
    assert!(!client.credentials().qualified().is_set());

    client
        .credentials()
        .aliased()
        .replace_provider(provider("https://auth.example.test/token"))
        .await
        .expect("same-material provider replacement");
    assert_eq!(first_calls.load(Ordering::SeqCst), 0);
    assert!(!client.credentials().aliased().is_set());
}

#[tokio::test]
async fn oauth_extra_executes_through_generated_provider_lifecycle_and_generic_replacement() {
    const TOKEN: &str = "oauth-provider-response-token-sentinel-40de";
    const RESPONSE: &[u8] =
        br#"{"access_token":"oauth-provider-response-token-sentinel-40de","token_type":"bearer","expires_in":3600}"#;
    let server = TlsServer::start().expect("OAuth provider TLS server");
    server
        .queue_response(200, RESPONSE)
        .expect("OAuth provider response");
    let token_url = server.url("localhost", "/oauth/token?tenant=url-sentinel-2a9c");
    let limiter_calls = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(FailingLimiter {
        calls: Arc::clone(&limiter_calls),
    });
    let certificate = server.certificate_pem();
    let client = OAuthRuntimeApi::builder()
        .oauth_provider(provider(&token_url))
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("generated OAuth client");

    let error = client
        .protected()
        .await
        .expect_err("explicit limiter stops before application transport");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(limiter_calls.load(Ordering::SeqCst), 1);
    assert!(client.credentials().oauth().is_set());
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("provider request reached isolated TLS server");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(
        requests[0].target(),
        "/oauth/token?tenant=url-sentinel-2a9c"
    );
    assert_eq!(
        requests[0].header("authorization"),
        Some("Basic Y2xpZW50K2lkJTNBcGx1cyUyQjpzK2UlM0FjcmV0JTJC")
    );
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        requests[0].body(),
        b"grant_type=client_credentials&scope=users.read+offline"
    );

    let local_calls = Arc::new(AtomicUsize::new(0));
    client
        .credentials()
        .oauth()
        .replace_provider(LocalAccessTokenProvider {
            calls: Arc::clone(&local_calls),
            token: "replacement-local-token-sentinel-f4c2",
        })
        .await
        .expect("generic provider replacement");
    assert!(!client.credentials().oauth().is_set());
    let second = client
        .protected()
        .await
        .expect_err("replacement still reaches explicit limiter");
    assert_eq!(second.category(), ErrorCategory::RateLimit);
    assert_eq!(local_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_calls.load(Ordering::SeqCst), 2);
    assert_eq!(server.requests().len(), 1);

    let rendered = format!(
        "{}\n{}",
        render_error_chain(&error),
        render_error_chain(&second)
    );
    for sentinel in [
        "client id:plus+",
        "s e:cret+",
        "Basic Y2xpZW50K2lkJTNBcGx1cyUyQjpzK2UlM0FjcmV0JTJC",
        "users.read offline",
        "url-sentinel-2a9c",
        TOKEN,
        "replacement-local-token-sentinel-f4c2",
    ] {
        assert!(
            !rendered.contains(sentinel),
            "safe error output exposed `{sentinel}`"
        );
    }
}

#[tokio::test]
async fn oversized_provider_response_remains_bounded_and_body_safe() {
    const SENTINEL: &str = "oversized-oauth-provider-body-sentinel-803ef4";
    let server = TlsServer::start().expect("bounded OAuth provider TLS server");
    server
        .queue_response(200, SENTINEL.as_bytes())
        .expect("oversized OAuth response");
    let token_url = server.url("localhost", "/oversized");
    let limiter_calls = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(FailingLimiter {
        calls: Arc::clone(&limiter_calls),
    });
    let certificate = server.certificate_pem();
    let client = OAuthRuntimeApi::builder()
        .oauth_provider(provider(&token_url))
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(move |runtime| {
            runtime
                .provider_response_limit(NonZeroUsize::new(4).expect("finite response limit"))
                .rate_limiter(limiter)
        })
        .build()
        .expect("bounded generated OAuth client");

    let error = client
        .protected()
        .await
        .expect_err("oversized provider response");
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(limiter_calls.load(Ordering::SeqCst), 0);
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("bounded provider request");
    let rendered = render_error_chain(&error);
    assert!(!rendered.contains(SENTINEL));
}
