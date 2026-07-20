use std::marker::PhantomData;
use std::num::NonZeroUsize;

use bytes::BytesMut;
use http::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;

use crate::response::ResponseHead;

use super::{RecordDecoder, RecordError, RecordErrorKind};

const MAX_STAGED_RECORD_BYTES: usize = 1024 * 1024;

/// Typed incremental CSV decoding with one required header row.
///
/// The fixed dialect is UTF-8, comma-delimited, double-quoted with doubled quote
/// escaping, and accepts LF or CRLF including quoted embedded newlines. Header
/// names drive typed deserialization; empty physical lines are ignored. The
/// final record may omit its newline. Undecoded staging is bounded to 1 MiB.
/// Parser text, headers, fields, and record values are absent from errors.
pub struct Csv<T> {
    headers: Option<csv::StringRecord>,
    scan: CsvScan,
    marker: PhantomData<fn() -> T>,
}

#[derive(Default)]
struct CsvScan {
    index: usize,
    in_quotes: bool,
    field_start: bool,
    after_quote: bool,
}

impl CsvScan {
    const fn new() -> Self {
        Self {
            index: 0,
            in_quotes: false,
            field_start: true,
            after_quote: false,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

impl<T> Csv<T>
where
    T: DeserializeOwned + Send + 'static,
{
    fn take_record(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<BytesMut>, RecordError> {
        while self.scan.index < buffer.len() {
            let index = self.scan.index;
            let byte = buffer[index];
            if self.scan.in_quotes {
                if byte != b'"' {
                    self.scan.index += 1;
                    continue;
                }
                if let Some(next) = buffer.get(index + 1) {
                    if *next == b'"' {
                        self.scan.index += 2;
                    } else {
                        self.scan.in_quotes = false;
                        self.scan.after_quote = true;
                        self.scan.index += 1;
                    }
                    continue;
                }
                if eof {
                    self.scan.in_quotes = false;
                    self.scan.after_quote = true;
                    self.scan.index += 1;
                    continue;
                }
                return Ok(None);
            }

            match byte {
                b'"' if self.scan.field_start => {
                    self.scan.in_quotes = true;
                    self.scan.field_start = false;
                    self.scan.index += 1;
                }
                b'"' => {
                    return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                }
                b',' => {
                    self.scan.field_start = true;
                    self.scan.after_quote = false;
                    self.scan.index += 1;
                }
                b'\n' => {
                    let mut record = buffer.split_to(index + 1);
                    record.truncate(index);
                    self.scan.reset();
                    if record.is_empty() {
                        continue;
                    }
                    return Ok(Some(record));
                }
                b'\r' => {
                    let Some(next) = buffer.get(index + 1) else {
                        if eof {
                            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                        }
                        return Ok(None);
                    };
                    if *next != b'\n' {
                        return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                    }
                    let mut record = buffer.split_to(index + 2);
                    record.truncate(index);
                    self.scan.reset();
                    if record.is_empty() {
                        continue;
                    }
                    return Ok(Some(record));
                }
                _ if self.scan.after_quote => {
                    return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                }
                _ => {
                    self.scan.field_start = false;
                    self.scan.index += 1;
                }
            }
        }

        if !eof {
            return Ok(None);
        }
        if self.scan.in_quotes {
            return Err(RecordError::new(RecordErrorKind::IncompleteRecord));
        }
        if buffer.is_empty() {
            return Ok(None);
        }
        let record = buffer.split_to(buffer.len());
        self.scan.reset();
        Ok(Some(record))
    }

    fn parse_record(bytes: &[u8]) -> Result<csv::StringRecord, RecordError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(false)
            .from_reader(bytes);
        let mut record = csv::StringRecord::new();
        let present = reader.read_record(&mut record).map_err(|error| {
            drop(error);
            RecordError::new(RecordErrorKind::InvalidFraming)
        })?;
        if !present {
            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
        }
        let mut extra = csv::StringRecord::new();
        if reader.read_record(&mut extra).map_err(|error| {
            drop(error);
            RecordError::new(RecordErrorKind::InvalidFraming)
        })? {
            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
        }
        Ok(record)
    }

    fn consume_record(&mut self, bytes: &[u8]) -> Result<Option<T>, RecordError> {
        let record = Self::parse_record(bytes)?;
        let Some(headers) = self.headers.as_ref() else {
            if record.is_empty() || record.iter().any(str::is_empty) {
                return Err(RecordError::new(RecordErrorKind::InvalidFraming));
            }
            let mut names = std::collections::HashSet::new();
            if !record.iter().all(|name| names.insert(name)) {
                return Err(RecordError::new(RecordErrorKind::InvalidFraming));
            }
            self.headers = Some(record);
            return Ok(None);
        };
        record
            .deserialize(Some(headers))
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            })
    }

    fn decode_available(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<T>, RecordError> {
        while let Some(record) = self.take_record(buffer, eof)? {
            if let Some(record) = self.consume_record(&record)? {
                return Ok(Some(record));
            }
        }
        if eof && self.headers.is_none() {
            return Err(RecordError::new(RecordErrorKind::IncompleteRecord));
        }
        Ok(None)
    }
}

impl<T> RecordDecoder for Csv<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Record = T;

    fn accept() -> Option<&'static str> {
        Some("text/csv")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        let valid = head
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<mime::Mime>().ok())
            .is_some_and(|media| media.type_() == mime::TEXT && media.subtype() == mime::CSV);
        if valid {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self {
            headers: None,
            scan: CsvScan::new(),
            marker: PhantomData,
        })
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, false)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, true)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(MAX_STAGED_RECORD_BYTES)
            .expect("the built-in CSV staging bound is positive")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::error::Error as _;
    use std::future::poll_fn;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use bytes::{Bytes, BytesMut};
    use futures_core::Stream;
    use http::{HeaderMap, HeaderValue, Method, StatusCode, Version};
    use serde::Deserialize;
    use url::Url;

    use super::*;
    use crate::error::ApiError;
    use crate::http::application::{ApplicationByteStream, bounded_byte_stream};
    use crate::records::RecordStream;
    use crate::response::{ByteStream, ExecutionMeta};

    struct Chunks(VecDeque<Bytes>);

    impl Stream for Chunks {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front().map(Ok))
        }
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Row {
        name: String,
        note: String,
        count: u32,
    }

    fn with_head<T>(content_type: &'static str, inspect: impl FnOnce(&ResponseHead<'_>) -> T) -> T {
        let method = Method::GET;
        let url = Url::parse("https://records.example.test/csv").expect("record URL");
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        let metadata = ExecutionMeta::new("RecordsApi::csv", method.clone(), 0, false);
        inspect(&ResponseHead {
            endpoint: "RecordsApi::csv",
            method: &method,
            url: &url,
            status: StatusCode::OK,
            version: Version::HTTP_11,
            headers: &headers,
            content_length: None,
            metadata: &metadata,
        })
    }

    fn decoder() -> Csv<Row> {
        with_head("text/csv; charset=utf-8", |head| {
            Csv::<Row>::validate(head).expect("CSV media");
            Csv::<Row>::start(head).expect("CSV decoder")
        })
    }

    fn feed(chunks: impl IntoIterator<Item = Vec<u8>>) -> Result<Vec<Row>, RecordError> {
        let mut decoder = decoder();
        let mut buffer = BytesMut::new();
        let mut rows = Vec::new();
        for chunk in chunks {
            buffer.extend_from_slice(&chunk);
            while let Some(row) = decoder.decode(&mut buffer)? {
                rows.push(row);
            }
        }
        while let Some(row) = decoder.decode_eof(&mut buffer)? {
            rows.push(row);
        }
        Ok(rows)
    }

    #[test]
    fn csv_frames_headers_quotes_line_endings_empty_lines_and_final_record_across_one_byte_chunks()
    {
        let input = b"\r\nname,note,count\r\nalpha,\"comma, value\",1\nbeta,\"line one\nline \"\"two\"\"\",2\r\n\ngamma,plain,3";
        let rows = feed(input.iter().map(|byte| vec![*byte])).expect("one-byte CSV splits");
        assert_eq!(
            rows,
            [
                Row {
                    name: "alpha".into(),
                    note: "comma, value".into(),
                    count: 1,
                },
                Row {
                    name: "beta".into(),
                    note: "line one\nline \"two\"".into(),
                    count: 2,
                },
                Row {
                    name: "gamma".into(),
                    note: "plain".into(),
                    count: 3,
                },
            ]
        );
    }

    #[test]
    fn csv_decodes_several_rows_from_one_chunk_with_optional_final_newline() {
        let rows = feed([b"name,note,count\na,first,1\nb,second,2\n".to_vec()])
            .expect("multi-row CSV chunk");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].name, "b");
    }

    #[test]
    fn csv_rejects_invalid_duplicate_headers_malformed_quotes_and_typed_values_safely() {
        let cases = [
            (
                b"name,,count\na,value,1\n".as_slice(),
                RecordErrorKind::InvalidFraming,
            ),
            (
                b"name,name,count\na,value,1\n".as_slice(),
                RecordErrorKind::InvalidFraming,
            ),
            (
                b"name,note,count\na,\"unterminated,1\n".as_slice(),
                RecordErrorKind::IncompleteRecord,
            ),
            (
                b"name,note,count\na,value,not-a-number\n".as_slice(),
                RecordErrorKind::Decode,
            ),
        ];
        for (input, expected) in cases {
            let error = feed([input.to_vec()]).expect_err("invalid CSV must fail");
            assert_eq!(error.kind(), expected);
            let rendered = format!("{error} {error:?}");
            for forbidden in ["unterminated", "not-a-number", "value"] {
                assert!(!rendered.contains(forbidden));
            }
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn csv_media_requires_text_csv_but_accepts_parameters() {
        with_head("application/json", |head| {
            let error = Csv::<Row>::validate(head).expect_err("wrong CSV media");
            assert_eq!(error.kind(), RecordErrorKind::InvalidMediaType);
        });
        assert_eq!(Csv::<Row>::accept(), Some("text/csv"));
        assert_eq!(Csv::<Row>::max_buffered_bytes().get(), 1024 * 1024);
    }

    #[tokio::test]
    async fn csv_first_framing_byte_above_staging_bound_is_withheld() {
        let chunks = Chunks(
            [
                Bytes::from_static(b"name,note,count\n"),
                Bytes::from(vec![b'x'; MAX_STAGED_RECORD_BYTES]),
                Bytes::from_static(b"\n"),
            ]
            .into_iter()
            .collect(),
        );
        let raw: ApplicationByteStream = Box::pin(chunks);
        let body = ByteStream::new(bounded_byte_stream(
            raw,
            "RecordsApi::csv_staging",
            Method::GET,
            u64::try_from(4 * MAX_STAGED_RECORD_BYTES).expect("test stream limit fits u64"),
        ));
        let mut records = RecordStream::new(
            body,
            decoder(),
            NonZeroUsize::new(MAX_STAGED_RECORD_BYTES).expect("positive CSV staging bound"),
        );
        let error = poll_fn(|context| Pin::new(&mut records).poll_next(context))
            .await
            .expect("CSV staging error item")
            .expect_err("oversized framed CSV record");
        assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
        assert!(!format!("{error} {error:?}").contains(&"x".repeat(64)));
    }
}
