use super::common::{MockResponse, MockTransport, TestAuthVars, TestCx, client};
use bytes::Bytes;
use concord_core::advanced::RawStreamResponse;
use concord_core::advanced::{
    ErrorContext, JsonSse, Mixed, OctetStream, RawResponsePart, RecordResponse, ResponseEntity,
    SseResponse, StreamResponse,
};
use concord_core::advanced::{MultipartResponse, MultipartStream, NdJson, RecordStream, SseStream};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute,
};
use concord_core::prelude::{ApiClientError, Endpoint, ReusableEndpoint};
use http::{HeaderValue, Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

fn request_plan(
    response_plan: concord_core::internal::ResponsePlan,
    name: &'static str,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: Method::GET,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", "/test"),
            policy: ResolvedPolicy::default(),
            body: BodyPlan::None,
            response: response_plan,
            pagination: None,
        },
        args: RequestArgs::default(),
        overrides: RequestOverrides::default(),
    }
}

fn mock_response(content_type: &'static str, body: impl Into<Bytes>) -> MockResponse {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    MockResponse {
        status: StatusCode::OK,
        headers,
        body: body.into(),
        content_length: None,
        chunks: None,
        read_count: None,
    }
}

#[derive(Clone, Copy, Default)]
struct StreamEndpoint;

impl Endpoint<TestCx> for StreamEndpoint {
    type Response = StreamResponse<OctetStream>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<StreamResponse<OctetStream>, ApiClientError>> + Send + 'a>,
    >
    where
        T: concord_core::advanced::Transport + 'a,
    {
        Box::pin(async move { RawStreamResponse::<OctetStream>::execute(client, plan).await })
    }
}

impl ReusableEndpoint<TestCx> for StreamEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let response_plan = RawStreamResponse::<OctetStream>::plan(ErrorContext {
            endpoint: "StreamEndpoint",
            method: Method::GET,
        })?
        .response_plan;
        Ok(request_plan(response_plan, "StreamEndpoint"))
    }
}

#[derive(Clone, Copy, Default)]
struct RecordEndpoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordRow {
    value: String,
}

impl Endpoint<TestCx> for RecordEndpoint {
    type Response = RecordStream<RecordRow>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<RecordStream<RecordRow>, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        Box::pin(async move { RecordResponse::<RecordRow, NdJson>::execute(client, plan).await })
    }
}

impl ReusableEndpoint<TestCx> for RecordEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let response_plan = RecordResponse::<RecordRow, NdJson>::plan(ErrorContext {
            endpoint: "RecordEndpoint",
            method: Method::GET,
        })?
        .response_plan;
        Ok(request_plan(response_plan, "RecordEndpoint"))
    }
}

#[derive(Clone, Copy, Default)]
struct MultipartEndpoint;

impl Endpoint<TestCx> for MultipartEndpoint {
    type Response = MultipartStream<RawResponsePart>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<MultipartStream<RawResponsePart>, ApiClientError>>
                + Send
                + 'a,
        >,
    >
    where
        T: concord_core::advanced::Transport + 'a,
    {
        Box::pin(
            async move { MultipartResponse::<RawResponsePart, Mixed>::execute(client, plan).await },
        )
    }
}

impl ReusableEndpoint<TestCx> for MultipartEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let response_plan = MultipartResponse::<RawResponsePart, Mixed>::plan(ErrorContext {
            endpoint: "MultipartEndpoint",
            method: Method::GET,
        })?
        .response_plan;
        Ok(request_plan(response_plan, "MultipartEndpoint"))
    }
}

#[derive(Clone, Copy, Default)]
struct SseEndpoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SseEvent {
    value: String,
}

impl Endpoint<TestCx> for SseEndpoint {
    type Response = SseStream<SseEvent>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<SseStream<SseEvent>, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        Box::pin(async move { SseResponse::<SseEvent, JsonSse>::execute(client, plan).await })
    }
}

impl ReusableEndpoint<TestCx> for SseEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let response_plan = SseResponse::<SseEvent, JsonSse>::plan(ErrorContext {
            endpoint: "SseEndpoint",
            method: Method::GET,
        })?
        .response_plan;
        Ok(request_plan(response_plan, "SseEndpoint"))
    }
}

#[tokio::test]
async fn pending_request_execute_stream_uses_response_entity_execution()
-> Result<(), ApiClientError> {
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "application/octet-stream",
            Bytes::from_static(b"stream-body"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let mut response: StreamResponse<OctetStream> =
        client.request(StreamEndpoint).execute_stream().await?;
    let chunk = response.next_chunk().await?.expect("stream chunk");
    assert_eq!(chunk, Bytes::from_static(b"stream-body"));
    Ok(())
}

#[tokio::test]
async fn pending_request_execute_records_uses_response_entity_execution()
-> Result<(), ApiClientError> {
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "application/x-ndjson",
            Bytes::from_static(b"{\"value\":\"record\"}\n"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let mut response: RecordStream<RecordRow> =
        client.request(RecordEndpoint).execute_records().await?;
    let record = response.next_record().await?.expect("record");
    assert_eq!(
        record,
        RecordRow {
            value: "record".into()
        }
    );
    Ok(())
}

#[tokio::test]
async fn pending_request_execute_multipart_uses_response_entity_execution()
-> Result<(), ApiClientError> {
    let boundary = "BOUNDARY";
    let body = format!(
        "--{boundary}\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=\"a.txt\"\r\n\r\nhello\r\n--{boundary}--\r\n"
    );
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "multipart/mixed; boundary=BOUNDARY",
            Bytes::from(body),
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let mut response: MultipartStream<RawResponsePart> = client
        .request(MultipartEndpoint)
        .execute_multipart()
        .await?;
    let mut part = response.next_part().await?.expect("multipart part");
    let mut collected = bytes::BytesMut::new();
    while let Some(chunk) = part.next_chunk().await? {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected.freeze(), Bytes::from_static(b"hello"));
    Ok(())
}

#[tokio::test]
async fn pending_request_execute_sse_uses_response_entity_execution() -> Result<(), ApiClientError>
{
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![mock_response(
            "text/event-stream",
            Bytes::from_static(b"event: update\ndata: {\"value\":\"sse\"}\n\n"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let mut response: SseStream<SseEvent> = client.request(SseEndpoint).execute_sse().await?;
    let event = response.next_event().await?.expect("sse event");
    assert_eq!(
        event.data,
        SseEvent {
            value: "sse".into()
        }
    );
    Ok(())
}
