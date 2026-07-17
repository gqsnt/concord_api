use std::num::NonZeroUsize;
use std::time::Duration;

use http::Method;

use crate::error::ApiError;

use super::build::TlsCapability;
use super::status::{StatusCapture, StatusDisposition, collect_terminal_status};

/// Private application HTTP owner. Its Reqwest client never crosses the
/// managed boundary.
pub(super) struct ApplicationHttp {
    client: reqwest::Client,
    tls: TlsCapability,
    status_body_limit: Option<NonZeroUsize>,
    retry_after_cap: Duration,
}

impl ApplicationHttp {
    pub(super) const fn new(
        client: reqwest::Client,
        tls: TlsCapability,
        status_body_limit: Option<NonZeroUsize>,
        retry_after_cap: Duration,
    ) -> Self {
        Self {
            client,
            tls,
            status_body_limit,
            retry_after_cap,
        }
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
        if protected && url.scheme() != "https" {
            return Err(ApiError::classified(
                endpoint,
                method,
                crate::error::ErrorCategory::AuthenticationUnavailable,
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
        request
            .send()
            .await
            .map_err(|error| ApiError::from_reqwest(endpoint, method, error, false))
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
