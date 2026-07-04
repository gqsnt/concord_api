use super::common::{
    GateTransport, MockOutcome, MockResponse, MockTransport, TestAuthVars, TestCx,
    buffered_endpoint_execute, client, request_plan,
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
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const REQUEST_SENTINEL: &str = "PR22_TRANSPORT_REQUEST_SENTINEL";

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

    let decoded = client
        .request(endpoint.clone())
        .execute_decoded_with::<Text<String>>()
        .await?;
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
