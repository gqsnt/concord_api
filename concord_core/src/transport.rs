use crate::auth::PlannedAuthPlacement;
use crate::rate_limit::RateLimitPlan;
use crate::retry_mode::{ProviderReqwestRetryInstall, ReqwestRetryInstall};
use bytes::Bytes;
#[cfg(test)]
use http::Method;
use http::{HeaderMap, StatusCode};
use http_body::{Body, Frame, SizeHint};
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use url::Url;

use std::error::Error;

#[derive(Clone, Debug)]
pub(crate) struct RequestExecutionContext {
    pub(crate) meta: crate::execution_meta::RequestExecutionMeta,
    /// The authoritative request URL before authentication transport material
    /// is applied to the native Reqwest request.
    pub(crate) logical_url: Url,
    pub(crate) timeout: Option<Duration>,
    pub(crate) body_errors: crate::body::RequestBodyErrorSlot,
}

pub(crate) struct BuiltRequest {
    pub(crate) message: reqwest::Request,
    pub(crate) context: RequestExecutionContext,
    pub(crate) auth_plan: crate::auth::AuthPlacementPlan,
    pub(crate) rate_limit: RateLimitPlan,
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
            .field("rate_limit", &self.rate_limit)
            .finish()
    }
}

impl BuiltRequest {
    pub(crate) fn debug_url(&self) -> String {
        crate::redaction::sanitize_url_for_debug(
            &self.context.logical_url,
            self.auth_plan.sensitive_query_keys.iter(),
        )
    }

    pub(crate) fn context(&self) -> &RequestExecutionContext {
        &self.context
    }
}

/// Feature-gated buffered raw-response escape hatch.
///
/// Raw response headers and body bytes have a reduced secrecy guarantee and
/// must be handled as potentially sensitive. Its URL accessor remains the
/// logical pre-authentication request URL; this type does not expose the native
/// Reqwest response or its materialized URL.
pub struct BuiltResponse {
    message: http::Response<bytes::Bytes>,
    context: ResponseContext,
}

#[derive(Clone, Debug)]
pub(crate) struct ResponseContext {
    pub(crate) meta: crate::execution_meta::RequestExecutionMeta,
    pub(crate) logical_url: Url,
    pub(crate) rate_limit: RateLimitPlan,
}

pub(crate) struct ExecutionResponse {
    pub(crate) body: BoundedResponseStream,
    pub(crate) context: ResponseContext,
}

impl ExecutionResponse {
    pub(crate) fn new(
        message: reqwest::Response,
        context: ResponseContext,
        error_mapper: NativeResponseErrorMapper,
        limit: Option<u64>,
    ) -> Self {
        Self {
            body: BoundedResponseStream::new(message, error_mapper, limit),
            context,
        }
    }

    pub(crate) fn logical_url(&self) -> &Url {
        &self.context.logical_url
    }

    pub(crate) fn status(&self) -> StatusCode {
        self.body.status()
    }

    pub(crate) fn headers(&self) -> &HeaderMap {
        self.body.headers()
    }

    pub(crate) fn set_body_limit(&mut self, limit: u64) {
        self.body.set_limit(limit);
    }

    pub(crate) fn body_mut(&mut self) -> &mut BoundedResponseStream {
        &mut self.body
    }

    #[cfg(test)]
    pub(crate) fn into_body(self) -> BoundedResponseStream {
        self.body
    }
}

/// The sole native response-byte accounting and error-mapping authority.
///
/// Consumers either call [`BoundedResponseStream::next_chunk`] (which skips
/// trailers) or use this type as an `http_body::Body` (which preserves frames).
/// Both paths execute the same `poll_frame` implementation.
pub(crate) struct BoundedResponseStream {
    body: reqwest::Body,
    status: StatusCode,
    version: http::Version,
    headers: HeaderMap,
    extensions: http::Extensions,
    limit: Option<u64>,
    seen: u64,
    terminal: bool,
    error_mapper: NativeResponseErrorMapper,
}

impl BoundedResponseStream {
    fn new(
        mut response: reqwest::Response,
        error_mapper: NativeResponseErrorMapper,
        limit: Option<u64>,
    ) -> Self {
        let status = response.status();
        let version = response.version();
        let headers = std::mem::take(response.headers_mut());
        let extensions = std::mem::take(response.extensions_mut());
        let body = response.into();
        Self {
            body,
            status,
            version,
            headers,
            extensions,
            limit,
            seen: 0,
            terminal: false,
            error_mapper,
        }
    }

    fn set_limit(&mut self, limit: u64) {
        self.limit = Some(limit);
    }

    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    pub(crate) fn version(&self) -> http::Version {
        self.version
    }

    pub(crate) fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub(crate) fn extensions(&self) -> &http::Extensions {
        &self.extensions
    }

    pub(crate) fn content_length(&self) -> Option<u64> {
        self.headers
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    }

    /// Polls until a data frame, terminal EOF, or terminal body/limit error.
    /// After an error, subsequent calls return `Ok(None)` without polling the
    /// native body again; dropping this value cancels any remaining work.
    pub(crate) async fn next_chunk(&mut self) -> Result<Option<Bytes>, crate::body::BodyError> {
        use http_body_util::BodyExt;

        loop {
            let Some(frame) = self.frame().await else {
                return Ok(None);
            };
            let frame = frame?;
            if let Ok(data) = frame.into_data() {
                return Ok(Some(data));
            }
        }
    }

    pub(crate) async fn collect_bytes(&mut self) -> Result<Bytes, crate::body::BodyError> {
        let mut collected = bytes::BytesMut::new();
        while let Some(chunk) = self.next_chunk().await? {
            collected.extend_from_slice(&chunk);
        }
        Ok(collected.freeze())
    }

    pub(crate) fn into_head(self) -> (StatusCode, http::Version, HeaderMap, http::Extensions) {
        let Self {
            status,
            version,
            headers,
            extensions,
            ..
        } = self;
        (status, version, headers, extensions)
    }
}

impl Body for BoundedResponseStream {
    type Data = Bytes;
    type Error = crate::body::BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            return Poll::Ready(None);
        }
        match Pin::new(&mut this.body).poll_frame(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.terminal = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.terminal = true;
                Poll::Ready(Some(Err(this.error_mapper.map_body_error(error))))
            }
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let actual = this.seen.saturating_add(data.len() as u64);
                    if let Some(limit) = this.limit
                        && actual > limit
                    {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(crate::body::BodyError::limit_exceeded(
                            limit, actual,
                        ))));
                    }
                    this.seen = actual;
                }
                Poll::Ready(Some(Ok(frame)))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return SizeHint::with_exact(0);
        }
        let inner = self.body.size_hint();
        let Some(limit) = self.limit else {
            return inner;
        };
        let remaining = limit.saturating_sub(self.seen);
        let mut hint = SizeHint::new();
        if inner.lower() <= remaining {
            hint.set_lower(inner.lower());
        }
        hint.set_upper(inner.upper().unwrap_or(remaining).min(remaining));
        hint
    }
}

/// Narrow error-mapping context retained with a native response body.
///
/// This carries no response metadata or body state; it only preserves the
/// managed client's proxy-aware redaction policy after `execute` returns.
#[derive(Clone)]
pub(crate) struct NativeResponseErrorMapper {
    proxies: Vec<SafeProxy>,
}

impl NativeResponseErrorMapper {
    pub(crate) fn map_body_error(&self, error: reqwest::Error) -> crate::body::BodyError {
        crate::body::BodyError::from(ReqwestError::from_reqwest(error, &self.proxies))
    }
}

impl fmt::Debug for BuiltResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltResponse")
            .field("meta", &self.context.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(
                    &self.context.logical_url,
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

#[allow(dead_code)]
impl BuiltResponse {
    pub(crate) fn new(message: http::Response<Bytes>, context: ResponseContext) -> Self {
        Self { message, context }
    }

    /// Wraps a standard buffered response with safe request execution context.
    pub fn from_http(
        message: http::Response<Bytes>,
        meta: crate::execution_meta::RequestExecutionMeta,
        request_url: Url,
        rate_limit: RateLimitPlan,
    ) -> Self {
        Self::new(
            message,
            ResponseContext {
                meta,
                logical_url: request_url,
                rate_limit,
            },
        )
    }

    pub fn meta(&self) -> &crate::execution_meta::RequestExecutionMeta {
        &self.context.meta
    }

    /// Returns the logical request URL captured before authentication material
    /// is placed on the native request.
    pub fn url(&self) -> &Url {
        &self.context.logical_url
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
    pub meta: crate::execution_meta::RequestExecutionMeta,
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

pub(crate) fn materialize_authentication(
    mut message: reqwest::Request,
    auth_plan: &crate::auth::AuthPlacementPlan,
    materials: &[crate::auth::AuthTransportMaterial],
) -> Result<reqwest::Request, crate::auth::AuthError> {
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
                let mut url = message.url().clone();
                url.query_pairs_mut()
                    .append_pair(name, secret.expose_secret());
                *message.url_mut() = url;
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

    Ok(message)
}

impl<T> DecodedResponse<T> {
    #[inline]
    pub fn meta(&self) -> &crate::execution_meta::RequestExecutionMeta {
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
    /// Returns the logical request URL captured before authentication material
    /// is placed on the native request.
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
#[allow(dead_code)]
pub(crate) enum ReqwestErrorKind {
    Timeout,
    Connect,
    Tls,
    Dns,
    Io,
    Request,
    Other,
}

pub(crate) struct ReqwestError {
    kind: ReqwestErrorKind,
    source: crate::error::FxError,
}

impl ReqwestError {
    #[inline]
    pub(crate) fn with_kind(kind: ReqwestErrorKind, e: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            source: Box::new(e),
        }
    }

    #[inline]
    pub(crate) fn kind(&self) -> ReqwestErrorKind {
        self.kind
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn source_error(&self) -> &(dyn Error + Send + Sync + 'static) {
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

    pub(crate) fn into_source(self) -> crate::error::FxError {
        self.source
    }
}

impl fmt::Display for ReqwestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "request execution error: {:?}", self.kind)
    }
}

impl fmt::Debug for ReqwestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl Error for ReqwestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

impl From<reqwest::Error> for ReqwestError {
    fn from(e: reqwest::Error) -> Self {
        let e = e.without_url();
        let kind = classify_reqwest_error(&e);
        Self {
            kind,
            source: sanitize_error_chain(&e),
        }
    }
}

impl ReqwestError {
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
    if let Some(error) = error.downcast_ref::<crate::body::BodyError>() {
        return Box::new(*error);
    }
    // Reqwest may report the resolved proxy socket (rather than the configured
    // origin) in a nested connector error. Once an explicit proxy is active,
    // retain only a stable safe category; marker replacement cannot prove that
    // resolver-produced addresses are harmless.
    if proxies.iter().any(SafeProxy::is_network_proxy) {
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
        if !proxy.is_network_proxy() {
            continue;
        }
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

impl From<crate::body::BodyError> for ReqwestError {
    fn from(error: crate::body::BodyError) -> Self {
        let kind = match error.kind() {
            crate::body::BodyErrorKind::Io => ReqwestErrorKind::Io,
            crate::body::BodyErrorKind::LimitExceeded => ReqwestErrorKind::Request,
            _ => ReqwestErrorKind::Other,
        };
        Self::with_kind(kind, error)
    }
}

fn classify_reqwest_error(err: &reqwest::Error) -> ReqwestErrorKind {
    if err.is_timeout() {
        return ReqwestErrorKind::Timeout;
    }
    if err.is_connect() {
        return ReqwestErrorKind::Connect;
    }
    if err.is_request() {
        return ReqwestErrorKind::Request;
    }
    ReqwestErrorKind::Other
}

#[derive(Clone)]
pub(crate) struct ManagedReqwestClient {
    pub(crate) client: reqwest::Client,
    configured_proxies: Vec<SafeProxy>,
    provider: ManagedProviderReqwestClient,
}

/// The separately managed Reqwest authority used only for credential-provider
/// HTTP operations.
#[derive(Clone)]
pub(crate) struct ManagedProviderReqwestClient {
    pub(crate) client: reqwest::Client,
    configured_proxies: Vec<SafeProxy>,
}

/// A credential-free explicit proxy target for the managed Reqwest transport.
/// Only a credential-free HTTP(S) origin is accepted, so a target cannot
/// become a secret-bearing diagnostic surface.
#[derive(Clone)]
pub struct SafeProxy {
    target: Url,
    #[cfg(feature = "dangerous-dev-tools")]
    test_origin_override: bool,
    #[cfg(feature = "dangerous-dev-tools")]
    _test_guard: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

impl PartialEq for SafeProxy {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && {
            #[cfg(feature = "dangerous-dev-tools")]
            {
                self.test_origin_override == other.test_origin_override
            }
            #[cfg(not(feature = "dangerous-dev-tools"))]
            {
                true
            }
        }
    }
}

impl Eq for SafeProxy {}

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
        Ok(Self {
            target,
            #[cfg(feature = "dangerous-dev-tools")]
            test_origin_override: false,
            #[cfg(feature = "dangerous-dev-tools")]
            _test_guard: None,
        })
    }

    fn is_network_proxy(&self) -> bool {
        #[cfg(feature = "dangerous-dev-tools")]
        {
            !self.test_origin_override
        }
        #[cfg(not(feature = "dangerous-dev-tools"))]
        {
            true
        }
    }

    #[doc(hidden)]
    #[cfg(feature = "dangerous-dev-tools")]
    pub fn __test_origin_override(target: &str) -> Result<Self, SafeProxyError> {
        let mut proxy = Self::all(target)?;
        proxy.test_origin_override = true;
        Ok(proxy)
    }

    #[doc(hidden)]
    #[cfg(feature = "dangerous-dev-tools")]
    pub fn __test_origin_override_with_guard(
        target: &str,
        guard: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<Self, SafeProxyError> {
        let mut proxy = Self::__test_origin_override(target)?;
        proxy._test_guard = Some(guard);
        Ok(proxy)
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
/// Raw builders/clients, default headers, cookies, redirects, application
/// retry policies, unsafe TLS, verbose wire logging, and unrestricted proxy
/// objects are absent. Provider retry selection is exposed only through its
/// narrow provider-operation mode.
pub struct SafeReqwestBuilder {
    builder: reqwest::ClientBuilder,
    provider_builder: reqwest::ClientBuilder,
    provider_retry_mode: crate::retry_mode::ProviderOperationRetryMode,
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
            provider_builder: reqwest::Client::builder().no_proxy(),
            provider_retry_mode: Default::default(),
            configured_proxies: Vec::new(),
            proxy_error: None,
        }
    }

    pub fn connect_timeout(self, timeout: Duration) -> Self {
        Self {
            builder: self.builder.connect_timeout(timeout),
            provider_builder: self.provider_builder.connect_timeout(timeout),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn read_timeout(self, timeout: Duration) -> Self {
        Self {
            builder: self.builder.read_timeout(timeout),
            provider_builder: self.provider_builder.read_timeout(timeout),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn pool_idle_timeout(self, timeout: Option<Duration>) -> Self {
        Self {
            builder: self.builder.pool_idle_timeout(timeout),
            provider_builder: self.provider_builder.pool_idle_timeout(timeout),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn pool_max_idle_per_host(self, maximum: usize) -> Self {
        Self {
            builder: self.builder.pool_max_idle_per_host(maximum),
            provider_builder: self.provider_builder.pool_max_idle_per_host(maximum),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn tcp_keepalive(self, interval: Option<Duration>) -> Self {
        Self {
            builder: self.builder.tcp_keepalive(interval),
            provider_builder: self.provider_builder.tcp_keepalive(interval),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn tcp_nodelay(self, enabled: bool) -> Self {
        Self {
            builder: self.builder.tcp_nodelay(enabled),
            provider_builder: self.provider_builder.tcp_nodelay(enabled),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn https_only(self, enabled: bool) -> Self {
        Self {
            builder: self.builder.https_only(enabled),
            provider_builder: self.provider_builder.https_only(enabled),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn http1_only(self) -> Self {
        Self {
            builder: self.builder.http1_only(),
            provider_builder: self.provider_builder.http1_only(),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "http2")]
    pub fn http2_prior_knowledge(self) -> Self {
        Self {
            builder: self.builder.http2_prior_knowledge(),
            provider_builder: self.provider_builder.http2_prior_knowledge(),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    pub fn proxy(mut self, proxy: SafeProxy) -> Self {
        self.configured_proxies.push(proxy.clone());
        #[cfg(feature = "dangerous-dev-tools")]
        if proxy.test_origin_override {
            return self;
        }
        match reqwest::Proxy::all(proxy.target.clone()) {
            Ok(application_proxy) => match reqwest::Proxy::all(proxy.target) {
                Ok(provider_proxy) => {
                    self.builder = self.builder.proxy(application_proxy);
                    self.provider_builder = self.provider_builder.proxy(provider_proxy);
                }
                Err(_) => self.proxy_error = Some(SafeProxyError::InvalidOrigin),
            },
            // A SafeProxy has already structurally validated its URL. Keep a
            // sanitized configuration failure rather than asserting that
            // Reqwest conversion cannot fail.
            Err(_) => self.proxy_error = Some(SafeProxyError::InvalidOrigin),
        }
        self
    }
    #[cfg(feature = "default-tls")]
    pub fn add_trusted_root_pem(self, pem: &[u8]) -> Result<Self, ReqwestClientBuildError> {
        let certificate = reqwest::Certificate::from_pem(pem)
            .map_err(ReqwestClientBuildError::from_builder_reqwest)?;
        Ok(Self {
            builder: self.builder.tls_certs_merge([certificate.clone()]),
            provider_builder: self.provider_builder.tls_certs_merge([certificate]),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        })
    }
    #[cfg(feature = "default-tls")]
    pub fn client_identity_pem(self, pem: &[u8]) -> Result<Self, ReqwestClientBuildError> {
        let identity = reqwest::Identity::from_pem(pem)
            .map_err(ReqwestClientBuildError::from_builder_reqwest)?;
        Ok(Self {
            builder: self.builder.identity(identity.clone()),
            provider_builder: self.provider_builder.identity(identity),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        })
    }
    #[cfg(feature = "gzip")]
    pub fn disable_gzip(self) -> Self {
        Self {
            builder: self.builder.no_gzip(),
            provider_builder: self.provider_builder.no_gzip(),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "brotli")]
    pub fn disable_brotli(self) -> Self {
        Self {
            builder: self.builder.no_brotli(),
            provider_builder: self.provider_builder.no_brotli(),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }
    #[cfg(feature = "deflate")]
    pub fn disable_deflate(self) -> Self {
        Self {
            builder: self.builder.no_deflate(),
            provider_builder: self.provider_builder.no_deflate(),
            provider_retry_mode: self.provider_retry_mode,
            configured_proxies: self.configured_proxies,
            proxy_error: self.proxy_error,
        }
    }

    /// Selects the native retry policy for credential-provider operations.
    /// This setting is independent of the protected application's retry mode.
    pub fn provider_operation_retry_mode(
        mut self,
        mode: crate::retry_mode::ProviderOperationRetryMode,
    ) -> Self {
        self.provider_retry_mode = mode;
        self
    }
}

/// A sanitized managed-Reqwest client construction failure.
pub struct ReqwestClientBuildError {
    kind: crate::error::ClientBuildErrorKind,
    source: crate::error::FxError,
}

impl ReqwestClientBuildError {
    #[cfg(feature = "default-tls")]
    fn from_builder_reqwest(error: reqwest::Error) -> Self {
        let mut result = Self::from_reqwest(error);
        result.kind = crate::error::ClientBuildErrorKind::Builder;
        result
    }

    fn from_reqwest(error: reqwest::Error) -> Self {
        let source = error.without_url();
        let kind = if source.is_builder() {
            crate::error::ClientBuildErrorKind::Builder
        } else {
            match classify_reqwest_error(&source) {
                ReqwestErrorKind::Timeout => crate::error::ClientBuildErrorKind::Timeout,
                ReqwestErrorKind::Connect => crate::error::ClientBuildErrorKind::Connect,
                ReqwestErrorKind::Request => crate::error::ClientBuildErrorKind::Request,
                ReqwestErrorKind::Io => crate::error::ClientBuildErrorKind::Body,
                ReqwestErrorKind::Tls | ReqwestErrorKind::Dns | ReqwestErrorKind::Other => {
                    crate::error::ClientBuildErrorKind::Other
                }
            }
        };
        Self {
            kind,
            source: Box::new(source),
        }
    }

    fn from_safe_proxy(error: SafeProxyError) -> Self {
        Self {
            kind: crate::error::ClientBuildErrorKind::Builder,
            source: Box::new(error),
        }
    }

    /// Returns the safe structural category of the client-build failure.
    pub fn kind(&self) -> crate::error::ClientBuildErrorKind {
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

impl ManagedReqwestClient {
    #[inline]
    pub fn new() -> Self {
        Self::from_managed_build(Self::try_new())
    }

    pub fn try_new() -> Result<Self, ReqwestClientBuildError> {
        Self::with_builder(|builder| builder)
    }

    /// Applies reviewed client-wide settings before Concord installs its
    /// redirect policy. The general retry policy defaults to Reqwest's built-in
    /// protocol recovery; a different policy is selected through the retry-mode
    /// aware constructors.
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
        Self::with_builder_fallible_retry(configure, ReqwestRetryInstall::ProtocolRecovery)
    }

    /// Retry-mode aware managed-client construction. The retry install is the
    /// single general retry authority; Concord retains no general resend path.
    pub(crate) fn with_builder_fallible_retry(
        configure: impl FnOnce(
            SafeReqwestBuilder,
        ) -> Result<SafeReqwestBuilder, ReqwestClientBuildError>,
        retry: ReqwestRetryInstall,
    ) -> Result<Self, ReqwestClientBuildError> {
        let configured = configure(SafeReqwestBuilder::new())?;
        if let Some(error) = configured.proxy_error {
            return Err(ReqwestClientBuildError::from_safe_proxy(error));
        }
        let configured_proxies = configured.configured_proxies.clone();
        let provider_proxies = configured.configured_proxies;
        let provider_retry = configured.provider_retry_mode.resolve();
        let mut builder = configured
            .builder
            .redirect(reqwest::redirect::Policy::none());
        let mut provider_builder = configured
            .provider_builder
            .redirect(reqwest::redirect::Policy::none());
        // Reqwest is the sole general HTTP retry executor. ProtocolRecovery
        // keeps Reqwest's built-in safe protocol recovery; the other modes
        // replace it entirely.
        builder = match retry {
            ReqwestRetryInstall::ProtocolRecovery => builder,
            ReqwestRetryInstall::Never => builder.retry(reqwest::retry::never()),
            ReqwestRetryInstall::Custom(policy) => builder.retry(policy),
        };
        let client = builder
            .build()
            .map_err(ReqwestClientBuildError::from_reqwest)?;
        // Provider retry selection is deliberately narrower than application
        // retry selection: status policies cannot be represented here.
        provider_builder = match provider_retry {
            ProviderReqwestRetryInstall::ProtocolRecovery => provider_builder,
            ProviderReqwestRetryInstall::Never => provider_builder.retry(reqwest::retry::never()),
        };
        let provider_client = provider_builder
            .build()
            .map_err(ReqwestClientBuildError::from_reqwest)?;
        Ok(Self {
            client,
            configured_proxies,
            provider: ManagedProviderReqwestClient {
                client: provider_client,
                configured_proxies: provider_proxies,
            },
        })
    }

    fn from_managed_build(result: Result<Self, ReqwestClientBuildError>) -> Self {
        match result {
            Ok(client) => client,
            Err(_) => panic!("managed reqwest client construction failed"),
        }
    }
}

impl Default for ManagedReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ManagedReqwestClient {
    pub(crate) async fn execute(
        &self,
        request: reqwest::Request,
        context: Option<&RequestExecutionContext>,
    ) -> Result<reqwest::Response, ReqwestError> {
        execute_managed(&self.client, &self.configured_proxies, request, context).await
    }

    pub(crate) fn provider(&self) -> &ManagedProviderReqwestClient {
        &self.provider
    }

    pub(crate) fn response_error_mapper(&self) -> NativeResponseErrorMapper {
        NativeResponseErrorMapper {
            proxies: self.configured_proxies.clone(),
        }
    }
}

impl ManagedProviderReqwestClient {
    pub(crate) async fn execute(
        &self,
        request: reqwest::Request,
        context: Option<&RequestExecutionContext>,
    ) -> Result<reqwest::Response, ReqwestError> {
        execute_managed(&self.client, &self.configured_proxies, request, context).await
    }

    pub(crate) fn response_error_mapper(&self) -> NativeResponseErrorMapper {
        NativeResponseErrorMapper {
            proxies: self.configured_proxies.clone(),
        }
    }
}

async fn execute_managed(
    client: &reqwest::Client,
    configured_proxies: &[SafeProxy],
    request: reqwest::Request,
    _context: Option<&RequestExecutionContext>,
) -> Result<reqwest::Response, ReqwestError> {
    #[cfg(feature = "dangerous-dev-tools")]
    let mut request = request;
    #[cfg(feature = "dangerous-dev-tools")]
    if let Some(target) = configured_proxies
        .iter()
        .find(|proxy| proxy.test_origin_override)
        .map(|proxy| &proxy.target)
    {
        let native_url = request.url().clone();
        let mut rewritten = target.clone();
        rewritten.set_path(native_url.path());
        rewritten.set_query(native_url.query());
        let logical_url = _context
            .map(|context| context.logical_url.clone())
            .unwrap_or_else(|| {
                // Direct internal transport tests can execute without a
                // Concord request context. In that case retain only an
                // origin-local, query-free routing representation.
                let mut opaque = target.clone();
                opaque.set_path(native_url.path());
                opaque.set_query(None);
                opaque
            });
        let logical_header = http::HeaderValue::from_str(logical_url.as_str()).map_err(|_| {
            ReqwestError::with_kind(
                ReqwestErrorKind::Request,
                std::io::Error::other("logical test URL cannot be represented safely"),
            )
        })?;
        request.headers_mut().insert(
            http::HeaderName::from_static("x-concord-test-logical-url"),
            logical_header,
        );
        if let Some(context) = _context {
            let headers = request.headers_mut();
            if let Ok(value) = http::HeaderValue::from_str(context.meta.endpoint) {
                headers.insert(
                    http::HeaderName::from_static("x-concord-test-endpoint"),
                    value,
                );
            }
            headers.insert(
                http::HeaderName::from_static("x-concord-test-page-index"),
                http::HeaderValue::from_str(&context.meta.page_index.to_string())
                    .expect("page index is a valid header value"),
            );
            if let Some(timeout) = context.timeout {
                headers.insert(
                    http::HeaderName::from_static("x-concord-test-timeout-ms"),
                    http::HeaderValue::from_str(&timeout.as_millis().to_string())
                        .expect("timeout is a valid header value"),
                );
            }
        }
        *request.url_mut() = rewritten;
    }
    #[cfg(not(feature = "default-tls"))]
    if request.url().scheme() == "https" {
        return Err(ReqwestError::with_kind(
            ReqwestErrorKind::Tls,
            TlsCapabilityUnavailable,
        ));
    }
    client
        .execute(request)
        .await
        .map_err(|error| ReqwestError::from_reqwest(error, configured_proxies))
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

#[cfg(test)]
#[allow(dead_code)]
mod reqwest_transport_tests {
    use super::*;
    use bytes::Bytes;
    use futures_core::Stream;
    use http_body::{Frame, SizeHint};
    use std::pin::Pin;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::task::{Context, Poll};

    #[derive(Clone, Debug)]
    struct RequestMarker(&'static str);

    struct PollProbe(Arc<AtomicBool>);

    impl Stream for PollProbe {
        type Item = Result<Bytes, crate::body::BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.0.store(true, Ordering::SeqCst);
            Poll::Ready(None)
        }
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

    #[tokio::test]
    async fn response_body_errors_retain_proxy_redaction_context() {
        let proxy = SafeProxy::all("http://proxy.example.test:8080").expect("safe proxy");
        let mut response = http::Response::new(reqwest::Body::wrap(FailingResponseBody));
        *response.status_mut() = StatusCode::OK;
        let mut response = reqwest::Response::from(response);
        let error = response.chunk().await.expect_err("body stream must fail");
        let error = NativeResponseErrorMapper {
            proxies: vec![proxy],
        }
        .map_body_error(error);
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
    fn managed_builder_failures_are_structural_and_url_free() {
        let error = managed_build_error();
        assert_eq!(error.kind(), crate::error::ClientBuildErrorKind::Builder);
        let _: Result<ManagedReqwestClient, ReqwestClientBuildError> = Err(error);
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
        let result = ManagedReqwestClient::with_builder_fallible(|builder| {
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
        let result = ManagedReqwestClient::with_builder_fallible(|builder| {
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
            ManagedReqwestClient::from_managed_build(Err(error))
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
        assert!(ManagedReqwestClient::with_builder(|builder| builder.proxy(proxy)).is_ok());
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
    async fn failing_proxy_target_is_absent_from_reqwest_diagnostics() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("proxy sink bind");
        let proxy_marker = listener.local_addr().expect("proxy marker");
        let proxy_server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("proxy accept");
            drop(stream);
        });
        let proxy = SafeProxy::all(&format!("http://{proxy_marker}")).expect("safe proxy");
        let client =
            ManagedReqwestClient::with_builder_fallible(|builder| Ok(builder.proxy(proxy)))
                .expect("managed client");
        let request = reqwest::Request::new(
            Method::GET,
            Url::parse("http://127.0.0.1:6554/proxy-redaction").expect("URL"),
        );
        let error = client
            .execute(request, None)
            .await
            .expect_err("proxy must fail");
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

    #[tokio::test]
    async fn managed_client_executes_native_requests_and_returns_native_responses() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            use std::io::{Read as _, Write as _};
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0_u8; 2048];
            let length = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(request.starts_with("POST /native?visible=yes HTTP/1.1"));
            assert!(request.to_ascii_lowercase().contains("x-native: present"));
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                )
                .expect("response");
        });

        let managed = ManagedReqwestClient::new();
        let mut request = reqwest::Request::new(
            Method::POST,
            Url::parse(&format!("http://{address}/native?visible=yes")).expect("URL"),
        );
        request
            .headers_mut()
            .insert("x-native", http::HeaderValue::from_static("present"));
        *request.timeout_mut() = Some(Duration::from_secs(2));
        *request.body_mut() = Some(reqwest::Body::from(Bytes::from_static(b"hi")));

        let mut response = managed
            .execute(request, None)
            .await
            .expect("native execution");
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/plain"))
        );
        assert_eq!(
            response.chunk().await.expect("native body"),
            Some(Bytes::from_static(b"ok"))
        );
        assert_eq!(response.chunk().await.expect("native EOF"), None);
        server.join().expect("server");
    }

    #[cfg(not(feature = "default-tls"))]
    #[tokio::test]
    async fn https_without_tls_is_rejected_before_reqwest_execution() {
        let request = reqwest::Request::new(
            Method::GET,
            Url::parse("https://tls-secret.example.test/path").expect("URL"),
        );
        let error = ManagedReqwestClient::new()
            .execute(request, None)
            .await
            .expect_err("HTTPS must be preflighted without TLS support");
        assert_eq!(error.kind(), ReqwestErrorKind::Tls);
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
        let request = reqwest::Request::new(
            Method::GET,
            Url::parse(&format!("http://{address}/plain-http")).expect("URL"),
        );
        let result = ManagedReqwestClient::new().execute(request, None).await;
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
