use super::common::{
    GateTransport, MockOutcome, MockResponse, MockTransport, TestAuthVars, TestCx,
    buffered_endpoint_execute, buffered_endpoint_response_terminal, client, request_plan,
};
use crate::support::assert_error_chain_does_not_contain_any;
use bytes::Bytes;
use concord_core::advanced::{RetryBackoff, RetryConfig, RetryIdempotency, StreamBody};
use concord_core::error::ErrorCategory;
use concord_core::internal::{
    BodyPlan, Format, Replayability, RequestArgs, RequestPlan, ResolvedPolicy, RetrySetting,
};
use concord_core::prelude::{ApiClient, ApiClientError, Endpoint, ReusableEndpoint, Text};
use concord_core::transport::{TransportErrorKind, TransportRequestBody};
use http::{HeaderValue, Method, StatusCode, header::CONTENT_TYPE};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

const REQUEST_SENTINEL: &str = "PR22_TRANSPORT_REQUEST_SENTINEL";
const REDIRECT_DEFAULT_PORT: u16 = 38180;
const REDIRECT_CUSTOM_PORT: u16 = 38181;
const REDIRECT_TARGET_PORT: u16 = 38182;
const REDIRECT_CUSTOM_TARGET_PORT: u16 = 38183;
const REDIRECT_DEFAULT_HOST: &str = "127.0.0.1:38180";
const REDIRECT_CUSTOM_HOST: &str = "127.0.0.1:38181";

static REDIRECT_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn redirect_test_lock() -> &'static tokio::sync::Mutex<()> {
    REDIRECT_TEST_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[derive(Clone)]
enum TransportBody {
    Empty,
    Bytes(Bytes),
    Stream(Bytes),
}

#[derive(Clone)]
struct TransportContractEndpoint {
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: TransportBody,
}

impl TransportContractEndpoint {
    fn new(
        name: &'static str,
        method: Method,
        path: &'static str,
        policy: ResolvedPolicy,
        body: TransportBody,
    ) -> Self {
        Self {
            name,
            method,
            path,
            policy,
            body,
        }
    }

    fn bytes(
        name: &'static str,
        method: Method,
        path: &'static str,
        policy: ResolvedPolicy,
    ) -> Self {
        Self::new(
            name,
            method,
            path,
            policy,
            TransportBody::Bytes(Bytes::from_static(b"{\"transport\":true}")),
        )
    }

    fn stream(
        name: &'static str,
        method: Method,
        path: &'static str,
        policy: ResolvedPolicy,
    ) -> Self {
        Self::new(
            name,
            method,
            path,
            policy,
            TransportBody::Stream(Bytes::from_static(b"stream-contract")),
        )
    }

    fn empty(
        name: &'static str,
        method: Method,
        path: &'static str,
        policy: ResolvedPolicy,
    ) -> Self {
        Self::new(name, method, path, policy, TransportBody::Empty)
    }
}

impl Endpoint<TestCx> for TransportContractEndpoint {
    type Response = String;

    buffered_endpoint_execute!(TestCx, Text<String>);
}

buffered_endpoint_response_terminal!(TransportContractEndpoint, TestCx, Text<String>);

impl ReusableEndpoint<TestCx> for TransportContractEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            None,
        );
        match &self.body {
            TransportBody::Empty => {}
            TransportBody::Bytes(body) => {
                plan.endpoint.body = BodyPlan::Encoded {
                    content_type: Some(HeaderValue::from_static("application/json")),
                    format: Format::Text,
                };
                plan.args = RequestArgs::with_body_bytes(body.clone());
                plan.replayability = Replayability::Replayable;
            }
            TransportBody::Stream(body) => {
                plan.endpoint.body = BodyPlan::RawStream {
                    content_type: HeaderValue::from_static("application/octet-stream"),
                };
                plan.args = RequestArgs::with_stream_body(StreamBody::from_bytes(body.clone()));
                plan.replayability = Replayability::NonReplayable;
            }
        }
        Ok(plan)
    }
}

fn request_contract_policy() -> ResolvedPolicy {
    let mut policy = ResolvedPolicy {
        timeout: Some(Duration::from_millis(123)),
        ..ResolvedPolicy::default()
    };
    policy
        .headers
        .append("x-repeat", HeaderValue::from_static("first"));
    policy
        .headers
        .append("x-repeat", HeaderValue::from_static("second"));
    policy
        .headers
        .insert("x-single", HeaderValue::from_static("singleton"));
    policy.query = vec![
        ("policy".to_string(), "one".to_string()),
        ("policy".to_string(), "two".to_string()),
        ("mode".to_string(), "contract".to_string()),
    ];
    policy
}

fn retry_contract_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        retry: RetrySetting::Config(RetryConfig {
            max_attempts: 2,
            methods: vec![Method::GET],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: RetryBackoff::None,
            respect_retry_after: true,
            idempotency: RetryIdempotency::SafeMethodsOnly,
        }),
        ..request_contract_policy()
    }
}

fn assert_header_value(headers: &http::HeaderMap, name: &str, expected: &str) {
    match headers.get(name).and_then(|value| value.to_str().ok()) {
        Some(actual) if actual == expected => {}
        _ => panic!("expected header `{name}` to match the expected value"),
    }
}

fn assert_header_values(headers: &http::HeaderMap, name: &str, expected: &[&str]) {
    let actual: Vec<_> = headers
        .get_all(name)
        .iter()
        .map(|value| value.to_str().expect("header value should be valid UTF-8"))
        .collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "unexpected header value count for `{name}`"
    );
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual, expected,
            "header value mismatch at index {index} for `{name}`"
        );
    }
}

fn assert_query_pairs(url: &url::Url, expected: &[(&str, &str)]) {
    let actual: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(actual.len(), expected.len(), "unexpected query pair count");
    for (index, ((actual_key, actual_value), (expected_key, expected_value))) in
        actual.iter().zip(expected.iter()).enumerate()
    {
        assert_eq!(
            actual_key, expected_key,
            "query key mismatch at index {index}"
        );
        assert_eq!(
            actual_value, expected_value,
            "query value mismatch at index {index}"
        );
    }
}

fn assert_bytes_body(body: &TransportRequestBody, expected: &[u8]) {
    match body.as_bytes() {
        Some(actual) if actual.as_ref() == expected => {}
        Some(_) => panic!("transport body bytes did not match the expected payload"),
        None => panic!("expected a buffered transport body"),
    }
}

fn http_response(status: StatusCode, headers: &[(&str, String)], body: &str) -> String {
    let reason = status.canonical_reason().unwrap_or("OK");
    let mut response = format!("HTTP/1.1 {} {reason}\r\n", status.as_u16());
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    response
}

fn spawn_http_server(
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    hits: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    response: String,
    one_shot: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind local test port");
        listener
            .set_nonblocking(true)
            .expect("set local test listener nonblocking");

        let mut scratch = [0u8; 1024];
        while !shutdown.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = Vec::new();
                    loop {
                        match stream.read(&mut scratch) {
                            Ok(0) => break,
                            Ok(n) => {
                                request.extend_from_slice(&scratch[..n]);
                                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(err) => panic!("read local test request: {err}"),
                        }
                    }
                    hits.fetch_add(1, Ordering::SeqCst);
                    requests
                        .blocking_lock()
                        .push(String::from_utf8_lossy(&request).into_owned());
                    stream
                        .write_all(response.as_bytes())
                        .expect("write local test response");
                    let _ = stream.flush();
                    if one_shot {
                        break;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("accept local test request: {err}"),
            }
        }
    })
}

#[derive(Clone)]
struct RedirectCx;

impl concord_core::prelude::ClientContext for RedirectCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "127.0.0.1:38180";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a concord_core::advanced::AuthRequirement,
        request: &'a mut concord_core::advanced::AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, concord_core::advanced::AuthError>,
    > {
        Box::pin(async move {
            let token = auth.token.as_ref().ok_or_else(|| {
                concord_core::advanced::AuthError::new(
                    concord_core::advanced::AuthErrorKind::MissingCredential,
                    "missing redirect test bearer token",
                )
            })?;
            let material = concord_core::prelude::ApiKey::new(token.clone());
            let application =
                concord_core::advanced::apply_secret_credential(request, requirement, &material)?;
            let applied = concord_core::advanced::AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(concord_core::advanced::PreparedAuthCredential::new(
                applied,
                application,
            ))
        })
    }
}

#[derive(Clone)]
struct RedirectEndpoint {
    name: &'static str,
    path: &'static str,
    policy: ResolvedPolicy,
    host: &'static str,
}

impl Endpoint<RedirectCx> for RedirectEndpoint {
    type Response = String;

    buffered_endpoint_execute!(RedirectCx, Text<String>);
}

buffered_endpoint_response_terminal!(RedirectEndpoint, RedirectCx, Text<String>);

impl ReusableEndpoint<RedirectCx> for RedirectEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, RedirectCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(self.name, Method::GET, self.path, self.policy.clone(), None);
        plan.endpoint.route = concord_core::internal::ResolvedRoute::new(
            http::uri::Scheme::HTTP,
            self.host,
            self.path,
        );
        Ok(plan)
    }
}

fn redirect_policy() -> ResolvedPolicy {
    super::common::auth_policy(concord_core::advanced::AuthPlacement::Bearer)
}

#[tokio::test]
async fn finalized_request_metadata_and_materialization_are_preserved_through_a_custom_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
    let sent = transport.clone();
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    let endpoint = TransportContractEndpoint::bytes(
        "TransportContract",
        Method::PUT,
        "/transport/contract",
        request_contract_policy(),
    );

    let handle = tokio::spawn(async move { client.request(endpoint).execute_raw().await });
    sent.wait_for_sends(1).await;

    let requests = sent.requests().await;
    sent.release_all();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.meta.endpoint, "TransportContract");
    assert_eq!(request.meta.method, Method::PUT);
    assert_eq!(request.meta.attempt, 0);
    assert_eq!(request.meta.page_index, 0);
    assert_eq!(request.url.scheme(), "https");
    assert_eq!(request.url.host_str(), Some("example.com"));
    assert_eq!(request.url.path(), "/transport/contract");
    assert_query_pairs(
        &request.url,
        &[("policy", "one"), ("policy", "two"), ("mode", "contract")],
    );
    assert_header_values(&request.headers, "x-repeat", &["first", "second"]);
    assert_header_value(&request.headers, "X-SINGLE", "singleton");
    assert_header_value(&request.headers, CONTENT_TYPE.as_str(), "application/json");
    assert_eq!(
        request.headers.get_all(CONTENT_TYPE).iter().count(),
        1,
        "content-type should remain a singleton header"
    );
    assert_eq!(request.timeout, Some(Duration::from_millis(123)));
    assert!(request.body.is_bytes());
    assert_bytes_body(&request.body, br#"{"transport":true}"#);

    let raw = handle.await.expect("request task should complete");
    assert_eq!(
        raw.expect("transport request should succeed").status,
        StatusCode::OK
    );
}

#[cfg(feature = "transport-reqwest")]
#[tokio::test]
async fn default_reqwest_transport_does_not_follow_redirects_or_forward_auth_material() {
    let _guard = redirect_test_lock().lock().await;
    let first_requests = Arc::new(Mutex::new(Vec::new()));
    let second_requests = Arc::new(Mutex::new(Vec::new()));
    let first_hits = Arc::new(AtomicUsize::new(0));
    let second_hits = Arc::new(AtomicUsize::new(0));
    let first_shutdown = Arc::new(AtomicBool::new(false));
    let second_shutdown = Arc::new(AtomicBool::new(false));
    let redirect_location = format!("http://127.0.0.1:{REDIRECT_CUSTOM_TARGET_PORT}/final");

    let first_server = spawn_http_server(
        REDIRECT_DEFAULT_PORT,
        first_requests.clone(),
        first_hits.clone(),
        first_shutdown.clone(),
        http_response(
            StatusCode::FOUND,
            &[("Location", redirect_location.clone())],
            "",
        ),
        true,
    );
    let second_server = spawn_http_server(
        REDIRECT_CUSTOM_TARGET_PORT,
        second_requests.clone(),
        second_hits.clone(),
        second_shutdown.clone(),
        http_response(StatusCode::OK, &[], "redirected"),
        false,
    );

    std::thread::sleep(Duration::from_millis(50));

    let client = ApiClient::<RedirectCx, _>::new(
        (),
        TestAuthVars {
            token: Some("redirect-secret".to_string()),
            identity: "anon",
        },
    );
    let endpoint = RedirectEndpoint {
        name: "RedirectSafety",
        path: "/protected",
        policy: redirect_policy(),
        host: REDIRECT_DEFAULT_HOST,
    };

    let err = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect_err("redirect response should stop at the first response");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.http_status(), Some(StatusCode::FOUND));
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get("location"))
            .and_then(|value| value.to_str().ok()),
        Some(redirect_location.as_str())
    );
    assert_eq!(first_hits.load(Ordering::SeqCst), 1);
    assert_eq!(second_hits.load(Ordering::SeqCst), 0);
    let first_request = first_requests.lock().await;
    assert_eq!(first_request.len(), 1);
    let first_request = &first_request[0];
    assert!(first_request.contains("GET /protected HTTP/1.1"));
    assert!(
        first_request
            .to_ascii_lowercase()
            .contains("authorization: bearer redirect-secret"),
        "default reqwest transport should carry auth on the original request"
    );

    first_shutdown.store(true, Ordering::SeqCst);
    second_shutdown.store(true, Ordering::SeqCst);
    first_server
        .join()
        .expect("first redirect server should stop");
    second_server
        .join()
        .expect("second redirect server should stop");

    assert!(second_requests.lock().await.is_empty());
}

#[cfg(feature = "transport-reqwest")]
#[tokio::test]
async fn with_reqwest_client_keeps_caller_owned_redirect_policy() {
    let _guard = redirect_test_lock().lock().await;
    let first_requests = Arc::new(Mutex::new(Vec::new()));
    let second_requests = Arc::new(Mutex::new(Vec::new()));
    let first_hits = Arc::new(AtomicUsize::new(0));
    let second_hits = Arc::new(AtomicUsize::new(0));
    let first_shutdown = Arc::new(AtomicBool::new(false));
    let second_shutdown = Arc::new(AtomicBool::new(false));
    let redirect_location = format!("http://127.0.0.1:{REDIRECT_TARGET_PORT}/final");

    let first_server = spawn_http_server(
        REDIRECT_CUSTOM_PORT,
        first_requests.clone(),
        first_hits.clone(),
        first_shutdown.clone(),
        http_response(
            StatusCode::FOUND,
            &[("Location", redirect_location.clone())],
            "",
        ),
        true,
    );
    let second_server = spawn_http_server(
        REDIRECT_TARGET_PORT,
        second_requests.clone(),
        second_hits.clone(),
        second_shutdown.clone(),
        http_response(StatusCode::OK, &[], "redirected"),
        true,
    );

    std::thread::sleep(Duration::from_millis(50));

    let reqwest_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .expect("caller-owned client should build");
    let client = ApiClient::<RedirectCx, _>::with_reqwest_client(
        (),
        TestAuthVars {
            token: Some("redirect-secret".to_string()),
            identity: "anon",
        },
        reqwest_client,
    );
    let endpoint = RedirectEndpoint {
        name: "RedirectCallerOwned",
        path: "/protected",
        policy: redirect_policy(),
        host: REDIRECT_CUSTOM_HOST,
    };

    let response = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect("caller-owned reqwest client should follow redirects");

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body.as_ref(), b"redirected");
    assert_eq!(first_hits.load(Ordering::SeqCst), 1);
    assert_eq!(second_hits.load(Ordering::SeqCst), 1);
    assert_eq!(first_requests.lock().await.len(), 1);
    assert_eq!(second_requests.lock().await.len(), 1);

    first_shutdown.store(true, Ordering::SeqCst);
    second_shutdown.store(true, Ordering::SeqCst);
    first_server
        .join()
        .expect("first redirect server should stop");
    second_server
        .join()
        .expect("second redirect server should stop");
}

#[tokio::test]
async fn stream_request_body_remains_non_replayable_at_the_transport_boundary()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "stream-ok")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TransportContractEndpoint::stream(
        "TransportStream",
        Method::POST,
        "/transport/stream",
        request_contract_policy(),
    );

    let response = client.request(endpoint).execute_raw().await?;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.meta.endpoint, "TransportStream");
    assert_eq!(request.meta.method, Method::POST);
    assert_eq!(request.meta.attempt, 0);
    assert_eq!(request.meta.page_index, 0);
    assert!(request.body.is_stream());
    assert!(request.body.as_bytes().is_none());
    assert_header_value(
        &request.headers,
        CONTENT_TYPE.as_str(),
        "application/octet-stream",
    );
    Ok(())
}

#[tokio::test]
async fn response_metadata_is_preserved_on_raw_and_decoded_surfaces() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::ACCEPTED, "decoded-value"),
            MockResponse::text(StatusCode::OK, "raw-value"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TransportContractEndpoint::empty(
        "TransportResponse",
        Method::GET,
        "/transport/response",
        ResolvedPolicy::default(),
    );

    let decoded = client.request(endpoint.clone()).response().await?;
    assert_eq!(decoded.status(), StatusCode::ACCEPTED);
    assert_eq!(decoded.headers()[CONTENT_TYPE], "text/plain");
    assert_eq!(
        decoded.url().as_str(),
        "https://example.com/transport/response"
    );
    assert_eq!(decoded.meta().endpoint, "TransportResponse");
    assert_eq!(decoded.meta().method, Method::GET);
    assert_eq!(decoded.meta().attempt, 0);
    assert_eq!(decoded.meta().page_index, 0);
    assert_eq!(decoded.value(), "decoded-value");

    let raw = client.request(endpoint).execute_raw().await?;
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(raw.headers[CONTENT_TYPE], "text/plain");
    assert_eq!(raw.url.as_str(), "https://example.com/transport/response");
    assert_eq!(raw.meta.endpoint, "TransportResponse");
    assert_eq!(raw.meta.method, Method::GET);
    assert_eq!(raw.meta.attempt, 0);
    assert_eq!(raw.meta.page_index, 0);
    assert_eq!(raw.body, Bytes::from_static(b"raw-value"));
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn transport_error_preserves_context_and_redacts_request_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![MockOutcome::TransportError(TransportErrorKind::Timeout)],
    );
    let sent = transport.clone();
    let mut policy = request_contract_policy();
    policy
        .headers
        .insert("x-sensitive", HeaderValue::from_static(REQUEST_SENTINEL));
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TransportContractEndpoint::empty(
        "TransportFailure",
        Method::GET,
        "/transport/failure",
        policy,
    );

    let err = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect_err("transport failure should surface");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Timeout);
    assert_eq!(err.context().endpoint, "TransportFailure");
    assert_eq!(err.context().method, Method::GET);
    match &err {
        ApiClientError::Transport { source, .. } => {
            assert_eq!(source.kind(), TransportErrorKind::Timeout);
        }
        other => panic!("expected transport error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_header_value(&requests[0].headers, "X-SENSITIVE", REQUEST_SENTINEL);
    assert_error_chain_does_not_contain_any(&err, &[REQUEST_SENTINEL]);
}

#[tokio::test]
async fn retry_attempts_keep_transport_metadata_stable_for_replayable_bodies()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let policy = retry_contract_policy();
    let client = client(TestAuthVars::default(), transport);
    let endpoint =
        TransportContractEndpoint::bytes("TransportRetry", Method::GET, "/transport/retry", policy);

    let decoded = client
        .execute_plan::<Text<String>>(endpoint.request_plan_for_retry())
        .await?;
    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, "TransportRetry");
    assert_eq!(requests[1].meta.endpoint, "TransportRetry");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[1].meta.method, Method::GET);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[1].meta.page_index, 0);
    assert_eq!(requests[0].url, requests[1].url);
    assert_eq!(requests[0].headers, requests[1].headers);
    assert_bytes_body(&requests[0].body, br#"{"transport":true}"#);
    assert_bytes_body(&requests[1].body, br#"{"transport":true}"#);
    Ok(())
}

impl TransportContractEndpoint {
    fn request_plan_for_retry(&self) -> RequestPlan {
        let mut plan = request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            None,
        );
        if let TransportBody::Bytes(body) = &self.body {
            plan.endpoint.body = BodyPlan::Encoded {
                content_type: Some(HeaderValue::from_static("application/json")),
                format: Format::Text,
            };
            plan.args = RequestArgs::with_body_bytes(body.clone());
            plan.replayability = Replayability::Replayable;
        }
        plan
    }
}
