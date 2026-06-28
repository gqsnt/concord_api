use concord_core::advanced::{
    JsonWebSocket, RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, Transport, TransportError, TransportErrorKind,
    TransportRequest, TransportResponse, TransportWebSocket, TransportWebSocketConnection,
    TransportWsClose, TransportWsMessage, WebSocketClient,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientMsg {
    id: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerMsg {
    id: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BufferedResult {
    ok: bool,
}

const WS_SENTINEL: &str = "SECRET_WS_SENTINEL_MUST_NOT_APPEAR";

mod websocket_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client WebSocketHelperApi {
            base "https://example.com"
        }

        WS ConnectDefault
            path ["ws-default"]
            -> WebSocket<ClientMsg, ServerMsg>

        WS ConnectExplicit
            path ["ws-explicit"]
            -> WebSocket<ClientMsg, ServerMsg, JsonWebSocket>

        GET Buffered
            path ["buffered"]
            -> Json<BufferedResult>
    }

    pub(super) use web_socket_helper_api::WebSocketHelperApi;
}

use websocket_helper_contract::WebSocketHelperApi;

#[derive(Clone)]
struct CapturedRequest {
    debug: String,
    method: http::Method,
    url: String,
    body_empty: bool,
}

impl std::fmt::Debug for CapturedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedRequest")
            .field("debug", &self.debug)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("body_empty", &self.body_empty)
            .finish()
    }
}

#[derive(Clone, Default)]
struct MockWebSocket {
    state: Arc<StdMutex<MockWebSocketState>>,
}

#[derive(Default)]
struct MockWebSocketState {
    events: Vec<String>,
    sent: Vec<TransportWsMessage>,
    incoming: VecDeque<TransportWsMessage>,
    closed: bool,
    send_error: Option<String>,
    next_error: Option<String>,
    close_error: Option<String>,
}

impl MockWebSocket {
    fn new(incoming: Vec<TransportWsMessage>) -> Self {
        Self {
            state: Arc::new(StdMutex::new(MockWebSocketState {
                incoming: incoming.into(),
                ..Default::default()
            })),
        }
    }

    fn with_send_error(self, msg: impl Into<String>) -> Self {
        self.state.lock().expect("socket lock").send_error = Some(msg.into());
        self
    }

    fn with_next_error(self, msg: impl Into<String>) -> Self {
        self.state.lock().expect("socket lock").next_error = Some(msg.into());
        self
    }

    fn with_close_error(self, msg: impl Into<String>) -> Self {
        self.state.lock().expect("socket lock").close_error = Some(msg.into());
        self
    }

    fn events(&self) -> Vec<String> {
        self.state.lock().expect("socket lock").events.clone()
    }

    fn sent(&self) -> Vec<TransportWsMessage> {
        self.state.lock().expect("socket lock").sent.clone()
    }
}

impl TransportWebSocket for MockWebSocket {
    fn send<'a>(
        &'a mut self,
        msg: TransportWsMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut guard = state.lock().expect("socket lock");
            guard.events.push("socket_send".to_string());
            if let Some(msg) = guard.send_error.take() {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other(msg),
                ));
            }
            guard.sent.push(msg);
            Ok(())
        })
    }

    fn next<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportWsMessage>, TransportError>> + Send + 'a>>
    {
        let state = self.state.clone();
        Box::pin(async move {
            let mut guard = state.lock().expect("socket lock");
            guard.events.push("socket_next".to_string());
            if let Some(msg) = guard.next_error.take() {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other(msg),
                ));
            }
            Ok(guard.incoming.pop_front())
        })
    }

    fn close<'a>(
        &'a mut self,
        _close: Option<TransportWsClose>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut guard = state.lock().expect("socket lock");
            guard.events.push("socket_close".to_string());
            if let Some(msg) = guard.close_error.take() {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other(msg),
                ));
            }
            guard.closed = true;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct WebSocketTransport {
    events: Arc<StdMutex<Vec<String>>>,
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    connect_count: Arc<AtomicUsize>,
    send_count: Arc<AtomicUsize>,
    status: Option<StatusCode>,
    headers: HeaderMap,
    socket: MockWebSocket,
}

impl WebSocketTransport {
    fn new(status: Option<StatusCode>, socket: MockWebSocket) -> Self {
        Self {
            events: Arc::new(StdMutex::new(Vec::new())),
            requests: Arc::new(StdMutex::new(Vec::new())),
            connect_count: Arc::new(AtomicUsize::new(0)),
            send_count: Arc::new(AtomicUsize::new(0)),
            status,
            headers: HeaderMap::new(),
            socket,
        }
    }

    fn push_event(&self, event: impl Into<String>) {
        self.events.lock().expect("events lock").push(event.into());
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events lock").clone()
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    fn connect_count(&self) -> usize {
        self.connect_count.load(Ordering::SeqCst)
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }
}

impl Transport for WebSocketTransport {
    fn send(
        &self,
        _req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let transport = self.clone();
        Box::pin(async move {
            transport.send_count.fetch_add(1, Ordering::SeqCst);
            transport.push_event("transport_send");
            Err(TransportError::with_kind(
                TransportErrorKind::Other,
                std::io::Error::other("unexpected HTTP send for websocket endpoint"),
            ))
        })
    }

    fn connect_websocket(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportWebSocketConnection, TransportError>> + Send>>
    {
        let transport = self.clone();
        Box::pin(async move {
            transport.connect_count.fetch_add(1, Ordering::SeqCst);
            transport.push_event("connect");
            let debug = format!("{req:?}");
            transport
                .requests
                .lock()
                .expect("requests lock")
                .push(CapturedRequest {
                    debug,
                    method: req.meta.method.clone(),
                    url: req.url.as_str().to_string(),
                    body_empty: req.body.is_empty(),
                });
            Ok(TransportWebSocketConnection {
                meta: req.meta,
                url: req.url,
                status: transport.status,
                headers: transport.headers.clone(),
                rate_limit: req.rate_limit,
                socket: Box::new(transport.socket.clone()),
            })
        })
    }
}

#[derive(Clone, Default)]
struct RecordingRateLimiter {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("events lock")
                .push("rate_limit_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("events lock")
                .push("rate_limit_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[tokio::test]
async fn generated_websocket_execute_websocket_returns_client_without_buffering() {
    let socket = MockWebSocket::new(vec![
        TransportWsMessage::Text(serde_json::to_string(&ServerMsg { id: 2 }).unwrap()),
        TransportWsMessage::Close(None),
    ]);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let mut ws: WebSocketClient<ClientMsg, ServerMsg> = api
        .connect_default()
        .execute_websocket()
        .await
        .expect("websocket connect succeeds");

    assert_eq!(transport.connect_count(), 1);
    assert_eq!(transport.send_count(), 0);
    let request = &transport.requests()[0];
    assert_eq!(request.method, http::Method::GET);
    assert!(request.url.starts_with("wss://example.com/ws-default"));
    assert!(request.body_empty);
    assert!(format!("{ws:?}").contains("WebSocketClient"));
    assert!(!format!("{ws:?}").contains(WS_SENTINEL));
    assert_eq!(ws.status(), Some(StatusCode::SWITCHING_PROTOCOLS));
    assert_eq!(ws.url().scheme(), "wss");
    assert_eq!(ws.meta().endpoint, "ConnectDefault");

    ws.send(ClientMsg { id: 1 }).await.expect("send succeeds");
    assert_eq!(
        socket.sent(),
        vec![TransportWsMessage::Text(
            serde_json::to_string(&ClientMsg { id: 1 }).unwrap()
        )]
    );

    let event = ws.next().await.expect("next succeeds").expect("event");
    assert_eq!(event, ServerMsg { id: 2 });
    assert_eq!(ws.next().await.expect("close frame"), None);
}

#[tokio::test]
async fn generated_websocket_execute_also_returns_client() {
    let socket = MockWebSocket::new(vec![TransportWsMessage::Close(None)]);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let mut ws: WebSocketClient<ClientMsg, ServerMsg> = api
        .connect_explicit()
        .execute()
        .await
        .expect("execute succeeds");

    assert_eq!(transport.connect_count(), 1);
    assert_eq!(transport.send_count(), 0);
    assert_eq!(ws.next().await.expect("close frame"), None);
}

#[tokio::test]
async fn generated_websocket_rate_limit_precedes_connect() {
    let socket = MockWebSocket::new(vec![TransportWsMessage::Close(None)]);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket);
    let events = transport.events.clone();
    let api = WebSocketHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    });

    let _ = api.connect_default().execute_websocket().await.unwrap();

    let events = transport.events();
    let rate_idx = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .expect("rate limit event");
    let connect_idx = events
        .iter()
        .position(|event| event == "connect")
        .expect("connect event");
    assert!(rate_idx < connect_idx, "{events:?}");
}

#[tokio::test]
async fn generated_websocket_response_status_failure_is_body_free() {
    let socket = MockWebSocket::new(Vec::new());
    let transport = WebSocketTransport::new(Some(StatusCode::BAD_REQUEST), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let err = api
        .connect_default()
        .execute_websocket()
        .await
        .expect_err("bad status must fail");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(transport.connect_count(), 1);
    assert_eq!(transport.send_count(), 0);
    assert!(socket.events().is_empty());
    assert!(!format!("{err:?}").contains(WS_SENTINEL));
    assert!(!format!("{err}").contains(WS_SENTINEL));
}

#[tokio::test]
async fn generated_websocket_send_transport_error_is_sanitized() {
    let socket =
        MockWebSocket::new(vec![TransportWsMessage::Close(None)]).with_send_error(WS_SENTINEL);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let mut ws: WebSocketClient<ClientMsg, ServerMsg> = api
        .connect_default()
        .execute_websocket()
        .await
        .expect("websocket connect succeeds");

    let err = ws
        .send(ClientMsg { id: 1 })
        .await
        .expect_err("send must fail");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!format!("{err:?}").contains(WS_SENTINEL));
    assert!(!format!("{err}").contains(WS_SENTINEL));
}

#[tokio::test]
async fn generated_websocket_receive_transport_error_is_sanitized() {
    let socket = MockWebSocket::new(Vec::new()).with_next_error(WS_SENTINEL);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let mut ws: WebSocketClient<ClientMsg, ServerMsg> = api
        .connect_default()
        .execute_websocket()
        .await
        .expect("websocket connect succeeds");

    let err = ws.next().await.expect_err("next must fail");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!format!("{err:?}").contains(WS_SENTINEL));
    assert!(!format!("{err}").contains(WS_SENTINEL));
}

#[tokio::test]
async fn generated_websocket_close_transport_error_is_sanitized() {
    let socket = MockWebSocket::new(Vec::new()).with_close_error(WS_SENTINEL);
    let transport = WebSocketTransport::new(Some(StatusCode::SWITCHING_PROTOCOLS), socket.clone());
    let api = WebSocketHelperApi::new_with_transport(transport.clone());

    let mut ws: WebSocketClient<ClientMsg, ServerMsg> = api
        .connect_default()
        .execute_websocket()
        .await
        .expect("websocket connect succeeds");

    let err = ws.close().await.expect_err("close must fail");
    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(!format!("{err:?}").contains(WS_SENTINEL));
    assert!(!format!("{err}").contains(WS_SENTINEL));
}
