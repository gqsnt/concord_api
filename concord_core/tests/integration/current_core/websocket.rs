use super::common::{
    ObservationRuntimeHooks, RecordingRateLimiter, TestAuthVars, TestCx,
    assert_events_do_not_contain, decode_string, request_plan,
};
use bytes::Bytes;
use concord_core::advanced::{
    CodecError, JsonWebSocket, RetryDecision, RetryPolicy, Transport, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse, TransportWebSocket, TransportWsClose,
    TransportWsMessage, WebSocketClient, WebSocketEndpoint,
};
use concord_core::internal::{BodyPlan, PaginationPlan, RequestPlan, ResolvedPolicy};
use concord_core::prelude::{ApiClient, ApiClientError, Endpoint};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WsOut {
    id: u64,
    msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WsIn {
    id: u64,
    msg: String,
}

#[derive(Clone)]
struct WsEndpoint {
    policy: ResolvedPolicy,
    pagination: Option<PaginationPlan>,
    body: BodyPlan,
    no_content: bool,
}

impl Default for WsEndpoint {
    fn default() -> Self {
        Self {
            policy: ResolvedPolicy::default(),
            pagination: None,
            body: BodyPlan::None,
            no_content: false,
        }
    }
}

impl Endpoint<TestCx> for WsEndpoint {
    type Response = WebSocketClient<WsOut, WsIn>;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "WebSocket",
            Method::GET,
            "/ws",
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        );
        plan.endpoint.body = self.body.clone();
        plan.endpoint.response.accept = None;
        plan.endpoint.response.no_content = self.no_content;
        Ok(plan)
    }
}

impl WebSocketEndpoint<TestCx> for WsEndpoint {
    type Out = WsOut;
    type In = WsIn;
    type Codec = JsonWebSocket;
}

#[derive(Clone)]
struct MockWebSocket {
    events: Arc<Mutex<Vec<String>>>,
    sent: Arc<Mutex<Vec<TransportWsMessage>>>,
    incoming: Arc<Mutex<VecDeque<TransportWsMessage>>>,
    send_error: Option<TransportErrorKind>,
    next_error: Option<TransportErrorKind>,
    close_error: Option<TransportErrorKind>,
}

impl MockWebSocket {
    fn new(events: Arc<Mutex<Vec<String>>>, incoming: Vec<TransportWsMessage>) -> Self {
        Self {
            events,
            sent: Arc::new(Mutex::new(Vec::new())),
            incoming: Arc::new(Mutex::new(incoming.into())),
            send_error: None,
            next_error: None,
            close_error: None,
        }
    }

    fn with_send_error(mut self, kind: TransportErrorKind) -> Self {
        self.send_error = Some(kind);
        self
    }

    fn with_next_error(mut self, kind: TransportErrorKind) -> Self {
        self.next_error = Some(kind);
        self
    }

    fn with_close_error(mut self, kind: TransportErrorKind) -> Self {
        self.close_error = Some(kind);
        self
    }
}

impl TransportWebSocket for MockWebSocket {
    fn send<'a>(
        &'a mut self,
        msg: TransportWsMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        let events = self.events.clone();
        let sent = self.sent.clone();
        let send_error = self.send_error;
        Box::pin(async move {
            events.lock().await.push("socket_send".to_string());
            if let Some(kind) = send_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("SECRET_WS_SENTINEL"),
                ));
            }
            sent.lock().await.push(msg);
            Ok(())
        })
    }

    fn next<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportWsMessage>, TransportError>> + Send + 'a>>
    {
        let events = self.events.clone();
        let incoming = self.incoming.clone();
        let next_error = self.next_error;
        Box::pin(async move {
            events.lock().await.push("socket_next".to_string());
            if let Some(kind) = next_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("SECRET_WS_SENTINEL"),
                ));
            }
            Ok(incoming.lock().await.pop_front())
        })
    }

    fn close<'a>(
        &'a mut self,
        _close: Option<TransportWsClose>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        let events = self.events.clone();
        let close_error = self.close_error;
        Box::pin(async move {
            events.lock().await.push("socket_close".to_string());
            if let Some(kind) = close_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("SECRET_WS_SENTINEL"),
                ));
            }
            Ok(())
        })
    }
}

#[derive(Clone)]
struct WsTransport {
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<super::common::CapturedTransportRequest>>>,
    connect_count: Arc<AtomicUsize>,
    socket: MockWebSocket,
    status: Option<StatusCode>,
    headers: HeaderMap,
    connect_error: Option<TransportErrorKind>,
}

impl WsTransport {
    fn success(events: Arc<Mutex<Vec<String>>>, incoming: Vec<TransportWsMessage>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            connect_count: Arc::new(AtomicUsize::new(0)),
            socket: MockWebSocket::new(events.clone(), incoming),
            events,
            status: Some(StatusCode::SWITCHING_PROTOCOLS),
            headers: HeaderMap::new(),
            connect_error: None,
        }
    }

    fn failing(events: Arc<Mutex<Vec<String>>>, kind: TransportErrorKind) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            connect_count: Arc::new(AtomicUsize::new(0)),
            socket: MockWebSocket::new(events.clone(), Vec::new()),
            events,
            status: None,
            headers: HeaderMap::new(),
            connect_error: Some(kind),
        }
    }

    fn with_socket_send_error(mut self, kind: TransportErrorKind) -> Self {
        self.socket = self.socket.clone().with_send_error(kind);
        self
    }

    fn with_socket_next_error(mut self, kind: TransportErrorKind) -> Self {
        self.socket = self.socket.clone().with_next_error(kind);
        self
    }

    fn with_socket_close_error(mut self, kind: TransportErrorKind) -> Self {
        self.socket = self.socket.clone().with_close_error(kind);
        self
    }

    async fn connect_count(&self) -> usize {
        self.connect_count.load(Ordering::SeqCst)
    }

    async fn requests(&self) -> Vec<super::common::CapturedTransportRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for WsTransport {
    fn send(
        &self,
        _req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        Box::pin(async move {
            Err(TransportError::with_kind(
                TransportErrorKind::Other,
                std::io::Error::other("unexpected HTTP send"),
            ))
        })
    }

    fn connect_websocket(
        &self,
        req: TransportRequest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        concord_core::transport::TransportWebSocketConnection,
                        TransportError,
                    >,
                > + Send,
        >,
    > {
        let events = self.events.clone();
        let requests = self.requests.clone();
        let connect_count = self.connect_count.clone();
        let socket = self.socket.clone();
        let status = self.status;
        let response_headers = self.headers.clone();
        let connect_error = self.connect_error;
        Box::pin(async move {
            connect_count.fetch_add(1, Ordering::SeqCst);
            events.lock().await.push("connect".to_string());
            let concord_core::transport::TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                transport_auth,
                extensions,
            } = req;
            requests
                .lock()
                .await
                .push(super::common::CapturedTransportRequest {
                    meta: meta.clone(),
                    url: url.clone(),
                    headers: headers.clone(),
                    body,
                    timeout,
                    rate_limit: rate_limit.clone(),
                    transport_auth: transport_auth.clone(),
                    extensions: extensions.clone(),
                });
            if let Some(kind) = connect_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("SECRET_WS_SENTINEL"),
                ));
            }
            Ok(concord_core::transport::TransportWebSocketConnection {
                meta,
                url,
                status,
                headers: response_headers,
                rate_limit,
                socket: Box::new(socket),
            })
        })
    }
}

struct RetryOncePolicy;

impl RetryPolicy for RetryOncePolicy {
    fn max_retries(&self) -> u32 {
        3
    }

    fn should_retry(&self, _ctx: &concord_core::advanced::RetryContext<'_>) -> RetryDecision {
        RetryDecision::Retry
    }
}

fn websocket_plan(
    body: BodyPlan,
    pagination: Option<PaginationPlan>,
    no_content: bool,
) -> RequestPlan {
    let mut plan = request_plan(
        "WebSocket",
        Method::GET,
        "/ws",
        ResolvedPolicy::default(),
        pagination,
        decode_string,
    );
    plan.endpoint.body = body;
    plan.endpoint.response.accept = None;
    plan.endpoint.response.no_content = no_content;
    plan
}

fn websocket_client(
    transport: WsTransport,
    events: Arc<Mutex<Vec<String>>>,
) -> ApiClient<TestCx, WsTransport> {
    ApiClient::with_transport((), TestAuthVars::default(), transport)
        .with_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())))
        .with_rate_limiter(Arc::new(RecordingRateLimiter::new(events)))
}

#[tokio::test]
async fn pending_request_execute_websocket_maps_scheme_and_orders_events()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new());
    let api = websocket_client(transport.clone(), events.clone());
    let client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    assert_eq!(client.url().scheme(), "wss");
    assert_eq!(client.status(), Some(StatusCode::SWITCHING_PROTOCOLS));
    assert_eq!(transport.connect_count().await, 1);

    let captured = transport.requests().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].url.scheme(), "wss");
    let recorded = events.lock().await.clone();
    let rate_acquire = recorded
        .iter()
        .position(|event| event == "rate_acquire")
        .expect("rate limit should be acquired");
    let connect = recorded
        .iter()
        .position(|event| event == "connect")
        .expect("connect should be attempted");
    assert!(rate_acquire < connect, "{recorded:?}");
    assert_events_do_not_contain(&events, &["SECRET_WS_SENTINEL"]).await;
    let rendered = format!("{client:?}");
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn websocket_send_and_receive_json_text_and_binary() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(
        events.clone(),
        vec![
            TransportWsMessage::Ping(Bytes::from_static(b"ping")),
            TransportWsMessage::Text(r#"{"id":2,"msg":"hello"}"#.to_string()),
            TransportWsMessage::Binary(Bytes::from_static(br#"{"id":3,"msg":"world"}"#)),
            TransportWsMessage::Close(Some(TransportWsClose {
                code: 1000,
                reason: "done".to_string(),
            })),
        ],
    );
    let api = websocket_client(transport.clone(), events.clone());
    let mut client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    client
        .send(WsOut {
            id: 1,
            msg: "outbound".to_string(),
        })
        .await?;

    let sent = transport.socket.sent.lock().await.clone();
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        TransportWsMessage::Text(text) => {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(
                value,
                serde_json::json!({
                    "id": 1,
                    "msg": "outbound"
                })
            );
        }
        other => panic!("expected text websocket frame, got {other:?}"),
    }

    let first = client.next().await?.expect("first event");
    assert_eq!(
        first,
        WsIn {
            id: 2,
            msg: "hello".to_string()
        }
    );

    let second = client.next().await?.expect("second event");
    assert_eq!(
        second,
        WsIn {
            id: 3,
            msg: "world".to_string()
        }
    );

    assert_eq!(client.next().await?, None);
    assert!(format!("{client:?}").contains("WebSocketClient"));
    Ok(())
}

#[tokio::test]
async fn websocket_codec_errors_are_body_safe() -> Result<(), ApiClientError> {
    #[derive(Clone)]
    struct UnsafeCodec;

    impl concord_core::advanced::WebSocketCodec<WsOut, WsIn> for UnsafeCodec {
        fn encode(_msg: WsOut) -> Result<TransportWsMessage, CodecError> {
            Err(CodecError::new("SECRET_WS_SENTINEL"))
        }

        fn decode(_msg: TransportWsMessage) -> Result<Option<WsIn>, CodecError> {
            Err(CodecError::new("SECRET_WS_SENTINEL"))
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new());
    let api = websocket_client(transport, events);
    let plan = WsEndpoint::default().plan(&api.plan_context())?;
    let mut client = api
        .execute_plan_websocket::<WsOut, WsIn, UnsafeCodec>(plan)
        .await?;

    let err = client
        .send(WsOut {
            id: 1,
            msg: "hello".to_string(),
        })
        .await
        .expect_err("encode should fail");
    let rendered = format!("{err:?}\n{err}");
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));

    Ok(())
}

#[tokio::test]
async fn websocket_send_transport_errors_are_body_safe() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new())
        .with_socket_send_error(TransportErrorKind::Other);
    let api = websocket_client(transport, events);
    let mut client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    let err = client
        .send(WsOut {
            id: 1,
            msg: "SECRET_WS_SENTINEL".to_string(),
        })
        .await
        .expect_err("send should fail");
    let rendered = format!("{err:?}\n{err}");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn websocket_receive_transport_errors_are_body_safe() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new())
        .with_socket_next_error(TransportErrorKind::Other);
    let api = websocket_client(transport, events);
    let mut client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    let err = client.next().await.expect_err("receive should fail");
    let rendered = format!("{err:?}\n{err}");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn websocket_close_transport_errors_are_body_safe() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new())
        .with_socket_close_error(TransportErrorKind::Other);
    let api = websocket_client(transport, events);
    let mut client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    let err = client.close().await.expect_err("close should fail");
    let rendered = format!("{err:?}\n{err}");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn websocket_inbound_decode_errors_are_body_safe() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(
        events.clone(),
        vec![TransportWsMessage::Text(
            r#"{"id":1,"msg":"SECRET_WS_SENTINEL""#.to_string(),
        )],
    );
    let api = websocket_client(transport, events);
    let mut client = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await?;

    let err = client.next().await.expect_err("decode should fail");
    let rendered = format!("{err:?}\n{err}");
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn websocket_rejects_http_bodies_and_policy_violations_before_connect()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new());
    let api = websocket_client(transport.clone(), events.clone());

    for body in [
        BodyPlan::Encoded {
            content_type: Some(HeaderValue::from_static("application/json")),
            format: concord_core::internal::Format::Text,
        },
        BodyPlan::RawStream {
            content_type: HeaderValue::from_static("application/octet-stream"),
        },
        BodyPlan::Records {
            content_type: HeaderValue::from_static("application/x-ndjson"),
            format: concord_core::internal::Format::Text,
        },
        BodyPlan::Multipart {
            content_type: HeaderValue::from_static("multipart/form-data; boundary=abc"),
            format: concord_core::internal::Format::Text,
        },
    ] {
        let plan = websocket_plan(body, None, false);
        let err = api
            .execute_plan_websocket::<WsOut, WsIn, JsonWebSocket>(plan)
            .await
            .expect_err("body plans must be rejected");
        assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    }

    let pagination_plan = websocket_plan(
        BodyPlan::None,
        Some(PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
            page: 1,
            per_page: 10,
        }),
        false,
    );
    let err = api
        .execute_plan_websocket::<WsOut, WsIn, JsonWebSocket>(pagination_plan)
        .await
        .expect_err("pagination must be rejected");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));

    let no_content_plan = websocket_plan(BodyPlan::None, None, true);
    let err = api
        .execute_plan_websocket::<WsOut, WsIn, JsonWebSocket>(no_content_plan)
        .await
        .expect_err("no-content must be rejected");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));

    assert_eq!(transport.connect_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn websocket_rejects_retry_policy_before_connect() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::success(events.clone(), Vec::new());
    let api = websocket_client(transport.clone(), events.clone());

    let mut plan = websocket_plan(BodyPlan::None, None, false);
    plan.endpoint.policy.retry =
        concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            ..Default::default()
        });

    let err = api
        .execute_plan_websocket::<WsOut, WsIn, JsonWebSocket>(plan)
        .await
        .expect_err("retry-enabled websocket plan must be rejected");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    let rendered = format!("{err:?}\n{err}");
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    assert_eq!(transport.connect_count().await, 0);
    assert_events_do_not_contain(&events, &["rate_acquire", "connect"]).await;
    Ok(())
}

#[tokio::test]
async fn websocket_connect_failure_is_not_retried() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = WsTransport::failing(events.clone(), TransportErrorKind::Connect);
    let api =
        websocket_client(transport.clone(), events).with_retry_policy(Arc::new(RetryOncePolicy));
    let err = api
        .request(WsEndpoint::default())
        .execute_websocket()
        .await
        .expect_err("connect failure should not retry");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    let rendered = format!("{err:?}\n{err}");
    assert!(!rendered.contains("SECRET_WS_SENTINEL"));
    assert_eq!(transport.connect_count().await, 1);
    Ok(())
}
