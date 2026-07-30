use std::fmt;

use http::{HeaderMap, Method, StatusCode, Version};
use url::Url;

use crate::endpoint::EndpointId;

/// Safe metadata describing the successful execution that produced a value.
pub struct ExecutionMeta {
    endpoint: EndpointId,
    method: Method,
    page_index: u32,
    attempts_started: u32,
    policy_retries: u32,
    recovered_authentication: bool,
}

impl ExecutionMeta {
    pub(crate) fn new(
        endpoint: EndpointId,
        method: Method,
        page_index: u32,
        attempts_started: u32,
        policy_retries: u32,
        recovered_authentication: bool,
    ) -> Self {
        Self {
            endpoint,
            method,
            page_index,
            attempts_started,
            policy_retries,
            recovered_authentication,
        }
    }

    #[must_use]
    /// Returns the stable generated or runtime endpoint identity.
    pub fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    #[must_use]
    /// Returns the typed stable endpoint identity.
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint
    }

    #[must_use]
    /// Returns the application HTTP method.
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[must_use]
    /// Returns the zero-based page index; direct requests use zero.
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    #[must_use]
    /// Returns the number of visible attempts whose before-send stage began.
    pub const fn attempts_started(&self) -> u32 {
        self.attempts_started
    }

    #[must_use]
    /// Returns the number of policy retries started for this operation.
    pub const fn policy_retries(&self) -> u32 {
        self.policy_retries
    }

    #[must_use]
    /// Returns whether the result followed one authentication recovery.
    pub const fn recovered_authentication(&self) -> bool {
        self.recovered_authentication
    }
}

impl fmt::Debug for ExecutionMeta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionMeta")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("page_index", &self.page_index)
            .field("attempts_started", &self.attempts_started)
            .field("policy_retries", &self.policy_retries)
            .field("recovered_authentication", &self.recovered_authentication)
            .finish()
    }
}

/// A successful semantic value together with its logical response metadata.
///
/// This is the single metadata wrapper for buffered values, bytes, no-content,
/// byte streams, record streams, and application-defined outputs. Its `Debug`
/// implementation is structural: it never formats `T`, the logical URL, or any
/// response header value. Accessors intentionally expose the actual accepted
/// headers and credential-sanitized logical URL to application code, which owns
/// later logging and storage. Public URL values may still contain operational
/// identifiers or personally identifiable information.
pub struct ApiResponse<T> {
    url: Url,
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    metadata: ExecutionMeta,
    value: T,
}

impl<T> ApiResponse<T> {
    pub(crate) fn new(
        url: Url,
        status: StatusCode,
        version: Version,
        headers: HeaderMap,
        metadata: ExecutionMeta,
        value: T,
    ) -> Self {
        Self {
            url,
            status,
            version,
            headers,
            metadata,
            value,
        }
    }

    #[must_use]
    /// Returns the credential-sanitized logical URL. Public values may still
    /// contain operational identifiers or personally identifiable information.
    pub const fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    /// Returns the accepted response status.
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    /// Returns the response HTTP version.
    pub const fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    /// Returns the actual accepted response headers.
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[must_use]
    /// Returns the safe structural execution metadata.
    pub const fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }

    #[must_use]
    /// Borrows the decoded application value.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Extracts the decoded application value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> fmt::Debug for ApiResponse<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiResponse")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_count", &self.headers.len())
            .field("metadata", &self.metadata)
            .field("value", &"<application value omitted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use http::header::{AUTHORIZATION, HeaderValue};

    use super::*;

    const VALUE_SECRET: &str = "decoded-value-sentinel-7bd91a";
    const HEADER_SECRET: &str = "decoded-header-sentinel-e4a31f";

    #[test]
    fn debug_omits_application_value_url_and_every_header_value() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static(HEADER_SECRET));
        headers.insert("x-public", HeaderValue::from_static("public-header-value"));
        let response = ApiResponse::new(
            Url::parse("https://metadata.example.test/sensitive-route?token=query-sentinel")
                .expect("logical URL"),
            StatusCode::OK,
            Version::HTTP_11,
            headers,
            ExecutionMeta::new(
                EndpointId::from_static("metadata_api.value"),
                Method::GET,
                0,
                1,
                0,
                false,
            ),
            VALUE_SECRET.to_owned(),
        );

        let debug = format!("{response:?}");
        for forbidden in [
            VALUE_SECRET,
            HEADER_SECRET,
            "public-header-value",
            "query-sentinel",
            "sensitive-route",
        ] {
            assert!(!debug.contains(forbidden), "debug leaked {forbidden}");
        }
        assert_eq!(response.value(), VALUE_SECRET);
        assert_eq!(response.headers()[AUTHORIZATION], HEADER_SECRET);
    }
}
