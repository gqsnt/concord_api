use std::fmt;

#[cfg(feature = "dangerous-raw-response")]
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Version};
use url::Url;

/// Safe metadata describing the successful execution that produced a value.
pub struct ExecutionMeta {
    endpoint: &'static str,
    method: Method,
    page_index: u32,
    recovered_authentication: bool,
}

impl ExecutionMeta {
    #[allow(dead_code, reason = "constructed by the production executor")]
    pub(crate) fn new(
        endpoint: &'static str,
        method: Method,
        page_index: u32,
        recovered_authentication: bool,
    ) -> Self {
        Self {
            endpoint,
            method,
            page_index,
            recovered_authentication,
        }
    }

    #[must_use]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    #[must_use]
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
pub struct DecodedResponse<T> {
    url: Url,
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    metadata: ExecutionMeta,
    value: T,
}

impl<T> DecodedResponse<T> {
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
    pub const fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[must_use]
    pub const fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }

    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> fmt::Debug for DecodedResponse<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedResponse")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_count", &self.headers.len())
            .field("metadata", &self.metadata)
            .field("value", &"<application value omitted>")
            .finish()
    }
}

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
    pub(crate) fn from_decoded(response: DecodedResponse<Bytes>) -> Self {
        let DecodedResponse {
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
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[must_use]
    pub fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }

    #[must_use]
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    #[must_use]
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
        let response = DecodedResponse::new(
            Url::parse("https://metadata.example.test/sensitive-route?token=query-sentinel")
                .expect("logical URL"),
            StatusCode::OK,
            Version::HTTP_11,
            headers,
            ExecutionMeta::new("MetadataApi::value", Method::GET, 0, false),
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
