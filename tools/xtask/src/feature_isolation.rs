//! Clippy coverage without workspace feature unification.
//!
//! Cargo's metadata feature map includes implicit optional-dependency features,
//! so newly added features automatically become separate rows of this gate.

use std::path::Path;
use std::process::Command;

use cargo_metadata::{Metadata, MetadataCommand};

const PRODUCT_PACKAGES: &[&str] = &["concord-core", "concord-extras", "concord"];

#[derive(Debug, PartialEq, Eq)]
struct FeatureCheck {
    package: String,
    feature: Option<String>,
}

impl FeatureCheck {
    fn arguments(&self) -> Vec<&str> {
        let mut arguments = vec![
            "clippy",
            "-p",
            self.package.as_str(),
            "--all-targets",
            "--no-default-features",
            "--locked",
        ];
        if let Some(feature) = &self.feature {
            arguments.extend(["--features", feature.as_str()]);
        }
        arguments.extend(["--", "-D", "warnings"]);
        arguments
    }
}

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let metadata = MetadataCommand::new()
        .current_dir(root)
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--locked".into()])
        .exec()
        .map_err(|error| format!("could not read feature-isolation Cargo metadata: {error}"))?;
    let checks = feature_checks(&metadata)?;
    for (index, check) in checks.iter().enumerate() {
        let arguments = check.arguments();
        let description = format!("cargo {}", arguments.join(" "));
        println!(
            "feature isolation {}/{}: {description}",
            index + 1,
            checks.len()
        );
        let status = Command::new(env!("CARGO"))
            .current_dir(root)
            .args(arguments)
            .status()
            .map_err(|error| format!("could not run `{description}`: {error}"))?;
        if !status.success() {
            return Err(format!(
                "feature isolation failed ({status}): {description}"
            ));
        }
    }
    println!(
        "feature isolation passed ({} product/feature rows)",
        checks.len()
    );
    Ok(())
}

fn feature_checks(metadata: &Metadata) -> Result<Vec<FeatureCheck>, String> {
    let mut checks = Vec::new();
    for name in PRODUCT_PACKAGES {
        let package = metadata
            .packages
            .iter()
            .find(|package| {
                package.name.as_str() == *name && metadata.workspace_members.contains(&package.id)
            })
            .ok_or_else(|| format!("feature isolation requires workspace package `{name}`"))?;
        checks.push(FeatureCheck {
            package: (*name).to_owned(),
            feature: None,
        });
        // Skip the `default` aggregate, not the individual features it enables.
        // BTreeMap iteration gives each package a stable feature order.
        checks.extend(
            package
                .features
                .keys()
                .filter(|feature| feature.as_str() != "default")
                .map(|feature| FeatureCheck {
                    package: (*name).to_owned(),
                    feature: Some(feature.clone()),
                }),
        );
    }
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{FeatureCheck, Metadata, PRODUCT_PACKAGES, feature_checks};

    fn metadata() -> Metadata {
        // This is Cargo metadata's feature representation: the optional
        // dependency's implicit feature is present just like explicit features.
        let packages = [
            ("concord-extras", json!({"default": [], "json": []})),
            ("concord-xtask", json!({"examples": []})),
            (
                "concord",
                json!({"default": ["http2"], "http2": ["concord_core/http2"]}),
            ),
            (
                "concord-core",
                json!({
                    "default": [],
                    "records": [],
                    "optional-integration": ["dep:optional-integration"],
                }),
            ),
        ]
        .into_iter()
        .map(|(name, features)| {
            json!({
                "name": name,
                "version": "1.0.0",
                "id": name,
                "manifest_path": format!("/workspace/{name}/Cargo.toml"),
                "dependencies": [],
                "targets": [],
                "features": features,
            })
        })
        .collect::<Vec<_>>();
        let members = packages
            .iter()
            .map(|package| package["id"].clone())
            .collect::<Vec<Value>>();
        serde_json::from_value(json!({
            "packages": packages,
            "workspace_members": members,
            "workspace_root": "/workspace",
            "target_directory": "/workspace/target",
            "version": 1,
        }))
        .expect("metadata fixture")
    }

    #[test]
    fn metadata_features_include_empty_implicit_and_default_enabled_rows() {
        let metadata = metadata();
        let checks = feature_checks(&metadata).expect("feature matrix");
        let actual = checks
            .iter()
            .map(|check| (check.package.as_str(), check.feature.as_deref()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [
                ("concord-core", None),
                ("concord-core", Some("optional-integration")),
                ("concord-core", Some("records")),
                ("concord-extras", None),
                ("concord-extras", Some("json")),
                ("concord", None),
                ("concord", Some("http2")),
            ]
        );
    }

    #[test]
    fn each_new_manifest_feature_is_automatically_a_separate_row() {
        let mut metadata = metadata();
        for package in &mut metadata.packages {
            package.features.insert("new-feature".into(), Vec::new());
        }
        let checks = feature_checks(&metadata).expect("feature matrix");
        let added = checks
            .iter()
            .filter(|check| check.feature.as_deref() == Some("new-feature"))
            .map(|check| check.package.as_str())
            .collect::<Vec<_>>();
        assert_eq!(added, PRODUCT_PACKAGES);
    }

    #[test]
    fn every_command_is_one_locked_package_with_defaults_disabled() {
        for check in feature_checks(&metadata()).expect("feature matrix") {
            let arguments = check.arguments();
            assert_eq!(
                &arguments[..6],
                [
                    "clippy",
                    "-p",
                    check.package.as_str(),
                    "--all-targets",
                    "--no-default-features",
                    "--locked",
                ]
            );
            assert_eq!(arguments.iter().filter(|arg| **arg == "-p").count(), 1);
            assert!(!arguments.contains(&"--workspace"));
            assert!(!arguments.contains(&"--all-features"));
            let mut suffix = Vec::new();
            if let Some(feature) = &check.feature {
                suffix.extend(["--features", feature.as_str()]);
            }
            suffix.extend(["--", "-D", "warnings"]);
            assert_eq!(&arguments[6..], suffix);
        }
    }

    #[test]
    fn missing_product_or_non_workspace_package_cannot_silently_reduce_coverage() {
        let mut metadata = metadata();
        let core_id = metadata
            .packages
            .iter()
            .find(|package| package.name.as_str() == "concord-core")
            .expect("core package")
            .id
            .clone();
        metadata.workspace_members.retain(|id| *id != core_id);
        let error = feature_checks(&metadata).expect_err("non-workspace core");
        assert!(error.contains("concord-core"), "{error}");
        metadata.packages.retain(|package| package.id != core_id);
        let error = feature_checks(&metadata).expect_err("missing core");
        assert!(error.contains("concord-core"), "{error}");
    }

    #[test]
    fn empty_row_has_no_feature_argument() {
        let check = FeatureCheck {
            package: "concord".into(),
            feature: None,
        };
        assert_eq!(
            check.arguments(),
            [
                "clippy",
                "-p",
                "concord",
                "--all-targets",
                "--no-default-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ]
        );
    }
}
