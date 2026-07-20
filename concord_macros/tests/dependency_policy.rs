use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("macro crate should be inside the workspace")
        .to_owned()
}

fn direct_tree(package: &str, edges: &str, target_key: &str) -> String {
    let output = run(
        &workspace_root(),
        target_key,
        "tree",
        ["-p", package, "-e", edges, "--depth", "1", "--no-dedupe"],
    );
    assert!(
        output.status.success(),
        "direct dependency tree failed\n{}",
        diagnostics(&output)
    );
    String::from_utf8(output.stdout).expect("Cargo tree output should be UTF-8")
}

fn has_direct_package(tree: &str, package: &str) -> bool {
    tree.lines().any(|line| {
        let dependency = line.trim_start_matches([' ', '├', '└', '─']).trim_start();
        dependency.starts_with(package)
            && dependency
                .strip_prefix(package)
                .is_some_and(|suffix| suffix.starts_with(" v"))
    })
}

fn core_feature_tree(features: Option<&str>, target_key: &str) -> String {
    let mut arguments = vec![
        "-p",
        "concord_core",
        "-e",
        "normal,build",
        "--depth",
        "1",
        "--no-dedupe",
        "--no-default-features",
    ];
    if let Some(features) = features {
        arguments.extend(["--features", features]);
    }
    let output = run(&workspace_root(), target_key, "tree", arguments);
    assert!(
        output.status.success(),
        "core feature dependency tree failed\n{}",
        diagnostics(&output)
    );
    String::from_utf8(output.stdout).expect("Cargo tree output should be UTF-8")
}

#[test]
fn transport_stack_exception_is_h2_dev_only() {
    for package in ["concord_core", "concord_macros"] {
        let normal = direct_tree(
            package,
            "normal,build",
            &format!("dependency-policy/{package}-normal-build"),
        );
        for prohibited in ["h2", "hyper", "hyper-util", "tower"] {
            assert!(
                !has_direct_package(&normal, prohibited),
                "{package} unexpectedly has direct normal/build dependency {prohibited}\n{normal}"
            );
        }
    }

    let core_dev = direct_tree("concord_core", "dev", "dependency-policy/core-development");
    assert!(
        has_direct_package(&core_dev, "h2"),
        "the isolated REFUSED_STREAM probes require the reviewed h2 dev dependency\n{core_dev}"
    );
}

#[test]
fn trybuild_is_confined_to_core_and_macro_development_dependencies() {
    for package in ["concord_core", "concord_macros"] {
        let normal = direct_tree(
            package,
            "normal,build",
            &format!("dependency-policy/{package}-normal-build-trybuild"),
        );
        assert!(!has_direct_package(&normal, "trybuild"), "{normal}");

        let development = direct_tree(
            package,
            "dev",
            &format!("dependency-policy/{package}-development-trybuild"),
        );
        assert!(
            has_direct_package(&development, "trybuild"),
            "{development}"
        );
    }
}

#[test]
fn stream_media_is_common_and_json_data_dependencies_remain_feature_owned() {
    let disabled = core_feature_tree(None, "dependency-policy/core-json-disabled");
    assert!(
        has_direct_package(&disabled, "mime"),
        "stream media validation requires common mime support\n{disabled}"
    );
    for package in ["serde", "serde_json"] {
        assert!(
            !has_direct_package(&disabled, package),
            "JSON-disabled core unexpectedly activates {package}\n{disabled}"
        );
    }

    let enabled = core_feature_tree(Some("json"), "dependency-policy/core-json-enabled");
    for package in ["mime", "serde", "serde_json"] {
        assert!(
            has_direct_package(&enabled, package),
            "JSON-enabled core omitted {package}\n{enabled}"
        );
    }
}

#[test]
fn record_dependencies_are_optional_and_directionally_isolated() {
    let records = core_feature_tree(Some("records"), "dependency-policy/core-records");
    for package in ["csv", "serde", "serde_json"] {
        assert!(
            !has_direct_package(&records, package),
            "the records base unexpectedly activates {package}\n{records}"
        );
    }

    let csv = core_feature_tree(Some("csv"), "dependency-policy/core-csv");
    for package in ["csv", "serde"] {
        assert!(
            has_direct_package(&csv, package),
            "CSV support omitted {package}\n{csv}"
        );
    }
    assert!(
        !has_direct_package(&csv, "serde_json"),
        "CSV support unexpectedly activates serde_json\n{csv}"
    );
    assert!(
        csv.lines().any(|line| line.contains("csv v1.3.1")),
        "CSV support must use the reviewed exact parser version\n{csv}"
    );

    let ndjson = core_feature_tree(Some("ndjson"), "dependency-policy/core-ndjson");
    for package in ["serde", "serde_json"] {
        assert!(
            has_direct_package(&ndjson, package),
            "NDJSON support omitted {package}\n{ndjson}"
        );
    }
    assert!(
        !has_direct_package(&ndjson, "csv"),
        "NDJSON support unexpectedly activates csv\n{ndjson}"
    );

    let macros = direct_tree(
        "concord_macros",
        "normal,build",
        "dependency-policy/macro-record-isolation",
    );
    for package in ["csv", "serde_json"] {
        assert!(
            !has_direct_package(&macros, package),
            "the macro crate unexpectedly depends on {package}\n{macros}"
        );
    }
}
