use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics, run};

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

#[test]
fn generated_public_rustdoc_uses_stable_paths_without_private_integration_names() {
    let root = workspace_root();
    let manifest = fixture_root().join("renamed_core/Cargo.toml");
    let target_key = "macros/renamed-core-rustdoc";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        output.status.success(),
        "renamed generated Rustdoc failed\n{}",
        diagnostics(&output)
    );

    let documentation = root
        .join("target/cargo-fixtures")
        .join(target_key)
        .join("doc/concord_fixture_renamed_core");
    for relative in [
        "struct.RenamedApi.html",
        "struct.RenamedApiBuilder.html",
        "renamed_api/facades/struct.ResourcesFacade.html",
        "renamed_api/endpoints/resources/struct.ById.html",
    ] {
        let path = documentation.join(relative);
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for forbidden in ["__private", "ClientCore", "EndpointSpec", "ResponseHandler"] {
            assert!(
                !page.contains(forbidden),
                "public generated Rustdoc {} exposed {forbidden}",
                path.display()
            );
        }
        if relative.contains("facades") {
            assert!(
                page.contains("Request"),
                "generated facade Rustdoc omitted the stable Request wrapper"
            );
        }
        if relative.contains("endpoints") {
            for expected in ["Bodyless", "GET", "NoContent"] {
                assert!(
                    page.contains(expected),
                    "generated endpoint Rustdoc omitted supported public fact {expected}"
                );
            }
        }
    }
}
