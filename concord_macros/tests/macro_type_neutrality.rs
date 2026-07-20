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

    for relative in files {
        let source = production_source(&root.join(relative));
        let identifiers = identifiers(&source);
        for forbidden in forbidden_categories
            .iter()
            .chain(forbidden_semantic_names.iter())
        {
            assert!(
                !identifiers.contains(forbidden),
                "{relative} contains production body/response semantic identifier {forbidden}"
            );
        }
    }

    let ast = production_source(&root.join("src/ast.rs"));
    let model = production_source(&root.join("src/model.rs"));
    for (owner, source) in [("AST", ast), ("normalized model", model)] {
        assert!(
            source.contains("pub(crate) body: Option<Type>")
                && source.contains("pub(crate) response: Type"),
            "{owner} must store request and response specifications as syn::Type"
        );
    }

    let codegen = production_source(&root.join("src/codegen.rs"));
    for required in ["RequestBodySpec", "ResponseBodySpec", "PageResponseSpec"] {
        assert!(
            codegen.contains(required),
            "generic code generation omitted {required}"
        );
    }
}
