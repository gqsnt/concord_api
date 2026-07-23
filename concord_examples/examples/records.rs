#![forbid(unsafe_code)]

use std::num::NonZeroUsize;

use concord_core::advanced::{BytesMut, RecordDecoder, RecordError, ResponseHead};
use concord_macros::api;

pub struct PipeRecords;

impl RecordDecoder for PipeRecords {
    type Record = String;

    fn accept() -> Option<&'static str> {
        Some("application/x-pipe-records")
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<String>, RecordError> {
        let Some(end) = buffer.iter().position(|byte| *byte == b'|') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        String::from_utf8(record.to_vec())
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(concord_core::advanced::RecordErrorKind::Decode)
            })
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<String>, RecordError> {
        if buffer.is_empty() {
            Ok(None)
        } else {
            let record = buffer.split_to(buffer.len());
            String::from_utf8(record.to_vec())
                .map(Some)
                .map_err(|error| {
                    drop(error);
                    RecordError::new(concord_core::advanced::RecordErrorKind::Decode)
                })
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64 * 1024).expect("positive example staging bound")
    }
}

api! {
    client RecordsApi {
        base "https://records.example.test/";
    }

    GET Custom() -> Records<crate::PipeRecords>;
    GET CsvRows() -> Records<Csv<(String, u32)>>;
    GET NdJsonValues() -> Records<NdJson<u32>>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RecordsApi::builder().build()?;
    let b = client.custom().await?;
    let r = client.csv_rows().await?;
    let c = client.nd_json_values().await?;
    Ok(())
}
