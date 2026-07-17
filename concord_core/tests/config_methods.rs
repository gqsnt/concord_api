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
        &format!("config-features/{bin}-{}", features.unwrap_or("base")),
        &fixture_manifest(),
        &arguments,
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
        "json",
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
        ("absence_json", "Json"),
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
