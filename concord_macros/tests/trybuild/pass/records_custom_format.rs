use concord_core::advanced::{
    CodecError, MediaType, RecordBody, RecordDecoder, RecordEncoder, RecordFormat, RecordStream,
};
use concord_core::prelude::Json;
use concord_macros::api;
use self::records_custom_format_api::RecordsCustomFormatApi;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PipeEntry {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PipeText;

impl MediaType for PipeText {
    const CONTENT_TYPE: &'static str = "text/x-pipe-records";
}

struct PipeTextEncoder;

impl RecordEncoder<PipeEntry> for PipeTextEncoder {
    fn encode_record(&mut self, value: PipeEntry) -> Result<bytes::Bytes, CodecError> {
        if value.message.contains('|') || value.message.contains('\n') || value.message.contains('\r') {
            return Err(CodecError::new("record encoding failed"));
        }
        Ok(bytes::Bytes::from(format!("{}|{}\n", value.id, value.message)))
    }
}

#[derive(Default)]
struct PipeTextDecoder {
    buffer: Vec<u8>,
}

impl PipeTextDecoder {
    fn decode_line(&self, line: &[u8]) -> Result<PipeEntry, CodecError> {
        let text = std::str::from_utf8(line).map_err(|_| CodecError::new("record decode failed"))?;
        if text.is_empty() || text.contains('\r') {
            return Err(CodecError::new("record decode failed"));
        }
        let mut parts = text.split('|');
        let id = parts
            .next()
            .ok_or_else(|| CodecError::new("record decode failed"))?;
        let message = parts
            .next()
            .ok_or_else(|| CodecError::new("record decode failed"))?;
        if parts.next().is_some() || id.is_empty() {
            return Err(CodecError::new("record decode failed"));
        }
        let id = id
            .parse::<u64>()
            .map_err(|_| CodecError::new("record decode failed"))?;
        Ok(PipeEntry {
            id,
            message: message.to_string(),
        })
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<PipeEntry>, CodecError> {
        let mut out = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop();
            out.push(self.decode_line(&line)?);
        }
        if finalizing && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            out.push(self.decode_line(&line)?);
        }
        Ok(out)
    }
}

impl RecordDecoder<PipeEntry> for PipeTextDecoder {
    fn push_chunk(&mut self, chunk: bytes::Bytes) -> Result<Vec<PipeEntry>, CodecError> {
        self.buffer.extend_from_slice(&chunk);
        self.parse_available(false)
    }

    fn finish(&mut self) -> Result<Vec<PipeEntry>, CodecError> {
        self.parse_available(true)
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
    client RecordsCustomFormatApi {
        base "https://example.com"
    }

    POST Upload(body: Records<PipeEntry, PipeText>)
        path ["logs"]
        -> Json<PipeEntry>

    GET Tail
        path ["logs"]
        -> Records<PipeEntry, PipeText>
}

async fn usage(api: RecordsCustomFormatApi) {
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
