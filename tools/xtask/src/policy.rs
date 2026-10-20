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
    "dev_doc/evidence/raw",
    "crates/concord/wip",
    "crates/concord-core/wip",
    "crates/concord-macros/wip",
];

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let metadata = metadata(root.join("Cargo.toml"))?;
    check_dependency_direction(&metadata)?;
    check_versions(&metadata)?;
    check_licenses(root, &metadata)?;
    check_release_stage(metadata.packages.iter().map(|package| {
        ReleaseFacts {
            name: package.name.as_str(),
            version: &package.version,
            // `publish = false` is `Some([])`; an absent key allows every registry.
            publishable: package
                .publish
                .as_ref()
                .is_none_or(|registries| !registries.is_empty()),
        }
    }))?;
    check_process_artifacts(root)?;
    check_generated_paths(&root.join("crates/concord-macros/src/emit"))?;
    let limit_copies = crate::limit_drift::check(root)?;
    let justfile = fs::read_to_string(root.join("justfile"))
        .map_err(|error| format!("could not read justfile: {error}"))?;
    check_locked_recipes(&justfile)?;
    println!(
        "repository policy checks passed ({limit_copies} macro limit copies match concord-core; verify-implementation cargo commands are --locked)"
    );
    Ok(())
}

/// Keep the owner-selected SPDX grant and the texts shipped by public crates
/// aligned with the repository. Third-party dependency grants remain separate.
fn check_licenses(root: &Path, metadata: &Metadata) -> Result<(), String> {
    for package in &metadata.packages {
        if package.license.as_deref() != Some("MIT OR Apache-2.0") {
            return Err(format!(
                "{} must declare the workspace license `MIT OR Apache-2.0`",
                package.name
            ));
        }
    }
    for filename in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let canonical = fs::read(root.join(filename))
            .map_err(|error| format!("could not read {filename}: {error}"))?;
        if canonical.is_empty() {
            return Err(format!("{filename} is empty"));
        }
        for name in PRODUCT_PACKAGES {
            let manifest = &package(metadata, name)?.manifest_path;
            let directory = manifest
                .parent()
                .ok_or_else(|| format!("{name} has no package directory"))?;
            let path = directory.join(filename);
            let text =
                fs::read(&path).map_err(|error| format!("could not read {path}: {error}"))?;
            if text != canonical {
                return Err(format!("{path} differs from the root {filename}"));
            }
        }
    }
    Ok(())
}

/// The recipe whose closure is the complete machine validation (§28.2).
const VERIFY_RECIPE: &str = "verify-implementation";

/// FEATURE-01 (§20.2): every cargo command reachable from
/// `verify-implementation` resolves dependencies with `--locked`. `cargo fmt`
/// resolves nothing and `--version` probes run no build.
fn check_locked_recipes(justfile: &str) -> Result<(), String> {
    let recipes = justfile_recipes(justfile);
    let Some(dependencies) = recipes.get(VERIFY_RECIPE).map(|recipe| &recipe.0) else {
        return Err(format!("justfile has no `{VERIFY_RECIPE}` recipe"));
    };
    let mut unlocked = Vec::new();
    for name in dependencies {
        let Some((_, commands)) = recipes.get(name.as_str()) else {
            return Err(format!(
                "`{VERIFY_RECIPE}` depends on the unknown recipe `{name}`"
            ));
        };
        for command in commands {
            let words = command.split_whitespace().collect::<Vec<_>>();
            let cargo = words.first() == Some(&"cargo");
            let exempt =
                words.get(1).is_some_and(|word| *word == "fmt") || words.contains(&"--version");
            // Arguments after a bare `--` belong to the invoked program.
            let locked = words
                .iter()
                .take_while(|word| **word != "--")
                .any(|word| *word == "--locked");
            if cargo && !exempt && !locked {
                unlocked.push(format!("{name}: {command}"));
            }
        }
    }
    if unlocked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "justfile cargo commands without --locked (FEATURE-01):\n{}",
            unlocked.join("\n")
        ))
    }
}

/// `recipe -> (dependencies, commands)` of a justfile (settings, comments
/// and platform attributes are skipped).
fn justfile_recipes(justfile: &str) -> BTreeMap<String, (Vec<String>, Vec<String>)> {
    let mut recipes = BTreeMap::<String, (Vec<String>, Vec<String>)>::new();
    let mut current: Option<String> = None;
    for line in justfile.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(name) = &current {
                let command = line.trim();
                if !command.is_empty() && !command.starts_with('#') {
                    recipes
                        .entry(name.clone())
                        .or_default()
                        .1
                        .push(command.trim_start_matches('@').to_owned());
                }
            }
            continue;
        }
        current = None;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        if trimmed.starts_with("set ") || trimmed.contains(":=") {
            continue;
        }
        if let Some((name, dependencies)) = trimmed.split_once(':') {
            let name = name.trim().to_owned();
            let dependencies = dependencies
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            recipes.entry(name.clone()).or_default().0 = dependencies;
            current = Some(name);
        }
    }
    recipes
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
        // The inspection companion reads core descriptions only; generated
        // clients are exercised through a facade dev-dependency.
        ("concord-inspect", BTreeSet::from(["concord-core"])),
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

/// The release-relevant facts of one workspace package.
struct ReleaseFacts<'a> {
    name: &'a str,
    version: &'a cargo_metadata::semver::Version,
    publishable: bool,
}

/// Every workspace package shares one version, and while that version is a
/// pre-release (the v3 beta) no package may be publishable.
fn check_release_stage<'a>(packages: impl Iterator<Item = ReleaseFacts<'a>>) -> Result<(), String> {
    let packages = packages.collect::<Vec<_>>();
    let versions = packages
        .iter()
        .map(|package| package.version.to_string())
        .collect::<BTreeSet<_>>();
    if versions.len() > 1 {
        return Err(format!(
            "workspace packages must share one version, found {versions:?}"
        ));
    }
    let publishable = packages
        .iter()
        .filter(|package| !package.version.pre.is_empty() && package.publishable)
        .map(|package| format!("{} {}", package.name, package.version))
        .collect::<Vec<_>>();
    if publishable.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "pre-release packages must set `publish = false`: {}",
            publishable.join(", ")
        ))
    }
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

#[cfg(test)]
mod tests {
    use cargo_metadata::semver::Version;

    use super::{ReleaseFacts, check_locked_recipes, check_release_stage, justfile_recipes};

    const JUSTFILE: &str = "set shell := [\"bash\"]\n\n[windows]\nset shell := [\"pwsh\"]\n\ndefault:\n    @just --list\n\nfmt:\n    cargo fmt --all -- --check\n\ncheck:\n    cargo check --workspace --locked\n    cargo run --locked -p concord-xtask -- policy --locked\n\n# Complete validation.\nverify-implementation: fmt check test\n\ntest:\n    cargo nextest --version\n    cargo test --workspace --locked\n\nbench:\n    cargo bench --no-run\n";

    #[test]
    fn justfile_recipes_are_parsed_with_dependencies_and_commands() {
        let recipes = justfile_recipes(JUSTFILE);
        assert_eq!(recipes["verify-implementation"].0, ["fmt", "check", "test"]);
        assert_eq!(recipes["check"].1.len(), 2);
        assert_eq!(recipes["default"].1, ["just --list"]);
        assert!(!recipes.contains_key("set shell"));
    }

    /// FEATURE-01: only the verify-implementation closure is checked, and
    /// `cargo fmt` / `--version` probes are exempt.
    #[test]
    fn verify_implementation_cargo_commands_must_be_locked() {
        check_locked_recipes(JUSTFILE).expect("locked closure");
        let unlocked = JUSTFILE.replace(
            "cargo check --workspace --locked",
            "cargo check --workspace",
        );
        let error = check_locked_recipes(&unlocked).expect_err("unlocked check");
        assert!(error.contains("check: cargo check --workspace"), "{error}");
        // `--locked` after a bare `--` is an argument of the invoked tool.
        let misplaced = JUSTFILE.replace(
            "cargo check --workspace --locked",
            "cargo check --workspace -- --locked",
        );
        assert!(check_locked_recipes(&misplaced).is_err());
        let unknown = JUSTFILE.replace("fmt check test", "fmt check test lint");
        assert!(check_locked_recipes(&unknown).is_err());
    }

    #[test]
    fn repository_justfile_verify_closure_is_locked() {
        let justfile = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../justfile"),
        )
        .expect("justfile");
        check_locked_recipes(&justfile).expect("repository justfile");
    }

    fn facts<'a>(name: &'a str, version: &'a Version, publishable: bool) -> ReleaseFacts<'a> {
        ReleaseFacts {
            name,
            version,
            publishable,
        }
    }

    #[test]
    fn pre_release_workspace_must_be_unpublishable_and_share_one_version() {
        let beta = Version::parse("3.0.0-beta.1").expect("version");
        let stable = Version::parse("3.0.0").expect("version");
        check_release_stage(
            [
                facts("concord", &beta, false),
                facts("concord-core", &beta, false),
            ]
            .into_iter(),
        )
        .expect("beta workspace with publish = false");
        let error = check_release_stage(
            [
                facts("concord", &beta, false),
                facts("concord-core", &beta, true),
            ]
            .into_iter(),
        )
        .expect_err("publishable beta package");
        assert!(error.contains("concord-core 3.0.0-beta.1"), "{error}");
        let error = check_release_stage(
            [
                facts("concord", &beta, false),
                facts("concord-core", &stable, false),
            ]
            .into_iter(),
        )
        .expect_err("split versions");
        assert!(error.contains("share one version"), "{error}");
        check_release_stage([facts("concord", &stable, true)].into_iter())
            .expect("a stable release may be publishable");
    }
}
