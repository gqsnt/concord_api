use super::super::*;

pub(super) fn expanded(input: TokenStream2) -> String {
    let resolved = crate::sema::analyze_tokens_for_test(input);
    emit(resolved)
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

pub(super) fn ident_text(ident: &syn::Ident) -> String {
    ident.to_string()
}

pub(super) fn ident_vec_text(idents: &[syn::Ident]) -> Vec<String> {
    idents.iter().map(ToString::to_string).collect()
}

pub(super) fn type_text(ty: &syn::Type) -> String {
    quote::quote!(#ty).to_string()
}

pub(super) fn assert_contains_all(expanded: &str, snippets: &[&str]) {
    for snippet in snippets {
        let compact: String = snippet.chars().filter(|ch| !ch.is_whitespace()).collect();
        assert!(
            expanded.contains(&compact),
            "expanded code did not contain `{snippet}`\n\nexpanded:\n{expanded}"
        );
    }
}

pub(super) fn assert_not_contains_all(expanded: &str, snippets: &[&str]) {
    for snippet in snippets {
        let compact: String = snippet.chars().filter(|ch| !ch.is_whitespace()).collect();
        assert!(
            !expanded.contains(&compact),
            "expanded code unexpectedly contained `{snippet}`\n\nexpanded:\n{expanded}"
        );
    }
}

pub(super) fn forbidden_request_body_plan_encoded() -> String {
    ["BodyPlan", "::", "Encoded"].concat()
}

pub(super) fn forbidden_request_body_plan_raw_stream() -> String {
    ["BodyPlan", "::", "RawStream"].concat()
}

pub(super) fn forbidden_request_body_plan_multipart() -> String {
    ["BodyPlan", "::", "Multipart"].concat()
}

pub(super) fn forbidden_request_body_plan_none() -> String {
    ["BodyPlan", "::", "None"].concat()
}

pub(super) fn forbidden_request_args_with_body_bytes() -> String {
    ["RequestArgs", "::", "with_body_bytes"].concat()
}

pub(super) fn forbidden_request_args_with_stream_body() -> String {
    ["RequestArgs", "::", "with_stream_body"].concat()
}

pub(super) fn forbidden_request_args_with_multipart_body() -> String {
    ["RequestArgs", "::", "with_multipart_body"].concat()
}

pub(super) fn forbidden_body_codec_encode() -> String {
    ["BodyCodec", "::", "encode"].concat()
}

pub(super) fn forbidden_content_type_check_name() -> String {
    ["try_", "content_", "type"].concat()
}

pub(super) fn forbidden_stream_exec_call() -> String {
    ["execute_plan_", "stream::<", "OctetStream", ">"].concat()
}

pub(super) fn forbidden_endpoint_execute_box_wrapper() -> String {
    [
        "Box::pin(asyncmove{",
        "prepare_generated_response",
        ">::execute",
    ]
    .concat()
}

pub(super) fn forbidden_response_plan_struct() -> String {
    ["ResponsePlan", " {"].concat()
}

pub(super) fn assert_runtime_response_plan_has_no_format_field(expanded: &str) {
    let start = expanded
        .find("let__response_preparation=")
        .expect("generated response preparation adapter");
    let end = expanded[start..]
        .find("let__pagination_plan=")
        .map(|offset| start + offset)
        .expect("runtime response-plan section terminator");
    let runtime_response_plan = &expanded[start..end];
    assert!(
        !runtime_response_plan.contains("format:"),
        "generated response preparation unexpectedly reconstructed a `format` field: {runtime_response_plan}"
    );
}

pub(super) fn forbidden_response_codec_try_accept() -> String {
    ["ResponseCodec", ">::try_accept()"].concat()
}

pub(super) fn forbidden_response_codec_decode() -> String {
    ["ResponseCodec", ">::decode"].concat()
}

pub(super) fn forbidden_generated_decode_binding() -> String {
    ["decode", " : __decode_"].concat()
}

pub(super) fn generated_doc_attrs(expanded: &str) -> Vec<&str> {
    let mut docs = Vec::new();
    let mut rest = expanded;
    while let Some(start) = rest.find("#[doc=\"") {
        let after_start = &rest[start + "#[doc=\"".len()..];
        let Some(end) = after_start.find("\"]") else {
            break;
        };
        docs.push(&after_start[..end]);
        rest = &after_start[end + 2..];
    }
    docs
}

pub(super) fn assert_generated_doc_attrs_do_not_contain(expanded: &str, needle: &str) {
    for doc in generated_doc_attrs(expanded) {
        assert!(
            !doc.contains(needle),
            "generated rustdoc `{doc}` must not contain `{needle}`"
        );
    }
}

pub(super) fn assert_generated_doc_attrs_do_not_expose_hidden_names(expanded: &str) {
    for needle in ["__", "EpSearch", "EpCreate"] {
        assert_generated_doc_attrs_do_not_contain(expanded, needle);
    }
}

pub(super) fn without_doc_attrs(expanded: &str) -> String {
    let mut out = String::new();
    let mut rest = expanded;
    while let Some(start) = rest.find("#[doc=\"") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + "#[doc=\"".len()..];
        let Some(end) = after_start.find("\"]") else {
            break;
        };
        rest = &after_start[end + 2..];
    }
    out.push_str(rest);
    out
}
