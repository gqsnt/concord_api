use crate::auth::PlannedAuthPlacement;
use crate::rate_limit::RateLimitPlan;
use crate::retry::RetrySetting;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use url::Url;

use std::error::Error;

#[derive(Clone, Debug)]
pub struct RequestMeta {
    pub endpoint: &'static str,
    pub method: Method,
    pub idempotent: bool,
    /// Zero-based metadata index derived from the request-local physical
    /// attempt count. The first `Transport::send` is physical attempt 1;
    /// this legacy metadata field remains 0 for that send.
    pub attempt: u32,
    pub page_index: u32,
}

#[derive(Clone, Debug)]
pub struct RequestExecutionContext {
    pub meta: RequestMeta,
    pub timeout: Option<Duration>,
}

pub(crate) struct BuiltRequest {
    pub(crate) message: http::Request<crate::body::DynBody>,
    pub(crate) retry: RetrySetting,
    pub(crate) rate_limit: RateLimitPlan,
    pub(crate) stream_like: bool,
}

impl fmt::Debug for BuiltRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltRequest")
            .field("meta", &self.context().meta)
            .field("url", &self.debug_url())
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(self.message.headers()),
            )
            .field("version", &self.message.version())
            .field("body", &self.message.body())
            .field("timeout", &self.context().timeout)
            .field("retry", &self.retry)
            .field("rate_limit", &self.rate_limit)
            .field("stream_like", &self.stream_like)
            .finish()
    }
}

impl BuiltRequest {
    pub(crate) fn debug_url(&self) -> String {
        let url = Url::parse(&self.message.uri().to_string())
            .unwrap_or_else(|_| Url::parse("http://invalid.invalid/").expect("static URL"));
        let sensitive = self
            .message
            .extensions()
            .get::<crate::auth::AuthPlacementPlan>()
            .map(|plan| plan.sensitive_query_keys.iter());
        crate::redaction::sanitize_url_for_debug(&url, sensitive.into_iter().flatten())
    }

    pub(crate) fn context(&self) -> &RequestExecutionContext {
        self.message
            .extensions()
            .get::<RequestExecutionContext>()
            .expect("built request context is installed during construction")
    }
}

pub struct BuiltResponse {
    message: http::Response<bytes::Bytes>,
    context: ResponseContext,
}

#[derive(Clone, Debug)]
pub(crate) struct ResponseContext {
    pub(crate) meta: RequestMeta,
    pub(crate) request_url: Url,
    pub(crate) rate_limit: RateLimitPlan,
}

pub(crate) struct AttemptResponse {
    pub(crate) message: http::Response<crate::body::DynBody>,
    pub(crate) context: ResponseContext,
}

impl AttemptResponse {
    pub(crate) fn status(&self) -> StatusCode {
        self.message.status()
    }

    pub(crate) fn headers(&self) -> &HeaderMap {
        self.message.headers()
    }
}

impl fmt::Debug for BuiltResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltResponse")
            .field("meta", &self.context.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(
                    &self.context.request_url,
                    [] as [&str; 0],
                ),
            )
            .field("status", &self.message.status())
            .field("version", &self.message.version())
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(self.message.headers()),
            )
            .field("body", &format!("<{} bytes>", self.message.body().len()))
            .field("rate_limit", &self.context.rate_limit)
            .finish()
    }
}

impl BuiltResponse {
    pub(crate) fn new(message: http::Response<Bytes>, context: ResponseContext) -> Self {
        Self { message, context }
    }

    /// Wraps a standard buffered response with safe request execution context.
    pub fn from_http(
        message: http::Response<Bytes>,
        meta: RequestMeta,
        request_url: Url,
        rate_limit: RateLimitPlan,
    ) -> Self {
        Self::new(
            message,
            ResponseContext {
                meta,
                request_url,
                rate_limit,
            },
        )
    }

    pub fn meta(&self) -> &RequestMeta {
        &self.context.meta
    }

    pub fn url(&self) -> &Url {
        &self.context.request_url
    }

    pub fn status(&self) -> StatusCode {
        self.message.status()
    }

    pub fn version(&self) -> http::Version {
        self.message.version()
    }

    pub fn headers(&self) -> &HeaderMap {
        self.message.headers()
    }

    pub fn extensions(&self) -> &http::Extensions {
        self.message.extensions()
    }

    pub fn body(&self) -> &Bytes {
        self.message.body()
    }

    pub fn rate_limit(&self) -> &RateLimitPlan {
        &self.context.rate_limit
    }

    pub fn into_body(self) -> Bytes {
        self.message.into_body()
    }

    pub(crate) fn into_parts(self) -> (http::Response<Bytes>, ResponseContext) {
        (self.message, self.context)
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

pub(crate) fn materialize_transport_request(
    built: BuiltRequest,
    materials: &[crate::auth::AuthTransportMaterial],
    stream_request_limit: Option<usize>,
) -> Result<http::Request<crate::body::DynBody>, crate::auth::AuthError> {
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

    let BuiltRequest {
        mut message,
        stream_like,
        ..
    } = built;
    let auth_plan = message
        .extensions()
        .get::<crate::auth::AuthPlacementPlan>()
        .cloned()
        .unwrap_or_default();

    for slot in &auth_plan.slots {
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
                PlannedAuthPlacement::Bearer,
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                let value = format!("Bearer {}", secret.expose_secret());
                let value = HeaderValue::from_str(&value).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::UnsupportedScheme,
                        "invalid bearer header value",
                    )
                })?;
                message.headers_mut().insert(AUTHORIZATION, value);
            }
            (
                PlannedAuthPlacement::Header(name),
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                let value = HeaderValue::from_str(secret.expose_secret()).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::UnsupportedScheme,
                        "invalid auth header value",
                    )
                })?;
                message.headers_mut().insert(name.clone(), value);
            }
            (
                PlannedAuthPlacement::Query(name),
                crate::auth::AuthTransportMaterial::Secret { secret, .. },
            ) => {
                let mut url = Url::parse(&message.uri().to_string()).map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        "request URI could not accept query authentication",
                    )
                })?;
                url.query_pairs_mut()
                    .append_pair(name, secret.expose_secret());
                *message.uri_mut() = url.as_str().parse().map_err(|_| {
                    crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        "materialized authentication URI is invalid",
                    )
                })?;
            }
            (
                PlannedAuthPlacement::Basic,
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
                message.headers_mut().insert(AUTHORIZATION, value);
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

    if stream_like && let Some(limit) = stream_request_limit {
        let body = std::mem::replace(message.body_mut(), crate::body::DynBody::empty());
        *message.body_mut() = body.limited(limit as u64);
    }

    Ok(message)
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
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    #[inline]
    pub fn source_error(&self) -> &(dyn Error + Send + Sync + 'static) {
        &*self.source
    }

    pub(crate) fn body_error(&self) -> Option<crate::body::BodyError> {
        let mut source: &(dyn Error + 'static) = &*self.source;
        loop {
            if let Some(error) = source.downcast_ref::<crate::body::BodyError>() {
                return Some(*error);
            }
            source = source.source()?;
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        let e = e.without_url();
        let kind = classify_reqwest_error(&e);
        Self {
            kind,
            source: Box::new(e),
        }
    }
}

impl From<crate::body::BodyError> for TransportError {
    fn from(error: crate::body::BodyError) -> Self {
        let kind = match error.kind() {
            crate::body::BodyErrorKind::Io => TransportErrorKind::Io,
            crate::body::BodyErrorKind::LimitExceeded => TransportErrorKind::Request,
            _ => TransportErrorKind::Other,
        };
        Self::with_kind(kind, error)
    }
}

#[cfg(feature = "transport-reqwest")]
fn classify_reqwest_error(err: &reqwest::Error) -> TransportErrorKind {
    if err.is_timeout() {
        return TransportErrorKind::Timeout;
    }
    if err.is_connect() {
        return TransportErrorKind::Connect;
    }
    if err.is_request() {
        return TransportErrorKind::Request;
    }
    TransportErrorKind::Other
}

/// Injectable transport layer.
///
/// Contract:
/// - One call represents one physical send.
/// - Request and response heads use standard HTTP message ownership.
/// - Must not leak a concrete HTTP client type in its public surface.
pub trait Transport: Send + Sync + 'static {
    fn send(
        &self,
        request: http::Request<crate::body::DynBody>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<http::Response<crate::body::DynBody>, TransportError>>
                + Send,
        >,
    >;
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
        _request: http::Request<crate::body::DynBody>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<http::Response<crate::body::DynBody>, TransportError>>
                + Send,
        >,
    > {
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

/// A sanitized managed-Reqwest client construction failure.
#[cfg(feature = "transport-reqwest")]
pub struct ReqwestClientBuildError {
    kind: TransportErrorKind,
    source: reqwest::Error,
}

#[cfg(feature = "transport-reqwest")]
impl ReqwestClientBuildError {
    fn from_reqwest(error: reqwest::Error) -> Self {
        let source = error.without_url();
        Self {
            kind: classify_reqwest_error(&source),
            source,
        }
    }

    /// Returns the safe structural category of the client-build failure.
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }
}

#[cfg(feature = "transport-reqwest")]
impl fmt::Display for ReqwestClientBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "managed reqwest client construction failed ({:?})",
            self.kind
        )
    }
}

#[cfg(feature = "transport-reqwest")]
impl fmt::Debug for ReqwestClientBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestClientBuildError")
            .field("kind", &self.kind)
            .finish()
    }
}

#[cfg(feature = "transport-reqwest")]
impl Error for ReqwestClientBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[cfg(feature = "transport-reqwest")]
impl ReqwestTransport {
    #[inline]
    pub fn new() -> Self {
        Self::from_managed_build(Self::try_new())
    }

    pub fn try_new() -> Result<Self, ReqwestClientBuildError> {
        Self::with_builder(|builder| builder)
    }

    /// Applies caller-selected client-wide settings before Concord installs
    /// its non-negotiable retry and redirect policies.
    pub fn with_builder(
        configure: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Self, ReqwestClientBuildError> {
        let client = configure(reqwest::Client::builder())
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(ReqwestClientBuildError::from_reqwest)?;
        Ok(Self { client })
    }

    fn from_managed_build(result: Result<Self, ReqwestClientBuildError>) -> Self {
        match result {
            Ok(transport) => transport,
            Err(_) => panic!("managed reqwest client construction failed"),
        }
    }
}

#[cfg(feature = "transport-reqwest")]
impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "transport-reqwest")]
impl Transport for ReqwestTransport {
    fn send(
        &self,
        request: http::Request<crate::body::DynBody>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<http::Response<crate::body::DynBody>, TransportError>>
                + Send,
        >,
    > {
        let client = self.client.clone();
        Box::pin(async move {
            let request = reqwest_request_from_http(request)?;
            let response = client
                .execute(request)
                .await
                .map_err(TransportError::from)?;
            Ok(http_response_from_reqwest(response))
        })
    }
}

#[cfg(feature = "transport-reqwest")]
fn reqwest_request_from_http(
    request: http::Request<crate::body::DynBody>,
) -> Result<reqwest::Request, TransportError> {
    let timeout = request
        .extensions()
        .get::<RequestExecutionContext>()
        .and_then(|context| context.timeout);
    let mut request = reqwest::Request::try_from(request.map(reqwest::Body::wrap))
        .map_err(TransportError::from)?;
    *request.timeout_mut() = timeout;
    Ok(request)
}

#[cfg(feature = "transport-reqwest")]
fn http_response_from_reqwest(response: reqwest::Response) -> http::Response<crate::body::DynBody> {
    let response: http::Response<reqwest::Body> = response.into();
    response.map(|body| crate::body::DynBody::from_body(SanitizedReqwestBody { inner: body }))
}

#[cfg(feature = "transport-reqwest")]
struct SanitizedReqwestBody {
    inner: reqwest::Body,
}

#[cfg(feature = "transport-reqwest")]
impl http_body::Body for SanitizedReqwestBody {
    type Data = bytes::Bytes;
    type Error = crate::body::BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        http_body::Body::poll_frame(Pin::new(&mut self.inner), cx)
            .map(|frame| frame.map(|result| result.map_err(sanitized_reqwest_body_error)))
    }

    fn is_end_stream(&self) -> bool {
        http_body::Body::is_end_stream(&self.inner)
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::Body::size_hint(&self.inner)
    }
}

#[cfg(feature = "transport-reqwest")]
fn sanitized_reqwest_body_error(error: reqwest::Error) -> crate::body::BodyError {
    crate::body::BodyError::from(TransportError::from(error.without_url()))
}

#[cfg(all(test, feature = "transport-reqwest"))]
mod reqwest_transport_tests {
    use super::*;
    use bytes::Bytes;
    use futures_core::Stream;
    use http_body::{Body as _, Frame, SizeHint};
    use http_body_util::BodyExt as _;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::task::{Context, Poll};

    #[derive(Clone, Debug)]
    struct RequestMarker(&'static str);

    #[derive(Clone, Debug)]
    struct ResponseMarker(&'static str);

    struct PollProbe(Arc<AtomicBool>);

    impl Stream for PollProbe {
        type Item = Result<Bytes, crate::body::BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.0.store(true, Ordering::SeqCst);
            Poll::Ready(None)
        }
    }

    struct FrameSequence {
        frames: VecDeque<Result<Frame<Bytes>, crate::body::BodyError>>,
    }

    impl Stream for FrameSequence {
        type Item = Result<Frame<Bytes>, crate::body::BodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.frames.pop_front())
        }
    }

    fn failing_builder(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        builder.user_agent("invalid\r\nhttps://proxy-user:PROXY_SECRET@example.test")
    }

    fn managed_build_error() -> ReqwestClientBuildError {
        match ReqwestTransport::with_builder(failing_builder) {
            Ok(_) => panic!("invalid default header should fail client construction"),
            Err(error) => error,
        }
    }

    fn source_chain(error: &(dyn Error + 'static)) -> String {
        let mut rendered = String::new();
        let mut current = Some(error);
        while let Some(source) = current {
            rendered.push_str(&format!("{source}\n{source:?}\n"));
            current = source.source();
        }
        rendered
    }

    #[test]
    fn whole_request_conversion_preserves_head_extensions_timeout_and_laziness() {
        let polled = Arc::new(AtomicBool::new(false));
        let mut request = http::Request::new(crate::body::DynBody::from_byte_stream(PollProbe(
            polled.clone(),
        )));
        *request.method_mut() = Method::POST;
        *request.uri_mut() = "http://example.test/items?public=ok".parse().expect("URI");
        *request.version_mut() = http::Version::HTTP_2;
        request
            .headers_mut()
            .insert("x-request", http::HeaderValue::from_static("present"));
        request.extensions_mut().insert(RequestMarker("request"));
        request.extensions_mut().insert(RequestExecutionContext {
            meta: RequestMeta {
                endpoint: "ReqwestWholeRequest",
                method: Method::POST,
                idempotent: false,
                attempt: 3,
                page_index: 2,
            },
            timeout: Some(Duration::from_secs(7)),
        });

        let request = reqwest_request_from_http(request).expect("whole conversion should work");
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.version(), http::Version::HTTP_2);
        assert_eq!(
            request.headers().get("x-request"),
            Some(&http::HeaderValue::from_static("present"))
        );
        assert_eq!(request.timeout(), Some(&Duration::from_secs(7)));
        let request = http::Request::<reqwest::Body>::try_from(request)
            .expect("standard request conversion should be reversible");
        assert_eq!(request.uri(), "http://example.test/items?public=ok");
        assert_eq!(
            request
                .extensions()
                .get::<RequestMarker>()
                .map(|marker| marker.0),
            Some("request")
        );
        assert!(!polled.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn request_body_wrap_preserves_frames_trailers_and_hint() {
        let trailers = {
            let mut trailers = http::HeaderMap::new();
            trailers.insert("x-trailer", http::HeaderValue::from_static("present"));
            trailers
        };
        let mut hint = SizeHint::new();
        hint.set_exact(4);
        let body = crate::body::DynBody::from_frame_stream(FrameSequence {
            frames: VecDeque::from([
                Ok(Frame::data(Bytes::from_static(b"data"))),
                Ok(Frame::trailers(trailers)),
            ]),
        })
        .with_size_hint(hint);
        let mut request = http::Request::new(body);
        *request.method_mut() = Method::POST;
        *request.uri_mut() = "http://example.test/upload".parse().expect("URI");

        let request = reqwest_request_from_http(request).expect("whole conversion should work");
        let request = http::Request::<reqwest::Body>::try_from(request)
            .expect("standard request conversion should be reversible");
        assert_eq!(request.body().size_hint().exact(), Some(4));
        let collected = request.into_body().collect().await.expect("request body");
        assert_eq!(
            collected
                .trailers()
                .and_then(|trailers| trailers.get("x-trailer"))
                .and_then(|value| value.to_str().ok()),
            Some("present")
        );
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"data"));
    }

    #[tokio::test]
    async fn whole_response_conversion_preserves_head_frames_trailers_and_hint() {
        let trailers = {
            let mut trailers = http::HeaderMap::new();
            trailers.insert("x-trailer", http::HeaderValue::from_static("present"));
            trailers
        };
        let mut hint = SizeHint::new();
        hint.set_exact(4);
        let body = crate::body::DynBody::from_frame_stream(FrameSequence {
            frames: VecDeque::from([
                Ok(Frame::data(Bytes::from_static(b"data"))),
                Ok(Frame::trailers(trailers)),
            ]),
        })
        .with_size_hint(hint);
        let mut response = http::Response::new(reqwest::Body::wrap(body));
        *response.status_mut() = StatusCode::CREATED;
        *response.version_mut() = http::Version::HTTP_2;
        response
            .headers_mut()
            .insert("x-response", http::HeaderValue::from_static("present"));
        response.extensions_mut().insert(ResponseMarker("response"));

        let response = http_response_from_reqwest(reqwest::Response::from(response));
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.version(), http::Version::HTTP_2);
        assert_eq!(
            response.headers().get("x-response"),
            Some(&http::HeaderValue::from_static("present"))
        );
        assert_eq!(
            response
                .extensions()
                .get::<ResponseMarker>()
                .map(|marker| marker.0),
            Some("response")
        );
        assert_eq!(response.body().size_hint().exact(), Some(4));
        let collected = response.into_body().collect().await.expect("response body");
        assert_eq!(
            collected
                .trailers()
                .and_then(|trailers| trailers.get("x-trailer"))
                .and_then(|value| value.to_str().ok()),
            Some("present")
        );
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"data"));
    }

    #[test]
    fn managed_builder_failures_are_structural_and_url_free() {
        let error = managed_build_error();
        let _: Result<ReqwestTransport, ReqwestClientBuildError> = Err(error);
        let error = managed_build_error();
        let diagnostics = format!("{error}\n{error:?}\n{}", source_chain(&error));
        for sentinel in [
            "https://proxy-user:PROXY_SECRET@example.test",
            "proxy-user",
            "PROXY_SECRET",
            "example.test",
        ] {
            assert!(
                !diagnostics.contains(sentinel),
                "managed construction diagnostics leaked {sentinel}: {diagnostics}"
            );
        }
        assert!(diagnostics.contains("managed reqwest client construction failed"));
    }

    #[test]
    fn infallible_constructor_panic_is_static_and_drops_build_error() {
        let error = managed_build_error();
        let panic = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ReqwestTransport::from_managed_build(Err(error))
        })) {
            Ok(_) => panic!("infallible constructor helper must panic"),
            Err(panic) => panic,
        };
        let message = panic
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .expect("panic message should be static text");
        assert_eq!(message, "managed reqwest client construction failed");
        assert!(!message.contains("PROXY_SECRET"));
    }
}
