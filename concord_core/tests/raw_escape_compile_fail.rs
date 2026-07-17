use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/raw_escape/Cargo.toml")
}

fn check_fixture() -> Output {
    let target =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/compile-fixtures/raw-escape");
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
        .expect("raw-escape Cargo fixture should run")
}

#[test]
fn downstream_code_cannot_compile_raw_header_escape_attempts() {
    let output = check_fixture();
    let diagnostics = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
