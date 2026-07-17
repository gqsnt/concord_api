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

fn check_fixture() -> Output {
    check_manifest(
        &workspace_root(),
        "config-features/managed-escape",
        &fixture_manifest(),
        &["--bin", "managed_escape"],
    )
}

#[test]
fn managed_reqwest_owners_and_raw_values_do_not_escape_publicly() {
    let output = check_fixture();
    let diagnostics = diagnostics(&output);
    assert!(
        !output.status.success(),
        "managed-client escape fixture unexpectedly compiled\n{diagnostics}"
    );
    for expected in [
        "ApplicationHttp",
        "ProviderHttpInner",
        "no method named `client`",
        "no method named `reqwest_builder`",
    ] {
        assert!(
            diagnostics.contains(expected),
            "managed-client diagnostic omitted {expected:?}\n{diagnostics}"
        );
    }
}
