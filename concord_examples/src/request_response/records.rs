//! Generic bounded record streaming with custom, CSV, and NDJSON decoders.
//!
//! Required features: `records`, `csv`, and `ndjson`. Mode: locally runnable
//! through scripted loopback responses only. Documentation owner:
//! `docs/guide/records.md`. Proving test: `tests/local_runtime.rs`.

use std::num::NonZeroUsize;

use concord::api;
use concord::codec::BytesMut;
use concord::records::{RecordDecoder, RecordError, RecordErrorKind};
use concord::response::ResponseHead;

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
                RecordError::new(RecordErrorKind::Decode)
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
                    RecordError::new(RecordErrorKind::Decode)
                })
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64 * 1024).expect("positive example staging bound")
    }
}

api! {
    client RecordsApi {
        base "http://records.example.test/";
    }

    GET custom() -> Records<crate::request_response::records::PipeRecords>;
    GET csv_rows() -> Records<Csv<(String, u32)>>;
    GET nd_json_values() -> Records<NdJson<u32>>;
}

/// Constructs all record-stream requests without opening a connection.
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = RecordsApi::builder().build()?;
    let _: records_api::CustomRequest = client.custom();
    let _: records_api::CsvRowsRequest = client.csv_rows();
    let _: records_api::NdJsonValuesRequest = client.nd_json_values();
    Ok(())
}
