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
