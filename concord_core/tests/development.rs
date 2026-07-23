use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_features/Cargo.toml")
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
    let present = check_fixture("presence_development", Some("dangerous-dev-tools"));
    assert!(
        present.status.success(),
        "development presence fixture failed\n{}",
        diagnostics(&present)
    );

    let absent = check_fixture("absence_development", None);
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
