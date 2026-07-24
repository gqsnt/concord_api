use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::assert_semantic_compile_failures;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn facade_compile_failures_have_stable_msrv_semantics() {
    let root = workspace_root();
    let owner = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui/msrv");
    assert_semantic_compile_failures(
        &root,
        "msrv/facade-semantic-ui",
        &root.join("fixtures/Cargo.toml"),
        "concord-msrv-facade",
        &owner.join("expectations.txt"),
        &owner.join("forbidden.txt"),
    );
}
