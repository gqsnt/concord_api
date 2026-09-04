use std::marker::PhantomData;
use std::num::NonZeroUsize;

use concord_core::contracts::{
    Buffered, Bytes, BytesMut, ItemStream, PageResponseRepresentation, ResponseBodyError,
    ResponseDriver, ResponseHead, ResponseRepresentation, Streaming,
};
use concord_core::records::{
    RecordDecoder, RecordError, RecordErrorKind, decode_complete_records, validate_record_decoder,
};
use serde::de::DeserializeOwned;

const MAX_STAGED_RECORD_BYTES: usize = 1024 * 1024;

/// Typed newline-delimited JSON responses and incremental record decoding.
///
/// One typed JSON value is decoded from each non-empty UTF-8 LF or CRLF line;
/// whitespace-only lines are ignored and the final line may omit its newline.
/// `NdJson` with `Vec<T>` completely acquires the bounded body; `NdJson` with
/// [`ItemStream<T>`] returns a lazy semantic stream through the same decoder
/// and sealed core driver.
/// Undecoded staging is bounded to 1 MiB. Parser text, raw lines, and JSON values
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

    fn accept() -> Option<&'static str> {
        Some("application/x-ndjson")
    }

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

impl<T> ResponseRepresentation<Vec<T>> for NdJson
where
    T: DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        NdJsonDecoder::<T>::accept()
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<NdJsonDecoder<T>>(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<T>, ResponseBodyError> {
        decode_complete_records::<NdJsonDecoder<T>>(body, head)
    }
}

impl<T> PageResponseRepresentation<Vec<T>> for NdJson where T: DeserializeOwned + Send + 'static {}

impl<T> ResponseRepresentation<ItemStream<T>> for NdJson
where
    T: DeserializeOwned + Send + 'static,
{
    type Driver = Streaming;

    fn accept() -> Option<&'static str> {
        NdJsonDecoder::<T>::accept()
    }

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
    fn ndjson_accepts_both_registered_media_names_with_parameters() {
        for media in ["application/x-ndjson", "application/ndjson; charset=utf-8"] {
            assert!(is_ndjson_media_type(media));
        }
        assert!(!is_ndjson_media_type("application/json"));
        assert!(!is_ndjson_media_type(
            "application/ndjson; broken=\"unterminated"
        ));
        assert_eq!(
            <NdJsonDecoder<Value> as RecordDecoder>::accept(),
            Some("application/x-ndjson")
        );
        assert_eq!(
            NdJsonDecoder::<Value>::max_buffered_bytes().get(),
            1024 * 1024
        );

        fn complete_page_spec<S>()
        where
            S: ResponseRepresentation<Vec<Value>, Driver = Buffered>
                + PageResponseRepresentation<Vec<Value>>,
        {
        }
        complete_page_spec::<NdJson>();
        assert_eq!(
            <NdJson as ResponseRepresentation<Vec<Value>>>::accept(),
            Some("application/x-ndjson")
        );
    }
}
