use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package};

const PRODUCT_PACKAGES: &[&str] = &[
    "concord-core",
    "concord-extras",
    "concord-macros",
    "concord",
];

const FORBIDDEN_PROCESS_ARTIFACTS: &[&str] = &[
    ".codex",
    ".config/nextest.toml",
    "docs/contributor/evidence",
    "crates/concord/wip",
    "crates/concord-core/wip",
    "crates/concord-macros/wip",
];

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let metadata = metadata(root.join("Cargo.toml"))?;
    check_dependency_direction(&metadata)?;
    check_versions(&metadata)?;
    check_process_artifacts(root)?;
    check_generated_paths(&root.join("crates/concord-macros/src/emit"))?;
    println!("repository policy checks passed");
    Ok(())
}

fn metadata(manifest: PathBuf) -> Result<Metadata, String> {
    let mut command = MetadataCommand::new();
    command
        .manifest_path(manifest)
        .no_deps()
        .other_options(vec!["--locked".into(), "--offline".into()]);
    command
        .exec()
        .map_err(|error| format!("could not read Cargo metadata: {error}"))
}

fn check_dependency_direction(metadata: &Metadata) -> Result<(), String> {
    let allowed = BTreeMap::from([
        ("concord-core", BTreeSet::new()),
        ("concord-extras", BTreeSet::from(["concord-core"])),
        ("concord-macros", BTreeSet::new()),
        (
            "concord",
            BTreeSet::from(["concord-core", "concord-extras", "concord-macros"]),
        ),
        ("concord-examples", BTreeSet::from(["concord"])),
    ]);

    for (name, allowed_siblings) in allowed {
        let package = package(metadata, name)?;
        let actual = normal_dependencies(package)
            .filter(|dependency| PRODUCT_PACKAGES.contains(&dependency.name.as_str()))
            .map(|dependency| dependency.name.as_str())
            .collect::<BTreeSet<_>>();
        if actual != allowed_siblings {
            return Err(format!(
                "{name} has invalid normal Concord dependencies: expected {allowed_siblings:?}, found {actual:?}"
            ));
        }
        if normal_dependencies(package)
            .any(|dependency| dependency.name.as_str() == "concord-test-support")
        {
            return Err(format!(
                "{name} has a normal dependency on concord-test-support"
            ));
        }
    }

    let macros = package(metadata, "concord-macros")?;
    if normal_dependencies(macros)
        .any(|dependency| matches!(dependency.name.as_str(), "concord-core" | "concord-extras"))
    {
        return Err("concord-macros must remain independent of core and extras".to_owned());
    }
    Ok(())
}

fn check_versions(metadata: &Metadata) -> Result<(), String> {
    let versions = metadata
        .packages
        .iter()
        .filter(|package| PRODUCT_PACKAGES.contains(&package.name.as_str()))
        .map(|package| (package.name.as_str(), package.version.to_string()))
        .collect::<BTreeMap<_, _>>();

    for package in &metadata.packages {
        for dependency in &package.dependencies {
            if dependency.name.as_str() == "concord-test-support" {
                if dependency.path.is_none() || dependency.req.to_string() != "*" {
                    return Err(format!(
                        "{} must use an unversioned path-only concord-test-support dependency",
                        package.name
                    ));
                }
                continue;
            }
            if let Some(version) = versions.get(dependency.name.as_str()) {
                let expected = format!("={version}");
                if dependency.req.to_string() != expected {
                    return Err(format!(
                        "{} must require sibling {} with {expected}",
                        package.name, dependency.name
                    ));
                }
            } else if dependency.req.to_string().starts_with('=') {
                return Err(format!(
                    "{} pins external dependency {} exactly; use a compatible semver requirement",
                    package.name, dependency.name
                ));
            }
        }
    }
    Ok(())
}

fn package<'a>(metadata: &'a Metadata, name: &str) -> Result<&'a Package, String> {
    metadata
        .packages
        .iter()
        .find(|package| package.name.as_str() == name)
        .ok_or_else(|| format!("Cargo metadata omitted workspace package {name}"))
}

fn normal_dependencies(package: &Package) -> impl Iterator<Item = &cargo_metadata::Dependency> {
    package
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind == DependencyKind::Normal)
}

fn check_process_artifacts(root: &Path) -> Result<(), String> {
    let present = FORBIDDEN_PROCESS_ARTIFACTS
        .iter()
        .filter(|relative| root.join(relative).exists())
        .copied()
        .collect::<Vec<_>>();
    if present.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "forbidden process artifacts remain: {}",
            present.join(", ")
        ))
    }
}

fn check_generated_paths(directory: &Path) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            check_generated_paths(&path)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if source.contains("::concord_core") || source.contains(":: concord_core") {
                return Err(format!(
                    "{} can emit a direct concord_core path",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}
