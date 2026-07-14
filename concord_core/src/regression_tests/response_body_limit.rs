use super::common::{
    MockResponse, NativeMockHarness, NativeMockReply, ObservationRateLimiter,
    ObservationRuntimeHooks, SafeRecordingDebugSink, TestAuthVars, TestCx, TextEndpoint,
    buffered_endpoint_response_terminal, client, configure_runtime, execute_buffered,
};
use crate::regression_tests::test_api::{
    EndpointMeta, EndpointPlan, PreparedBody, RegressionEndpoint, RegressionPlanContext,
    RegressionReusableEndpoint, RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute,
    ResponsePlan,
};
use crate::regression_tests::test_api::{RawStreamResponse, ResponseEntity};
use bytes::Bytes;
use concord_core::advanced::{
    CodecError, DecodeContext, OctetStream, ResponseCodec, StreamResponse, TextContentType,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use http::{HeaderValue, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

fn assert_limit(error: &ApiClientError, limit: usize) {
    assert!(
        matches!(
            error,
            ApiClientError::ResponseBodyLimitExceeded { limit: actual, .. }
                | ApiClientError::ResponseTooLarge { limit: actual, .. }
                if *actual == limit
        ),
        "expected response limit {limit}, got {error:?}"
    );
}

#[derive(Clone)]
pub(super) struct ByteBodyEndpoint {
    pub body: Bytes,
}

impl RegressionEndpoint<TestCx> for ByteBodyEndpoint {
    type Response = String;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, concord_core::prelude::Text<String>>(client, plan)
    }
}

buffered_endpoint_response_terminal!(
    ByteBodyEndpoint,
    TestCx,
    concord_core::prelude::Text<String>
);

impl RegressionReusableEndpoint<TestCx> for ByteBodyEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "ByteBody",
                    method: Method::POST,
                    idempotent: false,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTP, "example.com", "/byte-body"),
                policy: ResolvedPolicy::default(),
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: crate::regression_tests::test_api::Format::Text,
                },
                pagination: None,
            },
            body: PreparedBody::reusable_bytes(
                self.body.clone(),
                Some(HeaderValue::from_static("text/plain")),
            ),
            overrides: RequestOverrides::default(),
        })
    }
}

#[derive(Clone)]
struct StreamEndpoint;

impl RegressionEndpoint<TestCx> for StreamEndpoint {
    type Response = StreamResponse<OctetStream>;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        <RawStreamResponse<OctetStream> as ResponseEntity>::execute(client, plan)
    }
}

impl RegressionReusableEndpoint<TestCx> for StreamEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "StreamLimit",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTP, "example.com", "/stream-limit"),
                policy: ResolvedPolicy::default(),
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("application/octet-stream")),
                    no_content: false,
                    format: crate::regression_tests::test_api::Format::Binary,
                },
                pagination: None,
            },
            body: PreparedBody::empty(),
            overrides: RequestOverrides::default(),
        })
    }
}

static DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);

struct CountingDecode;

impl ResponseCodec for CountingDecode {
    type Value = String;
    type Content = TextContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        String::from_utf8(bytes.to_vec())
            .map_err(|error| CodecError::with_source("text decode failed", error))
    }
}

#[derive(Clone)]
struct CountingDecodeEndpoint;

impl RegressionEndpoint<TestCx> for CountingDecodeEndpoint {
    type Response = String;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, CountingDecode>(client, plan)
    }
}

buffered_endpoint_response_terminal!(CountingDecodeEndpoint, TestCx, CountingDecode);

impl RegressionReusableEndpoint<TestCx> for CountingDecodeEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = super::common::request_plan(
            "CountingDecode",
            Method::GET,
            "/counting-decode",
            ResolvedPolicy::default(),
            None,
        );
        plan.endpoint.response.accept = Some(HeaderValue::from_static("text/plain"));
        Ok(plan)
    }
}

#[tokio::test]
async fn response_body_limit_authoritative_content_length_over_limit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "123456789").with_content_length(Some(9))],
    );
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("authoritative Content-Length must enforce the limit");
    assert_limit(&error, 4);
}

#[tokio::test]
async fn response_body_limit_zero_accepts_empty_and_rejects_delivered_data() {
    let empty_events = Arc::new(Mutex::new(Vec::new()));
    let empty_harness = NativeMockHarness::new(
        empty_events,
        vec![MockResponse::text(StatusCode::OK, Bytes::new())],
    );
    let mut empty_client = client(TestAuthVars::default(), empty_harness);
    empty_client.configure(|config| {
        config.max_response_body_bytes(0);
    });
    let empty = empty_client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect("empty body is valid at a zero-byte limit");
    assert_eq!(empty.value(), "");

    let nonempty_events = Arc::new(Mutex::new(Vec::new()));
    let nonempty_harness = NativeMockHarness::new(
        nonempty_events,
        vec![MockResponse::text(StatusCode::OK, "x")],
    );
    let mut nonempty_client = client(TestAuthVars::default(), nonempty_harness);
    nonempty_client.configure(|config| {
        config.max_response_body_bytes(0);
    });
    let error = nonempty_client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("any delivered byte must exceed a zero-byte limit");
    assert_limit(&error, 0);
}

#[tokio::test]
async fn response_body_limit_unknown_length_exceeds_during_collection() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![Bytes::from_static(b"abcd"), Bytes::from_static(b"e")]),
        ],
    );
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("unknown-length collection must stop at limit plus one");
    assert_limit(&error, 4);
}

#[tokio::test]
async fn response_body_limit_exact_boundary_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(events, vec![MockResponse::text(StatusCode::OK, "abcd")]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let response = client.request(TextEndpoint::default()).response().await?;
    assert_eq!(response.value(), "abcd");
    Ok(())
}

#[tokio::test]
async fn response_body_limit_plus_one_fails() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(events, vec![MockResponse::text(StatusCode::OK, "abcde")]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("limit plus one must fail");

    assert_limit(&error, 4);
}

#[tokio::test]
async fn response_body_limit_stream_fails_before_excess_delivery() {
    const EXCESS: &[u8] = b"EXCESS_SENTINEL";
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut response = MockResponse::text(StatusCode::OK, Bytes::new()).with_chunks(vec![
        Bytes::from_static(b"abcd"),
        Bytes::from_static(EXCESS),
    ]);
    response.headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/octet-stream"),
    );
    let harness = NativeMockHarness::new(events, vec![response]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_stream_response_body_bytes(4);
    });

    let mut response = client
        .request(StreamEndpoint)
        .execute()
        .await
        .expect("stream head succeeds");
    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"abcd".as_slice())
    );
    let error = response
        .next_chunk()
        .await
        .expect_err("the excess chunk must fail before delivery");
    assert_limit(&error, 4);
    assert!(!format!("{error:?}").contains("EXCESS_SENTINEL"));
}

#[tokio::test]
async fn response_body_limit_stream_zero_accepts_empty_and_rejects_data() {
    let empty_events = Arc::new(Mutex::new(Vec::new()));
    let mut empty_harness = MockResponse::text(StatusCode::OK, Bytes::new());
    empty_harness.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let empty_harness = NativeMockHarness::new(empty_events, vec![empty_harness]);
    let mut empty_client = client(TestAuthVars::default(), empty_harness);
    empty_client.configure(|config| {
        config.max_stream_response_body_bytes(0);
    });
    let mut empty = empty_client
        .request(StreamEndpoint)
        .execute()
        .await
        .expect("empty stream head succeeds at a zero-byte limit");
    assert_eq!(empty.next_chunk().await.expect("EOF"), None);

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut nonempty = MockResponse::text(StatusCode::OK, Bytes::new())
        .with_content_length(None)
        .with_chunks(vec![Bytes::from_static(b"x")]);
    nonempty.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let harness = NativeMockHarness::new(events, vec![nonempty]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_stream_response_body_bytes(0);
    });
    let mut response = client
        .request(StreamEndpoint)
        .execute()
        .await
        .expect("nonempty stream head succeeds before polling");
    let error = response
        .next_chunk()
        .await
        .expect_err("data must exceed a zero-byte stream limit");
    assert_limit(&error, 0);
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn response_body_limit_counts_decompressed_output_bytes() {
    const GZIP_HELLO: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x0a, 0xcb, 0x48, 0xcd, 0xc9, 0xc9,
        0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
    ];
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut response = MockResponse::text(StatusCode::OK, Bytes::from_static(GZIP_HELLO))
        .with_content_length(None);
    response.headers.insert(
        http::header::CONTENT_ENCODING,
        HeaderValue::from_static("gzip"),
    );
    let harness = NativeMockHarness::new(events, vec![response]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("decompressed output must be counted against the response limit");
    assert_limit(&error, 4);
}

#[tokio::test]
async fn response_body_limit_terminal_body_producer_failure_is_typed_and_redacted() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::from_native_replies(
        events,
        [NativeMockReply::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            )
            .with_response_steps([
                super::common::native_mock::ResponseStep::Chunk(Bytes::from_static(b"abc")),
                super::common::native_mock::ResponseStep::Disconnect,
            ])],
    );
    let client = client(TestAuthVars::default(), harness);

    let mut response = client
        .request(StreamEndpoint)
        .execute()
        .await
        .expect("stream head succeeds");
    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"abc".as_slice())
    );
    let error = response
        .next_chunk()
        .await
        .expect_err("disconnect must become a terminal body error");
    assert!(matches!(error, ApiClientError::ResponseBody { .. }));
    assert!(!format!("{error:?}").contains("abc"));
}

#[tokio::test]
async fn response_body_limit_buffered_partial_body_failure_precedes_decode() {
    DECODE_CALLS.store(0, Ordering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::from_native_replies(
        events,
        [NativeMockReply::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            )
            .with_response_steps([
                super::common::native_mock::ResponseStep::Chunk(Bytes::from_static(b"abc")),
                super::common::native_mock::ResponseStep::Disconnect,
            ])],
    );
    let client = client(TestAuthVars::default(), harness);

    let error = client
        .request(CountingDecodeEndpoint)
        .response()
        .await
        .expect_err("buffered collection must retain native body failures");
    assert!(matches!(error, ApiClientError::ResponseBody { .. }));
    assert_eq!(DECODE_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn response_body_limit_prevents_endpoint_decode() {
    DECODE_CALLS.store(0, Ordering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(events, vec![MockResponse::text(StatusCode::OK, "abcde")]);
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(CountingDecodeEndpoint)
        .response()
        .await
        .expect_err("limit failure must precede decode");
    assert_limit(&error, 4);
    assert_eq!(DECODE_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn response_body_limit_redacts_request_and_response_from_all_observers() {
    const REQUEST: &str = "REQUEST_BODY_SENTINEL_DO_NOT_OBSERVE";
    const RESPONSE: &str = "RESPONSE_BODY_SENTINEL_DO_NOT_OBSERVE";
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, RESPONSE)],
    );
    let sent = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));
    client.set_debug_level(DebugLevel::VV);
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.configure(|config| {
        config.max_response_body_bytes(4);
    });

    let error = client
        .request(ByteBodyEndpoint {
            body: Bytes::from_static(REQUEST.as_bytes()),
        })
        .response()
        .await
        .expect_err("response sentinel exceeds the configured limit");

    assert_limit(&error, 4);
    let rendered_error = format!("{error:?}\n{error}");
    let rendered_events = format!("{:?}", events.lock().await.as_slice());
    assert!(!rendered_error.contains(REQUEST));
    assert!(!rendered_error.contains(RESPONSE));
    assert!(!rendered_events.contains(REQUEST));
    assert!(!rendered_events.contains(RESPONSE));
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body.as_bytes().map(Bytes::as_ref),
        Some(REQUEST.as_bytes())
    );
    assert!(!format!("{:?}", requests[0]).contains(REQUEST));
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn response_body_limit_raw_and_decoded_paths_are_equivalent() {
    let decoded_events = Arc::new(Mutex::new(Vec::new()));
    let decoded_harness = NativeMockHarness::new(
        decoded_events,
        vec![MockResponse::text(StatusCode::OK, "abcde")],
    );
    let mut decoded_client = client(TestAuthVars::default(), decoded_harness);
    decoded_client.configure(|config| {
        config.max_response_body_bytes(4);
    });
    let decoded_error = decoded_client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("decoded path enforces response limit");

    let raw_events = Arc::new(Mutex::new(Vec::new()));
    let raw_harness = NativeMockHarness::new(
        raw_events,
        vec![MockResponse::text(StatusCode::OK, "abcde")],
    );
    let mut raw_client = client(TestAuthVars::default(), raw_harness);
    raw_client.configure(|config| {
        config.max_response_body_bytes(4);
    });
    let raw_error = raw_client
        .request(TextEndpoint::default())
        .execute_raw_response()
        .await
        .expect_err("raw path enforces response limit");

    assert_limit(&decoded_error, 4);
    assert_limit(&raw_error, 4);
}

#[tokio::test]
async fn response_body_limit_empty_request_body_executes_as_empty() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(events, vec![MockResponse::text(StatusCode::OK, "empty")]);
    let sent = harness.clone();
    let client = client(TestAuthVars::default(), harness);

    let response = client.request(TextEndpoint::default()).response().await?;
    assert_eq!(response.value(), "empty");
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].body.is_empty());
    Ok(())
}
