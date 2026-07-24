#[cfg(not(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response")))]
#[test]
fn compiled_default_surface_enables_no_dangerous_feature() {
    assert!(!cfg!(feature = "dangerous-custom-retry"));
    assert!(!cfg!(feature = "dangerous-raw-response"));
}

#[cfg(all(
    feature = "dangerous-custom-retry",
    not(feature = "dangerous-raw-response")
))]
#[test]
fn custom_retry_is_available_without_other_dangerous_or_optional_features() {
    use concord_core::advanced::HttpConfigBuilder;
    use concord_core::dangerous::ReqwestRetryBuilder;

    fn configure(builder: HttpConfigBuilder, retry: ReqwestRetryBuilder) -> HttpConfigBuilder {
        builder.dangerous_custom_retry(retry)
    }

    let _: fn(HttpConfigBuilder, ReqwestRetryBuilder) -> HttpConfigBuilder = configure;

    assert!(!cfg!(feature = "dangerous-raw-response"));
    assert!(!cfg!(feature = "default-tls"));
    assert!(!cfg!(feature = "http2"));
    assert!(!cfg!(feature = "multipart"));
    assert!(!cfg!(feature = "gzip"));
    assert!(!cfg!(feature = "brotli"));
    assert!(!cfg!(feature = "deflate"));
}

#[cfg(all(
    feature = "dangerous-raw-response",
    not(feature = "dangerous-custom-retry")
))]
#[test]
fn raw_response_api_is_available_without_other_dangerous_or_optional_features() {
    use concord_core::advanced::{Bytes, ExecutionMeta, HeaderMap, StatusCode, Url, Version};
    use concord_core::dangerous::{RawResponse, RawResponseRequest};

    let _: fn(&RawResponse) -> &Url = RawResponse::url;
    let _: fn(&RawResponse) -> StatusCode = RawResponse::status;
    let _: fn(&RawResponse) -> Version = RawResponse::version;
    let _: fn(&RawResponse) -> &HeaderMap = RawResponse::headers;
    let _: fn(&RawResponse) -> &ExecutionMeta = RawResponse::metadata;
    let _: fn(&RawResponse) -> &Bytes = RawResponse::body;
    let _: fn(RawResponse) -> Bytes = RawResponse::into_body;
    let _: Option<RawResponseRequest<()>> = None;

    assert!(cfg!(feature = "dangerous-raw-response"));
    assert!(!cfg!(feature = "dangerous-custom-retry"));
    assert!(!cfg!(feature = "default-tls"));
    assert!(!cfg!(feature = "http2"));
    assert!(!cfg!(feature = "multipart"));
    assert!(!cfg!(feature = "gzip"));
    assert!(!cfg!(feature = "brotli"));
    assert!(!cfg!(feature = "deflate"));
}

#[cfg(all(feature = "dangerous-raw-response", feature = "dangerous-custom-retry"))]
#[test]
fn raw_response_and_custom_retry_surfaces_coexist() {
    use concord_core::dangerous::{RawResponse, ReqwestRetryBuilder};

    let _: Option<RawResponse> = None;
    let _: Option<ReqwestRetryBuilder> = None;
}
