use concord_core::{CodecError, Decode, Encode, ResponseHead, bytes::Bytes, reqwest};
use std::net::IpAddr;

pub struct AddressEncoder;
pub struct AddressDecoder;

impl Encode<IpAddr> for AddressEncoder {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-ip-address");
    fn encode(value: IpAddr) -> Result<reqwest::Body, CodecError> {
        Ok(value.to_string().into())
    }
}
impl Decode<IpAddr> for AddressDecoder {
    type Context = u64;
    fn context(head: &ResponseHead<'_>) -> Result<u64, CodecError> {
        head.headers()
            .get("x-record")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| CodecError::new("missing record"))
    }
    fn decode_body(record: u64, body: Bytes) -> Result<IpAddr, CodecError> {
        std::str::from_utf8(&body)
            .ok()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                CodecError::new("invalid address")
                    .at("record", record)
                    .at("byte", 0)
            })
    }
}

#[test]
fn foreign_payload_encoder() {
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    assert_eq!(
        AddressEncoder::CONTENT_TYPE,
        Some("application/x-ip-address")
    );
    assert_eq!(
        AddressEncoder::encode(ip).unwrap().as_bytes(),
        Some(&b"127.0.0.1"[..])
    );
}
#[test]
fn decoder_and_head_context() {
    use reqwest::{
        StatusCode, Version,
        header::{HeaderMap, HeaderValue},
    };
    let _decoder = concord_core::Decoder::<IpAddr>::new::<AddressDecoder>();
    let mut headers = HeaderMap::new();
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
    assert_eq!(
        AddressDecoder::context(&head).unwrap_err().label(),
        "missing record"
    );
    headers.insert("x-record", HeaderValue::from_static("7"));
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
    AddressDecoder::check_content_type(&head).unwrap();
    let context = AddressDecoder::context(&head).unwrap();
    assert_eq!(
        AddressDecoder::decode_body(context, Bytes::from_static(b"::1")).unwrap(),
        "::1".parse::<IpAddr>().unwrap()
    );
    let err = AddressDecoder::decode_body(context, Bytes::from_static(b"bad")).unwrap_err();
    assert_eq!(err.context(), &[("record", 7), ("byte", 0)]);
}
#[test]
fn codec_error_positions_are_bounded() {
    let error = CodecError::new("bad record")
        .at("record", 2)
        .at("line", 3)
        .at("column", 4)
        .at("byte", 5);
    assert_eq!(
        error.context(),
        &[("record", 2), ("line", 3), ("column", 4)]
    );
    assert_eq!(error.get("column"), Some(4));
    assert_eq!(error.get("byte"), None);
    assert_eq!(error.to_string(), "bad record (record 2, line 3, column 4)");
}
