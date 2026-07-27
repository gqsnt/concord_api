//! Generic bounded buffered and streaming record delivery.
//!
//! Required features: `records`, `csv`, and `ndjson`. Mode: locally runnable
//! through scripted loopback responses only. Documentation owners:
//! `docs/features/records.md` and
//! `docs/extending/record-decoders.md`. Proving test:
//! `tests/local_runtime.rs`.

use std::num::NonZeroUsize;
use std::pin::Pin;

use concord::api;
use concord::codec::BytesMut;
use concord::records::{
    BufferedRecords, Csv, HasHeader, NdJson, RecordDecoder, RecordError, RecordErrorKind, Records,
    SemicolonDelimiter,
};
use concord::response::ResponseHead;
use futures_core::Stream as _;

/// Header-bearing, semicolon-delimited CSV used by the configured endpoint.
pub type ConfCsv<T> = Csv<T, HasHeader, SemicolonDelimiter>;

// ANCHOR: record_decoder_extension
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
// ANCHOR_END: record_decoder_extension

// ANCHOR: records_contract
api! {
    client RecordsApi {
        base "http://records.example.test/";
    }

    GET custom() -> Records<crate::request_response::records::PipeRecords>;
    GET buffered_custom() -> BufferedRecords<crate::request_response::records::PipeRecords>;
    GET csv_rows() -> Csv<(String, u32)>;
    GET csv_stream() -> Records<Csv<(String, u32)>>;
    GET configured_csv_rows() -> crate::request_response::records::ConfCsv<(String, u32)>;
    GET configured_csv_stream() -> Records<crate::request_response::records::ConfCsv<(String, u32)>>;
    GET nd_json_values() -> NdJson<u32>;
    GET nd_json_stream() -> Records<NdJson<u32>>;
}
// ANCHOR_END: records_contract

/// Constructs all record-stream requests without opening a connection.
// ANCHOR: record_requests
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = RecordsApi::builder().build()?;
    let _: records_api::CustomRequest = client.custom();
    let _: records_api::BufferedCustomRequest = client.buffered_custom();
    let _: records_api::CsvRowsRequest = client.csv_rows();
    let _: records_api::CsvStreamRequest = client.csv_stream();
    let _: records_api::ConfiguredCsvRowsRequest = client.configured_csv_rows();
    let _: records_api::ConfiguredCsvStreamRequest = client.configured_csv_stream();
    let _: records_api::NdJsonValuesRequest = client.nd_json_values();
    let _: records_api::NdJsonStreamRequest = client.nd_json_stream();
    Ok(())
}
// ANCHOR_END: record_requests

/// Opens a CSV record stream and consumes every decoded row.
// ANCHOR: consume_records
pub async fn read_csv_rows(
    client: &RecordsApi,
) -> Result<Vec<(String, u32)>, Box<dyn std::error::Error>> {
    let mut stream = client.csv_stream().await?;
    let mut rows = Vec::new();
    while let Some(row) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        rows.push(row?);
    }
    Ok(rows)
}
// ANCHOR_END: consume_records

/// Collects a complete CSV response through the ordinary buffered lifecycle.
// ANCHOR: collect_buffered_records
pub async fn read_buffered_csv_rows(
    client: &RecordsApi,
) -> Result<Vec<(String, u32)>, concord::ApiError> {
    client.csv_rows().await
}
// ANCHOR_END: collect_buffered_records
