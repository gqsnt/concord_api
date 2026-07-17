use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_features/Cargo.toml")
}

fn check_fixture(bin: &str, features: Option<&str>) -> Output {
    let target =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/compile-fixtures/config-features");
    let mut command = Command::new(env!("CARGO"));
    command
        .arg("check")
        .arg("--offline")
        .arg("--locked")
        .arg("--color")
        .arg("never")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .arg("--bin")
        .arg(bin)
        .env("CARGO_TARGET_DIR", target);
    if let Some(features) = features {
        command.arg("--features").arg(features);
    }
    command.output().expect("configuration fixture should run")
}

fn diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn feature_gated_configuration_methods_are_present_only_with_their_features() {
    let base = check_fixture("presence", None);
    assert!(
        base.status.success(),
        "base fixture failed\n{}",
        diagnostics(&base)
    );

    for feature in [
        "http2",
        "default-tls",
        "gzip",
        "brotli",
        "deflate",
        "dangerous-custom-retry",
    ] {
        let output = check_fixture("presence", Some(feature));
        assert!(
            output.status.success(),
            "presence fixture failed for {feature}\n{}",
            diagnostics(&output)
        );
    }

    for (bin, expected) in [
        ("absence_http2", "http2_prior_knowledge"),
        ("absence_tls", "add_root_certificate_pem"),
        ("absence_gzip", "disable_gzip"),
        ("absence_brotli", "disable_brotli"),
        ("absence_deflate", "disable_deflate"),
        ("absence_dangerous", "dangerous"),
    ] {
        let output = check_fixture(bin, None);
        let diagnostics = diagnostics(&output);
        assert!(
            !output.status.success(),
            "absence fixture {bin} unexpectedly compiled\n{diagnostics}"
        );
        assert!(
            diagnostics.contains(expected),
            "absence fixture {bin} omitted {expected:?}\n{diagnostics}"
        );
    }
}
