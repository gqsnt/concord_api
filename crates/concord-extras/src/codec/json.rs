use std::fmt;
use std::io::{self, Write};

use concord_core::body::BodyDescription;
use concord_core::contracts::{
    BodyError, BodyErrorKind, Buffered, Bytes, PageRequestRepresentation, PreparedBody,
    RequestBodyContext, RequestRepresentation, ResponseBodyError, ResponseBodyErrorKind,
    ResponseDriver, ResponseHead, ResponseRepresentation,
};

const INITIAL_JSON_CAPACITY: usize = 8 * 1024;
const JSON_MEDIA_TYPE: &str = "application/json";

/// Typed JSON request and response codec.
///
/// Request serialization writes directly into a buffer capped by
/// [`RequestBodyContext::body_limit`] and stops at the first encoded excess
/// (never serialize-then-truncate); the prepared body owns the one exact
/// `application/json` media type.
///
/// Responses negotiate `Accept: application/json` and require, on the
/// ordinary accepted branch, a syntactically valid `application/json` or
/// structured-suffix `application/*+json` `Content-Type` (media tokens are
/// case-insensitive; parameters must be syntactically valid). A missing,
/// malformed or incompatible media type is
/// [`ResponseBodyErrorKind::InvalidMediaType`] before any JSON conversion;
/// invalid JSON is a [`ResponseBodyErrorKind::Decode`] conversion error. Json
/// has no iterable-output requirement: a buffered envelope is pageable when a
/// typed pagination controller extracts its items.
///
/// Parser-, serializer-, and application-authored text is discarded when
/// errors cross the semantic codec boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Json;

impl<T> RequestRepresentation<T> for Json
where
    T: serde::Serialize + Send + 'static,
{
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some(JSON_MEDIA_TYPE));

    fn prepare(input: T, context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(encode_json(&input, context.body_limit.get())?)
            .content_type(JSON_MEDIA_TYPE)
    }
}

impl<T> PageRequestRepresentation<T> for Json where T: Clone + serde::Serialize + Send + 'static {}

impl<T> ResponseRepresentation<T> for Json
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some(JSON_MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        let content_type = head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok());
        if content_type.is_some_and(is_json_media_type) {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<T, ResponseBodyError> {
        decode_json(&body)
    }
}

fn encode_json<T: serde::Serialize>(input: &T, limit: usize) -> Result<Bytes, BodyError> {
    let mut writer = BoundedVecWriter::new(limit);
    match serde_json::to_writer(&mut writer, input) {
        Ok(()) => Ok(Bytes::from(writer.into_bytes())),
        Err(error) => {
            drop(error);
            let kind = if writer.limit_exceeded() {
                BodyErrorKind::LimitExceeded
            } else {
                BodyErrorKind::Producer
            };
            Err(BodyError::new(kind))
        }
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ResponseBodyError> {
    serde_json::from_slice(body).map_err(|error| {
        drop(error);
        ResponseBodyError::new(ResponseBodyErrorKind::Decode)
    })
}

struct BoundedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    limit_exceeded: bool,
}

impl BoundedVecWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(INITIAL_JSON_CAPACITY)),
            limit,
            limit_exceeded: false,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    const fn limit_exceeded(&self) -> bool {
        self.limit_exceeded
    }
}

impl Write for BoundedVecWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self
            .limit
            .checked_sub(self.bytes.len())
            .ok_or_else(json_limit_io_error)?;
        if bytes.len() > remaining {
            self.limit_exceeded = true;
            return Err(json_limit_io_error());
        }
        let next_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(json_limit_io_error)?;
        if next_len > self.bytes.capacity() {
            self.bytes
                .try_reserve_exact(bytes.len())
                .map_err(|_| json_writer_io_error())?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct JsonLimitExceeded;

impl fmt::Display for JsonLimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON body limit exceeded")
    }
}

impl std::error::Error for JsonLimitExceeded {}

fn json_limit_io_error() -> io::Error {
    io::Error::other(JsonLimitExceeded)
}

#[derive(Debug)]
struct JsonWriterFailed;

impl fmt::Display for JsonWriterFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON writer failed")
    }
}

impl std::error::Error for JsonWriterFailed {}

fn json_writer_io_error() -> io::Error {
    io::Error::other(JsonWriterFailed)
}

fn is_json_media_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|media_type| {
        media_type.type_() == mime::APPLICATION
            && (media_type.subtype() == mime::JSON || media_type.suffix() == Some(mime::JSON))
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error as _;

    use concord_core::contracts::ResponseBodyErrorKind;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    #[test]
    fn accepts_standard_and_structured_suffix_media_types() {
        for accepted in [
            "application/json",
            "APPLICATION/JSON; CHARSET=UTF-8",
            "application/problem+json",
            "application/vnd.example+json; version=2",
        ] {
            assert!(is_json_media_type(accepted), "{accepted}");
        }
        for rejected in [
            None,
            Some("text/json"),
            Some("application/json-seq"),
            Some("application/json; invalid"),
            Some("application/problem+json; broken=\"unterminated"),
        ] {
            assert!(!rejected.is_some_and(is_json_media_type));
        }
    }

    #[test]
    fn request_and_response_round_trip_without_exposing_serde_failures() {
        let encoded = encode_json(&vec![1, 2, 3], 1024).expect("encode");
        assert_eq!(
            encoded.as_ref(),
            serde_json::to_vec(&vec![1, 2, 3]).unwrap()
        );
        let decoded = decode_json::<Vec<u32>>(&encoded).expect("decode");
        assert_eq!(decoded, vec![1, 2, 3]);

        let error = decode_json::<Vec<u32>>(&Bytes::from_static(b"not-json-sentinel"))
            .expect_err("invalid JSON");
        assert_eq!(error.kind(), ResponseBodyErrorKind::Decode);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("not-json-sentinel"));
        }
    }

    #[test]
    fn request_encoding_accepts_exact_limit_and_rejects_first_excess() {
        let exact = encode_json(&"abc".to_owned(), 5).expect("encoded JSON exactly fits");
        assert_eq!(exact.as_ref(), br#""abc""#);

        let error =
            encode_json(&"abc".to_owned(), 4).expect_err("closing quote is the first excess byte");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(error.to_string(), "request body operation failed");
        assert!(error.source().is_none());

        let mut writer = BoundedVecWriter::new(4);
        let native = serde_json::to_writer(&mut writer, &"abc")
            .expect_err("serializer must stop on first excess");
        assert!(native.is_io());
        assert!(writer.limit_exceeded());
        assert_eq!(writer.bytes.len(), 4);
        assert!(writer.bytes.len() <= writer.limit);
        assert!(writer.bytes.capacity() <= writer.limit);
    }

    #[test]
    fn escaped_string_expansion_is_bounded_by_encoded_bytes() {
        const SENTINEL: &str = "escaped-input-sentinel-3d12";
        let input = format!("\0\0\0\0{SENTINEL}");
        assert!(input.len() < 64);
        let error = encode_json(&input, 16)
            .expect_err("escaped JSON expansion exceeds the encoded body limit");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    struct SerializerFailure;

    impl Serialize for SerializerFailure {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom(
                "serializer-application-sentinel-1f9b",
            ))
        }
    }

    #[test]
    fn serializer_failures_reduce_to_the_finite_encode_category() {
        let error = encode_json(&SerializerFailure, INITIAL_JSON_CAPACITY)
            .expect_err("application serializer failure");
        assert_eq!(error.kind(), BodyErrorKind::Producer);
        assert_eq!(error.to_string(), "request body operation failed");
        assert!(!format!("{error:?}").contains("serializer-application-sentinel-1f9b"));
        assert!(error.source().is_none());
    }

    struct SendNotSync(Cell<u8>);

    impl Serialize for SendNotSync {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_u8(self.0.get())
        }
    }

    impl<'de> Deserialize<'de> for SendNotSync {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            u8::deserialize(deserializer).map(|value| Self(Cell::new(value)))
        }
    }

    #[test]
    fn json_marker_is_non_owning_and_does_not_require_sync_values() {
        fn assert_encode<C: RequestRepresentation<SendNotSync>>() {}
        fn assert_decode<C: ResponseRepresentation<SendNotSync>>() {}
        fn assert_send_sync<T: Send + Sync>() {}

        assert_encode::<Json>();
        assert_decode::<Json>();
        assert_send_sync::<Json>();
        assert_eq!(std::mem::size_of::<Json>(), 0);
    }

    #[test]
    fn cloneable_json_input_is_page_repeatable() {
        fn assert_page_body<C, Input>()
        where
            C: PageRequestRepresentation<Input>,
            Input: Clone + Send + 'static,
        {
        }

        assert_page_body::<Json, Vec<u32>>();
    }
}
