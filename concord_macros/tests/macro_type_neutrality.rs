use std::collections::BTreeSet;
use std::path::Path;

fn production_source(path: &Path) -> String {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    source
        .split("#[cfg(test)]")
        .next()
        .expect("production source prefix")
        .to_owned()
}

fn identifiers(source: &str) -> BTreeSet<&str> {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect()
}

#[test]
fn production_macro_model_is_type_shaped_and_semantically_neutral() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/ast.rs",
        "src/model.rs",
        "src/parse.rs",
        "src/resolve.rs",
        "src/codegen.rs",
        "src/lib.rs",
    ];
    let forbidden_categories = ["RequestBodyKind", "ResponseKind", "CodecKind", "MediaKind"];
    let forbidden_semantic_names = [
        "Json",
        "Text",
        "Bytes",
        "NoContent",
        "Stream",
        "Multipart",
        "Records",
        "Csv",
        "NdJson",
    ];
    let downstream_application_names = [
        "DownstreamBody",
        "ApplicationRequestSpec",
        "VendorEnvelope",
        "CustomStreamingResponse",
    ];

    for relative in files {
        let source = production_source(&root.join(relative));
        let identifiers = identifiers(&source);
        for forbidden in forbidden_categories
            .iter()
            .chain(forbidden_semantic_names.iter())
            .chain(downstream_application_names.iter())
        {
            assert!(
                !identifiers.contains(forbidden),
                "{relative} contains production body/response semantic identifier {forbidden}"
            );
        }
    }

    let ast = production_source(&root.join("src/ast.rs"));
    assert!(
        ast.contains("pub(crate) body: Option<BodyDecl>")
            && ast.contains("pub(crate) struct BodyDecl")
            && ast.contains("pub(crate) ty: Type")
            && ast.contains("pub(crate) response: Type"),
        "AST must store explicit body roles and response specifications as syn::Type"
    );
    let model = production_source(&root.join("src/model.rs"));
    assert!(
        model.contains("pub(crate) body: Option<RequestBody>")
            && model.contains("pub(crate) struct RequestBody")
            && model.contains("pub(crate) ty: Type")
            && model.contains("pub(crate) response: Type"),
        "normalized model must store explicit body roles and response specifications as syn::Type"
    );

    let codegen = production_source(&root.join("src/codegen.rs"));
    for required in ["RequestBodySpec", "ResponseBodySpec", "PageResponseSpec"] {
        assert!(
            codegen.contains(required),
            "generic code generation omitted {required}"
        );
    }

    for structural_step in [
        "let endpoint = #endpoint_type::__concord_from_facade(",
        "(#client_expression).request(endpoint)",
        "pub fn request<E>(&self, endpoint: E)",
        "pub struct #request_name",
        "type Request = #request_name",
        "impl #generated_core::__private::EndpointSpec for #endpoint_name",
        "<Self::Body as #generated_core::__private::RequestBodySpec>::metadata(",
        ") -> <Self::Body as #generated_core::__private::RequestBodySpec>::Input",
        "bounds.push(quote!(#body: #core_path::advanced::RequestBodySpec))",
    ] {
        assert!(
            codegen.contains(structural_step),
            "facade and explicit endpoint generation no longer share the type-owned request-spec path: {structural_step}"
        );
    }

    for classifier in [
        "classify_body",
        "classify_request",
        "request_body_kind",
        "body_type_name",
        "last_type_segment",
    ] {
        assert!(
            !identifiers(&codegen).contains(classifier),
            "production code generation introduced application request-spec recognition: {classifier}"
        );
    }
}
