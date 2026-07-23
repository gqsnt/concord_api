use std::fs;
use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("façade crate should be inside the workspace")
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
    feature_tree("concord-core", features, target_key)
}

fn extras_feature_tree(features: Option<&str>, target_key: &str) -> String {
    feature_tree("concord-extras", features, target_key)
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

fn reqwest_feature_tree(feature: &str, target_key: &str) -> String {
    let output = run(
        &workspace_root(),
        target_key,
        "tree",
        [
            "-p",
            "concord-core",
            "--no-default-features",
            "--features",
            feature,
            "-e",
            "normal,build,features",
            "-i",
            "reqwest",
        ],
    );
    assert!(
        output.status.success(),
        "Reqwest feature tree failed for {feature}\n{}",
        diagnostics(&output)
    );
    String::from_utf8(output.stdout).expect("Cargo tree output should be UTF-8")
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| entry.expect("source directory entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            sources.push(path);
        }
    }
}

#[test]
fn facade_and_extras_edges_are_exact_and_directional() {
    let metadata = workspace_metadata();
    let facade = package_metadata(&metadata, "concord");
    let core = package_metadata(&metadata, "concord-core");
    let extras = package_metadata(&metadata, "concord-extras");
    let macros = package_metadata(&metadata, "concord-macros");
    let examples = package_metadata(&metadata, "concord-examples");
    let workspace_package_names = metadata["packages"]
        .as_array()
        .expect("workspace packages")
        .iter()
        .filter_map(|package| package["name"].as_str())
        .collect::<Vec<_>>();

    for dependency in ["concord-core", "concord-extras", "concord-macros"] {
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
        ["concord-core", "concord-extras", "concord-macros"],
        "the façade must own exactly the three implementation edges"
    );

    let extras_core = metadata_dependency(extras, "concord-core");
    assert_eq!(extras_core["req"].as_str(), Some("=0.1.0"));
    assert_eq!(extras_core["uses_default_features"].as_bool(), Some(false));
    for forbidden in ["concord", "concord-macros"] {
        assert!(
            !has_metadata_dependency(extras, forbidden, None),
            "extras must not depend on {forbidden}"
        );
    }
    for forbidden in ["concord", "concord-extras", "concord-macros"] {
        assert!(
            !has_metadata_dependency(core, forbidden, None),
            "core must not depend on {forbidden}"
        );
    }
    for kind in [None, Some("dev"), Some("build")] {
        assert!(
            !has_metadata_dependency(macros, "concord-extras", kind),
            "macros must remain extras-agnostic through dependency kind {kind:?}"
        );
    }

    assert!(
        has_metadata_dependency(examples, "concord", None),
        "canonical examples must consume the façade"
    );
    for dependency in ["concord-core", "concord-extras", "concord-macros"] {
        assert!(
            !has_metadata_dependency(examples, dependency, None),
            "canonical examples retain implementation dependency {dependency}"
        );
    }

    for package in metadata["packages"].as_array().expect("workspace packages") {
        for dependency in package["dependencies"]
            .as_array()
            .expect("package dependencies")
        {
            let Some(dependency_name) = dependency["name"].as_str() else {
                continue;
            };
            if !workspace_package_names.contains(&dependency_name) {
                continue;
            }
            assert_eq!(
                dependency["req"].as_str(),
                Some("=0.1.0"),
                "{} -> {dependency_name} must use the exact sibling version",
                package["name"]
            );
            assert_eq!(
                dependency["uses_default_features"].as_bool(),
                Some(false),
                "{} -> {dependency_name} must disable sibling defaults",
                package["name"]
            );
        }
    }
}

#[test]
fn canonical_consumers_never_import_implementation_crates() {
    let root = workspace_root();
    let mut sources = Vec::new();
    for directory in [
        root.join("concord_examples").join("src"),
        root.join("concord_examples").join("examples"),
        root.join("concord_examples").join("tests"),
        root.join("perf").join("benches"),
    ] {
        collect_rust_sources(&directory, &mut sources);
    }

    assert!(
        !sources.is_empty(),
        "canonical source policy scanned no files"
    );
    for source in sources {
        let contents = fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        for implementation_crate in ["concord_core", "concord_macros", "concord_extras"] {
            assert!(
                !contents.contains(implementation_crate),
                "{} names implementation crate {implementation_crate}",
                source.display()
            );
        }
    }
}

#[test]
fn facade_owns_the_complete_user_feature_topology_and_implementation_defaults_are_empty() {
    let metadata = workspace_metadata();
    let facade = package_metadata(&metadata, "concord");
    let core = package_metadata(&metadata, "concord-core");
    let extras = package_metadata(&metadata, "concord-extras");
    let macros = package_metadata(&metadata, "concord-macros");
    let support = package_metadata(&metadata, "concord-test-support");

    assert_eq!(
        core["features"],
        serde_json::json!({
            "default": [],
            "default-tls": ["dep:rustls", "reqwest/rustls-no-provider"],
            "http2": ["reqwest/http2"],
            "records": [],
            "multipart": ["reqwest/multipart"],
            "gzip": ["reqwest/gzip"],
            "brotli": ["reqwest/brotli"],
            "deflate": ["reqwest/deflate"],
            "dangerous-raw-response": [],
            "dangerous-custom-retry": [],
            "dangerous-dev-tools": ["dep:concord_test_support"],
        }),
        "core feature topology drifted from the semantic runtime kernel"
    );
    assert_eq!(
        extras["features"],
        serde_json::json!({
            "default": [],
            "json": ["dep:mime", "dep:serde", "dep:serde_json"],
            "csv": ["concord_core/records", "dep:csv", "dep:mime", "dep:serde"],
            "ndjson": ["concord_core/records", "dep:mime", "dep:serde", "dep:serde_json"],
            "rate-limit-governor": ["dep:http", "dep:httpdate", "dep:tokio"],
            "oauth2-client": ["dep:bytes", "dep:http", "dep:serde_json", "dep:url"],
        }),
        "extras feature topology drifted from downstream implementations"
    );
    assert_eq!(
        facade["features"],
        serde_json::json!({
            "default": ["default-tls", "http2", "rate-limit-governor"],
            "default-tls": ["concord_core/default-tls"],
            "http2": ["concord_core/http2"],
            "rate-limit-governor": ["concord_extras/rate-limit-governor"],
            "json": ["concord_extras/json"],
            "oauth2-client": ["concord_extras/oauth2-client"],
            "records": ["concord_core/records"],
            "csv": ["records", "concord_extras/csv"],
            "ndjson": ["records", "concord_extras/ndjson"],
            "multipart": ["concord_core/multipart"],
            "gzip": ["concord_core/gzip"],
            "brotli": ["concord_core/brotli"],
            "deflate": ["concord_core/deflate"],
            "dangerous-raw-response": ["concord_core/dangerous-raw-response"],
            "dangerous-custom-retry": ["concord_core/dangerous-custom-retry"],
            "dangerous-dev-tools": ["concord_core/dangerous-dev-tools"],
        }),
        "the façade must be the single exact user-facing feature owner"
    );

    for package in [facade, core, extras, macros, support] {
        assert!(
            package["description"]
                .as_str()
                .is_some_and(|description| !description.trim().is_empty()),
            "{} must have package documentation metadata",
            package["name"]
        );
        assert_eq!(
            package["metadata"]["docs"]["rs"]["all-features"].as_bool(),
            Some(true),
            "{} must document its complete public surface on docs.rs",
            package["name"]
        );
        assert_eq!(
            package["publish"],
            serde_json::json!([]),
            "{} must retain the reviewed pre-release publish=false posture",
            package["name"]
        );
    }
}

#[test]
fn core_and_macro_dependencies_follow_the_runtime_direction() {
    let metadata = workspace_metadata();
    let core = package_metadata(&metadata, "concord-core");
    let macros = package_metadata(&metadata, "concord-macros");

    for kind in [None, Some("dev"), Some("build")] {
        assert!(
            !has_metadata_dependency(core, "concord-macros", kind),
            "concord_core must not depend on concord_macros through dependency kind {kind:?}"
        );
    }
    assert!(
        !core["dependencies"]
            .as_array()
            .expect("core dependencies should be an array")
            .iter()
            .any(|dependency| dependency["name"].as_str() == Some("concord-macros")),
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
        !has_metadata_dependency(macros, "concord-core", None),
        "concord_macros must remain runtime-neutral in normal dependencies"
    );
    assert!(
        !has_metadata_dependency(macros, "concord-core", Some("build")),
        "concord_macros must remain runtime-neutral in build dependencies"
    );
    assert!(
        !has_metadata_dependency(macros, "concord-core", Some("dev")),
        "generated public tests belong to the façade; macros must not retain a core dev edge"
    );
}

#[test]
fn transport_stack_exception_is_h2_dev_only() {
    for package in ["concord-core", "concord-macros"] {
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

    let core_dev = direct_tree("concord-core", "dev", "dependency-policy/core-development");
    assert!(
        has_direct_package(&core_dev, "h2"),
        "the isolated REFUSED_STREAM probes require the reviewed h2 dev dependency\n{core_dev}"
    );
}

#[test]
fn trybuild_is_confined_to_public_contract_test_development_dependencies() {
    for package in ["concord-core", "concord"] {
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

    let macros = direct_tree(
        "concord-macros",
        "all",
        "dependency-policy/concord-macros-no-trybuild",
    );
    assert!(
        !has_direct_package(&macros, "trybuild"),
        "macro-only unit tests must not compile Trybuild\n{macros}"
    );
}

#[test]
fn core_transport_features_activate_only_their_reqwest_capabilities() {
    for enabled in ["gzip", "brotli", "deflate"] {
        let tree = reqwest_feature_tree(
            enabled,
            &format!("dependency-policy/reqwest-compression-{enabled}"),
        );
        for candidate in ["gzip", "brotli", "deflate"] {
            let marker = format!("reqwest feature \"{candidate}\"");
            assert_eq!(
                tree.contains(&marker),
                candidate == enabled,
                "feature {enabled} had an unexpected Reqwest compression edge for {candidate}\n{tree}"
            );
        }
    }

    let multipart = reqwest_feature_tree("multipart", "dependency-policy/reqwest-multipart");
    for expected in [
        "reqwest feature \"multipart\"",
        "reqwest feature \"stream\"",
    ] {
        assert!(
            multipart.contains(expected),
            "multipart feature tree omitted {expected}\n{multipart}"
        );
    }
    for forbidden in [
        "reqwest feature \"json\"",
        "reqwest feature \"gzip\"",
        "reqwest feature \"brotli\"",
        "reqwest feature \"deflate\"",
        "reqwest feature \"http2\"",
        "reqwest feature \"rustls",
    ] {
        assert!(
            !multipart.contains(forbidden),
            "multipart activated unrelated Reqwest capability {forbidden}\n{multipart}"
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
    let core = package_metadata(&metadata, "concord-core");
    let extras = package_metadata(&metadata, "concord-extras");
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
    let core = package_metadata(&metadata, "concord-core");
    let core_features = core["features"].as_object().expect("core features");
    for moved in ["json", "csv", "ndjson", "oauth2-client"] {
        assert!(
            !core_features.contains_key(moved),
            "core retains moved feature {moved}"
        );
    }

    let macros = direct_tree(
        "concord-macros",
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
    let core = package_metadata(&metadata, "concord-core");
    let extras = package_metadata(&metadata, "concord-extras");
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
