use crate::codec::CodecError;
use crate::error::{ApiClientError, ErrorContext};
use crate::rate_limit::RateLimitPlan;
use crate::transport::{
    RequestMeta, TransportError, TransportWebSocket, TransportWebSocketConnection,
    TransportWsClose, TransportWsMessage,
};
use http::{HeaderMap, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::marker::PhantomData;
use url::Url;

pub type WebSocketMessage = TransportWsMessage;
pub type WebSocketClose = TransportWsClose;

pub trait WebSocketCodec<Out, In>: Send + Sync + 'static {
    fn encode(msg: Out) -> Result<TransportWsMessage, CodecError>;
    fn decode(msg: TransportWsMessage) -> Result<Option<In>, CodecError>;
}

pub struct JsonWebSocket;

impl<Out, In> WebSocketCodec<Out, In> for JsonWebSocket
where
    Out: Serialize,
    In: DeserializeOwned,
{
    fn encode(msg: Out) -> Result<TransportWsMessage, CodecError> {
        serde_json::to_string(&msg)
            .map(TransportWsMessage::Text)
            .map_err(|_| CodecError::new("websocket message encode failed"))
    }

    fn decode(msg: TransportWsMessage) -> Result<Option<In>, CodecError> {
        match msg {
            TransportWsMessage::Text(text) => serde_json::from_str::<In>(&text)
                .map(Some)
                .map_err(|_| CodecError::new("websocket message decode failed")),
            TransportWsMessage::Binary(bytes) => serde_json::from_slice::<In>(&bytes)
                .map(Some)
                .map_err(|_| CodecError::new("websocket message decode failed")),
            TransportWsMessage::Ping(_) | TransportWsMessage::Pong(_) => Ok(None),
            TransportWsMessage::Close(_) => Ok(None),
        }
    }
}

pub struct WebSocketClient<Out, In> {
    meta: RequestMeta,
    url: Url,
    status: Option<StatusCode>,
    headers: HeaderMap,
    rate_limit: RateLimitPlan,
    socket: Box<dyn TransportWebSocket>,
    encode: fn(Out) -> Result<TransportWsMessage, CodecError>,
    decode: fn(TransportWsMessage) -> Result<Option<In>, CodecError>,
    closed: bool,
    _marker: PhantomData<(Out, In)>,
}

impl<Out, In> WebSocketClient<Out, In> {
    pub(crate) fn new<Codec>(conn: TransportWebSocketConnection) -> Self
    where
        Codec: WebSocketCodec<Out, In>,
        Out: Send + 'static,
        In: Send + 'static,
    {
        Self {
            meta: conn.meta,
            url: conn.url,
            status: conn.status,
            headers: conn.headers,
            rate_limit: conn.rate_limit,
            socket: conn.socket,
            encode: encode_message::<Out, In, Codec>,
            decode: decode_message::<Out, In, Codec>,
            closed: false,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn meta(&self) -> &RequestMeta {
        &self.meta
    }

    #[inline]
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[inline]
    pub fn status(&self) -> Option<StatusCode> {
        self.status
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[inline]
    pub fn rate_limit(&self) -> &RateLimitPlan {
        &self.rate_limit
    }

    pub async fn send(&mut self, msg: Out) -> Result<(), ApiClientError>
    where
        Out: Send + 'static,
    {
        if self.closed {
            return Err(ApiClientError::PolicyViolation {
                ctx: self.error_context(),
                msg: "websocket connection is closed",
            });
        }
        let encoded =
            (self.encode)(msg).map_err(|_| self.codec_error("websocket message encode failed"))?;
        self.socket.send(encoded).await.map_err(|source| {
            self.websocket_transport_error(source, "websocket message send failed")
        })
    }

    pub async fn next(&mut self) -> Result<Option<In>, ApiClientError>
    where
        In: Send + 'static,
    {
        if self.closed {
            return Ok(None);
        }
        loop {
            let Some(msg) = self.socket.next().await.map_err(|source| {
                self.websocket_transport_error(source, "websocket message read failed")
            })?
            else {
                self.closed = true;
                return Ok(None);
            };

            match msg {
                TransportWsMessage::Ping(_) | TransportWsMessage::Pong(_) => continue,
                TransportWsMessage::Close(_) => {
                    self.closed = true;
                    return Ok(None);
                }
                other => {
                    let decoded = (self.decode)(other)
                        .map_err(|_| self.codec_error("websocket message decode failed"))?;
                    if let Some(value) = decoded {
                        return Ok(Some(value));
                    }
                }
            }
        }
    }

    pub async fn close(&mut self) -> Result<(), ApiClientError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.socket
            .close(None)
            .await
            .map_err(|source| self.websocket_transport_error(source, "websocket close failed"))
    }

    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.endpoint,
            method: self.meta.method.clone(),
        }
    }

    fn codec_error(&self, msg: &'static str) -> ApiClientError {
        ApiClientError::Codec {
            ctx: self.error_context(),
            source: Box::new(CodecError::new(msg)),
        }
    }

    fn websocket_transport_error(
        &self,
        source: TransportError,
        msg: &'static str,
    ) -> ApiClientError {
        ApiClientError::Transport {
            ctx: self.error_context(),
            source: TransportError::with_kind(source.kind(), std::io::Error::other(msg)),
        }
    }
}

impl<Out, In> fmt::Debug for WebSocketClient<Out, In> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketClient")
            .field("meta", &self.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.url, [] as [&str; 0]),
            )
            .field("status", &self.status)
            .field("headers", &crate::debug::RedactedHeaders(&self.headers))
            .field("rate_limit", &self.rate_limit)
            .field("closed", &self.closed)
            .field("socket", &"<ws>")
            .finish()
    }
}

fn encode_message<Out, In, Codec>(msg: Out) -> Result<TransportWsMessage, CodecError>
where
    Codec: WebSocketCodec<Out, In>,
{
    Codec::encode(msg)
}

fn decode_message<Out, In, Codec>(msg: TransportWsMessage) -> Result<Option<In>, CodecError>
where
    Codec: WebSocketCodec<Out, In>,
{
    Codec::decode(msg)
}

pub(crate) fn websocketize_url(url: &mut Url, ctx: ErrorContext) -> Result<(), ApiClientError> {
    let target = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        "ws" => "ws",
        "wss" => "wss",
        _ => {
            return Err(ApiClientError::PolicyViolation {
                ctx,
                msg: "websocket endpoints require an http or https route scheme",
            });
        }
    };
    url.set_scheme(target)
        .map_err(|_| ApiClientError::PolicyViolation {
            ctx,
            msg: "websocket url scheme conversion failed",
        })
}
