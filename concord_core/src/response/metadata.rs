use std::fmt;

#[cfg(feature = "dangerous-raw-response")]
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Version};
use url::Url;

use crate::endpoint::EndpointId;

/// Safe metadata describing the successful execution that produced a value.
pub struct ExecutionMeta {
    endpoint: EndpointId,
    method: Method,
    page_index: u32,
    recovered_authentication: bool,
}

impl ExecutionMeta {
    #[allow(dead_code, reason = "constructed by the production executor")]
    pub(crate) fn new(
        endpoint: impl Into<EndpointId>,
        method: Method,
        page_index: u32,
        recovered_authentication: bool,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            method,
            page_index,
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
            .field("recovered_authentication", &self.recovered_authentication)
            .finish()
    }
}

/// A successful semantic value together with its logical response metadata.
///
/// This is the single metadata wrapper for buffered values, bytes, no-content,
/// byte streams, record streams, and application-defined outputs. Its `Debug`
/// implementation is structural: it never formats `T`, the logical URL, or any
/// response header value. Accessors intentionally expose the actual successful
/// headers and logical URL to application code, which owns later logging and
/// storage.
pub struct ApiResponse<T> {
    url: Url,
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    metadata: ExecutionMeta,
    value: T,
}

impl<T> ApiResponse<T> {
    #[allow(dead_code, reason = "constructed by the production executor")]
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
    /// Returns the logical URL without native-only authentication.
    pub const fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    /// Returns the successful response status.
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    /// Returns the response HTTP version.
    pub const fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    /// Returns the actual successful response headers.
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

/// Compatibility alias for the universal successful response wrapper.
///
/// This alias has no second implementation or storage representation.
pub type DecodedResponse<T> = ApiResponse<T>;

/// A complete bounded successful response whose declared decoder was skipped.
///
/// This value is available only with `dangerous-raw-response`. Its logical URL
/// remains free of native-only authentication, and it never exposes a Reqwest
/// response. Unlike ordinary decoded results, however, its actual successful
/// headers and body bytes may contain secrets. Concord excludes those values
/// from its own automatic diagnostics; callers assume responsibility for all
/// later logging, formatting, storage, and disclosure.
///
/// Fields are intentionally private:
///
/// ```compile_fail
/// fn inspect(response: concord_core::dangerous::RawResponse) {
///     let concord_core::dangerous::RawResponse { body, .. } = response;
///     let _ = body;
/// }
/// ```
#[cfg(feature = "dangerous-raw-response")]
pub struct RawResponse {
    url: Url,
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    metadata: ExecutionMeta,
    body: Bytes,
}

#[cfg(feature = "dangerous-raw-response")]
impl RawResponse {
    pub(crate) fn from_decoded(response: ApiResponse<Bytes>) -> Self {
        let ApiResponse {
            url,
            status,
            version,
            headers,
            metadata,
            value: body,
        } = response;
        Self {
            url,
            status,
            version,
            headers,
            metadata,
            body,
        }
    }

    #[must_use]
    /// Returns the logical URL without native-only authentication.
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    /// Returns the successful response status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    /// Returns the response HTTP version.
    pub fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    /// Returns the actual successful response headers.
    ///
    /// Header values may contain secrets and have reduced secrecy guarantees.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[must_use]
    /// Returns the safe structural execution metadata.
    pub fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }

    #[must_use]
    /// Borrows the complete bounded successful response bytes.
    ///
    /// Body bytes may contain secrets and have reduced secrecy guarantees.
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    #[must_use]
    /// Extracts the complete bounded successful response bytes.
    ///
    /// Body bytes may contain secrets and have reduced secrecy guarantees.
    pub fn into_body(self) -> Bytes {
        self.body
    }
}

#[cfg(feature = "dangerous-raw-response")]
impl fmt::Debug for RawResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawResponse")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_count", &self.headers.len())
            .field("metadata", &self.metadata)
            .field("body_len", &self.body.len())
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
            ExecutionMeta::new("metadata_api.value", Method::GET, 0, false),
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
