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
    pub(crate) body_category: crate::io::ProducedBodyCategory,
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
            .field("body_category", &self.body_category)
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
        body_category,
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

    message.extensions_mut().insert(body_category);

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
        let kind = classify_reqwest_error(&e);
        let e = e.without_url();
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
            let timeout = request
                .extensions()
                .get::<RequestExecutionContext>()
                .and_then(|context| context.timeout);
            let body_category = request
                .extensions()
                .get::<crate::io::ProducedBodyCategory>()
                .copied()
                // Requests constructed by a custom caller do not have a
                // prepared-body category. Preserve their body rather than
                // silently omitting it.
                .unwrap_or(crate::io::ProducedBodyCategory::OneShot);
            let (parts, body) = request.into_parts();
            let url = Url::parse(&parts.uri.to_string())
                .map_err(|error| TransportError::with_kind(TransportErrorKind::Request, error))?;
            let mut rb = client
                .request(parts.method, url)
                .version(parts.version)
                .headers(parts.headers);
            if reqwest_bridge_attaches_body(body_category, &body) {
                rb = rb.body(reqwest::Body::wrap_stream(body.into_data_stream()));
            }
            if let Some(t) = timeout {
                rb = rb.timeout(t);
            }
            let resp = rb.send().await.map_err(TransportError::from)?;
            let status = resp.status();
            let version = resp.version();
            let headers = resp.headers().clone();
            let body = crate::body::DynBody::from_byte_stream(resp.bytes_stream());
            let mut response = http::Response::new(body);
            *response.status_mut() = status;
            *response.version_mut() = version;
            *response.headers_mut() = headers;
            Ok(response)
        })
    }
}

#[cfg(feature = "transport-reqwest")]
fn reqwest_bridge_attaches_body(
    category: crate::io::ProducedBodyCategory,
    body: &crate::body::DynBody,
) -> bool {
    !matches!(
        category,
        crate::io::ProducedBodyCategory::Empty | crate::io::ProducedBodyCategory::ReusableBytes
    ) || !http_body::Body::is_end_stream(body)
}

#[cfg(all(test, feature = "transport-reqwest"))]
mod reqwest_bridge_tests {
    use super::*;
    use bytes::Bytes;
    use futures_core::Stream;
    use http_body::SizeHint;
    use std::pin::Pin;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::task::{Context, Poll};

    struct PollProbe {
        polled: Arc<AtomicBool>,
        error: bool,
    }

    impl Stream for PollProbe {
        type Item = Result<Bytes, crate::body::BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.polled.store(true, Ordering::SeqCst);
            if self.error {
                Poll::Ready(Some(Err(crate::body::BodyError::input())))
            } else {
                Poll::Ready(None)
            }
        }
    }

    fn exact_zero_hint() -> SizeHint {
        let mut hint = SizeHint::new();
        hint.set_exact(0);
        hint
    }

    #[test]
    fn reqwest_bridge_omits_only_known_empty_categories() {
        let mut empty = crate::io::PreparedBody::empty();
        let empty = empty.produce_for_attempt().expect("empty body");
        let category = empty.category();
        assert!(!reqwest_bridge_attaches_body(
            category,
            &empty.into_dyn_body()
        ));

        let mut bytes = crate::io::PreparedBody::reusable_bytes(Bytes::new(), None);
        let bytes = bytes.produce_for_attempt().expect("reusable bytes");
        let category = bytes.category();
        assert!(!reqwest_bridge_attaches_body(
            category,
            &bytes.into_dyn_body()
        ));
    }

    #[test]
    fn reqwest_bridge_attaches_zero_one_shot_and_factory_without_polling() {
        let one_shot_polled = Arc::new(AtomicBool::new(false));
        let one_shot = crate::body::DynBody::from_byte_stream(PollProbe {
            polled: one_shot_polled.clone(),
            error: false,
        })
        .with_size_hint(exact_zero_hint());
        let mut one_shot = crate::io::PreparedBody::one_shot(one_shot, None);
        let one_shot = one_shot.produce_for_attempt().expect("one-shot body");
        let category = one_shot.category();
        assert!(reqwest_bridge_attaches_body(
            category,
            &one_shot.into_dyn_body()
        ));
        assert!(!one_shot_polled.load(Ordering::SeqCst));

        let factory_polled = Arc::new(AtomicBool::new(false));
        let factory_probe = factory_polled.clone();
        let mut factory =
            crate::io::PreparedBody::replay_factory(exact_zero_hint(), None, move || {
                Ok(crate::body::DynBody::from_byte_stream(PollProbe {
                    polled: factory_probe.clone(),
                    error: false,
                }))
            });
        let factory = factory.produce_for_attempt().expect("factory body");
        let category = factory.category();
        assert!(reqwest_bridge_attaches_body(
            category,
            &factory.into_dyn_body()
        ));
        assert!(!factory_polled.load(Ordering::SeqCst));
    }

    #[test]
    fn reqwest_bridge_does_not_discard_zero_error_body_or_poll_it() {
        let polled = Arc::new(AtomicBool::new(false));
        let body = crate::body::DynBody::from_byte_stream(PollProbe {
            polled: polled.clone(),
            error: true,
        })
        .with_size_hint(exact_zero_hint());
        let mut prepared = crate::io::PreparedBody::one_shot(body, None);
        let produced = prepared.produce_for_attempt().expect("one-shot error body");
        let category = produced.category();
        assert!(reqwest_bridge_attaches_body(
            category,
            &produced.into_dyn_body()
        ));
        assert!(!polled.load(Ordering::SeqCst));
    }
}
