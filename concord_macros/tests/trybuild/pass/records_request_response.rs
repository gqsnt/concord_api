use concord_core::advanced::{NdJson, RecordBody, RecordStream};
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::records_request_response_api::RecordsRequestResponseApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    id: u64,
}

api! {
    client RecordsRequestResponseApi {
        base "https://example.com"
    }

    POST Mirror(body: Records<LogEntry, NdJson>)
        path ["mirror"]
        -> Records<LogEntry, NdJson>
}

async fn usage(api: RecordsRequestResponseApi) {
    let _records: RecordStream<LogEntry> = api
        .mirror(RecordBody::<LogEntry>::from_iter(vec![LogEntry { id: 1 }]))
        .execute_records()
        .await
        .unwrap();
}

fn main() {}
