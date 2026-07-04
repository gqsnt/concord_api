use concord_core::advanced::{
    JsonSse, MultipartStream, OctetStream, RecordStream, SseStream, StreamResponse,
};
use concord_core::prelude::Json;
use concord_macros::api;
use self::sse_coexists_api::SseCoexistsApi;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MyEvent {
    id: u64,
    msg: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct LogEntry {
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct UploadResult {
    ok: bool,
}

api! {
    client SseCoexistsApi {
        base "https://example.com"
    }

    GET Buffered
        path ["buffered"]
        -> Json<UploadResult>

    GET Download
        path ["download"]
        -> Stream<OctetStream>

    GET Tail
        path ["tail"]
        -> Records<LogEntry, concord_core::advanced::NdJson>

    GET Multipart
        path ["multipart"]
        -> Multipart<concord_core::advanced::RawResponsePart>

    GET Events
        path ["events"]
        -> Sse<MyEvent, JsonSse>
}

async fn usage(api: SseCoexistsApi) {
    let _: UploadResult = api.buffered().execute().await.unwrap();
    let _: StreamResponse<OctetStream> = api.download().execute().await.unwrap();
    let _: RecordStream<LogEntry> = api.tail().execute_records().await.unwrap();
    let _: MultipartStream<concord_core::advanced::RawResponsePart> =
        api.multipart().execute_multipart().await.unwrap();
    let _: SseStream<MyEvent> = api.events().execute_sse().await.unwrap();
    let _: SseStream<MyEvent> = api.events().execute().await.unwrap();
}

fn main() {}
