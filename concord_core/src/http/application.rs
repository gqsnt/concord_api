use std::num::NonZeroUsize;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Version};

#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::executor::ApplicationChannel;
#[cfg(test)]
use concord_test_support::script::BoundaryRequest;

use crate::error::{
    ApiError, ClientBuildError, ClientBuildErrorKind, ConfigurationErrorKind, HttpResponseFailure,
};
use crate::hooks::BodyKind;

use super::build::TlsCapability;
use super::status::{StatusCapture, StatusDisposition, collect_terminal_status};

pub(crate) struct ApplicationRequest {
    native: reqwest::Request,
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

    fn into_native(self) -> reqwest::Response {
        self.native
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
        body: Option<Bytes>,
    ) -> Result<ApplicationRequest, ApiError> {
        self.preflight_url(endpoint, method, url)?;
        let builder = self
            .client
            .request(method.clone(), url.clone())
            .headers(public_headers.clone());
        let builder = match body {
            Some(body) => builder.body(body),
            None => builder,
        };
        let native = builder.build().map_err(|error| {
            drop(error);
            ApiError::classified(
                endpoint,
                method.clone(),
                crate::error::ErrorCategory::RequestEncoding,
            )
        })?;
        Ok(ApplicationRequest { native })
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
        boundary_body: Bytes,
    ) -> Result<ApplicationResponse, ApiError> {
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        let native =
            match &self.development {
                Some(development) => {
                    let boundary = super::development::boundary_request(
                        endpoint,
                        method.clone(),
                        Some(safe_url.to_owned()),
                        public_headers,
                        boundary_body_category(body_kind),
                        known_body_length.and_then(|length| usize::try_from(length).ok()),
                        Some(boundary_body),
                    );
                    drop(request);
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
                None => self.client.execute(request.native).await.map_err(|error| {
                    ApiError::from_reqwest(endpoint, method.clone(), error, false)
                })?,
            };

        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        let native = {
            let _ = (
                safe_url,
                public_headers,
                body_kind,
                known_body_length,
                boundary_body,
            );
            self.client
                .execute(request.native)
                .await
                .map_err(|error| ApiError::from_reqwest(endpoint, method.clone(), error, false))?
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
const fn boundary_body_category(kind: BodyKind) -> concord_test_support::script::BodyCategory {
    match kind {
        BodyKind::Empty => concord_test_support::script::BodyCategory::Empty,
        BodyKind::Buffered => concord_test_support::script::BodyCategory::Bytes,
        BodyKind::Stream => concord_test_support::script::BodyCategory::Streaming,
        BodyKind::Multipart => concord_test_support::script::BodyCategory::Multipart,
    }
}
