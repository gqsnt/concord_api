use concord_core::advanced::{
    MediaType, NdJson, RecordBody, RecordDecoder, RecordEncoder, RecordFormat,
};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::records_custom_format_api::RecordsCustomFormatApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    id: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CustomRecordsFormat;

impl MediaType for CustomRecordsFormat {
    const CONTENT_TYPE: &'static str = "application/vnd.example.ndjson";
}

impl RecordFormat<LogEntry> for CustomRecordsFormat {
    fn encoder() -> Box<dyn RecordEncoder<LogEntry>> {
        NdJson::encoder()
    }

    fn decoder() -> Box<dyn RecordDecoder<LogEntry>> {
        NdJson::decoder()
    }
}

api! {
    client RecordsCustomFormatApi {
        base "https://example.com"
    }

    POST Upload(body: Records<LogEntry, CustomRecordsFormat>)
        path ["logs"]
        -> Json<LogEntry>
}

async fn usage(api: RecordsCustomFormatApi) {
    let _ = api.upload(RecordBody::<LogEntry>::from_iter(vec![LogEntry { id: 1 }]));
}

fn main() {}
