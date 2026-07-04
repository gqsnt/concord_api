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
            "StatusResponse",
        ],
    );
    assert_not_contains_all(&expanded, &[&forbidden_request_body_plan_none()]);
}

#[test]
fn emit_uses_record_request_and_response_codegen() {
    let expanded = expanded(quote! {
        api! {
            client RecordCodegen {
                base "https://example.com"
            }

            POST Upload(body: Records<LogEntry, NdJson>)
                path ["upload"]
                -> Records<LogEntry, NdJson>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "RecordBody < LogEntry >",
            "RequestEntity",
            "RecordRequest",
            "prepare(",
            "RecordStream < LogEntry >",
            "ResponseEntity",
            "RecordResponse",
            "ResponseEntity>::execute",
        ],
    );
    assert_not_contains_all(
        &expanded,
        &[
            &forbidden_request_body_plan_records(),
            &forbidden_request_args_with_record_body(),
            &forbidden_records_exec_call(),
            &["Record", "ResponseEndpoint"].concat(),
        ],
    );
}

#[test]
fn emit_uses_multipart_request_and_response_codegen() {
    let expanded = expanded(quote! {
        api! {
            client MultipartCodegen {
                base "https://example.com"
            }

            POST Upload(body: Multipart<RawResponsePart>)
                path ["upload"]
                -> Multipart<RawResponsePart, Mixed>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "MultipartBody",
            "RequestEntity",
            "MultipartRequest",
            "prepare(",
            "MultipartStream < RawResponsePart >",
            "ResponseEntity",
            "MultipartResponse",
            "ResponseEntity>::execute",
        ],
    );
    assert_not_contains_all(
        &expanded,
        &[
            &forbidden_request_body_plan_multipart(),
            &forbidden_request_args_with_multipart_body(),
            &forbidden_content_type_check_name(),
            &forbidden_multipart_exec_call(),
            &["Multipart", "ResponseEndpoint"].concat(),
        ],
    );
}

#[test]
fn emit_uses_sse_response_codegen() {
    let expanded = expanded(quote! {
        api! {
            client SseCodegen {
                base "https://example.com"
            }

            GET Events
                path ["events"]
                -> Sse<MyEvent>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "SseStream < MyEvent >",
            "ResponseEntity",
            "SseResponse",
            "ResponseEntity>::execute",
        ],
    );
    assert_not_contains_all(&expanded, &[&["Sse", "ResponseEndpoint"].concat()]);
}

#[test]
fn emit_uses_explicit_sse_codec_codegen() {
    let expanded = expanded(quote! {
        api! {
            client ExplicitSseCodegen {
                base "https://example.com"
            }

            GET Events
                path ["events"]
                -> Sse<MyEvent, MyCodec>
        }
    });

    assert_contains_all(
        &expanded,
        &[
            "SseStream < MyEvent >",
            "ResponseEntity",
            "SseResponse",
            "ResponseEntity>::execute",
        ],
    );
    assert_not_contains_all(&expanded, &[&["Sse", "ResponseEndpoint"].concat()]);
}
