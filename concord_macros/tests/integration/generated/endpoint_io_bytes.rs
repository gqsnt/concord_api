use bytes::Bytes;
use concord_core::error::ErrorCategory;
use concord_core::prelude::ApiClientError;
use concord_macros::api;
use concord_test_support::{MockHandle, MockReply, MockServer, mock};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};

mod bytes_response_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client BytesResponseApi {
            base "https://example.com"
        }

        GET Download
            path ["download"]
            -> Bytes
    }

    pub(super) use bytes_response_api::BytesResponseApi;
}

use bytes_response_contract::BytesResponseApi;

#[derive(Clone)]
struct RecordingBytesTransport {
    server: MockServer,
    handle: Arc<StdMutex<MockHandle>>,
}

#[derive(Debug, Clone)]
struct CapturedRequest {
    method: Method,
    accept: Option<String>,
    client_header: Option<String>,
}

#[derive(Clone)]
enum ResponseFixture {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
    },
}

impl RecordingBytesTransport {
    fn new(fixture: ResponseFixture) -> Self {
        let ResponseFixture::Buffered {
            status,
            headers,
            chunks,
            content_length,
            ..
        } = fixture;
        let mut reply = MockReply::status(status);
        for (name, value) in headers {
            if let Some(name) = name {
                reply = reply.with_header(name, value);
            }
        }
        reply = if let Some(length) = content_length {
            reply
                .with_header(
                    http::header::CONTENT_LENGTH,
                    HeaderValue::from_str(&length.to_string()).expect("length"),
                )
                .with_body(Bytes::from(chunks.concat()))
        } else {
            reply.with_chunks(chunks)
        };
        let (server, handle) = mock().reply(reply).build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.handle
            .lock()
            .expect("handle lock")
            .recorded()
            .into_iter()
            .map(|request| CapturedRequest {
                method: request.method,
                accept: request
                    .headers
                    .get(http::header::ACCEPT)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                client_header: request
                    .headers
                    .get("x-client-wide")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
            })
            .collect()
    }

    fn send_count(&self) -> usize {
        self.handle.lock().expect("handle lock").recorded_len()
    }

    fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_reqwest(builder)
    }
}

#[tokio::test]
async fn generated_client_exposes_client_wide_headers_and_safe_reqwest_constructor() {
    let fixture = buffered_fixture(StatusCode::OK, vec![Bytes::from_static(b"hello")]);
    let transport = RecordingBytesTransport::new(fixture);
    let mut api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");
    let mut headers = HeaderMap::new();
    headers.insert("x-client-wide", HeaderValue::from_static("present"));
    api.set_api_headers(headers)
        .expect("generated header facade");
    let api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .with_api_headers(HeaderMap::from_iter([(
        http::header::HeaderName::from_static("x-client-wide"),
        HeaderValue::from_static("present"),
    )]))
    .expect("generated header with facade");
    assert_eq!(
        api.api_headers().get("x-client-wide"),
        Some(&HeaderValue::from_static("present"))
    );
    assert_eq!(
        api.api_headers().get("x-client-wide"),
        Some(&HeaderValue::from_static("present"))
    );
    api.download().execute().await.expect("download succeeds");
    assert_eq!(
        transport.requests()[0].client_header.as_deref(),
        Some("present")
    );

    let _api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        builder.connect_timeout(std::time::Duration::from_secs(1))
    })
    .expect("generated safe Reqwest constructor");
}

#[tokio::test]
async fn generated_safe_reqwest_builder_fallible_reports_pem_error_without_leak() {
    let root_error = match BytesResponseApi::new_with_safe_reqwest_builder_fallible(|builder| {
        builder.add_trusted_root_pem(
            b"-----BEGIN CERTIFICATE-----\nGENERATED_ROOT_SENTINEL\nnot-base64\n-----END CERTIFICATE-----",
        )
    }) {
        Ok(_) => panic!("invalid pem must fail"),
        Err(error) => error,
    };
    let identity_error = match BytesResponseApi::new_with_safe_reqwest_builder_fallible(|builder| {
        builder.client_identity_pem(
            b"-----BEGIN PRIVATE KEY-----\nGENERATED_IDENTITY_SENTINEL\nnot-a-key\n-----END PRIVATE KEY-----",
        )
    }) {
        Ok(_) => panic!("invalid identity pem must fail"),
        Err(error) => error,
    };
    for (label, error) in [("root", root_error), ("identity", identity_error)] {
        let diagnostics = format!("{error}\n{error:?}");
        assert!(!diagnostics.contains("GENERATED_ROOT_SENTINEL"), "{label}");
        assert!(
            !diagnostics.contains("GENERATED_IDENTITY_SENTINEL"),
            "{label}"
        );
        let mut current: &(dyn std::error::Error + 'static) = &error;
        while let Some(source) = current.source() {
            let rendered = format!("{source}\n{source:?}");
            assert!(!rendered.contains("GENERATED_ROOT_SENTINEL"), "{label}");
            assert!(!rendered.contains("GENERATED_IDENTITY_SENTINEL"), "{label}");
            current = source;
        }
    }
}

fn buffered_fixture(status: StatusCode, chunks: Vec<Bytes>) -> ResponseFixture {
    ResponseFixture::Buffered {
        status,
        headers: HeaderMap::new(),
        chunks,
        content_length: None,
    }
}

#[tokio::test]
async fn generated_bytes_response_reads_body_without_accept_header() {
    let fixture = buffered_fixture(
        StatusCode::OK,
        vec![Bytes::from_static(b"hel"), Bytes::from_static(b"lo")],
    );
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let response = api
        .download()
        .execute()
        .await
        .expect("bytes download succeeds");
    assert_eq!(response, Bytes::from_static(b"hello"));

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].accept.as_deref(), Some("*/*"));
}

#[tokio::test]
async fn generated_bytes_limit_failure_is_body_limited() {
    let fixture = buffered_fixture(
        StatusCode::OK,
        vec![Bytes::from_static(b"abcd"), Bytes::from_static(b"efgh")],
    );
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = api
        .download()
        .execute()
        .await
        .expect_err("bytes response over the limit should fail");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn generated_bytes_status_failure_is_body_free() {
    let sentinel = Bytes::from_static(b"SECRET_BYTES_STATUS_SENTINEL_MUST_NOT_APPEAR");
    let fixture = buffered_fixture(StatusCode::INTERNAL_SERVER_ERROR, vec![sentinel.clone()]);
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let err = api
        .download()
        .execute()
        .await
        .expect_err("status failure should not decode bytes");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "Download");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(transport.send_count(), 1);
    let rendered = format!("{err:?}");
    assert!(!rendered.contains("SECRET_BYTES_STATUS_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_bytes_response_includes_metadata_and_value() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::HeaderName::from_static("x-response-id"),
        HeaderValue::from_static("abc123"),
    );
    let fixture = ResponseFixture::Buffered {
        status: StatusCode::CREATED,
        headers,
        chunks: vec![Bytes::from_static(b"hello")],
        content_length: Some(5),
    };
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let response = api
        .download()
        .response()
        .await
        .expect("bytes response succeeds");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response
            .headers()
            .get(http::header::HeaderName::from_static("x-response-id"))
            .and_then(|value| value.to_str().ok()),
        Some("abc123")
    );
    assert_eq!(response.meta().endpoint, "Download");
    assert_eq!(response.meta().method, Method::GET);
    assert_eq!(response.into_value(), Bytes::from_static(b"hello"));
    assert_eq!(transport.send_count(), 1);
}
