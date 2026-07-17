use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/raw_escape/Cargo.toml")
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
        "raw-escape/negative-surface",
        &fixture_manifest(),
        &[],
    )
}

#[test]
fn downstream_code_cannot_compile_raw_header_escape_attempts() {
    let output = check_fixture();
    let diagnostics = diagnostics(&output);
    assert!(
        !output.status.success(),
        "raw-escape fixture unexpectedly compiled\n{diagnostics}"
    );

    for expected in [
        // Owned sanitized map escape attempts.
        "doesn't satisfy `SanitizedHeaderMap: AsRef<_>`",
        "expected `&HeaderMap`, found `&SanitizedHeaderMap`",
        "HeaderMap: From<SanitizedHeaderMap>",
        "no method named `raw` found for reference `&SanitizedHeaderMap`",
        "expected `Option<(&HeaderName, &HeaderValue)>`",
        // Borrowed sanitized view escape attempts.
        "no method named `as_ref` found for struct `SanitizedHeaders`",
        "type `SanitizedHeaders<'_>` cannot be dereferenced",
        "HeaderMap: From<SanitizedHeaders<'_>>",
        "no method named `raw` found for struct `SanitizedHeaders`",
        "Vec<(&HeaderName, &HeaderValue)>` cannot be built from an iterator over elements of type `(&HeaderName, SanitizedHeaderValue<'_>)`",
    ] {
        assert!(
            diagnostics.contains(expected),
            "raw-escape diagnostic omitted {expected:?}\n{diagnostics}"
        );
    }
}
