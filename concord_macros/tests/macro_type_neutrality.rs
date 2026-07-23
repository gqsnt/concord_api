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

fn rust_sources(directory: &Path) -> Vec<std::path::PathBuf> {
    fn collect(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
        let mut entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("failed to enumerate {}: {error}", directory.display()));
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, output);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                output.push(path);
            }
        }
    }

    let mut output = Vec::new();
    collect(directory, &mut output);
    output
}

#[test]
fn production_macro_model_is_type_shaped_and_semantically_neutral() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = rust_sources(&root.join("src"));
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

    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("macro source is below the package root")
            .display();
        let source = production_source(&path);
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

    let ast = production_source(&root.join("src/syntax/ast.rs"));
    assert!(
        ast.contains("pub(crate) body: Option<BodyDecl>")
            && ast.contains("pub(crate) struct BodyDecl")
            && ast.contains("pub(crate) ty: Type")
            && ast.contains("pub(crate) response: Type"),
        "AST must store explicit body roles and response specifications as syn::Type"
    );
    let model = production_source(&root.join("src/semantic/model.rs"));
    assert!(
        model.contains("pub(crate) body: Option<RequestBody>")
            && model.contains("pub(crate) struct RequestBody")
            && model.contains("pub(crate) ty: Type")
            && model.contains("pub(crate) response: Type"),
        "normalized model must store explicit body roles and response specifications as syn::Type"
    );

    let emission = rust_sources(&root.join("src/emit"))
        .into_iter()
        .map(|path| production_source(&path))
        .collect::<Vec<_>>()
        .join("\n");
    for required in ["RequestBodySpec", "ResponseBodySpec", "PageResponseSpec"] {
        assert!(
            emission.contains(required),
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
            emission.contains(structural_step),
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
            !identifiers(&emission).contains(classifier),
            "production code generation introduced application request-spec recognition: {classifier}"
        );
    }
}
