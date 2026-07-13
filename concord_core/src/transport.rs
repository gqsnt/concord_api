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

impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        let e = e.without_url();
        let kind = classify_reqwest_error(&e);
        Self {
            kind,
            source: sanitize_error_chain(&e),
        }
    }
}

impl TransportError {
    fn from_reqwest(error: reqwest::Error, proxies: &[SafeProxy]) -> Self {
        let e = error.without_url();
        let kind = classify_reqwest_error(&e);
        Self {
            kind,
            source: sanitize_error_chain_with_proxies(&e, proxies),
        }
    }
}

struct SanitizedError {
    display: String,
    debug: String,
    source: Option<crate::error::FxError>,
}

impl fmt::Display for SanitizedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl fmt::Debug for SanitizedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("TransportSourceError");
        debug.field("details", &self.debug);
        if let Some(source) = &self.source {
            debug.field("source", source.as_ref());
        }
        debug.finish_non_exhaustive()
    }
}

impl std::error::Error for SanitizedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

fn sanitize_error_chain(error: &(dyn Error + 'static)) -> crate::error::FxError {
    sanitize_error_chain_with_proxies(error, &[])
}

fn sanitize_error_chain_with_proxies(
    error: &(dyn Error + 'static),
    proxies: &[SafeProxy],
) -> crate::error::FxError {
    // Reqwest may report the resolved proxy socket (rather than the configured
    // origin) in a nested connector error. Once an explicit proxy is active,
    // retain only a stable safe category; marker replacement cannot prove that
    // resolver-produced addresses are harmless.
    if !proxies.is_empty() {
        return Box::new(SanitizedError {
            display: "explicit proxy transport failure".to_string(),
            debug: "explicit proxy transport failure".to_string(),
            source: None,
        });
    }
    let source = error
        .source()
        .map(|source| sanitize_error_chain_with_proxies(source, proxies));
    let markers = proxy_redaction_markers(proxies);
    Box::new(SanitizedError {
        display: sanitize_error_text(&format!("{error}"), &markers),
        debug: sanitize_error_text(&format!("{error:?}"), &markers),
        source,
    })
}

fn sanitize_error_text(input: &str, markers: &[String]) -> String {
    if markers.is_empty() {
        return input.to_string();
    }
    let mut out = input.to_string();
    for marker in markers {
        out = out.replace(marker, "<redacted-proxy-target>");
    }
    out
}

fn proxy_redaction_markers(proxies: &[SafeProxy]) -> Vec<String> {
    use std::cmp::Reverse;

    let mut markers = Vec::<String>::new();
    for proxy in proxies {
        let scheme = proxy.target.scheme();
        let explicit_port = proxy.target.port();
        let default_port = if matches!(scheme, "http") {
            Some(80)
        } else if matches!(scheme, "https") {
            Some(443)
        } else {
            None
        };
        let host = match proxy.target.host_str() {
            Some(host) if !host.is_empty() => host,
            _ => continue,
        };
        markers.push(proxy.target.to_string());

        let add_host_form = |host: &str, scheme: &str, port: Option<u16>| -> Vec<String> {
            let mut markers = Vec::new();
            if host.contains(':') {
                let bracketed = format!("[{host}]");
                markers.push(bracketed.clone());
                markers.push(format!("{scheme}://{bracketed}"));
                markers.push(format!("{scheme}://{bracketed}/"));
                if let Some(port) = port {
                    markers.push(format!("{bracketed}:{port}"));
                    markers.push(format!("{scheme}://{bracketed}:{port}"));
                    markers.push(format!("{scheme}://{bracketed}:{port}/"));
                }
                markers.push(format!("{scheme}://[{host}]"));
                markers.push(format!("{scheme}://[{host}]/"));
            }
            markers.push(host.to_string());
            if host.is_empty() {
                return markers;
            }
            if let Some(port) = port {
                markers.push(format!("{host}:{port}"));
                markers.push(format!("{scheme}://{host}:{port}"));
                markers.push(format!("{scheme}://{host}:{port}/"));
            }
            markers.push(format!("{scheme}://{host}"));
            markers.push(format!("{scheme}://{host}/"));
            markers
        };

        markers.extend(add_host_form(host, scheme, explicit_port));

        if explicit_port.is_none()
            && let Some(port) = default_port
        {
            markers.extend(add_host_form(host, scheme, Some(port)));
        }
    }

    markers.sort_unstable_by_key(|marker| Reverse(marker.len()));
    markers.dedup();
    markers
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

#[doc(hidden)]
pub type DefaultTransport = ReqwestTransport;

impl DefaultTransportMarker for ReqwestTransport {}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    configured_proxies: Vec<SafeProxy>,
}

/// A credential-free explicit proxy target for the managed Reqwest transport.
/// Only a credential-free HTTP(S) origin is accepted, so a target cannot
/// become a secret-bearing diagnostic surface.
#[derive(Clone, Eq, PartialEq)]
pub struct SafeProxy {
    target: Url,
}

impl SafeProxy {
    pub fn all(target: &str) -> Result<Self, SafeProxyError> {
        let target = Url::parse(target).map_err(|_| SafeProxyError::InvalidOrigin)?;
        if !matches!(target.scheme(), "http" | "https")
            || target.host_str().is_none()
            || !target.username().is_empty()
            || target.password().is_some()
            || target.query().is_some()
            || target.fragment().is_some()
            || target.path() != "/"
        {
            return Err(SafeProxyError::InvalidOrigin);
        }
        if cfg!(not(feature = "default-tls")) && target.scheme() == "https" {
            return Err(SafeProxyError::TlsUnavailable);
        }
        Ok(Self { target })
    }
}

impl fmt::Debug for SafeProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SafeProxy(<configured>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeProxyError {
    InvalidOrigin,
    TlsUnavailable,
}

impl fmt::Display for SafeProxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOrigin => {
                f.write_str("proxy target must be a credential-free HTTP(S) origin")
            }
            Self::TlsUnavailable => {
                f.write_str("HTTPS proxy configuration requires Concord's `default-tls` feature")
            }
        }
    }
}

impl Error for SafeProxyError {}

/// Concord's deliberately small managed Reqwest configuration surface.
/// Raw builders/clients, default headers, cookies, redirects, retries, unsafe
/// TLS, verbose wire logging, and unrestricted proxy objects are absent.
pub struct SafeReqwestBuilder {
    builder: reqwest::ClientBuilder,
    configured_proxies: Vec<SafeProxy>,
    proxy_error: Option<SafeProxyError>,
}

impl SafeReqwestBuilder {
    fn new() -> Self {
        // Concord does not activate Reqwest's `system-proxy` feature. This is
        // also explicit at runtime so feature unification cannot change the
        // managed client's proxy policy.
        Self {
            builder: reqwest::Client::builder().no_proxy(),
            configured_proxies: Vec::new(),
            proxy_error: None,
        }
    }

    pub fn connect_timeout(self, timeout: Duration) -> Self {
        Self {
            builder: self.builder.connect_timeout(timeout),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn read_timeout(self, timeout: Duration) -> Self {
        Self {
            builder: self.builder.read_timeout(timeout),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn pool_idle_timeout(self, timeout: Option<Duration>) -> Self {
        Self {
            builder: self.builder.pool_idle_timeout(timeout),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn pool_max_idle_per_host(self, maximum: usize) -> Self {
        Self {
            builder: self.builder.pool_max_idle_per_host(maximum),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn tcp_keepalive(self, interval: Option<Duration>) -> Self {
        Self {
            builder: self.builder.tcp_keepalive(interval),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn tcp_nodelay(self, enabled: bool) -> Self {
        Self {
            builder: self.builder.tcp_nodelay(enabled),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn https_only(self, enabled: bool) -> Self {
        Self {
            builder: self.builder.https_only(enabled),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn http1_only(self) -> Self {
        Self {
            builder: self.builder.http1_only(),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "http2")]
    pub fn http2_prior_knowledge(self) -> Self {
        Self {
            builder: self.builder.http2_prior_knowledge(),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn proxy(mut self, proxy: SafeProxy) -> Self {
        self.configured_proxies.push(proxy.clone());
        match reqwest::Proxy::all(proxy.target) {
            Ok(proxy) => self.builder = self.builder.proxy(proxy),
            // A SafeProxy has already structurally validated its URL. Keep a
            // sanitized configuration failure rather than asserting that
            // Reqwest conversion cannot fail.
            Err(_) => self.proxy_error = Some(SafeProxyError::InvalidOrigin),
        }
        self
    }
    #[cfg(feature = "default-tls")]
    pub fn add_trusted_root_pem(self, pem: &[u8]) -> Result<Self, ReqwestClientBuildError> {
        let certificate =
            reqwest::Certificate::from_pem(pem).map_err(ReqwestClientBuildError::from_reqwest)?;
        Ok(Self {
            builder: self.builder.tls_certs_merge([certificate]),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        })
    }
    #[cfg(feature = "default-tls")]
    pub fn client_identity_pem(self, pem: &[u8]) -> Result<Self, ReqwestClientBuildError> {
        let identity =
            reqwest::Identity::from_pem(pem).map_err(ReqwestClientBuildError::from_reqwest)?;
        Ok(Self {
            builder: self.builder.identity(identity),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        })
    }
    #[cfg(feature = "gzip")]
    pub fn disable_gzip(self) -> Self {
        Self {
            builder: self.builder.no_gzip(),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "brotli")]
    pub fn disable_brotli(self) -> Self {
        Self {
            builder: self.builder.no_brotli(),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "deflate")]
    pub fn disable_deflate(self) -> Self {
        Self {
            builder: self.builder.no_deflate(),
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
}

/// A sanitized managed-Reqwest client construction failure.
pub struct ReqwestClientBuildError {
    kind: TransportErrorKind,
    source: crate::error::FxError,
}

impl ReqwestClientBuildError {
    fn from_reqwest(error: reqwest::Error) -> Self {
        let source = error.without_url();
        Self {
            kind: classify_reqwest_error(&source),
            source: Box::new(source),
        }
    }

    fn from_safe_proxy(error: SafeProxyError) -> Self {
        Self {
            kind: TransportErrorKind::Request,
            source: Box::new(error),
        }
    }

    /// Returns the safe structural category of the client-build failure.
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }
}

impl fmt::Display for ReqwestClientBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "managed reqwest client construction failed ({:?})",
            self.kind
        )
    }
}

impl fmt::Debug for ReqwestClientBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestClientBuildError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for ReqwestClientBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

impl ReqwestTransport {
    #[inline]
    pub fn new() -> Self {
        Self::from_managed_build(Self::try_new())
    }

    pub fn try_new() -> Result<Self, ReqwestClientBuildError> {
        Self::with_builder(|builder| builder)
    }

    /// Applies reviewed client-wide settings before Concord installs its
    /// non-negotiable retry and redirect policies.
    pub fn with_builder(
        configure: impl FnOnce(SafeReqwestBuilder) -> SafeReqwestBuilder,
    ) -> Result<Self, ReqwestClientBuildError> {
        Self::with_builder_fallible(|builder| Ok(configure(builder)))
    }

    /// Fallible form for configuration operations that can fail (for example,
    /// PEM parsing). This keeps the public safe-construction path non-panicking.
    pub fn with_builder_fallible(
        configure: impl FnOnce(
            SafeReqwestBuilder,
        ) -> Result<SafeReqwestBuilder, ReqwestClientBuildError>,
    ) -> Result<Self, ReqwestClientBuildError> {
        let configured = configure(SafeReqwestBuilder::new())?;
        if let Some(error) = configured.proxy_error {
            return Err(ReqwestClientBuildError::from_safe_proxy(error));
        }
        let configured_proxies = configured.configured_proxies.clone();
        let client = configured
            .builder
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(ReqwestClientBuildError::from_reqwest)?;
        Ok(Self {
            client,
            configured_proxies,
        })
    }

    fn from_managed_build(result: Result<Self, ReqwestClientBuildError>) -> Self {
        match result {
            Ok(transport) => transport,
            Err(_) => panic!("managed reqwest client construction failed"),
        }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

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
        let configured_proxies = self.configured_proxies.clone();
        Box::pin(async move {
            #[cfg(not(feature = "default-tls"))]
            if request.uri().scheme_str() == Some("https") {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Tls,
                    TlsCapabilityUnavailable,
                ));
            }
            let request = reqwest_request_from_http(request)?;
            let response = client
                .execute(request)
                .await
                .map_err(|error| TransportError::from_reqwest(error, &configured_proxies))?;
            Ok(http_response_from_reqwest(response, &configured_proxies))
        })
    }
}

#[cfg(not(feature = "default-tls"))]
#[derive(Debug)]
struct TlsCapabilityUnavailable;

#[cfg(not(feature = "default-tls"))]
impl fmt::Display for TlsCapabilityUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HTTPS execution requires Concord's `default-tls` feature")
    }
}

#[cfg(not(feature = "default-tls"))]
impl Error for TlsCapabilityUnavailable {}

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

fn http_response_from_reqwest(
    response: reqwest::Response,
    proxies: &[SafeProxy],
) -> http::Response<crate::body::DynBody> {
    let response: http::Response<reqwest::Body> = response.into();
    response.map(|body| {
        crate::body::DynBody::from_body(SanitizedReqwestBody {
            inner: body,
            proxies: proxies.to_vec(),
        })
    })
}

struct SanitizedReqwestBody {
    inner: reqwest::Body,
    proxies: Vec<SafeProxy>,
}

impl http_body::Body for SanitizedReqwestBody {
    type Data = bytes::Bytes;
    type Error = crate::body::BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        http_body::Body::poll_frame(Pin::new(&mut self.inner), cx).map(|frame| {
            let proxies = &self.proxies;
            frame.map(|result| result.map_err(|error| sanitized_reqwest_body_error(error, proxies)))
        })
    }

    fn is_end_stream(&self) -> bool {
        http_body::Body::is_end_stream(&self.inner)
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::Body::size_hint(&self.inner)
    }
}

fn sanitized_reqwest_body_error(
    error: reqwest::Error,
    proxies: &[SafeProxy],
) -> crate::body::BodyError {
    crate::body::BodyError::from(TransportError::from_reqwest(error, proxies))
}

#[cfg(test)]
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

    struct FailingResponseBody;

    impl http_body::Body for FailingResponseBody {
        type Data = Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(Some(Err(std::io::Error::other(
                "resolved proxy socket 203.0.113.99:49152 failed ✓",
            ))))
        }

        fn is_end_stream(&self) -> bool {
            false
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::default()
        }
    }

    impl Stream for FrameSequence {
        type Item = Result<Frame<Bytes>, crate::body::BodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.frames.pop_front())
        }
    }

    #[tokio::test]
    async fn response_body_errors_retain_proxy_redaction_context() {
        let proxy = SafeProxy::all("http://proxy.example.test:8080").expect("safe proxy");
        let mut response = http::Response::new(reqwest::Body::wrap(FailingResponseBody));
        *response.status_mut() = StatusCode::OK;
        let response = http_response_from_reqwest(reqwest::Response::from(response), &[proxy]);
        let error = response
            .into_body()
            .collect()
            .await
            .expect_err("body stream must fail");
        let diagnostics = format!("{error}\n{error:?}");
        assert!(!diagnostics.contains("proxy.example.test"));
        assert!(!diagnostics.contains("203.0.113.99"));
        assert!(!diagnostics.contains("49152"));
    }

    fn managed_build_error() -> ReqwestClientBuildError {
        let error = reqwest::Proxy::all("http://proxy-user:PROXY_SECRET@")
            .expect_err("invalid Reqwest input should fail construction");
        ReqwestClientBuildError::from_reqwest(error)
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

    fn assert_absent_in_error_chain(error: &(dyn Error + 'static), needle: &str) {
        let mut current = Some(error as &(dyn Error + 'static));
        while let Some(source) = current {
            let rendered = format!("{source}\n{source:?}");
            assert!(
                !rendered.contains(needle),
                "proxy target leaked in error chain: {rendered}"
            );
            current = source.source();
        }
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

        let response = http_response_from_reqwest(reqwest::Response::from(response), &[]);
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

    #[cfg(feature = "default-tls")]
    #[test]
    fn safe_builder_rejects_invalid_trusted_root_pem_without_leaking_secret() {
        let marker = "PEM_SENTINEL_ROOT";
        let input = format!(
            "-----BEGIN CERTIFICATE-----\n{marker}\nnot-base64-content\n-----END CERTIFICATE-----"
        );
        let result = ReqwestTransport::with_builder_fallible(|builder| {
            builder.add_trusted_root_pem(input.as_bytes())
        });
        let error = match result {
            Ok(_) => panic!("invalid cert must fail"),
            Err(error) => error,
        };
        let mut diagnostics = format!("{error}").to_string();
        diagnostics.push('\n');
        diagnostics.push_str(&format!("{:?}", error));
        assert!(!diagnostics.contains(marker), "{diagnostics}");
        let mut source: &(dyn Error + 'static) = &error;
        while let Some(next) = source.source() {
            let rendered = format!("{next}\n{next:?}");
            assert!(!rendered.contains(marker), "{rendered}");
            source = next;
        }
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn safe_builder_rejects_invalid_client_identity_pem_without_leaking_secret() {
        let marker = "PEM_SENTINEL_IDENTITY";
        let input = format!(
            "-----BEGIN PRIVATE KEY-----\n{marker}\nnot-a-valid-private-key\n-----END PRIVATE KEY-----"
        );
        let result = ReqwestTransport::with_builder_fallible(|builder| {
            builder.client_identity_pem(input.as_bytes())
        });
        let error = match result {
            Ok(_) => panic!("invalid identity must fail"),
            Err(error) => error,
        };
        let diagnostics = format!("{error}\n{:?}", error);
        assert!(!diagnostics.contains(marker), "{diagnostics}");
        let mut source: &(dyn Error + 'static) = &error;
        while let Some(next) = source.source() {
            let rendered = format!("{next}\n{next:?}");
            assert!(!rendered.contains(marker), "{rendered}");
            source = next;
        }
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

    #[test]
    fn safe_proxy_accepts_only_credential_free_origins() {
        for target in [
            "ftp://proxy.example.test",
            "http://user:password@proxy.example.test",
            "http://proxy.example.test/path",
            "http://proxy.example.test/?query=value",
            "http://proxy.example.test/#fragment",
        ] {
            assert!(SafeProxy::all(target).is_err(), "{target}");
        }
        assert!(SafeProxy::all("http://proxy.example.test:8080").is_ok());
    }

    #[test]
    fn safe_proxy_conversion_has_no_panic_path() {
        let proxy = SafeProxy::all("http://proxy.example.test:8080").expect("safe proxy");
        assert!(ReqwestTransport::with_builder(|builder| builder.proxy(proxy)).is_ok());
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_build_rejects_https_proxy_with_clear_tls_message() {
        let error = SafeProxy::all("https://proxy.example.test:443")
            .expect_err("HTTPS proxy must be blocked without TLS support");
        assert_eq!(error, SafeProxyError::TlsUnavailable);
        assert!(
            format!("{error}").contains("default-tls"),
            "diagnostic should describe TLS capability requirement"
        );
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_rejects_non_origin_https_proxy_as_invalid_origin() {
        let error = SafeProxy::all("https://user:pass@proxy.example.test:443")
            .expect_err("HTTPS proxy with credentials must be rejected by origin validation");
        assert_eq!(error, SafeProxyError::InvalidOrigin);
        let path_error = SafeProxy::all("https://proxy.example.test:443/path")
            .expect_err("HTTPS proxy with path must be rejected by origin validation");
        assert_eq!(path_error, SafeProxyError::InvalidOrigin);
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn https_proxy_is_accepted_when_tls_is_enabled() {
        assert!(SafeProxy::all("https://proxy.example.test:443").is_ok());
    }

    #[tokio::test]
    async fn failing_proxy_target_is_absent_from_transport_diagnostics() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("proxy sink bind");
        let proxy_marker = listener.local_addr().expect("proxy marker");
        let proxy_server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("proxy accept");
            drop(stream);
        });
        let proxy = SafeProxy::all(&format!("http://{proxy_marker}")).expect("safe proxy");
        let transport = ReqwestTransport::with_builder_fallible(|builder| Ok(builder.proxy(proxy)))
            .expect("managed transport");
        let request = http::Request::builder()
            .uri("http://127.0.0.1:6554/proxy-redaction")
            .body(crate::body::DynBody::empty())
            .expect("request");
        let error = transport.send(request).await.expect_err("proxy must fail");
        let marker = proxy_marker.to_string();
        let diagnostics = format!(
            "{error}\n{error:?}\n{}\n{}",
            error.source_error(),
            source_chain(&error)
        );
        assert!(!diagnostics.contains(&marker), "{diagnostics}");
        assert_absent_in_error_chain(&error, &marker);
        proxy_server.join().expect("proxy thread");
    }

    #[test]
    fn proxy_redaction_handles_default_port_without_network_access() {
        let proxy = SafeProxy::all("http://127.0.0.1").expect("safe proxy");
        let source = std::io::Error::other(
            "connect 127.0.0.1:80 failed; unrelated UTF-8 ✓ text remains meaningful",
        );
        let sanitized = sanitize_error_chain_with_proxies(&source, &[proxy]);
        let diagnostics = format!("{sanitized}\n{sanitized:?}");
        assert!(!diagnostics.contains("127.0.0.1"));
        assert!(!diagnostics.contains("127.0.0.1:80"));
        assert!(diagnostics.contains("explicit proxy transport failure"));
    }

    #[cfg(not(feature = "default-tls"))]
    #[tokio::test]
    async fn https_without_tls_is_rejected_before_reqwest_execution() {
        let request = http::Request::builder()
            .uri("https://tls-secret.example.test/path")
            .body(crate::body::DynBody::empty())
            .expect("request");
        let error = ReqwestTransport::new()
            .send(request)
            .await
            .expect_err("HTTPS must be preflighted without TLS support");
        assert_eq!(error.kind(), TransportErrorKind::Tls);
        let diagnostics = format!("{error}\n{error:?}\n{}", error.source_error());
        assert!(diagnostics.contains("default-tls"));
        assert!(!diagnostics.contains("tls-secret.example.test"));
    }

    #[cfg(not(feature = "default-tls"))]
    #[tokio::test]
    async fn http_without_tls_reaches_reqwest_execution() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            use std::io::{Read as _, Write as _};
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .expect("response");
        });
        let request = http::Request::builder()
            .uri(format!("http://{address}/plain-http"))
            .body(crate::body::DynBody::empty())
            .expect("request");
        let result = ReqwestTransport::new().send(request).await;
        let response = match result {
            Ok(response) => response,
            Err(error) => panic!(
                "HTTP must remain available without TLS: {error:?} source={}",
                error.source_error()
            ),
        };
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        server.join().expect("server");
    }
}
