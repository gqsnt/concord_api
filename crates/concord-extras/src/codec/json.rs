use std::fmt;
use std::io::{self, Write};
use std::marker::PhantomData;

use concord_core::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord_core::contracts::{Bytes, PageRequestBodySpec};

const INITIAL_JSON_CAPACITY: usize = 8 * 1024;

/// Typed JSON request and response codec.
///
/// Request serialization writes directly into a capped buffer when
/// [`EncodeContext::body_limit`] is configured and stops at the first encoded
/// excess. Parser-, serializer-, and application-authored text is discarded
/// when errors cross the semantic codec boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Json<T>(PhantomData<fn() -> T>);

impl<T> EncodeBody for Json<T>
where
    T: serde::Serialize + Send + 'static,
{
    type Input = T;

    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(input: Self::Input, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        let mut writer = BoundedVecWriter::new(context.body_limit.unwrap_or(usize::MAX));
        match serde_json::to_writer(&mut writer, &input) {
            Ok(()) => Ok(Bytes::from(writer.into_bytes())),
            Err(error) => {
                drop(error);
                let kind = if writer.limit_exceeded() {
                    CodecErrorKind::Limit
                } else {
                    CodecErrorKind::Encode
                };
                Err(CodecError::new(kind))
            }
        }
    }
}

impl<T> PageRequestBodySpec for Json<T> where T: Clone + serde::Serialize + Send + 'static {}

impl<T> DecodeBody for Json<T>
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    type Output = T;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type.is_some_and(is_json_media_type) {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        serde_json::from_slice(&body).map_err(|error| {
            drop(error);
            CodecError::new(CodecErrorKind::Decode)
        })
    }
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

    use concord_core::contracts::{Method, StatusCode};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    fn encode_context(limit: usize) -> EncodeContext<'static> {
        EncodeContext {
            endpoint: "CodecApi::messages::create",
            method: &Method::POST,
            body_limit: Some(limit),
        }
    }

    fn decode_context(content_type: Option<&str>) -> DecodeContext<'_> {
        DecodeContext {
            endpoint: "CodecApi::messages::create",
            method: &Method::POST,
            status: StatusCode::OK,
            content_type,
        }
    }

    #[test]
    fn accepts_standard_and_structured_suffix_media_types() {
        for accepted in [
            "application/json",
            "APPLICATION/JSON; CHARSET=UTF-8",
            "application/problem+json",
            "application/vnd.example+json; version=2",
        ] {
            Json::<String>::validate_content_type(Some(accepted)).expect(accepted);
        }
        for rejected in [
            None,
            Some("text/json"),
            Some("application/json-seq"),
            Some("application/json; invalid"),
            Some("application/problem+json; broken=\"unterminated"),
        ] {
            assert!(Json::<String>::validate_content_type(rejected).is_err());
        }
    }

    #[test]
    fn request_and_response_round_trip_without_exposing_serde_failures() {
        let encoded =
            Json::<Vec<u32>>::encode(vec![1, 2, 3], encode_context(1024)).expect("encode");
        assert_eq!(
            encoded.as_ref(),
            serde_json::to_vec(&vec![1, 2, 3]).unwrap()
        );
        let decoded = Json::<Vec<u32>>::decode(
            encoded,
            decode_context(Some("application/json; charset=utf-8")),
        )
        .expect("decode");
        assert_eq!(decoded, vec![1, 2, 3]);

        let error = Json::<Vec<u32>>::decode(
            Bytes::from_static(b"not-json-sentinel"),
            decode_context(Some("application/json")),
        )
        .expect_err("invalid JSON");
        assert_eq!(error.kind(), CodecErrorKind::Decode);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("not-json-sentinel"));
        }
    }

    #[test]
    fn request_encoding_accepts_exact_limit_and_rejects_first_excess() {
        let exact = Json::<String>::encode("abc".to_owned(), encode_context(5))
            .expect("encoded JSON exactly fits");
        assert_eq!(exact.as_ref(), br#""abc""#);

        let error = Json::<String>::encode("abc".to_owned(), encode_context(4))
            .expect_err("closing quote is the first excess byte");
        assert_eq!(error.kind(), CodecErrorKind::Limit);
        assert_eq!(error.to_string(), "codec operation failed (Limit)");
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
        let error = Json::<String>::encode(input, encode_context(16))
            .expect_err("escaped JSON expansion exceeds the encoded body limit");
        assert_eq!(error.kind(), CodecErrorKind::Limit);
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
        let error = Json::<SerializerFailure>::encode(
            SerializerFailure,
            encode_context(INITIAL_JSON_CAPACITY),
        )
        .expect_err("application serializer failure");
        assert_eq!(error.kind(), CodecErrorKind::Encode);
        assert_eq!(error.to_string(), "codec operation failed (Encode)");
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
        fn assert_encode<C: EncodeBody<Input = SendNotSync>>() {}
        fn assert_decode<C: DecodeBody<Output = SendNotSync>>() {}
        fn assert_send_sync<T: Send + Sync>() {}

        assert_encode::<Json<SendNotSync>>();
        assert_decode::<Json<SendNotSync>>();
        assert_send_sync::<Json<SendNotSync>>();
        assert_eq!(std::mem::size_of::<Json<SendNotSync>>(), 0);
    }

    #[test]
    fn cloneable_json_input_is_page_repeatable() {
        fn assert_page_body<C>()
        where
            C: PageRequestBodySpec,
            C::Input: Clone,
        {
        }

        assert_page_body::<Json<Vec<u32>>>();
    }
}
