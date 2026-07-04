use concord_core::advanced::{NdJson, RecordBody};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::records_request_buffered_response_api::RecordsRequestBufferedResponseApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    ok: bool,
}

api! {
    client RecordsRequestBufferedResponseApi {
        base "https://example.com"
    }

    POST Upload(body: Records<LogEntry, NdJson>)
        path ["logs"]
        -> Json<UploadResult>
}

async fn usage(api: RecordsRequestBufferedResponseApi) {
    let _ = api.upload(RecordBody::<LogEntry>::from_iter(vec![LogEntry { id: 1 }]));
    let _ = api
        .upload(RecordBody::<LogEntry>::from_iter(vec![LogEntry { id: 1 }]))
        .execute()
        .await;
}

fn main() {}
