use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use concord_core::advanced::{ConfigurationErrorKind, SafeProxy};

const PROXY_SENTINEL: &str = "proxy-user-password-sentinel-430f5a";

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_features/Cargo.toml")
}

#[test]
fn credential_bearing_proxy_error_is_structured_and_sentinel_free() {
    let value = format!("http://user:{PROXY_SENTINEL}@proxy.example");
    let error = match SafeProxy::parse(value) {
        Ok(_) => panic!("credential-bearing proxy unexpectedly parsed"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidProxy);
    assert!(!error.to_string().contains(PROXY_SENTINEL));
    assert!(!format!("{error:?}").contains(PROXY_SENTINEL));
    assert!(error.source().is_none());
}

#[test]
fn downstream_safe_proxy_has_no_raw_url_escape() {
    let target =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/compile-fixtures/config-features");
    let output = Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--locked")
        .arg("--color")
        .arg("never")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .arg("--bin")
        .arg("proxy_escape")
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("proxy escape fixture should run");
    let diagnostics = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "fixture unexpectedly compiled\n{diagnostics}"
    );
    assert!(diagnostics.contains("private"), "{diagnostics}");
    assert!(diagnostics.contains("From<SafeProxy>"), "{diagnostics}");
}
