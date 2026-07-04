use concord_core::advanced::{NdJson, RecordStream};
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::records_response_api::RecordsResponseApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    id: u64,
}

api! {
    client RecordsResponseApi {
        base "https://example.com"
    }

    GET Tail
        path ["logs"]
        -> Records<LogEntry, NdJson>
}

async fn usage(api: RecordsResponseApi) {
    let _records: RecordStream<LogEntry> = api.tail().execute_records().await.unwrap();
    let _also: RecordStream<LogEntry> = api.tail().execute().await.unwrap();
}

fn main() {}
