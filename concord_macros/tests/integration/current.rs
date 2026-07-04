use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn current_fixture_directories_exist() {
    let root = manifest_dir();
    for rel in [
        "tests/trybuild/pass",
        "tests/trybuild/fail/auth",
        "tests/trybuild/fail/route",
        "tests/trybuild/fail/fmt",
        "tests/trybuild/fail/policy",
        "tests/trybuild/fail/pagination",
        "tests/trybuild/fail/codegen",
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
    assert!(
        fixture.is_file(),
        "missing canonical fixture: {}",
        fixture.display()
    );
}

#[test]
fn required_stage_snapshots_exist() {
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
        assert!(path.is_file(), "missing stage snapshot: {}", path.display());
    }
}

#[test]
fn production_macro_source_has_no_validation_dependent_panics() {
    let root = manifest_dir().join("../concord_macros/src");
    let mut hits = Vec::new();

    visit_rs_files(&root, &mut |path| {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let production = text
            .split_once("#[cfg(test)]")
            .map(|(head, _)| head)
            .unwrap_or(&text);
        for (line_no, line) in production.lines().enumerate() {
            let trimmed = line.trim();
            for needle in [
                "expect(\"validated",
                "expect(\"valid",
                "expect(\"resolved",
                "unreachable!(\"sema",
                "panic!(\"invalid DSL",
            ] {
                if trimmed.contains(needle) {
                    hits.push(format!("{}:{}: {}", path.display(), line_no + 1, trimmed));
                }
            }
        }
    });

    assert!(
        hits.is_empty(),
        "validation-dependent panic/expect patterns remain in production macro source:\n{}",
        hits.join("\n")
    );
}

fn visit_rs_files(root: &std::path::Path, visit: &mut impl FnMut(&std::path::Path)) {
    let entries = std::fs::read_dir(root)
        .unwrap_or_else(|err| panic!("failed to read dir {}: {err}", root.display()));
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|err| panic!("failed to read entry in {}: {err}", root.display()));
        let path = entry.path();
        if path.is_dir() {
            visit_rs_files(&path, visit);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            visit(&path);
        }
    }
}
