use crate::auth::{PendingAuthPlacement, RequestExtensions};
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

#[derive(Clone)]
pub struct BuiltRequest {
    pub meta: RequestMeta,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<bytes::Bytes>,
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
            .field("headers", &crate::debug::RedactedHeaders(&self.headers))
            .field(
                "body",
                &self
                    .body
                    .as_ref()
                    .map(|body| format!("<{} bytes>", body.len())),
            )
            .field("timeout", &self.timeout)
            .field("retry", &self.retry)
            .field("rate_limit", &self.rate_limit)
            .field("extensions", &self.extensions)
            .finish()
    }
}

impl BuiltRequest {
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
            .field("headers", &crate::debug::RedactedHeaders(&self.headers))
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
            .field("headers", &crate::debug::RedactedHeaders(&self.headers))
            .field("value", &self.value)
            .finish()
    }
}

#[derive(Clone)]
pub struct TransportRequest {
    pub meta: RequestMeta,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<bytes::Bytes>,
    pub timeout: Option<Duration>,
    pub rate_limit: RateLimitPlan,
    pub transport_auth: Option<TransportAuth>,
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
                PendingAuthPlacement::Query(_) | PendingAuthPlacement::Certificate => {}
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
            .field("headers", &crate::debug::RedactedHeaders(&headers))
            .field(
                "body",
                &self
                    .body
                    .as_ref()
                    .map(|body| format!("<{} bytes>", body.len())),
            )
            .field("timeout", &self.timeout)
            .field("rate_limit", &self.rate_limit)
            .field("transport_auth", &self.transport_auth)
            .field("extensions", &self.extensions)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum TransportAuth {
    ClientCertificate { identity_id: String },
}

impl fmt::Debug for TransportAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClientCertificate { identity_id } => f
                .debug_struct("ClientCertificate")
                .field(
                    "identity_id",
                    &format_args!("<redacted:{}>", identity_id.len()),
                )
                .finish(),
        }
    }
}

pub(crate) fn validate_transport_auth_collisions(
    built: &BuiltRequest,
) -> Result<(), crate::auth::AuthError> {
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
            PendingAuthPlacement::Certificate => {}
        }
    }

    Ok(())
}

pub(crate) fn materialize_transport_request(
    built: &BuiltRequest,
    materials: &[crate::auth::AuthTransportMaterial],
) -> Result<TransportRequest, crate::auth::AuthError> {
    use base64::Engine;
    use http::header::{AUTHORIZATION, HeaderValue};
    use std::collections::HashMap;

    let mut by_slot = HashMap::new();
    for material in materials {
        let slot_id = match material {
            crate::auth::AuthTransportMaterial::Secret { slot_id, .. }
            | crate::auth::AuthTransportMaterial::Basic { slot_id, .. }
            | crate::auth::AuthTransportMaterial::Certificate { slot_id, .. } => *slot_id,
        };
        by_slot.insert(slot_id, material);
    }

    validate_transport_auth_collisions(built)?;

    let mut req = TransportRequest {
        meta: built.meta.clone(),
        url: built.url.clone(),
        headers: built.headers.clone(),
        body: built.body.clone(),
        timeout: built.timeout,
        rate_limit: built.rate_limit.clone(),
        transport_auth: None,
        extensions: built.extensions.clone(),
    };

    for slot in &built.extensions.pending_auth_slots {
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
                let value = format!("Bearer {}", secret.expose());
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
                let value = HeaderValue::from_str(secret.expose()).map_err(|_| {
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
                req.url.query_pairs_mut().append_pair(name, secret.expose());
            }
            (
                PendingAuthPlacement::Basic,
                crate::auth::AuthTransportMaterial::Basic {
                    username, password, ..
                },
            ) => {
                let raw = format!("{}:{}", username.expose(), password.expose());
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
            (
                PendingAuthPlacement::Certificate,
                crate::auth::AuthTransportMaterial::Certificate { identity_id, .. },
            ) => {
                req.transport_auth = Some(TransportAuth::ClientCertificate {
                    identity_id: identity_id.clone(),
                });
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

    Ok(req)
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
        let kind = classify_reqwest_error(&e);
        let e = e.without_url();
        Self {
            kind,
            source: Box::new(e),
        }
    }
}

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
            || msg.contains("certificate")
            || msg.contains("handshake")
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

#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

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

struct ReqwestBody {
    resp: reqwest::Response,
}

impl TransportBody for ReqwestBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { self.resp.chunk().await.map_err(TransportError::from) })
    }
}

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
                transport_auth,
                ..
            } = req;
            if matches!(
                transport_auth,
                Some(TransportAuth::ClientCertificate { .. })
            ) {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Request,
                    std::io::Error::other(
                        "ReqwestTransport does not support per-request client certificate auth",
                    ),
                ));
            }
            // reqwest needs an owned Url; we keep a copy for returning meta.
            let url_for_resp = url.clone();
            let method = meta.method.clone();
            let mut rb = client.request(method, url).headers(headers);
            if let Some(b) = body {
                rb = rb.body(b);
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
