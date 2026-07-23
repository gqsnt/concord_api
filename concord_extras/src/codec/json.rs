use std::marker::PhantomData;

use concord_core::advanced::Bytes;
use concord_core::codec::{CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext};

/// Typed JSON request and response codec.
///
/// Parser-authored text and body values are discarded when errors cross the
/// semantic codec boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Json<T>(PhantomData<T>);

impl<T> EncodeBody for Json<T>
where
    T: serde::Serialize + Send + Sync + 'static,
{
    type Input = T;

    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        serde_json::to_vec(&input)
            .map(Bytes::from)
            .map_err(|error| {
                drop(error);
                CodecError::new("JSON request encoding failed")
            })
    }
}

impl<T> DecodeBody for Json<T>
where
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Output = T;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type.is_some_and(is_json_media_type) {
            Ok(())
        } else {
            Err(CodecError::new("response media type was not JSON"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        serde_json::from_slice(&body).map_err(|error| {
            drop(error);
            CodecError::new("JSON response decoding failed")
        })
    }
}

fn is_json_media_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|media_type| {
        media_type.type_() == mime::APPLICATION
            && (media_type.subtype() == mime::JSON || media_type.suffix() == Some(mime::JSON))
    })
}

#[cfg(test)]
mod tests {
    use concord_core::advanced::{Method, StatusCode};

    use super::*;

    fn encode_context() -> EncodeContext<'static> {
        EncodeContext {
            endpoint: "CodecApi::messages::create",
            method: &Method::POST,
            body_limit: Some(1024),
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
        let encoded = Json::<Vec<u32>>::encode(vec![1, 2, 3], encode_context()).expect("encode");
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
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("not-json-sentinel"));
        }
    }
}
