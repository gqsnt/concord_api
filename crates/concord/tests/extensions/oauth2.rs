//! The first-party OAuth2 client-credentials provider through the generated
//! provider lifecycle: declared token origin, bounded provider HTTP, local
//! credential controls and value-free failures (§11.2, AUTH-02).

#![cfg(all(feature = "oauth2-client", feature = "default-tls"))]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::auth::{AccessToken, BoxFuture, OAuth2ClientCredentials};
use concord::error::{ErrorCategory, FailureStage};
use concord::rate_limit::{
    Enforcement, LimiterCapability, NoopRateLimiter, RateLimitError, RateLimitErrorKind,
    RateLimitPermit, RateLimitRequest, RateLimiter,
};
use concord::{Binary, Bytes, api};
use concord_test_support::tls::TlsServer;

type OAuthAlias = OAuth2ClientCredentials;

api! {
    client OAuthRuntimeApi {
        timeout 30s;
        base "https://application.invalid/";
        credentials {
            credential oauth = custom OAuthAlias;
        }

        GET protected() -> Bytes via Binary {
            auth { bearer oauth challenge unauthorized; }
            rate_limit { id "concord.oauth2.1"; key [origin]; cost 1; allow 1 per 1s; }
        }
    }
}

api! {
    client ProviderPathApi {
        timeout 30s;
        base "https://provider-paths.invalid/";
        credentials {
            credential aliased = custom OAuthAlias;
            credential qualified = custom concord::auth::OAuth2ClientCredentials;
        }

        GET both() -> Bytes via Binary {
            auth {
                bearer aliased;
                header "X-Qualified" = qualified;
            }
        }
    }
}

struct FailingLimiter {
    calls: Arc<AtomicUsize>,
}

impl RateLimiter for FailingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.tests.failing", 1)
    }

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
async fn custom_provider_paths_are_type_neutral_inert_and_locally_controllable() {
    let server = TlsServer::start().expect("never-contacted token server");
    let token_url = server.url("localhost", "/token");
    let certificate = server.certificate_pem();
    // The alias and the fully qualified path select the same provider type:
    // spelling carries no macro semantics.
    let client = ProviderPathApi::builder()
        .aliased(provider(&token_url))
        .qualified(provider(&token_url))
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("provider-path client");

    // Building and planning perform no provider acquisition.
    let plan = client.both().plan().expect("inert plan");
    assert!(plan.capabilities().authentication());
    // Local controls replace or clear provider-slot material synchronously
    // without any network work. (v2's runtime provider replacement is
    // removed: providers are builder inputs; §11.3.)
    client
        .credentials()
        .replace_aliased(AccessToken::try_new("manual-aliased").expect("bounded token"))
        .expect("manual aliased material");
    client
        .credentials()
        .clear_qualified()
        .expect("clear qualified slot");
    client
        .credentials()
        .clear_aliased()
        .expect("clear aliased slot");
    std::thread::sleep(Duration::from_millis(50));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn oauth_extra_executes_through_generated_provider_lifecycle_and_local_replacement() {
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
        .oauth(provider(&token_url))
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("generated OAuth client");

    let error = client
        .protected()
        .await
        .expect_err("explicit limiter stops before application transport");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(limiter_calls.load(Ordering::SeqCst), 1);
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

    // The acquired material is installed: a later call reuses it without a
    // second token request.
    let reused = client
        .protected()
        .await
        .expect_err("limiter still stops the application request");
    assert_eq!(reused.category(), ErrorCategory::RateLimit);
    assert_eq!(server.requests().len(), 1);

    // A manual local replacement installs new material; no provider call.
    client
        .credentials()
        .replace_oauth(
            AccessToken::try_new("replacement-local-token-sentinel-f4c2").expect("bounded token"),
        )
        .expect("local replacement");
    let second = client
        .protected()
        .await
        .expect_err("replacement still reaches explicit limiter");
    assert_eq!(second.category(), ErrorCategory::RateLimit);
    assert_eq!(limiter_calls.load(Ordering::SeqCst), 3);
    assert_eq!(server.requests().len(), 1);

    let rendered = format!(
        "{}\n{}\n{}",
        render_error_chain(&error),
        render_error_chain(&reused),
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
async fn oauth_form_body_uses_runtime_provider_limit_before_transport() {
    const SCOPE_SENTINEL: &str = "oauth-runtime-limit-scope-sentinel-73e0";
    let server = TlsServer::start().expect("bounded OAuth provider TLS server");
    server
        .queue_response(200, br#"{"access_token":"unused"}"#)
        .expect("defensive provider response");
    let token_url = server.url("localhost", "/request-limit-sentinel-93c1");
    let certificate = server.certificate_pem();
    let oauth = OAuth2ClientCredentials::builder(&token_url)
        .client_id("runtime-limit-client-id-sentinel-1f02")
        .client_secret("runtime-limit-client-secret-sentinel-445d")
        .scope(SCOPE_SENTINEL)
        .build()
        .expect("bounded OAuth provider");
    let client = OAuthRuntimeApi::builder()
        .oauth(oauth)
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(|runtime| {
            runtime
                .provider_request_limit(1)
                .rate_limiter(Arc::new(NoopRateLimiter::new()))
        })
        .build()
        .expect("generated OAuth client");

    let error = client
        .protected()
        .await
        .expect_err("OAuth form exceeds the runtime provider request limit");
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert!(server.requests().is_empty());
    let rendered = render_error_chain(&error);
    for sentinel in [
        "runtime-limit-client-id-sentinel-1f02",
        "runtime-limit-client-secret-sentinel-445d",
        SCOPE_SENTINEL,
        "request-limit-sentinel-93c1",
    ] {
        assert!(!rendered.contains(sentinel));
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
        .oauth(provider(&token_url))
        .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
        .configure_runtime(move |runtime| runtime.provider_response_limit(4).rate_limiter(limiter))
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
