//! Generic bounded buffered and streaming record delivery.
//!
//! Required features: `records`, `csv`, and `ndjson`. Mode: locally runnable
//! through scripted loopback responses only. Documentation owners:
//! `docs/responses.md` and `docs/extensions.md`. Proving test:
//! `tests/local_runtime.rs`.

use std::num::NonZeroUsize;
use std::pin::Pin;

use concord::api;
use concord::codec::{Bytes, BytesMut};
use concord::records::{
    Csv, HasHeader, ItemStream, NdJson, RecordDecoder, RecordError, RecordErrorKind,
    SemicolonDelimiter, decode_complete_records, validate_record_decoder,
};
use concord::response::{
    Buffered, ResponseBodyError, ResponseDriver, ResponseHead, ResponseRepresentation, Streaming,
};
use futures_core::Stream as _;

/// Header-bearing, semicolon-delimited CSV used by the configured endpoint.
pub type ConfCsv = Csv<HasHeader, SemicolonDelimiter>;

// ANCHOR: record_decoder_extension
pub struct PipeRecords;

/// Representation marker hiding the pipe decoder from endpoint outputs.
pub struct PipeDelimited;

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

impl ResponseRepresentation<Vec<String>> for PipeDelimited {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        PipeRecords::accept()
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<PipeRecords>(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<String>, ResponseBodyError> {
        decode_complete_records::<PipeRecords>(body, head)
    }
}

impl ResponseRepresentation<ItemStream<String>> for PipeDelimited {
    type Driver = Streaming;

    fn accept() -> Option<&'static str> {
        PipeRecords::accept()
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<String>, ResponseBodyError> {
        ItemStream::decode_with::<PipeRecords>(body, head)
    }
}

// ANCHOR: records_contract
api! {
    client RecordsApi {
        timeout 30s;
        base "http://records.example.test/";


    GET custom() -> ItemStream<String> via crate::request_response::records::PipeDelimited;
    GET buffered_custom() -> Vec<String> via crate::request_response::records::PipeDelimited;
    GET csv_rows() -> Vec<(String, u32)> via Csv;
    GET csv_stream() -> ItemStream<(String, u32)> via Csv;
    GET configured_csv_rows() -> Vec<(String, u32)> via crate::request_response::records::ConfCsv;
    GET configured_csv_stream() -> ItemStream<(String, u32)> via crate::request_response::records::ConfCsv;
    GET nd_json_values() -> Vec<u32> via NdJson;
    GET nd_json_stream() -> ItemStream<u32> via NdJson;
}}
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
