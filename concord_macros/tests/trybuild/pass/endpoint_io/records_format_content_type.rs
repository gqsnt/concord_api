use concord_core::advanced::{
    CodecError, ContentType, RecordBody, RecordDecoder, RecordEncoder, RecordFormat, RecordStream,
};
use concord_core::prelude::Json;
use concord_macros::api;
use self::records_format_content_type_api::RecordsFormatContentTypeApi;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PipeEntry {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PipeText;

impl ContentType for PipeText {
    const CONTENT_TYPE: &'static str = "text/x-pipe-records";
}

struct PipeTextEncoder;

impl RecordEncoder<PipeEntry> for PipeTextEncoder {
    fn encode_record(&mut self, value: PipeEntry) -> Result<bytes::Bytes, CodecError> {
        Ok(bytes::Bytes::from(format!("{}|{}\n", value.id, value.message)))
    }
}

#[derive(Default)]
struct PipeTextDecoder {
    buffer: Vec<u8>,
}

impl RecordDecoder<PipeEntry> for PipeTextDecoder {
    fn push_chunk(&mut self, chunk: bytes::Bytes) -> Result<Vec<PipeEntry>, CodecError> {
        self.buffer.extend_from_slice(&chunk);
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Vec<PipeEntry>, CodecError> {
        Ok(Vec::new())
    }
}

impl RecordFormat<PipeEntry> for PipeText {
    fn encoder() -> Box<dyn RecordEncoder<PipeEntry>> {
        Box::new(PipeTextEncoder)
    }

    fn decoder() -> Box<dyn RecordDecoder<PipeEntry>> {
        Box::new(PipeTextDecoder::default())
    }
}

api! {
    client RecordsFormatContentTypeApi {
        base "https://example.com"
    }

    POST Upload(body: Records<PipeEntry, PipeText>)
        path ["logs"]
        -> Json<PipeEntry>

    GET Tail
        path ["logs"]
        -> Records<PipeEntry, PipeText>
}

async fn usage(api: RecordsFormatContentTypeApi) {
    let _ = api
        .upload(RecordBody::<PipeEntry>::from_iter(vec![PipeEntry {
            id: 1,
            message: "hello".to_string(),
        }]))
        .execute()
        .await
        .unwrap();

    let _records: RecordStream<PipeEntry> = api.tail().execute_records().await.unwrap();
}

fn main() {}
