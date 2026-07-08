use super::helpers::*;
use quote::quote;

#[test]
fn generated_invalid_codec_headers_return_typed_errors() {
    let out = expanded(quote! {
        client CodecErrorApi {
            base "https://example.com"
        }

        POST Upload(body: Json<UploadBody>)
            path ["upload"]
            -> Json<UploadResponse>
    });

    assert_contains_all(
        &out,
        &[
            "RequestEntity",
            "EncodedRequest",
            "prepare(",
            "ResponseEntity",
            "BufferedResponse",
            "__response_entity_plan",
        ],
    );
    assert_not_contains_all(
        &out,
        &[
            &forbidden_content_type_check_name(),
            &forbidden_request_body_plan_encoded(),
            &forbidden_response_codec_try_accept(),
            &forbidden_response_codec_decode(),
            &forbidden_generated_decode_binding(),
            &forbidden_endpoint_execute_box_wrapper(),
        ],
    );
}

#[test]
fn generated_bytes_response_uses_response_entity_plan() {
    let out = expanded(quote! {
        client BytesEntityApi {
            base "https://example.com"
        }

        GET Download
            path ["download"]
            -> Bytes
    });

    assert_contains_all(
        &out,
        &[
            "ResponseEntity",
            "BytesResponse",
            "__response_entity_plan",
            "response: __response_plan",
            "ResponseEntity>::execute",
            "NoRequestBody",
        ],
    );
    assert_not_contains_all(
        &out,
        &[
            &forbidden_response_plan_struct(),
            &forbidden_response_codec_try_accept(),
            &forbidden_response_codec_decode(),
            &forbidden_generated_decode_binding(),
            &forbidden_endpoint_execute_box_wrapper(),
            "no_content :",
            "format :",
        ],
    );
}

#[test]
fn generated_no_content_response_uses_response_entity_plan() {
    let out = expanded(quote! {
        client NoContentEntityApi {
            base "https://example.com"
        }

        DELETE Remove
            path ["remove"]
            -> NoContent
    });

    assert_contains_all(
        &out,
        &[
            "ResponseEntity",
            "NoContentResponse",
            "__response_entity_plan",
            "response: __response_plan",
            "ResponseEntity>::execute",
            "NoRequestBody",
        ],
    );
    assert_not_contains_all(
        &out,
        &[
            &forbidden_response_plan_struct(),
            &forbidden_response_codec_try_accept(),
            &forbidden_response_codec_decode(),
            &forbidden_generated_decode_binding(),
            "no_content :",
            "format :",
        ],
    );
}
