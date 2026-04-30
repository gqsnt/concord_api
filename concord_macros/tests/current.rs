use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn current_fixture_directories_exist() {
    let root = manifest_dir();
    for rel in [
        "tests/dsl/pass",
        "tests/dsl/fail",
        "tests/usage/pass",
        "tests/usage/fail",
        "tests/snapshots/raw_ast",
        "tests/snapshots/norm_api_tree",
        "tests/snapshots/resolved_api",
        "tests/snapshots/facade_ir",
        "tests/snapshots/generated",
    ] {
        let path = root.join(rel);
        assert!(
            path.is_dir(),
            "missing current test harness dir: {}",
            path.display()
        );
    }
}

#[test]
fn trybuild_is_available_for_current_fixtures() {
    let _ = trybuild::TestCases::new();
}

#[test]
fn canonical_small_api_fixture_exists() {
    let fixture = manifest_dir().join("tests/fixtures/current/small_api.concord");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", fixture.display()));

    assert!(body.contains("api!"));
    assert!(body.contains("client SmallApi"));
    assert!(body.contains("GET ping"));
}

#[test]
fn required_stage_snapshots_exist_and_are_stable_text() {
    let root = manifest_dir();
    for rel in [
        "tests/snapshots/raw_ast/raw_ast.snap",
        "tests/snapshots/norm_api_tree/norm_api_tree.snap",
        "tests/snapshots/resolved_api/resolved_api.snap",
        "tests/snapshots/resolved_api/resolved_endpoint.snap",
        "tests/snapshots/resolved_api/custom_codec_and_pagination.snap",
        "tests/snapshots/facade_ir/facade_ir.snap",
        "tests/snapshots/generated/facade_names.generated.snap",
        "tests/snapshots/generated/endpoint_plan.generated.snap",
        "tests/snapshots/generated/route.generated.snap",
        "tests/snapshots/generated/policy.generated.snap",
        "tests/snapshots/generated/response.generated.snap",
        "tests/snapshots/generated/request_builders.generated.snap",
        "tests/snapshots/generated/pagination.generated.snap",
        "tests/snapshots/generated/auth_helpers.generated.snap",
        "tests/snapshots/generated/rustdoc.generated.snap",
        "tests/snapshots/generated/custom_extensibility.generated.snap",
    ] {
        let path = root.join(rel);
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        assert!(
            !body.trim().is_empty(),
            "empty snapshot: {}",
            path.display()
        );
        assert!(
            !body.contains("Span {"),
            "snapshot contains unstable span debug output: {}",
            path.display()
        );
    }
}

#[test]
fn public_docs_and_examples_do_not_expose_hidden_facade_scope_names() {
    fn visit(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        if !path.exists() {
            return;
        }
        for entry in std::fs::read_dir(path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
        {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                visit(&path, files);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("md" | "rs")
            ) {
                files.push(path);
            }
        }
    }

    let repo = manifest_dir()
        .parent()
        .expect("concord_macros has repo parent")
        .to_path_buf();
    let mut files = Vec::new();
    for rel in ["docs", "concord_examples/src", "concord_examples/tests"] {
        visit(&repo.join(rel), &mut files);
    }

    for path in files {
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        assert!(
            !body.contains("__Facade") && !body.contains("__Scope"),
            "public docs/examples expose hidden generated facade/scope name: {}",
            path.display()
        );
    }
}
