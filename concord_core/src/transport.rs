use crate::auth::{RequestExtensions, TransportAuth};
use crate::rate_limit::RateLimitPlan;
use crate::retry::RetrySetting;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use url::Url;

use std::error::Error;
use std::fmt;

#[derive(Clone, Debug)]
pub struct RequestMeta {
    pub endpoint: &'static str,
    pub method: Method,
    pub idempotent: bool,
    pub attempt: u32,
    pub page_index: u32,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct BuiltResponse {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
    pub rate_limit: RateLimitPlan,
}

#[derive(Clone, Debug)]
pub struct DecodedResponse<T> {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub value: T,
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

#[derive(Debug)]
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
        write!(f, "{}", self.source)
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        Self {
            kind: classify_reqwest_error(&e),
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
/// - Must honor `BuiltRequest` fields (url/headers/body/timeout) as appropriate.
/// - Must not leak a concrete HTTP client type in its public surface.
pub trait Transport: Send + Clone + Sync + 'static {
    fn send(
        &self,
        req: BuiltRequest,
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
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let client = self.client.clone();
        Box::pin(async move {
            let BuiltRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                retry: _,
                rate_limit,
                extensions,
            } = req;
            if let Some(TransportAuth::ClientCertificate { identity_id }) =
                extensions.transport_auth
            {
                return Err(TransportError::with_kind(
                    TransportErrorKind::Request,
                    std::io::Error::other(format!(
                        "ReqwestTransport does not support per-request client certificate identity `{identity_id}`"
                    )),
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
