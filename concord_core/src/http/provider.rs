use std::num::NonZeroUsize;

use http::Method;

#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::executor::ProviderChannel;
#[cfg(any(test, feature = "dangerous-dev-tools"))]
use concord_test_support::script::BoundaryRequest;

use crate::error::{ErrorCategory, reqwest_category};

use super::build::TlsCapability;

/// Private provider HTTP owner with an independent client and pool.
pub(super) struct ProviderHttpInner {
    client: reqwest::Client,
    tls: TlsCapability,
    request_limit: NonZeroUsize,
    response_limit: NonZeroUsize,
    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    development: Option<ProviderChannel>,
}

impl ProviderHttpInner {
    pub(super) const fn new(
        client: reqwest::Client,
        tls: TlsCapability,
        request_limit: NonZeroUsize,
        response_limit: NonZeroUsize,
    ) -> Self {
        Self {
            client,
            tls,
            request_limit,
            response_limit,
            #[cfg(any(test, feature = "dangerous-dev-tools"))]
            development: None,
        }
    }

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    pub(super) fn with_development(mut self, development: ProviderChannel) -> Self {
        self.development = Some(development);
        self
    }

    pub(super) fn prepare(
        &self,
        method: Method,
        url: &str,
        known_body_length: Option<usize>,
    ) -> Result<reqwest::RequestBuilder, ErrorCategory> {
        let url = url::Url::parse(url).map_err(|_| ErrorCategory::Configuration)?;
        if url.scheme() != "https" || self.tls == TlsCapability::Unavailable {
            return Err(ErrorCategory::Configuration);
        }
        if known_body_length.is_some_and(|length| length > self.request_limit.get()) {
            return Err(ErrorCategory::RequestBody);
        }
        Ok(self.client.request(method, url))
    }

    pub(super) async fn execute_native(
        &self,
        endpoint: &'static str,
        method: Method,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ErrorCategory> {
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        {
            let boundary = super::development::minimal_boundary_request(endpoint, method);
            return self.execute_at_boundary(request, boundary).await;
        }

        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        let _ = (endpoint, method);
        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        let response = request
            .send()
            .await
            .map_err(|error| reqwest_category(error, false))?;
        #[cfg(not(any(test, feature = "dangerous-dev-tools")))]
        return self.validate_response(response);
    }

    fn validate_response(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ErrorCategory> {
        if response
            .content_length()
            .is_some_and(|length| length > self.response_limit.get() as u64)
        {
            return Err(ErrorCategory::ResponseBody);
        }
        Ok(response)
    }

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    pub(super) async fn execute_at_boundary(
        &self,
        request: reqwest::RequestBuilder,
        boundary: BoundaryRequest,
    ) -> Result<reqwest::Response, ErrorCategory> {
        let response = if let Some(development) = &self.development {
            drop(request);
            development
                .__execute(boundary)
                .await
                .map(|response| response.__into_http_response().into())
                .map_err(|failure| super::development::failure_category(failure.kind()))?
        } else {
            request
                .send()
                .await
                .map_err(|error| reqwest_category(error, false))?
        };
        self.validate_response(response)
    }
}
