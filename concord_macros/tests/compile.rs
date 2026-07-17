use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("macro crate should be inside the workspace")
        .to_owned()
}

fn check_fixture(name: &str) -> Output {
    let manifest = fixture_root().join(name).join("Cargo.toml");
    check_manifest(&workspace_root(), &format!("macros/{name}"), &manifest, &[])
}

fn assert_fixture_passes(name: &str) {
    let output = check_fixture(name);
    assert!(
        output.status.success(),
        "success fixture `{name}` failed\n{}",
        diagnostics(&output)
    );
}

fn assert_fixture_fails_with(name: &str, expected: &str) {
    let output = check_fixture(name);
    let diagnostic = diagnostics(&output);
    assert!(
        !output.status.success(),
        "compile-fail fixture `{name}` unexpectedly succeeded\n{diagnostic}"
    );
    assert!(
        diagnostic.contains(expected),
        "compile-fail fixture `{name}` omitted intentional diagnostic `{expected}`\n{diagnostic}"
    );
}

#[test]
fn renamed_core_dependency_resolves_through_package_metadata() {
    assert_fixture_passes("renamed_core");
}

#[test]
fn missing_core_dependency_has_a_focused_macro_diagnostic() {
    assert_fixture_fails_with("missing_core", "requires the `concord_core` package");
}

#[test]
fn incompatible_core_contract_fails_during_compilation() {
    assert_fixture_fails_with("contract_mismatch", "concord macro/core contract mismatch");
}
