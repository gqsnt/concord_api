use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use concord_core::advanced::{Bytes, StatusBody, StatusBodyState};

fn public_accessors(body: &StatusBody) {
    let _: StatusBodyState = body.state();
    let _: Option<usize> = body.len();
    let _: Option<&Bytes> = body.bytes();
}

fn owning_accessor(body: StatusBody) {
    let _: Option<Bytes> = body.into_bytes();
}

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/status_body_traits/Cargo.toml")
}

fn check_fixture() -> Output {
    let target =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/compile-fixtures/status-body");
    Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--locked")
        .arg("--color")
        .arg("never")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("status-body trait fixture should run")
}

#[test]
fn downstream_complete_only_accessors_have_final_signatures() {
    let _: fn(&StatusBody) = public_accessors;
    let _: fn(StatusBody) = owning_accessor;
}

#[test]
fn status_body_has_no_concord_provided_implicit_byte_or_clone_surface() {
    let output = check_fixture();
    let diagnostics = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "negative status-body fixture unexpectedly compiled\n{diagnostics}"
    );
    for expected in ["Clone", "AsRef", "cannot be dereferenced", "Serialize"] {
        assert!(
            diagnostics.contains(expected),
            "status-body diagnostic omitted {expected:?}\n{diagnostics}"
        );
    }
}
