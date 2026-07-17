use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/development_features/Cargo.toml")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

fn check_fixture(bin: &str, features: Option<&str>) -> Output {
    let mut arguments = vec!["--bin", bin];
    if let Some(features) = features {
        arguments.extend(["--features", features]);
    }
    check_manifest(
        &workspace_root(),
        &format!("development-features/{bin}-{}", features.unwrap_or("base")),
        &fixture_manifest(),
        &arguments,
    )
}

#[test]
fn development_module_is_downstream_accessible_only_with_its_feature() {
    let present = check_fixture("presence", Some("dangerous-dev-tools"));
    assert!(
        present.status.success(),
        "development presence fixture failed\n{}",
        diagnostics(&present)
    );

    let absent = check_fixture("absence", None);
    let absent_diagnostics = diagnostics(&absent);
    assert!(
        !absent.status.success(),
        "development absence fixture unexpectedly compiled\n{absent_diagnostics}"
    );
    assert!(
        absent_diagnostics.contains("__development"),
        "absence diagnostic omitted the gated module\n{absent_diagnostics}"
    );
}

#[test]
fn development_items_do_not_leak_into_stable_namespaces() {
    let output = check_fixture("namespace_escape", Some("dangerous-custom-retry"));
    let diagnostics = diagnostics(&output);
    assert!(
        !output.status.success(),
        "development namespace escape unexpectedly compiled\n{diagnostics}"
    );
    for namespace in ["advanced", "prelude", "dangerous"] {
        assert!(
            diagnostics.contains(namespace),
            "namespace diagnostic omitted {namespace:?}\n{diagnostics}"
        );
    }
}

#[test]
fn development_feature_changes_no_generated_client_signature() {
    let normal = check_fixture("generated_presence", Some("dangerous-dev-tools"));
    assert!(
        normal.status.success(),
        "normal generated client failed with development tools\n{}",
        diagnostics(&normal)
    );

    let escape = check_fixture("generated_escape", Some("dangerous-dev-tools"));
    let diagnostics = diagnostics(&escape);
    assert!(
        !escape.status.success(),
        "generated executor escape unexpectedly compiled\n{diagnostics}"
    );
    for expected in ["0 generic arguments", "0 arguments"] {
        assert!(
            diagnostics.contains(expected),
            "generated signature diagnostic omitted {expected:?}\n{diagnostics}"
        );
    }
}
