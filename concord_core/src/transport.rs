use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;
use url::Url;

use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;

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
}

#[derive(Clone, Debug)]
pub struct BuiltResponse {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
}

#[derive(Clone, Debug)]
pub struct DecodedResponse<T> {
    pub meta: RequestMeta,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub value: T,
}

#[derive(Debug)]
pub struct TransportError(crate::error::FxError);

impl TransportError {
    #[inline]
    pub fn new(e: impl Error + Send + Sync + 'static) -> Self {
        Self(Box::new(e))
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.0)
    }
}

impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        Self::new(e)
    }
}

pub trait TransportBody: Send + 'static {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>>;
}

pub struct TransportResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub content_length: Option<u64>,
    pub body: Box<dyn TransportBody>,
}

/// Injectable transport layer.
///
/// Contract:
/// - Must honor `BuiltRequest` fields (url/headers/body/timeout) as appropriate.
/// - Must not leak a concrete HTTP client type in its public surface.
pub trait Transport: Send + Sync + 'static {
    fn send<'a>(
        &'a self,
        req: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send + 'a>>;
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
    fn send<'a>(
        &'a self,
        req: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send + 'a>> {
        let client = self.client.clone();
        let method = req.meta.method.clone();
        let url = req.url.clone();
        let headers = req.headers.clone();
        let body = req.body.clone();
        let timeout = req.timeout;
        Box::pin(async move {
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
                status,
                headers,
                content_length,
                body: Box::new(ReqwestBody { resp }),
            })
        })
    }
}
