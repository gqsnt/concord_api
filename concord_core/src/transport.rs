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
    pub meta: RequestMeta,
    pub url: Url,
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
            let BuiltRequest { meta, url, headers, body, timeout } = req;
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
                body: Box::new(ReqwestBody { resp }),
            })
        })
    }
}
