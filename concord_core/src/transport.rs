use crate::auth::{PendingAuthPlacement, RequestExtensions};
use crate::codec::CodecError;
use crate::rate_limit::RateLimitPlan;
use crate::retry::RetrySetting;
use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, Method, StatusCode};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;
use url::Url;

use std::error::Error;

#[derive(Clone, Debug)]
pub struct RequestMeta {
    pub endpoint: &'static str,
    pub method: Method,
    pub idempotent: bool,
    pub attempt: u32,
    pub page_index: u32,
}

#[derive(Default)]
pub enum TransportRequestBody {
    #[default]
    Empty,
    Bytes(bytes::Bytes),
    Stream(TransportByteStream),
}

impl TransportRequestBody {
    #[inline]
    pub fn empty() -> Self {
        Self::Empty
    }

    #[inline]
    pub fn from_bytes(bytes: bytes::Bytes) -> Self {
        Self::Bytes(bytes)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    #[inline]
    pub fn is_bytes(&self) -> bool {
        matches!(self, Self::Bytes(_))
    }

    #[inline]
    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    #[inline]
    pub fn as_bytes(&self) -> Option<&bytes::Bytes> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Empty | Self::Stream(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamLimitDirection {
    Request,
    Response,
}

#[derive(Debug)]
pub(crate) struct StreamBodyLimitError {
    pub(crate) meta: RequestMeta,
    pub(crate) direction: StreamLimitDirection,
    pub(crate) limit: usize,
    pub(crate) seen: usize,
}

impl fmt::Display for StreamBodyLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = match self.direction {
            StreamLimitDirection::Request => "request",
            StreamLimitDirection::Response => "response",
        };
        write!(
            f,
            "{} {} stream body exceeded configured size limit {} bytes (seen {} bytes)",
            self.meta.method, direction, self.limit, self.seen
        )
    }
}

impl Error for StreamBodyLimitError {}

impl fmt::Debug for TransportRequestBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("<empty>"),
            Self::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            Self::Stream(_) => f.write_str("<stream>"),
        }
    }
}

pub struct TransportByteStream {
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, TransportError>> + Send>>,
    limit: Option<StreamByteLimit>,
}

impl TransportByteStream {
    pub fn new<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: Into<TransportError> + Send + 'static,
    {
        Self {
            inner: Box::pin(MapIntoTransportErrorStream::<S, E> {
                inner: Box::pin(stream),
                _marker: PhantomData,
            }),
            limit: None,
        }
    }

    pub(crate) fn with_limit(mut self, limit: usize, meta: RequestMeta) -> Self {
        self.limit = Some(StreamByteLimit {
            meta,
            direction: StreamLimitDirection::Request,
            limit,
            seen: 0,
            exceeded: false,
        });
        self
    }
}

impl fmt::Debug for TransportByteStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<stream>")
    }
}

impl Stream for TransportByteStream {
    type Item = Result<bytes::Bytes, TransportError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.limit.as_ref().is_some_and(|limit| limit.exceeded) {
            return std::task::Poll::Ready(None);
        }
        match this.inner.as_mut().poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => {
                if let Some(limit) = this.limit.as_mut() {
                    let next_seen = limit.seen.saturating_add(bytes.len());
                    if next_seen > limit.limit {
                        limit.exceeded = true;
                        let error = StreamBodyLimitError {
                            meta: limit.meta.clone(),
                            direction: limit.direction,
                            limit: limit.limit,
                            seen: next_seen,
                        };
                        return std::task::Poll::Ready(Some(Err(TransportError::with_kind(
                            TransportErrorKind::Request,
                            error,
                        ))));
                    }
                    limit.seen = next_seen;
                }
                std::task::Poll::Ready(Some(Ok(bytes)))
            }
            other => other,
        }
    }
}

struct StreamByteLimit {
    meta: RequestMeta,
    direction: StreamLimitDirection,
    limit: usize,
    seen: usize,
    exceeded: bool,
}

struct MapIntoTransportErrorStream<S, E> {
    inner: Pin<Box<S>>,
    _marker: PhantomData<fn() -> E>,
}

impl<S, E> Stream for MapIntoTransportErrorStream<S, E>
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
    E: Into<TransportError> + Send + 'static,
{
    type Item = Result<bytes::Bytes, TransportError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => std::task::Poll::Ready(Some(Ok(bytes))),
            std::task::Poll::Ready(Some(Err(err))) => std::task::Poll::Ready(Some(Err(err.into()))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

pub struct BuiltRequest {
    pub meta: RequestMeta,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: TransportRequestBody,
    pub(crate) stream_size_hint: Option<crate::stream_body::BodySizeHint>,
    pub timeout: Option<Duration>,
    pub retry: RetrySetting,
    pub rate_limit: RateLimitPlan,
    pub extensions: RequestExtensions,
}

impl fmt::Debug for BuiltRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltRequest")
            .field("meta", &self.meta)
            .field("url", &self.debug_url())
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &self.body)
            .field("stream_size_hint", &self.stream_size_hint)
            .field("timeout", &self.timeout)
            .field("retry", &self.retry)
            .field("rate_limit", &self.rate_limit)
            .field("extensions", &self.extensions)
            .finish()
    }
}

impl BuiltRequest {
    #[inline]
    pub fn has_stream_body(&self) -> bool {
        self.body.is_stream()
    }

    pub(crate) fn debug_url(&self) -> String {
        let mut url = self.url.clone();
        if self
            .extensions
            .pending_auth_slots
            .iter()
            .any(|slot| matches!(slot.placement, PendingAuthPlacement::Query(_)))
        {
            let mut pairs = url.query_pairs_mut();
            for slot in &self.extensions.pending_auth_slots {
                if let PendingAuthPlacement::Query(name) = &slot.placement {
                    pairs.append_pair(name, "<redacted>");
                }
            }
        }
        crate::redaction::sanitize_url_for_debug(&url, self.extensions.sensitive_query_keys.iter())
    }
}

#[derive(Clone)]
pub struct BuiltResponse {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
    pub rate_limit: RateLimitPlan,
}

impl fmt::Debug for BuiltResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltResponse")
            .field("meta", &self.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.url, [] as [&str; 0]),
            )
            .field("status", &self.status)
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &format!("<{} bytes>", self.body.len()))
            .field("rate_limit", &self.rate_limit)
            .finish()
    }
}

#[derive(Clone)]
pub struct DecodedResponse<T> {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub value: T,
}

impl<T: fmt::Debug> fmt::Debug for DecodedResponse<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedResponse")
            .field("meta", &self.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.url, [] as [&str; 0]),
            )
            .field("status", &self.status)
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("value", &self.value)
            .finish()
    }
}

pub struct TransportRequest {
    pub meta: RequestMeta,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: TransportRequestBody,
    pub timeout: Option<Duration>,
    pub rate_limit: RateLimitPlan,
    pub extensions: RequestExtensions,
}

impl fmt::Debug for TransportRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut headers = self.headers.clone();
        for slot in &self.extensions.pending_auth_slots {
            match &slot.placement {
                PendingAuthPlacement::Bearer | PendingAuthPlacement::Basic => {
                    headers.insert(
                        http::header::AUTHORIZATION,
                        http::HeaderValue::from_static("<redacted>"),
                    );
                }
                PendingAuthPlacement::Header(name) => {
                    headers.insert(name.clone(), http::HeaderValue::from_static("<redacted>"));
                }
                PendingAuthPlacement::Query(_) => {}
            }
        }
        f.debug_struct("TransportRequest")
            .field("meta", &self.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(
                    &self.url,
                    self.extensions.sensitive_query_keys.iter(),
                ),
            )
            .field("headers", &crate::debug::SanitizedHeaders::new(&headers))
            .field("body", &self.body)
            .field("timeout", &self.timeout)
            .field("rate_limit", &self.rate_limit)
            .field("extensions", &self.extensions)
            .finish()
    }
}

pub(crate) struct AuthCollisionValidatedBuiltRequest(BuiltRequest);

impl AuthCollisionValidatedBuiltRequest {
    pub(crate) fn into_inner(self) -> BuiltRequest {
        self.0
    }
}

impl std::ops::Deref for AuthCollisionValidatedBuiltRequest {
    type Target = BuiltRequest;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AuthCollisionValidatedBuiltRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn validate_auth_collisions_impl(built: &BuiltRequest) -> Result<(), crate::auth::AuthError> {
    use http::header::AUTHORIZATION;

    for slot in &built.extensions.pending_auth_slots {
        match &slot.placement {
            PendingAuthPlacement::Bearer => {
                if built.headers.contains_key(AUTHORIZATION) {
                    return Err(crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        "bearer auth collides with an existing public Authorization header",
                    ));
                }
            }
            PendingAuthPlacement::Header(name) => {
                if built.headers.contains_key(name) {
                    return Err(crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        format!("header auth key `{name}` collides with an existing public header"),
                    ));
                }
            }
            PendingAuthPlacement::Query(name) => {
                if built
                    .url
                    .query_pairs()
                    .any(|(existing, _)| existing == name.as_str())
                {
                    return Err(crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        format!(
                            "query auth key `{name}` collides with an existing public query parameter"
                        ),
                    ));
                }
            }
            PendingAuthPlacement::Basic => {
                if built.headers.contains_key(AUTHORIZATION) {
                    return Err(crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        "basic auth collides with an existing public Authorization header",
                    ));
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn validate_auth_collisions(
    built: BuiltRequest,
) -> Result<AuthCollisionValidatedBuiltRequest, crate::auth::AuthError> {
    validate_auth_collisions_impl(&built)?;
    Ok(AuthCollisionValidatedBuiltRequest(built))
}

pub(crate) fn materialize_transport_request_validated(
    built: AuthCollisionValidatedBuiltRequest,
    materials: &[crate::auth::AuthTransportMaterial],
    stream_request_limit: Option<usize>,
) -> Result<TransportRequest, crate::auth::AuthError> {
    use base64::Engine;
    use http::header::{AUTHORIZATION, HeaderValue};
    use std::collections::HashMap;

    let mut by_slot = HashMap::new();
    for material in materials {
        let slot_id = match material {
            crate::auth::AuthTransportMaterial::Secret { slot_id, .. }
            | crate::auth::AuthTransportMaterial::Basic { slot_id, .. } => *slot_id,
        };
        by_slot.insert(slot_id, material);
    }

    let built = built.into_inner();
    let extensions = built.extensions;
    let mut req = TransportRequest {
        meta: built.meta,
        url: built.url,
        headers: built.headers,
        body: built.body,
        timeout: built.timeout,
        rate_limit: built.rate_limit,
        extensions,
    };

    for slot in &req.extensions.pending_auth_slots {
        let Some(material) = by_slot.get(&slot.id).copied() else {
            return Err(crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::MissingCredential,
                format!(
                    "missing materialized credential `{}` for auth usage `{}`",
                    slot.credential.id, slot.usage_id
                ),
            ));
        };
        match (&slot.placement, material) {
            (
                PendingAuthPlacement::Bearer,
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                let value = format!("Bearer {}", secret.expose_secret());
                let value = HeaderValue::from_str(&value).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::UnsupportedScheme,
                        "invalid bearer header value",
                    )
                })?;
                req.headers.insert(AUTHORIZATION, value);
            }
            (
                PendingAuthPlacement::Header(name),
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                let value = HeaderValue::from_str(secret.expose_secret()).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::UnsupportedScheme,
                        "invalid auth header value",
                    )
                })?;
                req.headers.insert(name.clone(), value);
            }
            (
                PendingAuthPlacement::Query(name),
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                req.url
                    .query_pairs_mut()
                    .append_pair(name, secret.expose_secret());
            }
            (
                PendingAuthPlacement::Basic,
                crate::auth::AuthTransportMaterial::Basic {
                    username, password, ..
                },
            ) => {
                let raw = format!("{}:{}", username.expose_secret(), password.expose_secret());
                let value = format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(raw)
                );
                let value = HeaderValue::from_str(&value).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::UnsupportedScheme,
                        "invalid basic header value",
                    )
                })?;
                req.headers.insert(AUTHORIZATION, value);
            }
            _ => {
                return Err(crate::auth::AuthError::new(
                    crate::auth::AuthErrorKind::UnsupportedScheme,
                    format!(
                        "credential material does not match auth placement for `{}`",
                        slot.usage_id
                    ),
                ));
            }
        }
    }

    if let (Some(limit), TransportRequestBody::Stream(stream)) =
        (stream_request_limit, &mut req.body)
    {
        let meta = req.meta.clone();
        let stream = std::mem::replace(stream, TransportByteStream::new(EmptyStream));
        req.body = TransportRequestBody::Stream(stream.with_limit(limit, meta));
    }

    Ok(req)
}

pub(crate) fn materialize_transport_request(
    built: BuiltRequest,
    materials: &[crate::auth::AuthTransportMaterial],
    stream_request_limit: Option<usize>,
) -> Result<TransportRequest, crate::auth::AuthError> {
    let validated = validate_auth_collisions(built)?;
    materialize_transport_request_validated(validated, materials, stream_request_limit)
}

#[derive(Default)]
struct EmptyStream;

impl Stream for EmptyStream {
    type Item = Result<bytes::Bytes, TransportError>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

impl<T> DecodedResponse<T> {
    #[inline]
    pub fn meta(&self) -> &RequestMeta {
        &self.meta
    }

    #[inline]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[inline]
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    #[inline]
    pub fn into_value(self) -> T {
        self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportErrorKind {
    Timeout,
    Connect,
    Tls,
    Dns,
    Io,
    Request,
    Other,
}

pub struct TransportError {
    kind: TransportErrorKind,
    source: crate::error::FxError,
}

#[derive(Debug)]
struct ResponseBodyReadError;

impl fmt::Display for ResponseBodyReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("response body read failed")
    }
}

impl Error for ResponseBodyReadError {}

impl TransportError {
    #[inline]
    pub fn new(e: impl Error + Send + Sync + 'static) -> Self {
        let source: crate::error::FxError = Box::new(e);
        let kind = if source.downcast_ref::<std::io::Error>().is_some() {
            TransportErrorKind::Io
        } else {
            TransportErrorKind::Other
        };
        Self { kind, source }
    }

    #[inline]
    pub fn with_kind(kind: TransportErrorKind, e: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            source: Box::new(e),
        }
    }

    #[inline]
    pub(crate) fn response_body_read(kind: TransportErrorKind) -> Self {
        Self {
            kind,
            source: Box::new(ResponseBodyReadError),
        }
    }

    #[inline]
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    #[inline]
    pub fn source_error(&self) -> &(dyn Error + Send + Sync + 'static) {
        &*self.source
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self
            .source
            .downcast_ref::<ResponseBodyReadError>()
            .is_some()
        {
            return write!(f, "transport error: response body read failed");
        }
        write!(f, "transport error: {:?}", self.kind)
    }
}

impl fmt::Debug for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

#[cfg(feature = "transport-reqwest")]
impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        let kind = classify_reqwest_error(&e);
        let e = e.without_url();
        Self {
            kind,
            source: Box::new(e),
        }
    }
}

impl From<CodecError> for TransportError {
    fn from(error: CodecError) -> Self {
        TransportError::with_kind(TransportErrorKind::Request, error)
    }
}

#[cfg(feature = "transport-reqwest")]
fn classify_reqwest_error(err: &reqwest::Error) -> TransportErrorKind {
    if err.is_timeout() {
        return TransportErrorKind::Timeout;
    }
    if err.is_connect() {
        let msg = err.to_string().to_ascii_lowercase();
        if msg.contains("dns")
            || msg.contains("name or service not known")
            || msg.contains("failed to lookup address information")
            || msg.contains("no such host")
        {
            return TransportErrorKind::Dns;
        }
        if msg.contains("tls")
            || msg.contains("ssl")
            || msg.contains("handshake")
            || msg.contains("certificate")
        {
            return TransportErrorKind::Tls;
        }
        return TransportErrorKind::Connect;
    }
    let mut current: Option<&(dyn Error + 'static)> = err.source();
    while let Some(cause) = current {
        if let Some(ioe) = cause.downcast_ref::<std::io::Error>() {
            if matches!(
                ioe.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                return TransportErrorKind::Timeout;
            }
            return TransportErrorKind::Io;
        }
        current = cause.source();
    }
    if err.is_request() {
        return TransportErrorKind::Request;
    }
    TransportErrorKind::Other
}

pub trait TransportBody: Send + 'static {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>>;
}

pub struct TransportResponse {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub content_length: Option<u64>,
    pub rate_limit: RateLimitPlan,
    pub body: Box<dyn TransportBody>,
}

/// Injectable transport layer.
///
/// Contract:
/// - Must honor `TransportRequest` fields (url/headers/body/timeout) as appropriate.
/// - Must not leak a concrete HTTP client type in its public surface.
pub trait Transport: Send + Clone + Sync + 'static {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>>;
}

#[doc(hidden)]
pub trait DefaultTransportMarker: Transport + Clone {}

#[cfg(feature = "transport-reqwest")]
#[doc(hidden)]
pub type DefaultTransport = ReqwestTransport;

#[cfg(feature = "transport-reqwest")]
impl DefaultTransportMarker for ReqwestTransport {}

#[cfg(not(feature = "transport-reqwest"))]
#[doc(hidden)]
#[derive(Clone)]
pub struct DefaultTransport(());

#[cfg(not(feature = "transport-reqwest"))]
impl Transport for DefaultTransport {
    fn send(
        &self,
        _req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        Box::pin(async move {
            Err(TransportError::with_kind(
                TransportErrorKind::Request,
                std::io::Error::other(
                    "default reqwest transport is disabled; enable the `transport-reqwest` feature",
                ),
            ))
        })
    }
}

#[cfg(feature = "transport-reqwest")]
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

#[cfg(feature = "transport-reqwest")]
impl ReqwestTransport {
    #[inline]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    #[inline]
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

#[cfg(feature = "transport-reqwest")]
struct ReqwestBody {
    resp: reqwest::Response,
}

#[cfg(feature = "transport-reqwest")]
impl TransportBody for ReqwestBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { self.resp.chunk().await.map_err(TransportError::from) })
    }
}

#[cfg(feature = "transport-reqwest")]
impl Transport for ReqwestTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let client = self.client.clone();
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                ..
            } = req;
            // reqwest needs an owned Url; we keep a copy for returning meta.
            let url_for_resp = url.clone();
            let method = meta.method.clone();
            let mut rb = client.request(method, url).headers(headers);
            match body {
                TransportRequestBody::Empty => {}
                TransportRequestBody::Bytes(b) => {
                    rb = rb.body(b);
                }
                TransportRequestBody::Stream(stream) => {
                    rb = rb.body(reqwest::Body::wrap_stream(stream));
                }
            }
            if let Some(t) = timeout {
                rb = rb.timeout(t);
            }
            let resp = rb.send().await.map_err(TransportError::from)?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let content_length = resp.content_length();
            Ok(TransportResponse {
                meta,
                url: url_for_resp,
                status,
                headers,
                content_length,
                rate_limit,
                body: Box::new(ReqwestBody { resp }),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    #[test]
    fn transport_request_body_debug_is_safe() {
        let sentinel = Bytes::from_static(b"SECRET_BODY_SENTINEL_MUST_NOT_APPEAR");
        let empty = format!("{:?}", TransportRequestBody::Empty);
        let bytes = format!("{:?}", TransportRequestBody::from_bytes(sentinel.clone()));
        let stream = format!(
            "{:?}",
            TransportRequestBody::Stream(TransportByteStream::new(TransportErrorStream::error(
                sentinel.clone()
            )))
        );

        assert!(!empty.contains("SECRET_BODY_SENTINEL_MUST_NOT_APPEAR"));
        assert!(!bytes.contains("SECRET_BODY_SENTINEL_MUST_NOT_APPEAR"));
        assert!(!stream.contains("SECRET_BODY_SENTINEL_MUST_NOT_APPEAR"));
        assert_eq!(bytes, format!("<{} bytes>", sentinel.len()));
        assert_eq!(empty, "<empty>");
        assert_eq!(stream, "<stream>");
    }

    #[test]
    fn transport_byte_stream_propagates_errors_without_payload_leak() {
        let sentinel = Bytes::from_static(b"SECRET_STREAM_ERROR_SENTINEL_MUST_NOT_APPEAR");
        let mut stream = Box::pin(TransportByteStream::new(TransportErrorStream::error(
            sentinel.clone(),
        )));
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);

        match Stream::poll_next(stream.as_mut(), &mut cx) {
            Poll::Ready(Some(Err(err))) => {
                assert!(err.to_string().contains("transport error"));
                assert!(
                    !err.to_string()
                        .contains("SECRET_STREAM_ERROR_SENTINEL_MUST_NOT_APPEAR")
                );
            }
            other => panic!("unexpected stream poll result: {other:?}"),
        }
    }

    #[test]
    fn transport_byte_stream_accepts_send_not_sync_stream() {
        let mut stream = Box::pin(TransportByteStream::new(SendOnlyStream::new()));
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);

        match Stream::poll_next(stream.as_mut(), &mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from_static(b"chunk")),
            other => panic!("unexpected stream poll result: {other:?}"),
        }
    }

    struct TransportErrorStream {
        item: Option<Result<Bytes, TransportError>>,
    }

    impl TransportErrorStream {
        fn error(sentinel: Bytes) -> Self {
            Self {
                item: Some(Err(TransportError::new(std::io::Error::other(
                    String::from_utf8_lossy(&sentinel).to_string(),
                )))),
            }
        }
    }

    impl Stream for TransportErrorStream {
        type Item = Result<Bytes, TransportError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.item.take())
        }
    }

    struct SendOnlyStream {
        chunk: Cell<Option<Result<Bytes, TransportError>>>,
    }

    impl SendOnlyStream {
        fn new() -> Self {
            Self {
                chunk: Cell::new(Some(Ok(Bytes::from_static(b"chunk")))),
            }
        }
    }

    impl Stream for SendOnlyStream {
        type Item = Result<Bytes, TransportError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.chunk.take())
        }
    }
}
