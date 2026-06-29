use bytes::Bytes;
use concord_core::advanced::{
    Csv, CsvCommaDelim, CsvSemicolonDelim, CsvTabDelim, JsonSse, Mixed, MultipartBody,
    MultipartStream, NdJson, OctetStream, RawResponsePart, RecordBody, RecordStream, SseStream,
    StreamBody, StreamResponse,
};
use concord_core::prelude::Json;
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

api! {
    client EndpointIoApi {
        base "https://api.example.com"
    }

    GET JsonResponse
        as json_response
        path ["json"]
        -> Json<UploadResult>

    GET NoContentResponse
        as no_content_response
        path ["no-content"]
        -> NoContent

    GET BytesResponse
        as bytes_response
        path ["bytes"]
        -> Bytes

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

    POST IngestCsv(body: Records<LogEntry, Csv<CsvCommaDelim>>)
        as ingest_csv
        path ["records", "csv", "ingest"]
        -> Json<UploadResult>

    GET TailCsv
        as tail_csv
        path ["records", "csv", "tail"]
        -> Records<LogEntry, Csv<CsvCommaDelim>>

    GET TailCsvSemicolon
        as tail_csv_semicolon
        path ["records", "csv", "tail-semicolon"]
        -> Records<LogEntry, Csv<CsvSemicolonDelim>>

    GET TailCsvTab
        as tail_csv_tab
        path ["records", "csv", "tail-tab"]
        -> Records<LogEntry, Csv<CsvTabDelim>>

    POST UploadMultipart(body: Multipart<RawResponsePart>)
        as upload_multipart
        path ["multipart", "upload"]
        -> Json<UploadResult>

    GET DownloadMultipart
        as download_multipart
        path ["multipart", "download"]
        -> Multipart<RawResponsePart>

    POST UploadMultipartMixed(body: Multipart<RawResponsePart, Mixed>)
        as upload_multipart_mixed
        path ["multipart", "upload-mixed"]
        -> Json<UploadResult>

    GET DownloadMultipartMixed
        as download_multipart_mixed
        path ["multipart", "download-mixed"]
        -> Multipart<RawResponsePart, Mixed>

    GET Events
        path ["events"]
        -> Sse<Event>

    GET EventsExplicit
        path ["events", "explicit"]
        -> Sse<Event, JsonSse>
}

pub use self::endpoint_io_api::{EndpointIoApi, endpoints};

pub async fn json_example(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    api.json_response().execute().await
}

pub async fn no_content_example(
    api: EndpointIoApi,
) -> Result<(), concord_core::prelude::ApiClientError> {
    api.no_content_response().execute().await
}

pub async fn bytes_example(
    api: EndpointIoApi,
) -> Result<::bytes::Bytes, concord_core::prelude::ApiClientError> {
    api.bytes_response().execute().await
}

pub async fn stream_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let _request = api.upload_stream(StreamBody::from_bytes(Bytes::from_static(b"stream")));
    let _response: StreamResponse<OctetStream> = api.download_stream().execute_stream().await?;
    let _also: StreamResponse<OctetStream> = api.download_stream().execute().await?;
    _request.execute().await
}

pub async fn records_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let body = RecordBody::from_iter(vec![LogEntry {
        id: 1,
        message: "hello".to_string(),
    }]);
    let _request = api.ingest_records(body);
    let _response: RecordStream<LogEntry> = api.tail_records().execute_records().await?;
    let _also: RecordStream<LogEntry> = api.tail_records().execute().await?;
    _request.execute().await
}

pub async fn csv_records_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let body = RecordBody::from_iter(vec![LogEntry {
        id: 2,
        message: "csv".to_string(),
    }]);
    let _request = api.ingest_csv(body);
    let _response: RecordStream<LogEntry> = api.tail_csv().execute_records().await?;
    let _also: RecordStream<LogEntry> = api.tail_csv().execute().await?;
    let _semicolon: RecordStream<LogEntry> = api.tail_csv_semicolon().execute_records().await?;
    let _tab: RecordStream<LogEntry> = api.tail_csv_tab().execute_records().await?;
    _request.execute().await
}

pub async fn multipart_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let body = MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"));
    let _request = api.upload_multipart(body);
    let _response: MultipartStream<RawResponsePart> =
        api.download_multipart().execute_multipart().await?;
    let _also: MultipartStream<RawResponsePart> = api.download_multipart().execute().await?;
    _request.execute().await
}

pub async fn sse_examples(api: EndpointIoApi) -> Result<(), concord_core::prelude::ApiClientError> {
    let _response: SseStream<Event> = api.events().execute_sse().await?;
    let _also: SseStream<Event> = api.events().execute().await?;
    Ok(())
}

pub async fn multipart_mixed_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let body = MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"));
    let _request = api.upload_multipart_mixed(body);
    let _response: MultipartStream<RawResponsePart> =
        api.download_multipart_mixed().execute_multipart().await?;
    let _also: MultipartStream<RawResponsePart> = api.download_multipart_mixed().execute().await?;
    _request.execute().await
}

pub async fn sse_explicit_examples(
    api: EndpointIoApi,
) -> Result<(), concord_core::prelude::ApiClientError> {
    let _response: SseStream<Event> = api.events_explicit().execute_sse().await?;
    let _also: SseStream<Event> = api.events_explicit().execute().await?;
    Ok(())
}
