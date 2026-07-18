use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
#[cfg(any(test, feature = "dangerous-dev-tools"))]
use bytes::BytesMut;
use futures_core::Stream;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, StatusCode, Version};

#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::executor::ApplicationChannel;
#[cfg(test)]
use concord_test_support::script::BoundaryRequest;

use crate::body::{BodyFailureState, NativeBody};
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

pub(crate) struct ApplicationResponse {
    native: reqwest::Response,
    endpoint: &'static str,
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
                self.endpoint,
                self.method.clone(),
                crate::error::ErrorCategory::ResponseBody,
            )
        })
    }

    pub(crate) fn into_byte_stream(self) -> ApplicationByteStream {
        let endpoint = self.endpoint;
        let method = self.method;
        Box::pin(ReducedResponseStream {
            inner: Box::pin(self.native.bytes_stream()),
            endpoint,
            method,
        })
    }

    fn into_native(self) -> reqwest::Response {
        self.native
    }
}

pub(crate) type ApplicationByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, ApiError>> + Send + 'static>>;

struct ReducedResponseStream<S> {
    inner: Pin<Box<S>>,
    endpoint: &'static str,
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
                    this.endpoint,
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
    #[cfg(any(test, feature = "dangerous-dev-tools"))]
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
            #[cfg(any(test, feature = "dangerous-dev-tools"))]
            development: None,
        }
    }

    #[cfg(test)]
    pub(super) const fn https_only(&self) -> bool {
        self.https_only
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
        endpoint: &'static str,
        method: &Method,
        url: &url::Url,
    ) -> Result<(), ApiError> {
        if self.https_only && url.scheme() != "https" {
            return Err(ApiError::configuration(
                endpoint,
                method.clone(),
                ConfigurationErrorKind::InvalidRoute,
            ));
        }
        if url.scheme() == "https" && self.tls == TlsCapability::Unavailable {
            return Err(ApiError::classified(
                endpoint,
                method.clone(),
                crate::error::ErrorCategory::Configuration,
            ));
        }
        Ok(())
    }

    pub(crate) fn build_request(
        &self,
        endpoint: &'static str,
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
        let (builder, body_failure) = match body {
            NativeBody::Empty => (builder, None),
            NativeBody::Bytes(body) => (builder.body(body), None),
            NativeBody::Stream {
                body,
                known_length,
                failure,
            } => {
                let builder = match known_length {
                    Some(length) => builder.header(CONTENT_LENGTH, length),
                    None => builder,
                };
                (builder.body(body), Some(failure))
            }
        };
        let native = builder.build().map_err(|error| {
            drop(error);
            ApiError::classified(
                endpoint,
                method.clone(),
                crate::error::ErrorCategory::RequestEncoding,
            )
        })?;
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
        endpoint: &'static str,
        method: &Method,
        request: ApplicationRequest,
        safe_url: &str,
        public_headers: &HeaderMap,
        body_kind: BodyKind,
        known_body_length: Option<u64>,
    ) -> Result<ApplicationResponse, ApiError> {
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        let _ = public_headers;
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        let native = match &self.development {
            Some(development) => {
                let ApplicationRequest {
                    mut native,
                    body_failure,
                } = request;
                let boundary_body = match native.body_mut().take() {
                    Some(body) => collect_request_body(body).await.map_err(|()| {
                        ApiError::classified(
                            endpoint,
                            method.clone(),
                            crate::error::ErrorCategory::RequestBody,
                        )
                    })?,
                    None => Bytes::new(),
                };
                let boundary = super::development::boundary_request(
                    endpoint,
                    method.clone(),
                    Some(safe_url.to_owned()),
                    native.headers(),
                    boundary_body_category(body_kind),
                    known_body_length.and_then(|length| usize::try_from(length).ok()),
                    Some(boundary_body),
                );
                let _ = body_failure;
                drop(native);
                development
                    .__execute(boundary)
                    .await
                    .map(|response| response.__into_http_response().into())
                    .map_err(|failure| {
                        ApiError::classified(
                            endpoint,
                            method.clone(),
                            super::development::failure_category(failure.kind()),
                        )
                    })?
            }
            None => {
                let body_failure = request.body_failure.clone();
                self.client.execute(request.native).await.map_err(|error| {
                    ApiError::from_reqwest(
                        endpoint,
                        method.clone(),
                        error,
                        body_failure.as_ref().is_some_and(BodyFailureState::failed),
                    )
                })?
            }
        };

        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        let native = {
            let _ = (safe_url, public_headers, body_kind, known_body_length);
            let body_failure = request.body_failure.clone();
            self.client.execute(request.native).await.map_err(|error| {
                ApiError::from_reqwest(
                    endpoint,
                    method.clone(),
                    error,
                    body_failure.as_ref().is_some_and(BodyFailureState::failed),
                )
            })?
        };

        Ok(ApplicationResponse {
            native,
            endpoint,
            method: method.clone(),
        })
    }

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
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
        let url = url::Url::parse(url).map_err(|_| {
            ApiError::classified(
                endpoint,
                method.clone(),
                crate::error::ErrorCategory::Configuration,
            )
        })?;
        self.preflight_url(endpoint, &method, &url)?;
        if protected && url.scheme() != "https" {
            return Err(ApiError::configuration(
                endpoint,
                method,
                ConfigurationErrorKind::InvalidAuthentication,
            ));
        }
        if url.scheme() == "https" && self.tls == TlsCapability::Unavailable {
            return Err(ApiError::classified(
                endpoint,
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
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        {
            let boundary = super::development::minimal_boundary_request(endpoint, method.clone());
            return self
                .execute_at_boundary(endpoint, method, request, boundary)
                .await;
        }

        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        request
            .send()
            .await
            .map_err(|error| ApiError::from_reqwest(endpoint, method, error, false))
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
            return request
                .send()
                .await
                .map_err(|error| ApiError::from_reqwest(endpoint, method, error, false));
        };
        drop(request);
        development
            .__execute(boundary)
            .await
            .map(|response| response.__into_http_response().into())
            .map_err(|failure| {
                ApiError::classified(
                    endpoint,
                    method,
                    super::development::failure_category(failure.kind()),
                )
            })
    }

    pub(super) async fn terminal_status(
        &self,
        endpoint: &'static str,
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
        endpoint: &'static str,
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

#[cfg(any(test, feature = "dangerous-dev-tools"))]
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

#[cfg(any(test, feature = "dangerous-dev-tools"))]
const fn boundary_body_category(kind: BodyKind) -> concord_test_support::script::BodyCategory {
    match kind {
        BodyKind::Empty => concord_test_support::script::BodyCategory::Empty,
        BodyKind::Buffered => concord_test_support::script::BodyCategory::Bytes,
        BodyKind::Stream => concord_test_support::script::BodyCategory::Streaming,
        BodyKind::Multipart => concord_test_support::script::BodyCategory::Multipart,
    }
}
