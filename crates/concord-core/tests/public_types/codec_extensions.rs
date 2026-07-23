use concord_core::advanced::{
    Bytes, CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType,
};

struct VendorMedia;

impl MediaType for VendorMedia {
    const MEDIA_TYPE: &'static str = "application/vnd.example.record";
}

struct VendorCodec;

impl EncodeBody for VendorCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some(VendorMedia::MEDIA_TYPE);

    fn encode(input: Self::Input, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        assert_eq!(context.endpoint, "DownstreamApi::records::create");
        assert_eq!(context.method, http::Method::POST);
        assert_eq!(context.body_limit, Some(64));
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for VendorCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some(VendorMedia::MEDIA_TYPE);

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Some(VendorMedia::MEDIA_TYPE) {
            Ok(())
        } else {
            Err(CodecError::new("downstream-media-detail-secret"))
        }
    }

    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        assert_eq!(context.endpoint, "DownstreamApi::records::create");
        assert_eq!(context.method, http::Method::POST);
        assert_eq!(context.status, http::StatusCode::CREATED);
        assert_eq!(context.content_type, Some(VendorMedia::MEDIA_TYPE));
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new("downstream-decode-detail-secret")
        })
    }
}

#[test]
fn downstream_buffered_codecs_need_only_the_advanced_public_surface() {
    let method = http::Method::POST;
    let bytes = VendorCodec::encode(
        "extension-value".to_owned(),
        EncodeContext {
            endpoint: "DownstreamApi::records::create",
            method: &method,
            body_limit: Some(64),
        },
    )
    .expect("custom request encode");
    VendorCodec::validate_content_type(Some(VendorMedia::MEDIA_TYPE))
        .expect("custom response media");
    let value = VendorCodec::decode(
        bytes,
        DecodeContext {
            endpoint: "DownstreamApi::records::create",
            method: &method,
            status: http::StatusCode::CREATED,
            content_type: Some(VendorMedia::MEDIA_TYPE),
        },
    )
    .expect("custom response decode");
    assert_eq!(value, "extension-value");
}

#[test]
fn codec_error_formatting_and_source_chain_do_not_repeat_extension_text() {
    use std::error::Error as _;

    const SENTINEL: &str = "custom-codec-message-secret-95d7";
    let error = CodecError::new(SENTINEL);
    assert!(error.source().is_none());
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains(SENTINEL));
    }
}
