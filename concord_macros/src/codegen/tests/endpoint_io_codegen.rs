use super::helpers::*;
use quote::quote;

#[test]
fn emit_uses_stream_request_and_response_codegen() {
    let expanded = expanded(quote! {
        api! {
            client StreamCodegen {
                base "https://example.com"
            }

            POST Upload(body: Stream<OctetStream>)
                path ["upload"]
                -> Stream<OctetStream>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "StreamBody",
            "RequestEntity",
            "RawStreamRequest",
            "prepare(",
            "__prepared_request_entity.body",
            "body: __prepared_body",
            "StreamResponse<OctetStream>",
            "ResponseEntity",
            "RawStreamResponse",
            "ResponseEntity>::execute",
        ],
    );
    assert_not_contains_all(
        &expanded,
        &[
            &forbidden_request_body_plan_raw_stream(),
            &forbidden_request_args_with_stream_body(),
            &forbidden_stream_exec_call(),
            &forbidden_endpoint_execute_box_wrapper(),
            &["Stream", "ResponseEndpoint"].concat(),
        ],
    );
}

#[test]
fn emit_uses_buffered_request_entity_codegen() {
    let expanded = expanded(quote! {
        api! {
            client BufferedCodegen {
                base "https://example.com"
            }

            POST Create(body: Json<CreateBody>)
                path ["create"]
                -> Json<CreateResponse>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "RequestEntity",
            "EncodedRequest",
            "prepare(",
            "__prepared_request_entity.body",
            "body: __prepared_body",
            "CreateBody",
            "CreateResponse",
        ],
    );
    assert_not_contains_all(
        &expanded,
        &[
            &forbidden_request_body_plan_encoded(),
            &forbidden_request_args_with_body_bytes(),
            &forbidden_body_codec_encode(),
            &forbidden_content_type_check_name(),
        ],
    );
}

#[test]
fn emit_uses_no_request_body_entity_codegen() {
    let expanded = expanded(quote! {
        api! {
            client NoBodyCodegen {
                base "https://example.com"
            }

            GET Status
                path ["status"]
                -> Json<StatusResponse>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "NoRequestBody",
            "RequestEntity",
            "prepare(",
            "__prepared_request_entity.body",
            "body: __prepared_body",
            "StatusResponse",
        ],
    );
    assert_not_contains_all(&expanded, &[&forbidden_request_body_plan_none()]);
}

#[test]
fn emit_uses_multipart_request_codegen() {
    let expanded = expanded(quote! {
        api! {
            client MultipartCodegen {
                base "https://example.com"
            }

            POST Upload(body: Multipart<()>)
                path ["upload"]
                -> Json<UploadResult>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "MultipartBody",
            "RequestEntity",
            "MultipartRequest",
            "prepare(",
            "__prepared_request_entity.body",
            "body: __prepared_body",
        ],
    );
    assert_not_contains_all(
        &expanded,
        &[
            &forbidden_request_body_plan_multipart(),
            &forbidden_request_args_with_multipart_body(),
            &forbidden_content_type_check_name(),
            &forbidden_endpoint_execute_box_wrapper(),
        ],
    );
}
