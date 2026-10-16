use std::path::{Path, PathBuf};
use std::process::Output;

use crate::command::{check_manifest, diagnostics, run};

fn manifest(root: &Path) -> PathBuf {
    root.join("fixtures/Cargo.toml")
}

fn check_fixture(root: &Path, package: &str, target_key: &str, arguments: &[&str]) -> Output {
    let mut cargo_arguments = vec!["-p", package, "--no-default-features"];
    cargo_arguments.extend_from_slice(arguments);
    check_manifest(root, target_key, &manifest(root), &cargo_arguments)
}

fn require_success(
    root: &Path,
    package: &str,
    target_key: &str,
    arguments: &[&str],
) -> Result<(), String> {
    let output = check_fixture(root, package, target_key, arguments);
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "fixture package `{package}` failed\n{}",
            diagnostics(&output)
        ))
    }
}

fn require_failure(
    root: &Path,
    package: &str,
    target_key: &str,
    arguments: &[&str],
    expected: &[&str],
) -> Result<(), String> {
    let output = check_fixture(root, package, target_key, arguments);
    let diagnostic = diagnostics(&output);
    if output.status.success() {
        return Err(format!(
            "compile-fail fixture package `{package}` unexpectedly succeeded\n{diagnostic}"
        ));
    }
    for fragment in expected {
        if !diagnostic.contains(fragment) {
            return Err(format!(
                "compile-fail fixture package `{package}` omitted `{fragment}`\n{diagnostic}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn run_all(root: &Path) -> Result<(), String> {
    require_success(
        root,
        "concord-fixture-renamed-facade",
        "dependency/renamed-facade",
        &[],
    )?;
    require_failure(
        root,
        "concord-fixture-contract-mismatch",
        "dependency/contract-mismatch",
        &[],
        &["concord macro/facade contract mismatch: facade 999, generated 35"],
    )?;

    for (binary, features, expected) in [
        (
            "buffered_records_absent",
            "fixture-absence",
            &[
                "no `BufferedRecords` in `prelude`",
                "could not find `records` in `runtime`",
            ][..],
        ),
        (
            "records_codecs_absent",
            "fixture-absence,records",
            &[
                "no `Csv` in `records`",
                "no `NdJson` in `records`",
                "no `CsvHeaderMode` in `records`",
                "no `CsvDelimiter` in `records`",
                "no `HasHeader` in `records`",
                "no `NoHeader` in `records`",
                "no `CommaDelimiter` in `records`",
                "no `SemicolonDelimiter` in `records`",
                "no `TabDelimiter` in `records`",
            ][..],
        ),
        (
            "oauth2_absent",
            "fixture-absence",
            &["no `OAuth2ClientCredentials` in `auth`"][..],
        ),
        (
            "rate_limit_absent",
            "fixture-absence",
            // `LocalRateLimiter` is available only with `rate-limit`.
            &["could not find `LocalRateLimiter` in `rate_limit`"][..],
        ),
        (
            "multipart_absent",
            "fixture-absence",
            &["cannot find type `Multipart` in this scope"][..],
        ),
        (
            "transport_absent",
            "fixture-absence",
            &[
                "no method named `add_root_certificate_pem`",
                "no method named `http2_prior_knowledge`",
                "no method named `disable_gzip`",
                "no method named `disable_brotli`",
                "no method named `disable_deflate`",
            ][..],
        ),
    ] {
        require_failure(
            root,
            "concord-fixture-features",
            &format!("features/{binary}"),
            &["--bin", binary, "--features", features],
            expected,
        )?;
    }

    for (target_key, features) in [
        ("features/json-absent", "fixture-absence"),
        ("features/json-absent-with-ndjson", "fixture-absence,ndjson"),
    ] {
        require_failure(
            root,
            "concord-fixture-features",
            target_key,
            &["--bin", "json_absent", "--features", features],
            &["cannot find type `Json` in this scope"],
        )?;
    }

    verify_no_local_limiter(root)?;
    println!("fixture graph checks passed");
    Ok(())
}

fn verify_no_local_limiter(root: &Path) -> Result<(), String> {
    let manifest = manifest(root);
    let manifest = manifest
        .to_str()
        .ok_or_else(|| "fixture manifest path is not valid UTF-8".to_owned())?;
    let output = run(
        root,
        "features/no-local-limiter-runtime",
        "test",
        [
            "--manifest-path",
            manifest,
            "-p",
            "concord-fixture-features",
            "--no-default-features",
            "--test",
            "no_local_limiter",
            "--",
            "--nocapture",
        ],
    );
    let diagnostic = diagnostics(&output);
    if !output.status.success() {
        return Err(format!(
            "no-local-limiter runtime fixture failed\n{diagnostic}"
        ));
    }
    let sentinel = "CONCORD_NO_LOCAL_LIMITER_RATE_LIMITS_OK generated=4 runtime=4 fallback_sends=0";
    if !diagnostic.contains(sentinel) {
        return Err(format!(
            "no-local-limiter runtime omitted its post-assertion sentinel\n{diagnostic}"
        ));
    }

    let graph = run(
        root,
        "features/no-local-limiter-tree",
        "tree",
        [
            "--manifest-path",
            manifest,
            "-p",
            "concord-fixture-features",
            "--no-default-features",
            "-e",
            "features",
        ],
    );
    let graph_diagnostic = diagnostics(&graph);
    if !graph.status.success() {
        return Err(format!(
            "no-local-limiter feature graph failed\n{graph_diagnostic}"
        ));
    }
    if graph_diagnostic.contains("concord feature \"rate-limit\"") {
        return Err(format!(
            "no-local-limiter graph activated the concrete limiter feature\n{graph_diagnostic}"
        ));
    }
    Ok(())
}
