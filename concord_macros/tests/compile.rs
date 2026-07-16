use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn check_fixture(name: &str) -> Output {
    let root = fixture_root();
    let manifest = root.join(name).join("Cargo.toml");
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/compile-fixtures")
        .join(name);
    Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(manifest)
        .env("CARGO_TARGET_DIR", target)
        .output()
        .unwrap_or_else(|error| panic!("failed to run Cargo for fixture `{name}`: {error}"))
}

fn diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn downstream_compile_fixtures_prove_resolution_hygiene_and_failures() {
    for name in [
        "normal",
        "renamed_core",
        "nested_module",
        "repeated_invocation",
        "equal_name",
        "minimal_bounds",
    ] {
        let output = check_fixture(name);
        assert!(
            output.status.success(),
            "success fixture `{name}` failed\n{}",
            diagnostics(&output)
        );
        eprintln!("fixture `{name}`: success");
    }

    for (name, expected_components) in [
        (
            "contract_mismatch",
            &["concord macro/core contract mismatch"][..],
        ),
        (
            "missing_scope",
            &[
                "error[E0308]: mismatched types",
                "expected `ScopeValue`, found `EndpointValue`",
                "scope_value: crate::ScopeValue",
            ][..],
        ),
        ("missing_core", &["requires the `concord_core` package"][..]),
        (
            "malformed",
            &["unsupported endpoint method; PR 0.2 accepts only bodyless `GET`"][..],
        ),
    ] {
        let output = check_fixture(name);
        let diagnostic = diagnostics(&output);
        assert!(
            !output.status.success(),
            "compile-fail fixture `{name}` unexpectedly succeeded\n{diagnostic}"
        );
        for expected in expected_components {
            assert!(
                diagnostic.contains(expected),
                "compile-fail fixture `{name}` did not contain `{expected}`\n{diagnostic}"
            );
        }
        eprintln!("fixture `{name}`: expected failure matched {expected_components:?}");
    }
}
