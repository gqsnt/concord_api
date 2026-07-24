use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
#[cfg(test)]
use bytes::BytesMut;
use futures_core::Stream;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, StatusCode, Version};

#[cfg(test)]
use concord_test_support::executor::ApplicationChannel;
#[cfg(test)]
use concord_test_support::script::BoundaryRequest;

use crate::body::{BodyFailureState, NativeBody};
use crate::endpoint::EndpointId;
use crate::error::{
    ApiError, ClientBuildError, ClientBuildErrorKind, ConfigurationErrorKind, HttpResponseFailure,
};
use crate::hooks::BodyKind;

use super::build::TlsCapability;
use super::status::{StatusCapture, StatusDisposition, collect_terminal_status};

pub(crate) struct ApplicationRequest {
    native: reqwest::Request,
    body_failure: Option<BodyFailureState>,
}

impl ApplicationRequest {
    pub(crate) fn insert_auth_header(&mut self, name: http::HeaderName, value: http::HeaderValue) {
        self.native.headers_mut().insert(name, value);
    }

    pub(crate) fn append_auth_query(
        &mut self,
        name: &str,
        value: &str,
        encoded_pair_len: usize,
    ) -> Result<(), ()> {
        let separator_len = usize::from(
            self.native
                .url()
                .query()
                .is_some_and(|query| !query.is_empty()),
        );
        self.native
            .url()
            .as_str()
            .len()
            .checked_add(separator_len)
            .and_then(|length| length.checked_add(encoded_pair_len))
            .ok_or(())?;
        self.native
            .url_mut()
            .query_pairs_mut()
            .append_pair(name, value);
        Ok(())
    }
}

pub(crate) struct ApplicationResponse {
    native: reqwest::Response,
    endpoint: EndpointId,
    method: Method,
}

impl ApplicationResponse {
    pub(crate) fn status(&self) -> StatusCode {
        self.native.status()
    }

    pub(crate) fn version(&self) -> Version {
        self.native.version()
    }

    pub(crate) fn headers(&self) -> &HeaderMap {
        self.native.headers()
    }

    pub(crate) fn content_length(&self) -> Option<u64> {
        self.native.content_length()
    }

    pub(crate) async fn chunk(&mut self) -> Result<Option<Bytes>, ApiError> {
        self.native.chunk().await.map_err(|error| {
            drop(error);
            ApiError::classified(
                self.endpoint.clone(),
                self.method.clone(),
                crate::error::ErrorCategory::ResponseBody,
            )
        })
    }

    pub(crate) fn into_byte_stream(self, limit: u64) -> ApplicationByteStream {
        let endpoint = self.endpoint.clone();
        let method = self.method.clone();
        let source: ApplicationByteStream = Box::pin(ReducedResponseStream {
            inner: Box::pin(self.native.bytes_stream()),
            endpoint: endpoint.clone(),
            method: self.method,
        });
        bounded_byte_stream(source, endpoint, method, limit)
    }

    fn into_native(self) -> reqwest::Response {
        self.native
    }
}

pub(crate) type ApplicationByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, ApiError>> + Send + 'static>>;

pub(crate) fn bounded_byte_stream(
    source: ApplicationByteStream,
    endpoint: EndpointId,
    method: Method,
    limit: u64,
) -> ApplicationByteStream {
    Box::pin(BoundedResponseStream {
        source,
        endpoint,
        method,
        limit,
        observed: 0,
        finished: false,
    })
}

struct BoundedResponseStream {
    source: ApplicationByteStream,
    endpoint: EndpointId,
    method: Method,
    limit: u64,
    observed: u64,
    finished: bool,
}

impl Stream for BoundedResponseStream {
    type Item = Result<Bytes, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        match this.source.as_mut().poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.finished = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.finished = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(Some(Ok(bytes))) => {
                let Some(next) = this
                    .observed
                    .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                else {
                    this.finished = true;
                    return Poll::Ready(Some(Err(ApiError::classified(
                        this.endpoint.clone(),
                        this.method.clone(),
                        crate::error::ErrorCategory::ResponseBody,
                    ))));
                };
                if next > this.limit {
                    this.finished = true;
                    return Poll::Ready(Some(Err(ApiError::classified(
                        this.endpoint.clone(),
                        this.method.clone(),
                        crate::error::ErrorCategory::ResponseBody,
                    ))));
                }
                this.observed = next;
                Poll::Ready(Some(Ok(bytes)))
            }
        }
    }
}

struct ReducedResponseStream<S> {
    inner: Pin<Box<S>>,
    endpoint: EndpointId,
    method: Method,
}

impl<S> Stream for ReducedResponseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>>,
{
    type Item = Result<Bytes, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(context) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(error))) => {
                drop(error);
                Poll::Ready(Some(Err(ApiError::classified(
                    this.endpoint.clone(),
                    this.method.clone(),
                    crate::error::ErrorCategory::ResponseBody,
                ))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Private application HTTP owner. Its Reqwest client never crosses the
/// managed boundary.
pub(crate) struct ApplicationHttp {
    client: reqwest::Client,
    tls: TlsCapability,
    https_only: bool,
    status_body_limit: Option<NonZeroUsize>,
    retry_after_cap: Duration,
    #[cfg(test)]
    development: Option<ApplicationChannel>,
}

impl ApplicationHttp {
    pub(super) const fn new(
        client: reqwest::Client,
        tls: TlsCapability,
        https_only: bool,
        status_body_limit: Option<NonZeroUsize>,
        retry_after_cap: Duration,
    ) -> Self {
        Self {
            client,
            tls,
            https_only,
            status_body_limit,
            retry_after_cap,
            #[cfg(test)]
            development: None,
        }
    }

    #[cfg(test)]
    pub(super) const fn https_only(&self) -> bool {
        self.https_only
    }

    #[cfg(test)]
    pub(super) fn test_runtime_selection(&self) -> (bool, bool, Option<NonZeroUsize>, Duration) {
        (
            self.https_only,
            matches!(self.tls, TlsCapability::Available),
            self.status_body_limit,
            self.retry_after_cap,
        )
    }

    pub(crate) fn validate_fixed_base(&self, url: &url::Url) -> Result<(), ClientBuildError> {
        if self.https_only && url.scheme() != "https" {
            return Err(ClientBuildError::new(
                ClientBuildErrorKind::HttpConfiguration,
            ));
        }
        if url.scheme() == "https" && self.tls == TlsCapability::Unavailable {
            return Err(ClientBuildError::new(ClientBuildErrorKind::TlsUnavailable));
        }
        Ok(())
    }

    pub(crate) fn preflight_url(
        &self,
        endpoint: &EndpointId,
        method: &Method,
        url: &url::Url,
    ) -> Result<(), ApiError> {
        if self.https_only && url.scheme() != "https" {
            return Err(ApiError::configuration(
                endpoint.clone(),
                method.clone(),
                ConfigurationErrorKind::InvalidRoute,
            ));
        }
        if url.scheme() == "https" && self.tls == TlsCapability::Unavailable {
            return Err(ApiError::classified(
                endpoint.clone(),
                method.clone(),
                crate::error::ErrorCategory::Configuration,
            ));
        }
        Ok(())
    }

    pub(crate) fn preflight_protected_url(
        &self,
        endpoint: &EndpointId,
        method: &Method,
        url: &url::Url,
    ) -> Result<(), ApiError> {
        self.preflight_url(endpoint, method, url)?;
        if url.scheme() != "https" || self.tls == TlsCapability::Unavailable {
            return Err(ApiError::configuration(
                endpoint.clone(),
                method.clone(),
                ConfigurationErrorKind::InvalidAuthentication,
            ));
        }
        Ok(())
    }

    pub(crate) fn build_request(
        &self,
        endpoint: &EndpointId,
        method: &Method,
        url: &url::Url,
        public_headers: &HeaderMap,
        body: NativeBody,
    ) -> Result<ApplicationRequest, ApiError> {
        self.preflight_url(endpoint, method, url)?;
        let builder = self
            .client
            .request(method.clone(), url.clone())
            .headers(public_headers.clone());
        let (builder, body_failure, multipart_limit): (
            _,
            Option<BodyFailureState>,
            Option<NonZeroUsize>,
        ) = match body {
            NativeBody::Empty => (builder, None, None),
            NativeBody::Bytes(body) => (builder.body(body), None, None),
            NativeBody::Stream {
                body,
                known_length,
                failure,
            } => {
                let builder = match known_length {
                    Some(length) => builder.header(CONTENT_LENGTH, length),
                    None => builder,
                };
                (builder.body(body), Some(failure), None)
            }
            #[cfg(feature = "multipart")]
            NativeBody::Multipart { form, limit } => (builder.multipart(form), None, Some(limit)),
        };
        let native = builder.build().map_err(|error| {
            drop(error);
            ApiError::classified(
                endpoint.clone(),
                method.clone(),
                crate::error::ErrorCategory::RequestEncoding,
            )
        })?;
        #[cfg(feature = "multipart")]
        let (native, body_failure) = match multipart_limit {
            Some(limit) => {
                let mut native = native;
                let exact_length = native
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok());
                let limit_u64 = u64::try_from(limit.get()).unwrap_or(u64::MAX);
                if exact_length.is_some_and(|length| length > limit_u64) {
                    return Err(ApiError::classified(
                        endpoint.clone(),
                        method.clone(),
                        crate::error::ErrorCategory::RequestBody,
                    ));
                }
                let serialized = native.body_mut().take().ok_or_else(|| {
                    ApiError::classified(
                        endpoint.clone(),
                        method.clone(),
                        crate::error::ErrorCategory::RequestBody,
                    )
                })?;
                let (managed, failure) =
                    crate::body::wrap_multipart_native_body(serialized, exact_length, limit);
                *native.body_mut() = Some(managed);
                (native, Some(failure))
            }
            None => (native, body_failure),
        };
        #[cfg(not(feature = "multipart"))]
        let (native, body_failure) = {
            let _ = multipart_limit;
            (native, body_failure)
        };
        Ok(ApplicationRequest {
            native,
            body_failure,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the single managed execution boundary receives exact native and deterministic proof facts"
    )]
    pub(crate) async fn execute(
        &self,
        endpoint: &EndpointId,
        method: &Method,
        request: ApplicationRequest,
        safe_url: &str,
        public_headers: &HeaderMap,
        pagination_sensitive_headers: &std::collections::HashSet<http::HeaderName>,
        body_kind: BodyKind,
        known_body_length: Option<u64>,
    ) -> Result<ApplicationResponse, ApiError> {
        #[cfg(test)]
        let _ = public_headers;
        #[cfg(test)]
        let native = match &self.development {
            Some(development) => {
                let ApplicationRequest {
                    mut native,
                    body_failure,
                } = request;
                let boundary_body = match native.body_mut().take() {
                    Some(body) => collect_request_body(body).await.map_err(|()| {
                        ApiError::classified(
                            endpoint.clone(),
                            method.clone(),
                            crate::error::ErrorCategory::RequestBody,
                        )
                    })?,
                    None => Bytes::new(),
                };
                let boundary = super::development::boundary_request_with_sensitive(
                    endpoint.as_str(),
                    method.clone(),
                    Some(safe_url.to_owned()),
                    native.headers(),
                    pagination_sensitive_headers,
                    boundary_body_category(body_kind),
                    known_body_length.and_then(|length| usize::try_from(length).ok()),
                    Some(boundary_body),
                )
                .__with_native(native.url().as_str(), native.headers());
                let _ = body_failure;
                drop(native);
                development
                    .__execute(boundary)
                    .await
                    .map(super::development::scripted_response)
                    .map_err(|failure| {
                        ApiError::classified(
                            endpoint.clone(),
                            method.clone(),
                            super::development::failure_category(failure.kind()),
                        )
                    })?
            }
            None => {
                let body_failure = request.body_failure.clone();
                self.client.execute(request.native).await.map_err(|error| {
                    ApiError::from_reqwest(
                        endpoint.clone(),
                        method.clone(),
                        error,
                        body_failure.as_ref().is_some_and(BodyFailureState::failed),
                    )
                })?
            }
        };

        #[cfg(not(test))]
        let native = {
            let _ = (
                safe_url,
                public_headers,
                pagination_sensitive_headers,
                body_kind,
                known_body_length,
            );
            let body_failure = request.body_failure.clone();
            self.client.execute(request.native).await.map_err(|error| {
                ApiError::from_reqwest(
                    endpoint.clone(),
                    method.clone(),
                    error,
                    body_failure.as_ref().is_some_and(BodyFailureState::failed),
                )
            })?
        };

        Ok(ApplicationResponse {
            native,
            endpoint: endpoint.clone(),
            method: method.clone(),
        })
    }

    #[cfg(test)]
    pub(super) fn with_development(mut self, development: ApplicationChannel) -> Self {
        self.development = Some(development);
        self
    }

    #[cfg(test)]
    pub(super) fn prepare(
        &self,
        endpoint: &'static str,
        method: Method,
        url: &str,
        protected: bool,
    ) -> Result<reqwest::RequestBuilder, ApiError> {
        let endpoint_id = EndpointId::new(endpoint)
            .expect("deterministic test endpoint IDs use the public runtime grammar");
        let url = url::Url::parse(url).map_err(|_| {
            ApiError::classified(
                endpoint_id.clone(),
                method.clone(),
                crate::error::ErrorCategory::Configuration,
            )
        })?;
        self.preflight_url(&endpoint_id, &method, &url)?;
        if protected && url.scheme() != "https" {
            return Err(ApiError::configuration(
                endpoint_id,
                method,
                ConfigurationErrorKind::InvalidAuthentication,
            ));
        }
        if url.scheme() == "https" && self.tls == TlsCapability::Unavailable {
            return Err(ApiError::classified(
                endpoint_id,
                method,
                crate::error::ErrorCategory::Configuration,
            ));
        }
        Ok(self.client.request(method, url))
    }

    #[cfg(test)]
    pub(super) async fn execute_native(
        &self,
        endpoint: &'static str,
        method: Method,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ApiError> {
        #[cfg(test)]
        {
            let boundary = super::development::minimal_boundary_request(endpoint, method.clone());
            return self
                .execute_at_boundary(endpoint, method, request, boundary)
                .await;
        }

        #[cfg(not(test))]
        request.send().await.map_err(|error| {
            ApiError::from_reqwest(EndpointId::from_static(endpoint), method, error, false)
        })
    }

    #[cfg(test)]
    pub(super) async fn execute_at_boundary(
        &self,
        endpoint: &'static str,
        method: Method,
        request: reqwest::RequestBuilder,
        boundary: BoundaryRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let Some(development) = &self.development else {
            return request.send().await.map_err(|error| {
                ApiError::from_reqwest(EndpointId::from_static(endpoint), method, error, false)
            });
        };
        drop(request);
        development
            .__execute(boundary)
            .await
            .map(super::development::scripted_response)
            .map_err(|failure| {
                ApiError::classified(
                    EndpointId::from_static(endpoint),
                    method,
                    super::development::failure_category(failure.kind()),
                )
            })
    }

    pub(super) async fn terminal_status(
        &self,
        endpoint: EndpointId,
        method: Method,
        response: reqwest::Response,
        disposition: StatusDisposition,
        deadline: Option<tokio::time::Instant>,
    ) -> StatusCapture {
        collect_terminal_status(
            endpoint,
            method,
            response,
            disposition,
            self.status_body_limit,
            self.retry_after_cap,
            deadline,
        )
        .await
    }

    pub(crate) async fn terminal_status_managed(
        &self,
        endpoint: EndpointId,
        method: Method,
        response: ApplicationResponse,
        deadline: Option<tokio::time::Instant>,
    ) -> HttpResponseFailure {
        match self
            .terminal_status(
                endpoint,
                method,
                response.into_native(),
                StatusDisposition::Terminal,
                deadline,
            )
            .await
        {
            StatusCapture::Terminal(failure) => failure,
            StatusCapture::SkippedRecoverable => {
                unreachable!("terminal status capture cannot be recoverable")
            }
        }
    }

    #[cfg(test)]
    pub(super) async fn get(
        &self,
        endpoint: &'static str,
        url: &str,
    ) -> Result<reqwest::Response, ApiError> {
        let method = Method::GET;
        let request = self.prepare(endpoint, method.clone(), url, false)?;
        self.execute_native(endpoint, method, request).await
    }
}

#[cfg(test)]
async fn collect_request_body(body: reqwest::Body) -> Result<Bytes, ()> {
    use http_body::Body as _;

    let mut body = Box::pin(body);
    let mut collected = BytesMut::new();
    loop {
        let frame = std::future::poll_fn(|context| body.as_mut().poll_frame(context)).await;
        match frame {
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    collected.extend_from_slice(&data);
                }
            }
            Some(Err(error)) => {
                drop(error);
                return Err(());
            }
            None => return Ok(collected.freeze()),
        }
    }
}

#[cfg(test)]
const fn boundary_body_category(kind: BodyKind) -> concord_test_support::script::BodyCategory {
    match kind {
        BodyKind::Empty => concord_test_support::script::BodyCategory::Empty,
        BodyKind::Buffered => concord_test_support::script::BodyCategory::Bytes,
        BodyKind::Stream => concord_test_support::script::BodyCategory::Streaming,
        BodyKind::Multipart => concord_test_support::script::BodyCategory::Multipart,
    }
}
