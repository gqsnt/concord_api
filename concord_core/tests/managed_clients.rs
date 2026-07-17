use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_features/Cargo.toml")
}

fn check_fixture() -> Output {
    let target =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/compile-fixtures/config-features");
    Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--locked")
        .arg("--color")
        .arg("never")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .arg("--bin")
        .arg("managed_escape")
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("managed-client visibility fixture should run")
}

#[test]
fn managed_reqwest_owners_and_raw_values_do_not_escape_publicly() {
    let output = check_fixture();
    let diagnostics = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
