use std::marker::PhantomData;
use std::num::NonZeroUsize;

use std::io::{self, Write};

use concord_core::body::BodyDescription;
use concord_core::contracts::{
    BodyError, BodyErrorKind, Buffered, Bytes, BytesMut, ItemStream, PageRequestRepresentation,
    PreparedBody, RequestBodyContext, RequestRepresentation, ResponseBodyError, ResponseDriver,
    ResponseHead, ResponseRepresentation, Streaming,
};
use concord_core::records::{
    RecordDecoder, RecordError, RecordErrorKind, decode_complete_records, validate_record_decoder,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

const MAX_STAGED_RECORD_BYTES: usize = 1024 * 1024;
const NDJSON_MEDIA_TYPE: &str = "application/x-ndjson";

/// Typed newline-delimited JSON request and response records.
///
/// **Requests.** `NdJson` with `Vec<T>` (`T: Serialize`) frames each record as
/// one compact JSON value followed by exactly one LF, in order; compact JSON
/// never contains a raw line break, so every record is exactly one frame. The
/// encoder writes directly into a buffer capped by
/// [`RequestBodyContext::body_limit`] and stops at the first encoded excess
/// byte (never serialize-then-truncate); a single record larger than the 1 MiB
/// record frame bound is rejected as a limit failure. The prepared body owns
/// the one exact `application/x-ndjson` media type. An empty vector is an
/// empty body with that media type.
///
/// **Responses.**
/// One typed JSON value is decoded from each non-empty UTF-8 LF or CRLF line;
/// whitespace-only lines are ignored and a final nonempty line may omit its
/// newline, in which case it is parsed exactly once at end of body. A
/// malformed line, or a final line that ends inside a JSON value, is a
/// terminal failure; it is never silently dropped.
/// `NdJson` with `Vec<T>` completely acquires the bounded body; `NdJson` with
/// [`ItemStream<T>`] returns a managed lazy semantic stream through the same
/// decoder and sealed core driver. The stream yields exactly one correlated
/// `ApiError` on a decoding failure and then ends, and it inherits the core
/// stream lease, idle, byte and record limits.
/// Undecoded staging per line is bounded to 1 MiB (and to the runtime stream
/// frame limit when that is smaller). Parser text, raw lines, and JSON values
/// are absent from errors. This type is available through the independent
/// `ndjson` feature and does not require the general endpoint `json` feature.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NdJson;

struct NdJsonDecoder<T> {
    scan_index: usize,
    marker: PhantomData<fn() -> T>,
}

impl<T> NdJsonDecoder<T>
where
    T: DeserializeOwned + Send + 'static,
{
    fn take_line(buffer: &mut BytesMut, scan_index: &mut usize) -> Option<BytesMut> {
        let relative = buffer
            .get(*scan_index..)?
            .iter()
            .position(|byte| *byte == b'\n')?;
        let newline = *scan_index + relative;
        let mut line = buffer.split_to(newline + 1);
        line.truncate(newline);
        if line.last() == Some(&b'\r') {
            line.truncate(line.len() - 1);
        }
        *scan_index = 0;
        Some(line)
    }

    fn decode_line(bytes: &[u8], eof: bool) -> Result<Option<T>, RecordError> {
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        serde_json::from_slice(bytes).map(Some).map_err(|error| {
            let kind = if eof && error.is_eof() {
                RecordErrorKind::IncompleteRecord
            } else {
                RecordErrorKind::Decode
            };
            drop(error);
            RecordError::new(kind)
        })
    }

    fn decode_available(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<T>, RecordError> {
        loop {
            if let Some(line) = Self::take_line(buffer, &mut self.scan_index) {
                if let Some(record) = Self::decode_line(&line, false)? {
                    return Ok(Some(record));
                }
                continue;
            }
            self.scan_index = buffer.len();
            if !eof || buffer.is_empty() {
                return Ok(None);
            }
            let line = buffer.split_to(buffer.len());
            self.scan_index = 0;
            return Self::decode_line(&line, true);
        }
    }
}

impl<T> RecordDecoder for NdJsonDecoder<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Record = T;

    const ACCEPT: Option<&'static str> = Some(NDJSON_MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        let valid = head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(is_ndjson_media_type);
        if valid {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self {
            scan_index: 0,
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
            .expect("the built-in NDJSON staging bound is positive")
    }
}

impl<T> RequestRepresentation<Vec<T>> for NdJson
where
    T: Serialize + Send + 'static,
{
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some(NDJSON_MEDIA_TYPE));

    fn prepare(input: Vec<T>, context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(encode_records(&input, context.body_limit.get())?)
            .content_type(NDJSON_MEDIA_TYPE)
    }
}

impl<T> PageRequestRepresentation<Vec<T>> for NdJson where T: Clone + Serialize + Send + 'static {}

/// Frames `records` as NDJSON under the aggregate `limit` and the per-record
/// frame bound. Serializer text is discarded at the codec boundary.
fn encode_records<T: Serialize>(records: &[T], limit: usize) -> Result<Bytes, BodyError> {
    let mut writer = BoundedRecordWriter::new(limit);
    for record in records {
        writer.start_record();
        let written = serde_json::to_writer(&mut writer, record)
            .and_then(|()| writer.write_all(b"\n").map_err(serde_json::Error::io));
        if let Err(error) = written {
            drop(error);
            let kind = if writer.limit_exceeded {
                BodyErrorKind::LimitExceeded
            } else {
                BodyErrorKind::Producer
            };
            return Err(BodyError::new(kind));
        }
    }
    Ok(Bytes::from(writer.bytes))
}

/// Byte sink capped by the aggregate request-body limit and by the record
/// frame bound (the record's JSON value plus its LF).
struct BoundedRecordWriter {
    bytes: Vec<u8>,
    limit: usize,
    record_start: usize,
    limit_exceeded: bool,
}

impl BoundedRecordWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8 * 1024)),
            limit,
            record_start: 0,
            limit_exceeded: false,
        }
    }

    fn start_record(&mut self) {
        self.record_start = self.bytes.len();
    }
}

impl Write for BoundedRecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let total = self.bytes.len().checked_add(bytes.len());
        let frame = total.map(|total| total - self.record_start);
        match (total, frame) {
            (Some(total), Some(frame))
                if total <= self.limit && frame <= MAX_STAGED_RECORD_BYTES + 1 =>
            {
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            _ => {
                self.limit_exceeded = true;
                Err(io::Error::other("ndjson request record limit exceeded"))
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<T> ResponseRepresentation<Vec<T>> for NdJson
where
    T: DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = NdJsonDecoder::<T>::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<NdJsonDecoder<T>>(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<T>, ResponseBodyError> {
        decode_complete_records::<NdJsonDecoder<T>>(body, head)
    }
}

impl<T> ResponseRepresentation<ItemStream<T>> for NdJson
where
    T: DeserializeOwned + Send + 'static,
{
    type Driver = Streaming;

    const ACCEPT: Option<&'static str> = NdJsonDecoder::<T>::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        NdJsonDecoder::<T>::validate(head).map_err(|_| {
            ResponseBodyError::new(concord_core::contracts::ResponseBodyErrorKind::InvalidMediaType)
        })
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<T>, ResponseBodyError> {
        ItemStream::decode_with::<NdJsonDecoder<T>>(body, head)
    }
}

fn is_ndjson_media_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|media| {
        media.type_() == mime::APPLICATION
            && (media.subtype().as_str() == "x-ndjson" || media.subtype().as_str() == "ndjson")
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Value {
        id: u32,
        text: String,
    }

    fn decoder() -> NdJsonDecoder<Value> {
        NdJsonDecoder {
            scan_index: 0,
            marker: PhantomData,
        }
    }

    fn feed(chunks: impl IntoIterator<Item = Vec<u8>>) -> Result<Vec<Value>, RecordError> {
        let mut decoder = decoder();
        let mut buffer = BytesMut::new();
        let mut values = Vec::new();
        for chunk in chunks {
            buffer.extend_from_slice(&chunk);
            while let Some(value) = decoder.decode(&mut buffer)? {
                values.push(value);
            }
        }
        while let Some(value) = decoder.decode_eof(&mut buffer)? {
            values.push(value);
        }
        Ok(values)
    }

    #[test]
    fn ndjson_frames_lf_crlf_whitespace_multiple_values_and_final_line_across_one_byte_chunks() {
        let input = b"  \r\n{\"id\":1,\"text\":\"first\"}\n{\"id\":2,\"text\":\"second\"}\r\n\t\n{\"id\":3,\"text\":\"final\"}";
        let values = feed(input.iter().map(|byte| vec![*byte])).expect("one-byte NDJSON splits");
        assert_eq!(
            values,
            [
                Value {
                    id: 1,
                    text: "first".into(),
                },
                Value {
                    id: 2,
                    text: "second".into(),
                },
                Value {
                    id: 3,
                    text: "final".into(),
                },
            ]
        );
    }

    #[test]
    fn ndjson_rejects_malformed_and_incomplete_values_without_parser_text_or_sources() {
        let malformed = feed([b"{\"id\":broken,\"text\":\"sentinel-json-value\"}\n".to_vec()])
            .expect_err("malformed NDJSON");
        assert_eq!(malformed.kind(), RecordErrorKind::Decode);
        let incomplete = feed([b"{\"id\":4,\"text\":\"sentinel-incomplete\"".to_vec()])
            .expect_err("incomplete final NDJSON");
        assert_eq!(incomplete.kind(), RecordErrorKind::IncompleteRecord);
        for error in [malformed, incomplete] {
            let rendered = format!("{error} {error:?}");
            for forbidden in ["broken", "sentinel-json-value", "sentinel-incomplete"] {
                assert!(!rendered.contains(forbidden));
            }
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn ndjson_final_line_without_newline_is_decoded_exactly_once() {
        let mut decoder = decoder();
        let mut buffer = BytesMut::from(&b"\n{\"id\":9,\"text\":\"tail\"}"[..]);
        assert!(
            decoder
                .decode(&mut buffer)
                .expect("no complete line")
                .is_none()
        );
        let tail = decoder
            .decode_eof(&mut buffer)
            .expect("final line decodes")
            .expect("final record");
        assert_eq!(
            tail,
            Value {
                id: 9,
                text: "tail".into(),
            }
        );
        assert!(buffer.is_empty());
        assert!(decoder.decode_eof(&mut buffer).expect("EOF").is_none());
        assert!(decoder.decode_eof(&mut buffer).expect("EOF").is_none());
    }

    #[test]
    fn ndjson_accepts_both_registered_media_names_with_parameters() {
        for media in ["application/x-ndjson", "application/ndjson; charset=utf-8"] {
            assert!(is_ndjson_media_type(media));
        }
        assert!(!is_ndjson_media_type("application/json"));
        assert!(!is_ndjson_media_type(
            "application/ndjson; broken=\"unterminated"
        ));
        assert_eq!(
            <NdJsonDecoder<Value> as RecordDecoder>::ACCEPT,
            Some("application/x-ndjson")
        );
        assert_eq!(
            NdJsonDecoder::<Value>::max_buffered_bytes().get(),
            1024 * 1024
        );

        fn complete_page_spec<S>()
        where
            S: ResponseRepresentation<Vec<Value>, Driver = Buffered>,
        {
        }
        complete_page_spec::<NdJson>();
        assert_eq!(
            <NdJson as ResponseRepresentation<Vec<Value>>>::ACCEPT,
            Some("application/x-ndjson")
        );
    }

    #[derive(Clone, Debug, Serialize)]
    struct Outgoing {
        id: u32,
        text: String,
    }

    struct SerializerFailure;

    impl Serialize for SerializerFailure {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("serializer-sentinel-91f"))
        }
    }

    #[test]
    fn request_records_are_framed_one_compact_value_per_lf_and_decode_back() {
        let records = vec![
            Outgoing {
                id: 1,
                text: "line\nbreak inside a string".into(),
            },
            Outgoing {
                id: 2,
                text: "second".into(),
            },
        ];
        let encoded = encode_records(&records, 1024).expect("encode");
        assert_eq!(
            encoded.as_ref(),
            b"{\"id\":1,\"text\":\"line\\nbreak inside a string\"}\n{\"id\":2,\"text\":\"second\"}\n"
        );
        assert_eq!(encoded.iter().filter(|byte| **byte == b'\n').count(), 2);
        let decoded = feed([encoded.to_vec()]).expect("the response decoder reads the frames");
        assert_eq!(
            decoded,
            [
                Value {
                    id: 1,
                    text: "line\nbreak inside a string".into(),
                },
                Value {
                    id: 2,
                    text: "second".into(),
                },
            ]
        );
        assert!(
            encode_records::<Outgoing>(&[], 1)
                .expect("no records")
                .is_empty()
        );
    }

    #[test]
    fn request_records_accept_the_exact_limit_and_reject_the_first_excess_byte() {
        let records = vec![1_u32, 22];
        // "1\n22\n" is exactly 5 bytes.
        let exact = encode_records(&records, 5).expect("exact limit");
        assert_eq!(exact.as_ref(), b"1\n22\n");
        let error = encode_records(&records, 4).expect_err("the final LF is the first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert!(error.source().is_none());
    }

    #[test]
    fn one_request_record_is_bounded_by_the_record_frame_limit() {
        let fits = "x".repeat(MAX_STAGED_RECORD_BYTES - 2);
        // Quotes make the value exactly MAX bytes; the LF completes the frame.
        let encoded = encode_records(&[fits], usize::MAX).expect("frame at the bound");
        assert_eq!(encoded.len(), MAX_STAGED_RECORD_BYTES + 1);
        let oversized = "x".repeat(MAX_STAGED_RECORD_BYTES - 1);
        let error =
            encode_records(&[oversized], usize::MAX).expect_err("one record over the frame bound");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    }

    #[test]
    fn request_serializer_failures_reduce_to_the_producer_category_without_text() {
        let error = encode_records(&[SerializerFailure], 1024).expect_err("serializer failure");
        assert_eq!(error.kind(), BodyErrorKind::Producer);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("serializer-sentinel-91f"));
        }
    }

    #[test]
    fn request_records_prepare_one_body_with_the_exact_ndjson_media_type() {
        fn page_stable<S: PageRequestRepresentation<Vec<Outgoing>>>() {}
        page_stable::<NdJson>();
        assert_eq!(
            <NdJson as RequestRepresentation<Vec<Outgoing>>>::DESCRIPTION,
            BodyDescription::buffered(Some("application/x-ndjson"))
        );
        let endpoint =
            concord_core::contracts::EndpointId::new("records.upload").expect("endpoint ID");
        let method = concord_core::contracts::Method::POST;
        let context = RequestBodyContext::new(
            &endpoint,
            &method,
            std::num::NonZeroUsize::new(3).expect("positive"),
            None,
        );
        let error = <NdJson as RequestRepresentation<Vec<u32>>>::prepare(vec![1, 2], context)
            .expect_err("aggregate body limit applies to request records");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    }
}
