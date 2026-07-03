use super::common::{MockResponse, MockTransport, TestAuthVars, TestCx};
use bytes::{Bytes, BytesMut};
use concord_core::advanced::{ContentType, FormData, Mixed, MultipartFormat, RawResponsePart};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn multipart_response_plan<F: MultipartFormat>(
    name: &'static str,
    path: &'static str,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: Method::GET,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy: ResolvedPolicy::default(),
            body: BodyPlan::None,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static(F::CONTENT_TYPE)),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args: RequestArgs::default(),
        overrides: RequestOverrides::default(),
        replayability: concord_core::internal::Replayability::Replayable,
    }
}

#[derive(Debug)]
struct BadMultipartAccept;

impl ContentType for BadMultipartAccept {
    const CONTENT_TYPE: &'static str = "bad\nvalue";
}

impl MultipartFormat for BadMultipartAccept {}

fn multipart_headers(content_type: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    headers
}

fn multipart_fixture_response(
    content_type: &'static str,
    chunks: Vec<Bytes>,
    content_length: Option<u64>,
    read_count: Option<Arc<AtomicUsize>>,
) -> MockResponse {
    MockResponse {
        status: StatusCode::OK,
        headers: multipart_headers(content_type),
        body: Bytes::new(),
        content_length,
        chunks: Some(chunks),
        read_count,
    }
}

fn multipart_body_chunks(boundary: &str) -> Vec<Bytes> {
    vec![
        Bytes::from(format!(
            "--{boundary}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"a.txt\"\r\n\r\nhello\r\n--BOUN"
        )),
        Bytes::from(format!(
            "DARY\r\nContent-Type: text/plain\r\n\r\nworld\r\n--{boundary}--\r\n"
        )),
    ]
}

async fn collect_part_bytes(mut part: RawResponsePart) -> Result<Bytes, ApiClientError> {
    let mut out = BytesMut::new();
    while let Some(chunk) = part.next_chunk().await? {
        out.extend_from_slice(&chunk);
    }
    Ok(out.freeze())
}

#[tokio::test]
async fn multipart_mixed_response_yields_raw_parts_incrementally() -> Result<(), ApiClientError> {
    let boundary = "BOUNDARY";
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            multipart_body_chunks(boundary),
            None,
            Some(read_count.clone()),
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartMixedResponse",
            "/multipart-mixed",
        ))
        .await?;

    assert_eq!(read_count.load(Ordering::SeqCst), 0);
    assert!(!format!("{stream:?}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));

    let first = stream.next_part().await?.expect("first part");
    assert!(read_count.load(Ordering::SeqCst) > 0);
    assert_eq!(
        first.content_type().and_then(|value| value.to_str().ok()),
        Some("text/plain")
    );
    assert_eq!(
        first
            .content_disposition()
            .and_then(|value| value.to_str().ok()),
        Some("attachment; filename=\"a.txt\"")
    );
    assert!(!format!("{first:?}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    let first_bytes = collect_part_bytes(first).await?;
    assert_eq!(first_bytes, Bytes::from_static(b"hello"));

    let second = stream.next_part().await?.expect("second part");
    let second_bytes = collect_part_bytes(second).await?;
    assert_eq!(second_bytes, Bytes::from_static(b"world"));

    assert!(stream.next_part().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn multipart_response_missing_closing_boundary_is_rejected_body_safely()
-> Result<(), ApiClientError> {
    let sentinel = "SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR";
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            vec![Bytes::from(format!(
                "--BOUNDARY\r\nContent-Type: text/plain\r\n\r\n{sentinel}"
            ))],
            None,
            None,
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartMissingClosingBoundary",
            "/multipart-missing-closing-boundary",
        ))
        .await?;

    let part = stream.next_part().await?.expect("first part");
    let err = collect_part_bytes(part)
        .await
        .expect_err("missing closing boundary should fail");
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn multipart_response_invalid_implicit_accept_is_rejected_before_transport() {
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            vec![Bytes::from_static(b"--BOUNDARY--\r\n")],
            None,
            None,
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut plan = multipart_response_plan::<Mixed>(
        "MultipartInvalidImplicitAccept",
        "/multipart-invalid-implicit-accept",
    );
    plan.endpoint.response.accept = None;

    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, BadMultipartAccept> as concord_core::advanced::ResponseEntity>::execute(&client, plan)
        .await
        .expect_err("invalid accept should fail");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert!(format!("{err:?}").contains("content_type"));
    assert_eq!(transport.sent_count().await, 0);
}

#[tokio::test]
async fn multipart_form_data_response_content_type_is_accepted() -> Result<(), ApiClientError> {
    let boundary = "BOUNDARY";
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/form-data; boundary=BOUNDARY",
            multipart_body_chunks(boundary),
            None,
            None,
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, FormData> as concord_core::advanced::ResponseEntity>::execute(&client,
            multipart_response_plan::<FormData>(
                "MultipartFormDataResponse",
                "/multipart-form-data",
            ),
        )
        .await?;

    let part = stream.next_part().await?.expect("first part");
    assert_eq!(
        part.content_type().and_then(|value| value.to_str().ok()),
        Some("text/plain")
    );
    Ok(())
}

#[tokio::test]
async fn multipart_missing_boundary_is_rejected_before_body_exposure() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed",
            vec![Bytes::from_static(
                b"--BOUNDARY\r\nContent-Type: text/plain\r\n\r\nhello\r\n--BOUNDARY--\r\n",
            )],
            None,
            Some(read_count.clone()),
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartMissingBoundary",
            "/multipart-missing-boundary",
        ))
        .await
        .expect_err("missing boundary should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(format!("{err}").contains("missing a boundary parameter"));
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn multipart_headers_split_across_chunks_are_parsed_incrementally()
-> Result<(), ApiClientError> {
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            vec![
                Bytes::from_static(b"--BOUNDARY\r\nContent-Ty"),
                Bytes::from_static(b"pe: text/plain\r\n\r\nhello\r\n--BOUNDARY--\r\n"),
            ],
            None,
            None,
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartSplitHeaders",
            "/multipart-split-headers",
        ))
        .await?;

    let part = stream.next_part().await?.expect("first part");
    let bytes = collect_part_bytes(part).await?;
    assert_eq!(bytes, Bytes::from_static(b"hello"));
    assert!(stream.next_part().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn multipart_wrong_content_type_is_rejected_before_body_exposure() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/form-data; boundary=BOUNDARY",
            vec![Bytes::from_static(
                b"--BOUNDARY\r\nContent-Type: text/plain\r\n\r\nhello\r\n--BOUNDARY--\r\n",
            )],
            None,
            Some(read_count.clone()),
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartWrongContentType",
            "/multipart-wrong-content-type",
        ))
        .await
        .expect_err("content type mismatch should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(format!("{err}").contains("did not match expected media type"));
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn multipart_malformed_boundary_is_rejected_body_safely() -> Result<(), ApiClientError> {
    let sentinel = "SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR";
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            vec![Bytes::from(format!(
                "--BOUNDARY\r\nX-Bad-Header\r\n\r\n{sentinel}\r\n--BOUNDARY--\r\n"
            ))],
            None,
            None,
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartMalformedBoundary",
            "/multipart-malformed-boundary",
        ))
        .await?;

    assert!(!format!("{stream:?}").contains(sentinel));
    let err = stream
        .next_part()
        .await
        .expect_err("malformed boundary should fail");
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn multipart_content_length_limit_applies_before_body_exposure() {
    let boundary = "BOUNDARY";
    let chunks = multipart_body_chunks(boundary);
    let total_len: u64 = chunks.iter().map(|chunk| chunk.len() as u64).sum();
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            chunks,
            Some(total_len),
            None,
        )],
    );
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes((total_len - 1) as usize);
    });

    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartContentLengthLimit",
            "/multipart-content-length-limit",
        ))
        .await
        .expect_err("content-length should exceed configured limit");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert!(format!("{err}").contains("response Content-Length"));
}

#[tokio::test]
async fn multipart_response_stream_limit_applies_while_reading() -> Result<(), ApiClientError> {
    let boundary = "BOUNDARY";
    let first = Bytes::from(format!(
        "--{boundary}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"a.txt\"\r\n\r\nhello\r\n--BOUN"
    ));
    let second = Bytes::from(format!(
        "DARY\r\nContent-Type: text/plain\r\n\r\nworld\r\n--{boundary}--\r\n"
    ));
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            vec![first.clone(), second],
            None,
            None,
        )],
    );
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(first.len());
    });

    let mut stream = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, multipart_response_plan::<Mixed>(
            "MultipartStreamLimit",
            "/multipart-stream-limit",
        ))
        .await?;

    let mut part = stream.next_part().await?.expect("first part");
    let first_chunk = part.next_chunk().await?.expect("first body chunk");
    assert!(!first_chunk.is_empty());
    assert!(first_chunk.starts_with(b"h"));
    assert!(first_chunk.len() <= b"hello".len());

    let err = part
        .next_chunk()
        .await
        .expect_err("second body chunk should exceed limit");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    Ok(())
}

#[tokio::test]
async fn multipart_no_content_and_pagination_plans_are_rejected_before_transport() {
    let transport = MockTransport::new(
        Default::default(),
        vec![multipart_fixture_response(
            "multipart/mixed; boundary=BOUNDARY",
            multipart_body_chunks("BOUNDARY"),
            None,
            None,
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut paginated =
        multipart_response_plan::<Mixed>("MultipartPagination", "/multipart-pagination");
    paginated.endpoint.pagination = Some(concord_core::internal::PaginationMarker);
    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, paginated)
        .await
        .expect_err("pagination should be rejected");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(format!("{err}").contains("do not support pagination"));
    assert_eq!(transport.sent_count().await, 0);

    let mut no_content =
        multipart_response_plan::<Mixed>("MultipartNoContent", "/multipart-no-content");
    no_content.endpoint.response.no_content = true;
    let err = <concord_core::advanced::MultipartResponse<RawResponsePart, Mixed> as concord_core::advanced::ResponseEntity>::execute(&client, no_content)
        .await
        .expect_err("no-content should be rejected");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(format!("{err}").contains("no-content response plan"));
    assert_eq!(transport.sent_count().await, 0);
}
