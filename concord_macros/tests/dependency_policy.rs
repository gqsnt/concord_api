use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};
use serde_json::Value;

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

fn workspace_metadata() -> Value {
    let output = run(
        &workspace_root(),
        "dependency-policy/workspace-metadata",
        "metadata",
        ["--format-version", "1", "--no-deps"],
    );
    assert!(
        output.status.success(),
        "workspace metadata failed\n{}",
        diagnostics(&output)
    );
    serde_json::from_slice(&output.stdout).expect("Cargo metadata should be valid JSON")
}

fn package_metadata<'a>(metadata: &'a Value, name: &str) -> &'a Value {
    metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages should be an array")
        .iter()
        .find(|package| package["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("Cargo metadata omitted workspace package {name}"))
}

fn has_metadata_dependency(package: &Value, dependency: &str, kind: Option<&str>) -> bool {
    package["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .any(|candidate| {
            candidate["name"].as_str() == Some(dependency) && candidate["kind"].as_str() == kind
        })
}

fn metadata_dependency<'a>(package: &'a Value, dependency: &str) -> &'a Value {
    package["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .find(|candidate| {
            candidate["name"].as_str() == Some(dependency) && candidate["kind"].is_null()
        })
        .unwrap_or_else(|| {
            panic!(
                "{} omitted normal dependency {dependency}",
                package["name"].as_str().unwrap_or("<unknown package>")
            )
        })
}

fn core_feature_tree(features: Option<&str>, target_key: &str) -> String {
    feature_tree("concord_core", features, target_key)
}

fn extras_feature_tree(features: Option<&str>, target_key: &str) -> String {
    feature_tree("concord_extras", features, target_key)
}

fn feature_tree(package: &str, features: Option<&str>, target_key: &str) -> String {
    let mut arguments = vec![
        "-p",
        package,
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
        "{package} feature dependency tree failed\n{}",
        diagnostics(&output)
    );
    String::from_utf8(output.stdout).expect("Cargo tree output should be UTF-8")
}

#[test]
fn facade_and_extras_edges_are_exact_and_directional() {
    let metadata = workspace_metadata();
    let facade = package_metadata(&metadata, "concord");
    let core = package_metadata(&metadata, "concord_core");
    let extras = package_metadata(&metadata, "concord_extras");
    let macros = package_metadata(&metadata, "concord_macros");
    let examples = package_metadata(&metadata, "concord_examples");

    for dependency in ["concord_core", "concord_extras", "concord_macros"] {
        let edge = metadata_dependency(facade, dependency);
        assert_eq!(
            edge["req"].as_str(),
            Some("=0.1.0"),
            "façade sibling edge {dependency} must be exact"
        );
        assert_eq!(
            edge["uses_default_features"].as_bool(),
            Some(false),
            "façade sibling edge {dependency} must disable implementation defaults"
        );
    }

    let facade_internal = facade["dependencies"]
        .as_array()
        .expect("façade dependencies")
        .iter()
        .filter(|dependency| dependency["kind"].is_null())
        .filter_map(|dependency| dependency["name"].as_str())
        .filter(|name| name.starts_with("concord"))
        .collect::<Vec<_>>();
    assert_eq!(
        facade_internal,
        ["concord_core", "concord_extras", "concord_macros"],
        "the façade must own exactly the three implementation edges"
    );

    let extras_core = metadata_dependency(extras, "concord_core");
    assert_eq!(extras_core["req"].as_str(), Some("=0.1.0"));
    assert_eq!(extras_core["uses_default_features"].as_bool(), Some(false));
    for forbidden in ["concord", "concord_macros"] {
        assert!(
            !has_metadata_dependency(extras, forbidden, None),
            "extras must not depend on {forbidden}"
        );
    }
    for forbidden in ["concord", "concord_extras", "concord_macros"] {
        assert!(
            !has_metadata_dependency(core, forbidden, None),
            "core must not depend on {forbidden}"
        );
    }
    for kind in [None, Some("dev"), Some("build")] {
        assert!(
            !has_metadata_dependency(macros, "concord_extras", kind),
            "macros must remain extras-agnostic through dependency kind {kind:?}"
        );
    }

    assert!(
        has_metadata_dependency(examples, "concord", None),
        "canonical examples must consume the façade"
    );
    for dependency in ["concord_core", "concord_extras", "concord_macros"] {
        assert!(
            !has_metadata_dependency(examples, dependency, None),
            "canonical examples retain implementation dependency {dependency}"
        );
    }
}

#[test]
fn core_and_macro_dependencies_follow_the_runtime_direction() {
    let metadata = workspace_metadata();
    let core = package_metadata(&metadata, "concord_core");
    let macros = package_metadata(&metadata, "concord_macros");

    for kind in [None, Some("dev"), Some("build")] {
        assert!(
            !has_metadata_dependency(core, "concord_macros", kind),
            "concord_core must not depend on concord_macros through dependency kind {kind:?}"
        );
    }
    assert!(
        !core["dependencies"]
            .as_array()
            .expect("core dependencies should be an array")
            .iter()
            .any(|dependency| dependency["name"].as_str() == Some("concord_macros")),
        "concord_core must not have optional, renamed, or target-specific concord_macros dependencies"
    );

    let forwarded_macro_features = core["features"]
        .as_object()
        .expect("core features should be an object")
        .iter()
        .flat_map(|(feature, members)| {
            members
                .as_array()
                .expect("feature members should be an array")
                .iter()
                .filter_map(Value::as_str)
                .filter(|member| member.contains("concord_macros"))
                .map(move |member| (feature.as_str(), member))
        })
        .collect::<Vec<_>>();
    assert!(
        forwarded_macro_features.is_empty(),
        "concord_core features must not forward to concord_macros: {forwarded_macro_features:?}"
    );

    assert!(
        !has_metadata_dependency(macros, "concord_core", None),
        "concord_macros must remain runtime-neutral in normal dependencies"
    );
    assert!(
        !has_metadata_dependency(macros, "concord_core", Some("build")),
        "concord_macros must remain runtime-neutral in build dependencies"
    );
    assert!(
        has_metadata_dependency(macros, "concord_core", Some("dev")),
        "macro integration tests must compile generated code against concord_core"
    );
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
fn stream_media_is_common_and_oauth_json_is_extras_owned() {
    let core = core_feature_tree(None, "dependency-policy/core-oauth-removed");
    assert!(
        has_direct_package(&core, "mime"),
        "stream media validation requires common mime support\n{core}"
    );
    for package in ["serde", "serde_json"] {
        assert!(
            !has_direct_package(&core, package),
            "core unexpectedly retains OAuth parser dependency {package}\n{core}"
        );
    }

    let oauth = extras_feature_tree(
        Some("oauth2-client"),
        "dependency-policy/extras-oauth-enabled",
    );
    for package in ["bytes", "http", "serde_json", "url"] {
        assert!(
            has_direct_package(&oauth, package),
            "OAuth extra omitted {package}\n{oauth}"
        );
    }
    assert!(
        !has_direct_package(&oauth, "csv") && !has_direct_package(&oauth, "mime"),
        "OAuth support unexpectedly activates CSV or MIME\n{oauth}"
    );

    let metadata = workspace_metadata();
    let core = package_metadata(&metadata, "concord_core");
    let extras = package_metadata(&metadata, "concord_extras");
    let facade = package_metadata(&metadata, "concord");
    assert!(
        !core["features"]
            .as_object()
            .expect("core features")
            .contains_key("oauth2-client"),
        "core retains concrete OAuth feature ownership"
    );
    assert_eq!(
        extras["features"]["oauth2-client"],
        serde_json::json!(["dep:bytes", "dep:http", "dep:serde_json", "dep:url"])
    );
    assert_eq!(
        facade["features"]["oauth2-client"],
        serde_json::json!(["concord_extras/oauth2-client"])
    );
}

#[test]
fn moved_codec_and_record_dependencies_are_extras_owned_and_isolated() {
    let records = core_feature_tree(Some("records"), "dependency-policy/core-records");
    for package in ["csv", "serde", "serde_json"] {
        assert!(
            !has_direct_package(&records, package),
            "the core records engine unexpectedly activates {package}\n{records}"
        );
    }

    let csv = extras_feature_tree(Some("csv"), "dependency-policy/extras-csv");
    for package in ["csv", "mime", "serde"] {
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

    let ndjson = extras_feature_tree(Some("ndjson"), "dependency-policy/extras-ndjson");
    for package in ["mime", "serde", "serde_json"] {
        assert!(
            has_direct_package(&ndjson, package),
            "NDJSON support omitted {package}\n{ndjson}"
        );
    }
    assert!(
        !has_direct_package(&ndjson, "csv"),
        "NDJSON support unexpectedly activates CSV\n{ndjson}"
    );

    let json = extras_feature_tree(Some("json"), "dependency-policy/extras-json");
    for package in ["mime", "serde", "serde_json"] {
        assert!(
            has_direct_package(&json, package),
            "buffered JSON support omitted {package}\n{json}"
        );
    }
    assert!(
        !has_direct_package(&json, "csv"),
        "buffered JSON support unexpectedly activates CSV\n{json}"
    );

    let metadata = workspace_metadata();
    let core = package_metadata(&metadata, "concord_core");
    let core_features = core["features"].as_object().expect("core features");
    for moved in ["json", "csv", "ndjson", "oauth2-client"] {
        assert!(
            !core_features.contains_key(moved),
            "core retains moved feature {moved}"
        );
    }

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

#[test]
fn governor_ecosystem_and_default_composition_are_extras_and_facade_owned() {
    let metadata = workspace_metadata();
    let core = package_metadata(&metadata, "concord_core");
    let extras = package_metadata(&metadata, "concord_extras");
    let facade = package_metadata(&metadata, "concord");

    let core_features = core["features"].as_object().expect("core features");
    assert!(
        !core_features.contains_key("rate-limit-governor"),
        "core retains the concrete governor feature"
    );

    let extras_feature = extras["features"]["rate-limit-governor"]
        .as_array()
        .expect("extras governor feature")
        .iter()
        .map(|member| member.as_str().expect("feature member"))
        .collect::<Vec<_>>();
    assert_eq!(
        extras_feature,
        ["dep:http", "dep:httpdate", "dep:tokio"],
        "extras must own the complete concrete governor dependency set"
    );
    for dependency in ["http", "httpdate", "tokio"] {
        let dependency = metadata_dependency(extras, dependency);
        assert_eq!(
            dependency["optional"].as_bool(),
            Some(true),
            "governor ecosystem dependency {dependency:?} must remain optional"
        );
    }

    let facade_feature = facade["features"]["rate-limit-governor"]
        .as_array()
        .expect("façade governor feature")
        .iter()
        .map(|member| member.as_str().expect("feature member"))
        .collect::<Vec<_>>();
    assert_eq!(
        facade_feature,
        ["concord_extras/rate-limit-governor"],
        "the user-facing feature must compose the extras implementation"
    );

    let disabled = extras_feature_tree(None, "dependency-policy/extras-governor-disabled");
    for dependency in ["httpdate"] {
        assert!(
            !has_direct_package(&disabled, dependency),
            "governor-disabled extras unexpectedly activates {dependency}\n{disabled}"
        );
    }
    let enabled = extras_feature_tree(
        Some("rate-limit-governor"),
        "dependency-policy/extras-governor-enabled",
    );
    for dependency in ["http", "httpdate", "tokio"] {
        assert!(
            has_direct_package(&enabled, dependency),
            "governor support omitted {dependency}\n{enabled}"
        );
    }
}
