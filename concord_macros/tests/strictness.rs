use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn production_source(path: impl AsRef<Path>) -> String {
    let source = read(path);
    source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&source)
        .to_string()
}

fn codegen_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("read dir {}: {err}", dir.display()))
    {
        let entry = entry.expect("read codegen dir entry");
        let path = entry.path();
        if path.is_dir() {
            codegen_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn codegen_has_no_validation_dependent_panic_paths() {
    let root = crate_root();
    let mut files = Vec::new();
    codegen_files(&root.join("src/codegen"), &mut files);

    let forbidden = [
        "unreachable!()",
        "Option::None::<&str>",
        "expect(\"validated",
        "expect(\"valid retry status\")",
        "expect(\"valid OAuth2ClientCredentials token_url\")",
        "compile_error!(\"paginate auth vars",
        "compile_error!(\"paginate auth vars are not supported",
    ];

    for file in files {
        let source = production_source(&file);
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "{} must not contain `{needle}`",
                file.display()
            );
        }
    }
}

#[test]
fn public_policy_route_and_pagination_codegen_do_not_render_auth_value_ir() {
    let root = crate_root();
    for path in [
        "src/codegen/policy/ops.rs",
        "src/codegen/policy/route.rs",
        "src/codegen/policy/route_guards.rs",
        "src/codegen/policy/pagination.rs",
        "src/codegen/endpoints/endpoint.rs",
    ] {
        let source = production_source(root.join(path));
        for needle in ["ValueKind::AuthField", "FmtVarSource::Auth"] {
            assert!(
                !source.contains(needle),
                "{path} must not contain `{needle}`"
            );
        }
    }
}

#[test]
fn policy_optional_refs_are_not_independent_codegen_metadata() {
    let root = crate_root();
    for path in [
        "src/sema/ir.rs",
        "src/sema/policy.rs",
        "src/codegen/policy/ops.rs",
    ] {
        let source = production_source(root.join(path));
        for needle in ["conditional_on_optional_ref", "OptionalRefKind"] {
            assert!(
                !source.contains(needle),
                "{path} must not contain `{needle}`"
            );
        }
    }
}
