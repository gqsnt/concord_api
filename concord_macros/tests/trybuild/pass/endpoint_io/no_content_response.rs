use concord_core::advanced::{Csv, CsvCommaDelim, JsonSse, Mixed, NdJson, RawResponsePart};
use concord_core::prelude::Json;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::no_content_response_api::NoContentResponseApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonPayload {
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
}

api! {
    client NoContentResponseApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> NoContent

    GET JsonPing
        path ["json"]
        -> Json<JsonPayload>

    GET RecordsPing
        path ["records"]
        -> Records<JsonPayload, NdJson>

    GET CsvPing
        path ["csv"]
        -> Records<JsonPayload, Csv<CsvCommaDelim>>

    GET MultipartPing
        path ["multipart"]
        -> Multipart<RawResponsePart, Mixed>

    GET EventsPing
        path ["events"]
        -> Sse<Event, JsonSse>
}

async fn usage(api: NoContentResponseApi) {
    let _: () = api.ping().execute().await.unwrap();
}

fn main() {}
