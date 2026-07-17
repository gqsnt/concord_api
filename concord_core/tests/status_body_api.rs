use std::path::{Path, PathBuf};
use std::process::Output;

use concord_core::advanced::{Bytes, StatusBody, StatusBodyState};
use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

fn check_fixture() -> Output {
    check_manifest(
        &workspace_root(),
        "status-body/negative-traits",
        &fixture_manifest(),
        &[],
    )
}

#[test]
fn downstream_complete_only_accessors_have_final_signatures() {
    let _: fn(&StatusBody) = public_accessors;
    let _: fn(StatusBody) = owning_accessor;
}

#[test]
fn status_body_has_no_concord_provided_implicit_byte_or_clone_surface() {
    let output = check_fixture();
    let diagnostics = diagnostics(&output);
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
