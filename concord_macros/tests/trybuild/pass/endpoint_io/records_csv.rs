use concord_core::advanced::{Csv, CsvCommaDelim, RecordBody, RecordStream};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::records_csv_api::RecordsCsvApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    pub ok: bool,
}

api! {
    client RecordsCsvApi {
        base "https://example.com"
    }

    POST Upload(body: Records<LogEntry, Csv<CsvCommaDelim>>)
        path ["records", "upload"]
        -> Json<UploadResult>

    GET Tail
        path ["records", "tail"]
        -> Records<LogEntry, Csv<CsvCommaDelim>>
}

async fn usage(api: RecordsCsvApi) {
    let _request = api.upload(RecordBody::<LogEntry>::from_iter(vec![LogEntry {
        id: 1,
        message: "hello".to_string(),
    }]));
    let _response: RecordStream<LogEntry> = api.tail().execute_records().await.unwrap();
    let _also: RecordStream<LogEntry> = api.tail().execute().await.unwrap();
}

fn main() {}
