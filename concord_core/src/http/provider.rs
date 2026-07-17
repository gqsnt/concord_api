use std::num::NonZeroUsize;

use http::Method;

use crate::error::{ErrorCategory, reqwest_category};

use super::build::TlsCapability;

/// Private provider HTTP owner with an independent client and pool.
pub(super) struct ProviderHttpInner {
    client: reqwest::Client,
    tls: TlsCapability,
    request_limit: NonZeroUsize,
    response_limit: NonZeroUsize,
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
        }
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
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ErrorCategory> {
        let response = request
            .send()
            .await
            .map_err(|error| reqwest_category(error, false))?;
        if response
            .content_length()
            .is_some_and(|length| length > self.response_limit.get() as u64)
        {
            return Err(ErrorCategory::ResponseBody);
        }
        Ok(response)
    }
}
