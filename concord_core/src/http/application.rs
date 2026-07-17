use std::num::NonZeroUsize;
use std::time::Duration;

use http::Method;

#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::executor::ApplicationChannel;
#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::script::BoundaryRequest;

use crate::error::{ApiError, ClientBuildError, ClientBuildErrorKind, ConfigurationErrorKind};

use super::build::TlsCapability;
use super::status::{StatusCapture, StatusDisposition, collect_terminal_status};

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

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    pub(super) fn with_development(mut self, development: ApplicationChannel) -> Self {
        self.development = Some(development);
        self
    }

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

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
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
