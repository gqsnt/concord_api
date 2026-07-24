use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("façade crate should be inside crates/ in the workspace")
        .to_owned()
}

fn fixture_manifest() -> PathBuf {
    workspace_root().join("fixtures/Cargo.toml")
}

fn check_fixture(package: &str, target_key: &str, arguments: &[&str]) -> Output {
    let mut cargo_arguments = vec!["-p", package, "--no-default-features"];
    cargo_arguments.extend_from_slice(arguments);
    check_manifest(
        &workspace_root(),
        target_key,
        &fixture_manifest(),
        &cargo_arguments,
    )
}

fn assert_passes(package: &str, target_key: &str, arguments: &[&str]) {
    let output = check_fixture(package, target_key, arguments);
    assert!(
        output.status.success(),
        "fixture package `{package}` failed\n{}",
        diagnostics(&output)
    );
}

fn assert_fails_with(package: &str, target_key: &str, arguments: &[&str], expected: &str) {
    let output = check_fixture(package, target_key, arguments);
    let diagnostic = diagnostics(&output);
    assert!(
        !output.status.success(),
        "compile-fail fixture package `{package}` unexpectedly succeeded\n{diagnostic}"
    );
    assert!(
        diagnostic.contains(expected),
        "compile-fail fixture package `{package}` omitted `{expected}`\n{diagnostic}"
    );
}

#[test]
fn renamed_facade_dependency_is_the_only_dependency_needed_by_generated_code() {
    assert_passes(
        "concord-fixture-renamed-facade",
        "dependency/renamed-facade",
        &[],
    );
}

#[test]
fn direct_core_and_macro_dependencies_do_not_form_a_supported_user_setup() {
    assert_fails_with(
        "concord-fixture-direct-core-rejected",
        "dependency/direct-core-rejected",
        &[],
        "Concord `api!` requires the `concord` package",
    );
}

#[test]
fn incompatible_facade_bridge_fails_the_generated_contract_handshake() {
    assert_fails_with(
        "concord-fixture-contract-mismatch",
        "dependency/contract-mismatch",
        &[],
        "concord macro/facade contract mismatch: facade 999, generated 24",
    );
}

#[test]
fn ecosystem_capabilities_are_absent_without_their_facade_features() {
    for (bin, features, expected) in [
        (
            "csv_absent",
            "fixture-absence,records",
            "no `Csv` in `records`",
        ),
        (
            "ndjson_absent",
            "fixture-absence,records",
            "no `NdJson` in `records`",
        ),
        (
            "oauth2_absent",
            "fixture-absence",
            "no `OAuth2ClientCredentials` in `auth`",
        ),
    ] {
        assert_fails_with(
            "concord-fixture-features",
            &format!("features/{bin}"),
            &["--bin", bin, "--features", features],
            expected,
        );
    }

    for features in [None, Some("ndjson")] {
        let mut arguments = vec!["--bin", "json_absent", "--features", "fixture-absence"];
        let key = if let Some(features) = features {
            arguments[3] = if features == "ndjson" {
                "fixture-absence,ndjson"
            } else {
                unreachable!("only the NDJSON non-unification graph is tested")
            };
            "features/json-absent-with-ndjson"
        } else {
            "features/json-absent"
        };
        assert_fails_with(
            "concord-fixture-features",
            key,
            &arguments,
            "cannot find type `Json` in this scope",
        );
    }
}

#[test]
fn core_owned_and_dangerous_capabilities_are_absent_without_forwarded_features() {
    for (bin, expected) in [
        (
            "multipart_absent",
            "cannot find type `Multipart` in this scope",
        ),
        ("raw_response_absent", "no method named `raw_response`"),
        (
            "custom_retry_absent",
            "no method named `dangerous_custom_retry`",
        ),
    ] {
        assert_fails_with(
            "concord-fixture-features",
            &format!("features/{bin}"),
            &["--bin", bin, "--features", "fixture-absence"],
            expected,
        );
    }
}

#[test]
fn transport_configuration_methods_are_absent_independently() {
    for (bin, expected) in [
        (
            "default_tls_absent",
            "no method named `add_root_certificate_pem`",
        ),
        ("http2_absent", "no method named `http2_prior_knowledge`"),
        ("gzip_absent", "no method named `disable_gzip`"),
        ("brotli_absent", "no method named `disable_brotli`"),
        ("deflate_absent", "no method named `disable_deflate`"),
    ] {
        assert_fails_with(
            "concord-fixture-features",
            &format!("features/{bin}"),
            &["--bin", bin, "--features", "fixture-absence"],
            expected,
        );
    }
}

#[test]
fn no_governor_graph_fails_closed_and_respects_explicit_limiters() {
    let root = workspace_root();
    let manifest = fixture_manifest();
    let output = run(
        &root,
        "features/no-governor-runtime",
        "test",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "-p",
            "concord-fixture-features",
            "--no-default-features",
            "--test",
            "no_governor",
            "--",
            "--nocapture",
        ],
    );
    let diagnostic = diagnostics(&output);
    assert!(
        output.status.success(),
        "no-governor runtime fixture failed\n{diagnostic}"
    );
    assert!(
        diagnostic
            .contains("CONCORD_NO_GOVERNOR_RATE_LIMITS_OK generated=4 runtime=4 fallback_sends=0"),
        "no-governor runtime omitted its post-assertion sentinel\n{diagnostic}"
    );

    let graph = run(
        &root,
        "features/no-governor-tree",
        "tree",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "-p",
            "concord-fixture-features",
            "--no-default-features",
            "-e",
            "features",
        ],
    );
    let graph_diagnostic = diagnostics(&graph);
    assert!(
        graph.status.success(),
        "no-governor feature graph failed\n{graph_diagnostic}"
    );
    assert!(
        !graph_diagnostic.contains("rate-limit-governor"),
        "no-governor graph activated the governor\n{graph_diagnostic}"
    );
}
