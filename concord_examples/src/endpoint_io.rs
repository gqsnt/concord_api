use bytes::Bytes;
use concord_core::advanced::{
    MultipartBody, MultipartStream, NdJson, OctetStream, RawResponsePart, RecordBody, RecordStream,
    SseStream, StreamBody, StreamResponse, WebSocketClient,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UploadResult {
    pub id: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClientMsg {
    pub id: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerMsg {
    pub id: u64,
}

api! {
    client EndpointIoApi {
        base "https://api.example.com"
    }

    POST UploadStream(body: Stream<OctetStream>)
        as upload_stream
        path ["stream", "upload"]
        -> Json<UploadResult>

    GET DownloadStream
        as download_stream
        path ["stream", "download"]
        -> Stream<OctetStream>

    POST IngestRecords(body: Records<LogEntry, NdJson>)
        as ingest_records
        path ["records", "ingest"]
        -> Json<UploadResult>

    GET TailRecords
        as tail_records
        path ["records", "tail"]
        -> Records<LogEntry, NdJson>

    POST UploadMultipart(body: Multipart<RawResponsePart>)
        as upload_multipart
        path ["multipart", "upload"]
        -> Json<UploadResult>

    GET DownloadMultipart
        as download_multipart
        path ["multipart", "download"]
        -> Multipart<RawResponsePart>

    POST UploadMultipartMixed(body: Multipart<RawResponsePart, ::concord_core::advanced::Mixed>)
        as upload_multipart_mixed
        path ["multipart", "upload-mixed"]
        -> Json<UploadResult>

    GET DownloadMultipartMixed
        as download_multipart_mixed
        path ["multipart", "download-mixed"]
        -> Multipart<RawResponsePart, ::concord_core::advanced::Mixed>

    GET Events
        path ["events"]
        -> Sse<Event>

    GET EventsExplicit
        path ["events-explicit"]
        -> Sse<Event, ::concord_core::advanced::JsonSse>

    WS Connect
        as connect
        path ["ws"]
        -> WebSocket<ClientMsg, ServerMsg>

    WS ConnectExplicit
        as connect_explicit
        path ["ws-explicit"]
        -> WebSocket<ClientMsg, ServerMsg, ::concord_core::advanced::JsonWebSocket>
}

pub use self::endpoint_io_api::{EndpointIoApi, endpoints};

pub async fn stream_examples(api: EndpointIoApi) -> Result<UploadResult, ApiClientError> {
    let _request = api.upload_stream(StreamBody::from_bytes(Bytes::from_static(b"stream")));
    let _response: StreamResponse<OctetStream> = api.download_stream().execute_stream().await?;
    let _also: StreamResponse<OctetStream> = api.download_stream().execute().await?;
    _request.execute().await
}

pub async fn records_examples(api: EndpointIoApi) -> Result<UploadResult, ApiClientError> {
    let body = RecordBody::from_iter(vec![LogEntry {
        id: 1,
        message: "hello".to_string(),
    }]);
    let _request = api.ingest_records(body);
    let _response: RecordStream<LogEntry> = api.tail_records().execute_records().await?;
    let _also: RecordStream<LogEntry> = api.tail_records().execute().await?;
    _request.execute().await
}

pub async fn multipart_examples(api: EndpointIoApi) -> Result<UploadResult, ApiClientError> {
    let body = MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"));
    let _request = api.upload_multipart(body);
    let _response: MultipartStream<RawResponsePart> =
        api.download_multipart().execute_multipart().await?;
    let _also: MultipartStream<RawResponsePart> = api.download_multipart().execute().await?;
    let _mixed = api.upload_multipart_mixed(MultipartBody::new().text("kind", "mixed"));
    let _mixed_response: MultipartStream<RawResponsePart> =
        api.download_multipart_mixed().execute_multipart().await?;
    _request.execute().await
}

pub async fn sse_examples(api: EndpointIoApi) -> Result<(), ApiClientError> {
    let _response: SseStream<Event> = api.events().execute_sse().await?;
    let _also: SseStream<Event> = api.events().execute().await?;
    let _explicit: SseStream<Event> = api.events_explicit().execute().await?;
    Ok(())
}

pub async fn websocket_examples(api: EndpointIoApi) -> Result<(), ApiClientError> {
    let _response: WebSocketClient<ClientMsg, ServerMsg> =
        api.connect().execute_websocket().await?;
    let _also: WebSocketClient<ClientMsg, ServerMsg> = api.connect().execute().await?;
    let _explicit: WebSocketClient<ClientMsg, ServerMsg> =
        api.connect_explicit().execute_websocket().await?;
    Ok(())
}
