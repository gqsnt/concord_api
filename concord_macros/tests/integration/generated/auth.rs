use bytes::Bytes;
use concord_core::advanced::{
    RateLimitPlan, Transport, TransportBody, TransportError, TransportRequest, TransportResponse,
};
use concord_core::prelude::*;
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct LoginRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    name: String,
}

use self::auth_helper_contract::{AuthHelperApi, AuthHelperApiAcquireAsSessionExt};
use self::basic_endpoint_helper_contract::{
    BasicEndpointHelperApi, BasicEndpointHelperApiAcquireAsBasicSessionExt,
};
use self::basic_helper_contract::BasicHelperApi;
use self::o_auth_helper_contract::OAuthHelperApi;
use self::policy_merge_helper_contract::PolicyMergeHelperApi;

mod auth_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client AuthHelperApi {
            base "https://example.com"
            secret upstream_key: String
            credential upstream = api_key(secret.upstream_key)
            credential session = endpoint auth_api::LoginForSession
        }

        scope auth_api {
            POST LoginForSession(body: Json<LoginRequest>)
                path ["login"]
                auth header "X-Upstream-Key" = upstream
                -> Json<AccessToken>
        }

        scope protected {
            auth bearer session

            GET Me
                path ["me"]
                -> Json<User>
        }
    }

    pub(super) use auth_helper_api::AuthHelperApi;
}

mod basic_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client BasicHelperApi {
            base "https://example.com"
            secret username: String
            secret password: String
            credential login = basic(secret.username, secret.password)

            policies {
                retry once {
                    max_attempts 2
                    methods [GET]
                    on [500]
                }
            }
        }

        GET BasicMe
            path ["basic-me"]
            auth basic login
            -> Json<User>

        GET BasicRetry
            path ["basic-retry"]
            retry once
            auth basic login
            -> Json<User>
    }

    pub(super) use basic_helper_api::BasicHelperApi;
}

mod basic_endpoint_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client BasicEndpointHelperApi {
            base "https://example.com"
            credential basic_session = endpoint auth_api::LoginForBasic
        }

        scope auth_api {
            POST LoginForBasic(body: Json<LoginRequest>)
                path ["login-basic"]
                -> Json<BasicCredential>
        }

        scope protected {
            auth basic basic_session

            GET BasicMe
                path ["basic-me"]
                -> Json<User>
        }
    }

    pub(super) use basic_endpoint_helper_api::BasicEndpointHelperApi;
}

mod o_auth_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client OAuthHelperApi {
            base "https://api.example.com"
            secret client_id: String
            secret client_secret: String
            credential oauth = oauth2_client {
                token_url: "https://auth.example.com/oauth/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
                scope: "read:me",
            }

            policies {
                retry once {
                    max_attempts 2
                    methods [GET]
                    on [500]
                }
            }
        }

        GET OAuthMe
            path ["oauth-me"]
            auth bearer oauth
            -> Json<User>

        GET OAuthRetry
            path ["oauth-retry"]
            retry once
            auth bearer oauth
            -> Json<User>
    }

    pub(super) use o_auth_helper_api::OAuthHelperApi;
}

mod policy_merge_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client PolicyMergeHelperApi {
            base "https://example.com"
            var client_header_a: String
            var client_header_b: String
            var client_query_a: String
            var client_query_b: String

            header "X-Client-Key" = vars.client_header_a,
            headers {
                "X-Client-Token" = vars.client_header_b
            }
            query "client_key" = vars.client_query_a,
            query {
                "client_session" = vars.client_query_b
            }
        }

        scope merged {
            path ["merged"]

            header "X-Scope-Key" = "scope-a",
            headers {
                "X-Scope-Token" = "scope-b"
            }
            query "scope_key" = "scope-a",
            query {
                "scope_session" = "scope-b"
            }

            GET InlineThenBlock
                path ["inline-then-block"]
                header "X-Endpoint-Key" = "endpoint-a",
                headers {
                    "X-Endpoint-Token" = "endpoint-b"
                }
                query "endpoint_key" = "endpoint-a",
                query {
                    "endpoint_session" = "endpoint-b"
                }
                -> Json<User>

            GET BlockThenInline
                path ["block-then-inline"]
                headers {
                    "X-Endpoint-Block" = "block"
                }
                header "X-Endpoint-Inline" = "inline",
                query {
                    "endpoint_block" = "block"
                }
                query "endpoint_inline" = "inline",
                -> Json<User>
        }
    }

    pub(super) use policy_merge_helper_api::PolicyMergeHelperApi;
}

#[tokio::test]
async fn endpoint_backed_auth_acquire_clear_and_gate_protected_requests() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"{"access_token":"session-token"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = AuthHelperApi::new_with_transport("upstream-secret".to_string(), transport);

    let err = api
        .protected()
        .me()
        .execute()
        .await
        .expect_err("protected request must fail before session acquisition");
    let msg = err.to_string();
    assert!(msg.contains("missing credential"));
    assert!(msg.contains("client.acquire_auth_session(...)"));
    assert_eq!(sent.sent_count().await, 0);
    assert!(
        !api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    api.auth_api()
        .login_for_session(LoginRequest {
            username: "ada".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");
    assert!(
        api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    let user = api
        .protected()
        .me()
        .execute()
        .await
        .expect("protected request succeeds after acquisition");
    assert_eq!(user.name, "Ada");

    api.auth_state()
        .session()
        .clear()
        .await
        .expect("session clear succeeds");
    assert!(
        !api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "auth_api::LoginForSession");
    assert_eq!(
        requests[0]
            .headers
            .get("X-Upstream-Key")
            .and_then(|value| value.to_str().ok()),
        Some("upstream-secret")
    );
    assert_eq!(requests[1].meta.endpoint, "protected::Me");
    assert_eq!(
        requests[1]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer session-token")
    );
}

#[tokio::test]
async fn same_layer_policy_header_query_inline_then_block_are_preserved() {
    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"{"name":"Ada"}"#)]);
    let sent = transport.clone();
    let api = PolicyMergeHelperApi::new_with_transport(
        "client-header-a".to_string(),
        "client-header-b".to_string(),
        "client-query-a".to_string(),
        "client-query-b".to_string(),
        transport,
    );

    api.merged()
        .inline_then_block()
        .execute()
        .await
        .expect("request succeeds");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert_header(req, "X-Client-Key", "client-header-a");
    assert_header(req, "X-Client-Token", "client-header-b");
    assert_header(req, "X-Scope-Key", "scope-a");
    assert_header(req, "X-Scope-Token", "scope-b");
    assert_header(req, "X-Endpoint-Key", "endpoint-a");
    assert_header(req, "X-Endpoint-Token", "endpoint-b");
    assert_url_contains(req, "client_key=client-query-a");
    assert_url_contains(req, "client_session=client-query-b");
    assert_url_contains(req, "scope_key=scope-a");
    assert_url_contains(req, "scope_session=scope-b");
    assert_url_contains(req, "endpoint_key=endpoint-a");
    assert_url_contains(req, "endpoint_session=endpoint-b");
}

#[tokio::test]
async fn same_layer_policy_header_query_block_then_inline_are_preserved() {
    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"{"name":"Ada"}"#)]);
    let sent = transport.clone();
    let api = PolicyMergeHelperApi::new_with_transport(
        "client-header-a".to_string(),
        "client-header-b".to_string(),
        "client-query-a".to_string(),
        "client-query-b".to_string(),
        transport,
    );

    api.merged()
        .block_then_inline()
        .execute()
        .await
        .expect("request succeeds");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert_header(req, "X-Endpoint-Block", "block");
    assert_header(req, "X-Endpoint-Inline", "inline");
    assert_url_contains(req, "endpoint_block=block");
    assert_url_contains(req, "endpoint_inline=inline");
}

#[tokio::test]
async fn endpoint_backed_basic_credential_materializes_basic_authorization() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"{"username":"endpoint-user","password":"endpoint-password"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = BasicEndpointHelperApi::new_with_transport(transport);

    api.auth_api()
        .login_for_basic(LoginRequest {
            username: "ada".to_string(),
        })
        .acquire_as_basic_session()
        .await
        .expect("basic session acquisition succeeds");

    let user = api
        .protected()
        .basic_me()
        .execute()
        .await
        .expect("protected request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "auth_api::LoginForBasic");
    assert_eq!(requests[1].meta.endpoint, "protected::BasicMe");
    assert_eq!(
        requests[1]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Basic ZW5kcG9pbnQtdXNlcjplbmRwb2ludC1wYXNzd29yZA==")
    );
    let debug_output = format!("{:?}", requests[1]);
    assert!(!debug_output.contains("endpoint-user"));
    assert!(!debug_output.contains("endpoint-password"));
}

#[tokio::test]
async fn generated_basic_auth_keeps_username_and_password_secret_until_transport() {
    const USERNAME: &str = "LEAK_SENTINEL_GENERATED_BASIC_USER";
    const PASSWORD: &str = "LEAK_SENTINEL_GENERATED_BASIC_PASSWORD";

    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"{"name":"Ada"}"#)]);
    let sent = transport.clone();
    let api =
        BasicHelperApi::new_with_transport(USERNAME.to_string(), PASSWORD.to_string(), transport);

    let user = api
        .basic_me()
        .execute()
        .await
        .expect("basic request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let debug_output = format!("{:?}", requests[0]);
    assert!(!debug_output.contains(USERNAME));
    assert!(!debug_output.contains(PASSWORD));

    let header = requests[0]
        .headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .expect("basic auth header materialized");
    assert_eq!(
        header,
        "Basic TEVBS19TRU5USU5FTF9HRU5FUkFURURfQkFTSUNfVVNFUjpMRUFLX1NFTlRJTkVMX0dFTkVSQVRFRF9CQVNJQ19QQVNTV09SRA=="
    );
}

#[tokio::test]
async fn generated_static_basic_auth_reuses_preparation_across_transport_retry() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::status_json(StatusCode::INTERNAL_SERVER_ERROR, r#"{"error":"retry"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = BasicHelperApi::new_with_transport(
        "static-user".to_string(),
        "static-password".to_string(),
        transport,
    );

    let user = api
        .basic_retry()
        .execute()
        .await
        .expect("static basic retry request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "BasicRetry");
    assert_eq!(requests[1].meta.endpoint, "BasicRetry");
    let first_slot = requests[0]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("first request has prepared auth")
        .id;
    let second_slot = requests[1]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("retry request has prepared auth")
        .id;
    assert_eq!(
        first_slot, second_slot,
        "cached request-local preparation should reuse the auth slot"
    );
    for req in &requests {
        assert_eq!(
            req.headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Basic c3RhdGljLXVzZXI6c3RhdGljLXBhc3N3b3Jk")
        );
    }
}

#[tokio::test]
async fn generated_oauth_client_credentials_acquires_token_and_sends_bearer() {
    const CLIENT_ID: &str = "oauth-client";
    const CLIENT_SECRET: &str = "LEAK_SENTINEL_OAUTH_CLIENT_SECRET";
    const ACCESS_TOKEN: &str = "LEAK_SENTINEL_OAUTH_ACCESS_TOKEN";

    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(
            r#"{"access_token":"LEAK_SENTINEL_OAUTH_ACCESS_TOKEN","token_type":"Bearer","expires_in":3600}"#,
        ),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = OAuthHelperApi::new_with_transport(
        CLIENT_ID.to_string(),
        CLIENT_SECRET.to_string(),
        transport,
    );

    let user = api
        .oauth_me()
        .execute()
        .await
        .expect("oauth protected request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "<auth>");
    assert_eq!(requests[0].meta.method, http::Method::POST);
    assert_eq!(
        requests[0].url.as_str(),
        "https://auth.example.com/oauth/token"
    );
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Basic b2F1dGgtY2xpZW50OkxFQUtfU0VOVElORUxfT0FVVEhfQ0xJRU5UX1NFQ1JFVA==")
    );
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        requests[0]
            .body
            .as_bytes()
            .and_then(|body| std::str::from_utf8(body).ok()),
        Some("grant_type=client_credentials&scope=read%3Ame")
    );

    assert_eq!(requests[1].meta.endpoint, "OAuthMe");
    assert_eq!(
        requests[1]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer LEAK_SENTINEL_OAUTH_ACCESS_TOKEN")
    );
    assert!(!requests[1].url.as_str().contains(CLIENT_SECRET));
    assert!(!requests[1].body.as_bytes().is_some_and(|body| {
        body.windows(CLIENT_SECRET.len())
            .any(|window| window == CLIENT_SECRET.as_bytes())
    }));

    let token_debug = format!("{:?}", requests[0]);
    let protected_debug = format!("{:?}", requests[1]);
    assert!(!token_debug.contains(CLIENT_SECRET));
    assert!(!token_debug.contains(ACCESS_TOKEN));
    assert!(!protected_debug.contains(CLIENT_SECRET));
    assert!(!protected_debug.contains(ACCESS_TOKEN));
}

#[tokio::test]
async fn generated_oauth_client_credentials_reuses_valid_token() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(
            r#"{"access_token":"reuse-token","token_type":"Bearer","expires_in":3600}"#,
        ),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = OAuthHelperApi::new_with_transport(
        "oauth-client".to_string(),
        "oauth-secret".to_string(),
        transport,
    );

    api.oauth_me()
        .execute()
        .await
        .expect("first protected request succeeds");
    api.oauth_me()
        .execute()
        .await
        .expect("second protected request succeeds");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].meta.endpoint, "<auth>");
    assert_eq!(requests[1].meta.endpoint, "OAuthMe");
    assert_eq!(requests[2].meta.endpoint, "OAuthMe");
    for req in &requests[1..] {
        assert_eq!(
            req.headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer reuse-token")
        );
    }
}

#[tokio::test]
async fn generated_oauth_client_credentials_refreshes_after_unauthorized() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(
            r#"{"access_token":"token-a","token_type":"Bearer","expires_in":3600}"#,
        ),
        ResponseFixture::status_json(StatusCode::UNAUTHORIZED, r#"{"error":"expired"}"#),
        ResponseFixture::json(
            r#"{"access_token":"token-b","token_type":"Bearer","expires_in":3600}"#,
        ),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = OAuthHelperApi::new_with_transport(
        "oauth-client".to_string(),
        "oauth-secret".to_string(),
        transport,
    );

    let user = api
        .oauth_me()
        .execute()
        .await
        .expect("oauth protected request refreshes after 401");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].meta.endpoint, "<auth>");
    assert_eq!(requests[1].meta.endpoint, "OAuthMe");
    assert_eq!(
        requests[1]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer token-a")
    );
    assert_eq!(requests[2].meta.endpoint, "<auth>");
    assert_eq!(requests[3].meta.endpoint, "OAuthMe");
    assert_eq!(
        requests[3]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer token-b")
    );
}

#[tokio::test]
async fn generated_static_basic_auth_reprepares_after_auth_rejection_invalidation() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::status_json(StatusCode::UNAUTHORIZED, r#"{"error":"expired"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = BasicHelperApi::new_with_transport(
        "static-user".to_string(),
        "static-password".to_string(),
        transport,
    );

    let user = api
        .basic_me()
        .execute()
        .await
        .expect("static basic request retries after auth rejection");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let first_slot = requests[0]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("first request has prepared auth")
        .id;
    let second_slot = requests[1]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("auth retry request has prepared auth")
        .id;
    assert_eq!(
        first_slot, second_slot,
        "auth rejection refreshes material without rebuilding placement"
    );
    for req in &requests {
        assert_eq!(
            req.headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Basic c3RhdGljLXVzZXI6c3RhdGljLXBhc3N3b3Jk")
        );
    }
}

#[tokio::test]
async fn generated_oauth_prepares_each_transport_retry() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(
            r#"{"access_token":"oauth-retry-token","token_type":"Bearer","expires_in":3600}"#,
        ),
        ResponseFixture::status_json(StatusCode::INTERNAL_SERVER_ERROR, r#"{"error":"retry"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = OAuthHelperApi::new_with_transport(
        "oauth-client".to_string(),
        "oauth-secret".to_string(),
        transport,
    );

    let user = api
        .oauth_retry()
        .execute()
        .await
        .expect("oauth retry request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].meta.endpoint, "<auth>");
    assert_eq!(requests[1].meta.endpoint, "OAuthRetry");
    assert_eq!(requests[2].meta.endpoint, "OAuthRetry");
    let first_slot = requests[1]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("first protected request has prepared auth")
        .id;
    let second_slot = requests[2]
        .extensions
        .auth_plan
        .slots
        .first()
        .expect("retry protected request has prepared auth")
        .id;
    assert_eq!(
        first_slot, second_slot,
        "credential re-preparation must reuse the request-local placement plan"
    );
    for req in &requests[1..] {
        assert_eq!(
            req.headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer oauth-retry-token")
        );
    }
}

#[tokio::test]
async fn generated_oauth_client_credentials_token_failure_blocks_protected_request() {
    const CLIENT_SECRET: &str = "LEAK_SENTINEL_OAUTH_FAILURE_SECRET";

    let transport = RecordingTransport::new(vec![ResponseFixture::status_json(
        StatusCode::BAD_REQUEST,
        r#"{"error":"invalid_client"}"#,
    )]);
    let sent = transport.clone();
    let api = OAuthHelperApi::new_with_transport(
        "oauth-client".to_string(),
        CLIENT_SECRET.to_string(),
        transport,
    );

    let err = api
        .oauth_me()
        .execute()
        .await
        .expect_err("token failure blocks protected request");
    assert!(!err.to_string().contains(CLIENT_SECRET));
    assert!(!format!("{err:?}").contains(CLIENT_SECRET));

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "<auth>");
}

fn assert_header(req: &RecordedRequest, name: &'static str, expected: &'static str) {
    assert_eq!(
        req.headers.get(name).and_then(|value| value.to_str().ok()),
        Some(expected)
    );
}

fn assert_url_contains(req: &RecordedRequest, expected: &'static str) {
    assert!(
        req.url.as_str().contains(expected),
        "expected URL `{}` to contain `{expected}`",
        req.url
    );
}

#[derive(Clone)]
struct RecordingTransport {
    responses: Arc<Mutex<VecDeque<ResponseFixture>>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

struct RecordedRequest {
    meta: concord_core::transport::RequestMeta,
    url: url::Url,
    headers: http::HeaderMap,
    body: RecordedBody,
    timeout: Option<std::time::Duration>,
    rate_limit: RateLimitPlan,
    extensions: concord_core::auth::RequestExtensions,
}

#[derive(Clone, Debug)]
enum RecordedBody {
    Empty,
    Bytes(Bytes),
    Stream,
}

impl RecordedBody {
    fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Empty | Self::Stream => None,
        }
    }
}

impl std::fmt::Debug for RecordedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = match &self.body {
            RecordedBody::Empty => concord_core::advanced::TransportRequestBody::Empty,
            RecordedBody::Bytes(body) => {
                concord_core::advanced::TransportRequestBody::from_bytes(body.clone())
            }
            RecordedBody::Stream => concord_core::advanced::TransportRequestBody::Stream(
                concord_core::advanced::TransportByteStream::new(EmptyDebugStream),
            ),
        };
        let temp = TransportRequest {
            meta: self.meta.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body,
            timeout: self.timeout,
            rate_limit: self.rate_limit.clone(),
            extensions: self.extensions.clone(),
        };
        write!(f, "{temp:?}")
    }
}

struct EmptyDebugStream;

impl futures_core::Stream for EmptyDebugStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    async fn requests(&self) -> Vec<RecordedRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let body = match req.body {
                concord_core::advanced::TransportRequestBody::Empty => RecordedBody::Empty,
                concord_core::advanced::TransportRequestBody::Bytes(body) => {
                    RecordedBody::Bytes(body)
                }
                concord_core::advanced::TransportRequestBody::Stream(_) => RecordedBody::Stream,
            };
            requests.lock().await.push(RecordedRequest {
                meta: req.meta.clone(),
                url: req.url.clone(),
                headers: req.headers.clone(),
                body,
                timeout: req.timeout,
                rate_limit: req.rate_limit.clone(),
                extensions: req.extensions.clone(),
            });
            let response = responses.lock().await.pop_front().expect("test response");
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: Some(response.body.len() as u64),
                rate_limit: RateLimitPlan::default(),
                body: Box::new(StaticBody(Some(response.body))),
            })
        })
    }
}

struct ResponseFixture {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl ResponseFixture {
    fn json(body: &'static str) -> Self {
        Self::status_json(StatusCode::OK, body)
    }

    fn status_json(status: StatusCode, body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        Self {
            status,
            headers,
            body: Bytes::from_static(body.as_bytes()),
        }
    }
}

struct StaticBody(Option<Bytes>);

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}
