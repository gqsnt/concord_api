use super::common::{MockResponse, MockTransport, TestAuthVars, client, request_plan};
use bytes::{Bytes, BytesMut};
use concord_core::advanced::{
    ErrorContext, JsonSse, Mixed, MultipartResponse, MultipartStream, NdJson, NoContentResponse,
    OctetStream, RawResponsePart, RawStreamResponse, RecordResponse, RecordStream, ResponseEntity,
    SseResponse, SseStream, StreamResponse,
};
use concord_core::internal::{RequestPlan, ResolvedPolicy, ResponsePlan};
use concord_core::prelude::ApiClientError;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

fn ctx(endpoint: &'static str) -> ErrorContext {
    ErrorContext {
        endpoint,
        method: Method::GET,
    }
}

fn request_plan_with_response(
    response_plan: ResponsePlan,
    name: &'static str,
    path: &'static str,
) -> RequestPlan {
    let mut plan = request_plan(name, Method::GET, path, ResolvedPolicy::default(), None);
    plan.endpoint.response = response_plan;
    plan
}

fn mock_response(
    content_type: &'static str,
    status: StatusCode,
    body: impl Into<Bytes>,
) -> MockResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    MockResponse {
        status,
        headers,
        body: body.into(),
        content_length: None,
        chunks: None,
        read_count: None,
    }
}

fn mock_chunked_response(
    content_type: &'static str,
    status: StatusCode,
    chunks: Vec<Bytes>,
) -> MockResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    MockResponse {
        status,
        headers,
        body: Bytes::new(),
        content_length: None,
        chunks: Some(chunks),
        read_count: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
struct RecordRow {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct SseEvent {
    value: String,
}

async fn collect_part_bytes(mut part: RawResponsePart) -> Result<Bytes, ApiClientError> {
    let mut out = BytesMut::new();
    while let Some(chunk) = part.next_chunk().await? {
        out.extend_from_slice(&chunk);
    }
    Ok(out.freeze())
}

#[tokio::test]
async fn buffered_response_accepts_text_body_and_exposes_plan() -> Result<(), ApiClientError> {
    let entity_plan = concord_core::advanced::BufferedResponse::<
        concord_core::prelude::Text<String>,
    >::plan(ctx("BufferedText"))?;
    let accept = entity_plan
        .response_plan
        .accept
        .as_ref()
        .and_then(|value| value.to_str().ok())
        .expect("text accept header");
    assert!(accept.starts_with("text/plain"));
    assert_eq!(entity_plan.response_plan.no_content, false);
    assert_eq!(
        entity_plan.response_plan.format,
        concord_core::internal::Format::Text
    );
    assert!(entity_plan.capabilities.supports_pagination);
    assert!(!entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response("text/plain", StatusCode::CREATED, "created")],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = request_plan_with_response(entity_plan.response_plan, "BufferedText", "/text");

    let decoded =
        concord_core::advanced::BufferedResponse::<concord_core::prelude::Text<String>>::execute(
            &client, plan,
        )
        .await?;

    assert_eq!(decoded, "created");
    Ok(())
}

#[tokio::test]
async fn bytes_response_returns_raw_bytes() -> Result<(), ApiClientError> {
    let entity_plan = concord_core::advanced::BytesResponse::plan(ctx("BytesResponse"))?;
    assert_eq!(entity_plan.response_plan.accept, None);
    assert_eq!(entity_plan.response_plan.no_content, false);
    assert_eq!(
        entity_plan.response_plan.format,
        concord_core::internal::Format::Binary
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(!entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "application/octet-stream",
            StatusCode::OK,
            Bytes::from_static(b"raw-bytes"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = request_plan_with_response(entity_plan.response_plan, "BytesResponse", "/bytes");

    let response = concord_core::advanced::BytesResponse::execute(&client, plan).await?;
    assert_eq!(response, Bytes::from_static(b"raw-bytes"));
    Ok(())
}

#[tokio::test]
async fn no_content_response_accepts_204_without_body() -> Result<(), ApiClientError> {
    let entity_plan = NoContentResponse::plan(ctx("NoContentResponse"))?;
    assert_eq!(entity_plan.response_plan.accept, None);
    assert_eq!(entity_plan.response_plan.no_content, true);
    assert_eq!(
        entity_plan.response_plan.format,
        concord_core::internal::Format::Text
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(!entity_plan.capabilities.is_streaming);
    assert!(entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "text/plain",
            StatusCode::NO_CONTENT,
            Bytes::new(),
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = request_plan_with_response(
        entity_plan.response_plan,
        "NoContentResponse",
        "/no-content",
    );

    NoContentResponse::execute(&client, plan).await?;
    Ok(())
}

#[tokio::test]
async fn raw_stream_response_decodes_stream_body() -> Result<(), ApiClientError> {
    let entity_plan = RawStreamResponse::<OctetStream>::plan(ctx("RawStreamResponse"))?;
    assert_eq!(
        entity_plan.response_plan.accept,
        Some(HeaderValue::from_static("application/octet-stream"))
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_chunked_response(
            "application/octet-stream",
            StatusCode::OK,
            vec![Bytes::from_static(b"stream-body")],
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan =
        request_plan_with_response(entity_plan.response_plan, "RawStreamResponse", "/stream");

    let mut stream: StreamResponse<OctetStream> =
        RawStreamResponse::<OctetStream>::execute(&client, plan).await?;
    let chunk = stream.next_chunk().await?.expect("stream chunk");
    assert_eq!(chunk, Bytes::from_static(b"stream-body"));
    Ok(())
}

#[tokio::test]
async fn record_response_decodes_ndjson_body() -> Result<(), ApiClientError> {
    let entity_plan = RecordResponse::<RecordRow, NdJson>::plan(ctx("RecordResponse"))?;
    assert_eq!(
        entity_plan.response_plan.accept,
        Some(HeaderValue::from_static("application/x-ndjson"))
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "application/x-ndjson",
            StatusCode::OK,
            Bytes::from_static(b"{\"value\":\"record\"}\n"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = request_plan_with_response(entity_plan.response_plan, "RecordResponse", "/records");

    let mut stream: RecordStream<RecordRow> =
        RecordResponse::<RecordRow, NdJson>::execute(&client, plan).await?;
    let record = stream.next_record().await?.expect("record");
    assert_eq!(
        record,
        RecordRow {
            value: "record".to_string(),
        }
    );
    Ok(())
}

#[tokio::test]
async fn multipart_response_decodes_multipart_body() -> Result<(), ApiClientError> {
    let entity_plan = MultipartResponse::<RawResponsePart, Mixed>::plan(ctx("MultipartResponse"))?;
    assert_eq!(
        entity_plan.response_plan.accept,
        Some(HeaderValue::from_static("multipart/mixed"))
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let boundary = "BOUNDARY";
    let body = format!(
        "--{boundary}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"a.txt\"\r\n\r\nhello\r\n--{boundary}--\r\n"
    );
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_chunked_response(
            "multipart/mixed; boundary=BOUNDARY",
            StatusCode::OK,
            vec![Bytes::from(body)],
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan =
        request_plan_with_response(entity_plan.response_plan, "MultipartResponse", "/multipart");

    let mut stream: MultipartStream<RawResponsePart> =
        MultipartResponse::<RawResponsePart, Mixed>::execute(&client, plan).await?;
    let part = stream.next_part().await?.expect("multipart part");
    let bytes = collect_part_bytes(part).await?;
    assert_eq!(bytes, Bytes::from_static(b"hello"));
    Ok(())
}

#[tokio::test]
async fn sse_response_decodes_event_stream_body() -> Result<(), ApiClientError> {
    let entity_plan = SseResponse::<SseEvent, JsonSse>::plan(ctx("SseResponse"))?;
    assert_eq!(
        entity_plan.response_plan.accept,
        Some(HeaderValue::from_static("text/event-stream"))
    );
    assert!(!entity_plan.capabilities.supports_pagination);
    assert!(entity_plan.capabilities.is_streaming);
    assert!(!entity_plan.capabilities.is_no_content);

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_chunked_response(
            "text/event-stream",
            StatusCode::OK,
            vec![Bytes::from_static(
                b"event: update\ndata: {\"value\":\"sse\"}\n\n",
            )],
        )],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = request_plan_with_response(entity_plan.response_plan, "SseResponse", "/sse");

    let mut stream: SseStream<SseEvent> =
        SseResponse::<SseEvent, JsonSse>::execute(&client, plan).await?;
    let event = stream.next_event().await?.expect("sse event");
    assert_eq!(
        event.data,
        SseEvent {
            value: "sse".to_string(),
        }
    );
    Ok(())
}
