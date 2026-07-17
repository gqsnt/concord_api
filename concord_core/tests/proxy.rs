use std::error::Error;
use std::path::{Path, PathBuf};

use concord_core::advanced::{ConfigurationErrorKind, SafeProxy};
use concord_test_support::cargo_fixture::{check_manifest, diagnostics};

const PROXY_SENTINEL: &str = "proxy-user-password-sentinel-430f5a";

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_features/Cargo.toml")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
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
    let output = check_manifest(
        &workspace_root(),
        "config-features/proxy-escape",
        &fixture_manifest(),
        &["--bin", "proxy_escape"],
    );
    let diagnostics = diagnostics(&output);
    assert!(
        !output.status.success(),
        "fixture unexpectedly compiled\n{diagnostics}"
    );
    assert!(diagnostics.contains("private"), "{diagnostics}");
    assert!(diagnostics.contains("From<SafeProxy>"), "{diagnostics}");
}
