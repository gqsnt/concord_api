use bytes::Bytes;
use concord_core::advanced::{
    ClientCertificate, RateLimitPlan, Transport, TransportAuth, TransportBody, TransportError,
    TransportRequest, TransportResponse,
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
pub struct BasicLoginResponse {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct CertificateLoginResponse {
    identity_id: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    name: String,
}

use self::auth_helper_api::AuthHelperApi;
use self::basic_endpoint_helper_api::BasicEndpointHelperApi;
use self::basic_helper_api::BasicHelperApi;
use self::certificate_endpoint_helper_api::CertificateEndpointHelperApi;
use self::policy_merge_helper_api::PolicyMergeHelperApi;

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

api! {
    client BasicEndpointHelperApi {
        base "https://example.com"
        credential basic_session = endpoint auth_api::LoginForBasic
    }

    scope auth_api {
        POST LoginForBasic(body: Json<LoginRequest>)
            path ["login-basic"]
            -> Json<BasicLoginResponse>
            map BasicCredential {
                BasicCredential::new(r.username, r.password)
            }
    }

    scope protected {
        auth basic basic_session

        GET BasicMe
            path ["basic-me"]
            -> Json<User>
    }
}

api! {
    client CertificateEndpointHelperApi {
        base "https://example.com"
        credential cert_session = endpoint auth_api::GetCertificate
    }

    scope auth_api {
        POST GetCertificate(body: Json<LoginRequest>)
            path ["cert"]
            -> Json<CertificateLoginResponse>
            map ClientCertificate {
                ClientCertificate::new(r.identity_id)
            }
    }

    scope protected {
        auth certificate cert_session

        GET CertificateMe
            path ["certificate-me"]
            -> Json<User>
    }
}

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
async fn endpoint_backed_certificate_credential_materializes_transport_auth() {
    const IDENTITY_ID: &str = "endpoint-certificate-identity";

    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"{"identity_id":"endpoint-certificate-identity"}"#),
        ResponseFixture::json(r#"{"name":"Ada"}"#),
    ]);
    let sent = transport.clone();
    let api = CertificateEndpointHelperApi::new_with_transport(transport);

    api.auth_api()
        .get_certificate(LoginRequest {
            username: "ada".to_string(),
        })
        .acquire_as_cert_session()
        .await
        .expect("certificate session acquisition succeeds");

    let user = api
        .protected()
        .certificate_me()
        .execute()
        .await
        .expect("protected request succeeds");
    assert_eq!(user.name, "Ada");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "auth_api::GetCertificate");
    assert_eq!(requests[1].meta.endpoint, "protected::CertificateMe");
    assert_eq!(
        requests[1].transport_auth,
        Some(TransportAuth::ClientCertificate {
            identity_id: IDENTITY_ID.to_string(),
        })
    );
    assert!(!format!("{:?}", requests[1]).contains(IDENTITY_ID));
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

fn assert_header(req: &TransportRequest, name: &'static str, expected: &'static str) {
    assert_eq!(
        req.headers.get(name).and_then(|value| value.to_str().ok()),
        Some(expected)
    );
}

fn assert_url_contains(req: &TransportRequest, expected: &'static str) {
    assert!(
        req.url.as_str().contains(expected),
        "expected URL `{}` to contain `{expected}`",
        req.url
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
