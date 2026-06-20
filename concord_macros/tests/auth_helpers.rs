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
use tokio::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct LoginRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    name: String,
}

use self::auth_helper_api::AuthHelperApi;
use self::basic_helper_api::BasicHelperApi;

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
            -> Json<LoginResponse>
            map AccessToken { AccessToken::new(r.access_token) }
    }

    scope protected {
        auth bearer session

        GET Me
            path ["me"]
            -> Json<User>
    }
}

api! {
    client BasicHelperApi {
        base "https://example.com"
        secret username: String
        secret password: String
        credential login = basic(secret.username, secret.password)
    }

    GET BasicMe
        path ["basic-me"]
        auth basic login
        -> Json<User>
}

#[tokio::test]
async fn endpoint_backed_auth_helpers_acquire_clear_and_gate_protected_requests() {
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
async fn generated_basic_auth_keeps_username_and_password_secret_until_transport() {
    const USERNAME: &str = "LEAK_SENTINEL_GENERATED_BASIC_USER";
    const PASSWORD: &str = "LEAK_SENTINEL_GENERATED_BASIC_PASSWORD";

    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"{"name":"Ada"}"#)]);
    let sent = transport.clone();
    let api =
        BasicHelperApi::new_with_transport(PASSWORD.to_string(), USERNAME.to_string(), transport);

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

#[derive(Clone)]
struct RecordingTransport {
    responses: Arc<Mutex<VecDeque<ResponseFixture>>>,
    requests: Arc<Mutex<Vec<TransportRequest>>>,
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

    async fn requests(&self) -> Vec<TransportRequest> {
        self.requests.lock().await.clone()
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
            requests.lock().await.push(req.clone());
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
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        Self {
            status: StatusCode::OK,
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
