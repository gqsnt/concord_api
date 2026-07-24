use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::MetadataCommand;
use serde::Serialize;
use sha2::{Digest, Sha256};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value, value};

use crate::command::cargo_status;

struct Package<'a> {
    name: &'a str,
    siblings: &'a [(&'a str, &'a str)],
}

const PACKAGES: &[Package<'_>] = &[
    Package {
        name: "concord-core",
        siblings: &[],
    },
    Package {
        name: "concord-macros",
        siblings: &[],
    },
    Package {
        name: "concord-extras",
        siblings: &[("concord_core", "concord-core")],
    },
    Package {
        name: "concord",
        siblings: &[
            ("concord_core", "concord-core"),
            ("concord_macros", "concord-macros"),
            ("concord_extras", "concord-extras"),
        ],
    },
];

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let version = product_version(root)?;
    let package_output = package_target_directory(root).join("package");
    let mut packaged = BTreeMap::<&str, PathBuf>::new();

    for package in PACKAGES {
        let (command_root, directory) = if package.siblings.is_empty() {
            cargo_status(
                root,
                ["package", "--locked", "--allow-dirty", "-p", package.name],
            )?;
            (
                root.to_owned(),
                package_output.join(format!("{}-{version}", package.name)),
            )
        } else {
            let simulation = prepare_simulation(root, package, &packaged, &version)?;
            cargo_status(
                &simulation,
                [
                    "generate-lockfile",
                    "--offline",
                    "--manifest-path",
                    "Cargo.toml",
                ],
            )?;
            prepare_vendored_source(&simulation, package, &packaged, &version)?;
            cargo_status(
                &simulation,
                [
                    "package",
                    "--locked",
                    "--allow-dirty",
                    "--manifest-path",
                    "Cargo.toml",
                    "-p",
                    package.name,
                ],
            )?;
            let output = package_target_directory(&simulation).join("package");
            (
                simulation,
                output.join(format!("{}-{version}", package.name)),
            )
        };
        verify_packaged_manifest(package, &version, &directory)?;

        if !package.siblings.is_empty() {
            let mut lock_arguments = sibling_patches(package.siblings, &packaged)?;
            lock_arguments.extend([
                OsString::from("generate-lockfile"),
                OsString::from("--manifest-path"),
                directory.join("Cargo.toml").into_os_string(),
                OsString::from("--offline"),
            ]);
            cargo_status(&command_root, lock_arguments)?;
        }
        let mut check_arguments = sibling_patches(package.siblings, &packaged)?;
        check_arguments.extend([
            OsString::from("check"),
            OsString::from("--manifest-path"),
            directory.join("Cargo.toml").into_os_string(),
            OsString::from("--lib"),
            OsString::from("--all-features"),
            OsString::from("--locked"),
        ]);
        cargo_status(&command_root, check_arguments)?;
        packaged.insert(package.name, directory);
    }

    println!(
        "package verification passed in order: concord-core, concord-macros, concord-extras, concord"
    );
    Ok(())
}

fn prepare_simulation(
    root: &Path,
    package: &Package<'_>,
    packaged: &BTreeMap<&str, PathBuf>,
    version: &str,
) -> Result<PathBuf, String> {
    let simulation = package_target_directory(root)
        .join("package-simulation")
        .join(package.name);
    let simulation_parent = package_target_directory(root).join("package-simulation");
    if !simulation.starts_with(&simulation_parent) {
        return Err(format!(
            "package simulation escaped its target directory: {}",
            simulation.display()
        ));
    }
    if simulation.exists() {
        fs::remove_dir_all(&simulation)
            .map_err(|error| format!("could not reset {}: {error}", simulation.display()))?;
    }
    fs::create_dir_all(simulation.join("crates"))
        .map_err(|error| format!("could not create {}: {error}", simulation.display()))?;

    copy_tree(
        &root.join("crates").join(package.name),
        &simulation.join("crates").join(package.name),
    )?;
    copy_tree(
        &root.join("crates/concord-test-support"),
        &simulation.join("crates/concord-test-support"),
    )?;
    for (_, sibling) in package.siblings {
        let source = packaged.get(sibling).ok_or_else(|| {
            format!(
                "packaged dependency {sibling} must exist before {}",
                package.name
            )
        })?;
        copy_tree(source, &simulation.join("crates").join(sibling))?;
    }

    let root_manifest = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("could not read root Cargo.toml: {error}"))?;
    let mut manifest = root_manifest
        .parse::<DocumentMut>()
        .map_err(|error| format!("root Cargo.toml is not valid TOML: {error}"))?;
    let workspace = manifest
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| "root manifest has no workspace table".to_owned())?;
    workspace["members"] = value(string_array([
        format!("crates/{}", package.name),
        "crates/concord-test-support".to_owned(),
    ]));
    workspace["default-members"] = value(string_array([format!("crates/{}", package.name)]));
    workspace["exclude"] = value(string_array(
        package
            .siblings
            .iter()
            .map(|(_, sibling)| format!("crates/{sibling}")),
    ));

    let mut crates_io = Table::new();
    for (_, sibling) in package.siblings {
        let mut dependency = InlineTable::new();
        dependency.insert("path", Value::from(format!("crates/{sibling}")));
        crates_io.insert(sibling, Item::Value(Value::InlineTable(dependency)));
    }
    let mut patch = Table::new();
    patch.insert("crates-io", Item::Table(crates_io));
    manifest.insert("patch", Item::Table(patch));

    let workspace_package = manifest
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .and_then(|workspace| workspace.get_mut("package"))
        .and_then(Item::as_table_mut);
    if let Some(workspace_package) = workspace_package {
        workspace_package["version"] = value(version);
    }
    fs::write(simulation.join("Cargo.toml"), manifest.to_string())
        .map_err(|error| format!("could not write simulation manifest: {error}"))?;
    Ok(simulation)
}

fn prepare_vendored_source(
    simulation: &Path,
    package: &Package<'_>,
    packaged: &BTreeMap<&str, PathBuf>,
    version: &str,
) -> Result<(), String> {
    cargo_status(
        simulation,
        [
            "vendor",
            "--locked",
            "--versioned-dirs",
            "--quiet",
            "vendor",
        ],
    )?;

    let vendor = simulation.join("vendor");
    for (_, sibling) in package.siblings {
        let source = packaged.get(sibling).ok_or_else(|| {
            format!(
                "packaged dependency {sibling} must exist before {}",
                package.name
            )
        })?;
        let destination = vendor.join(format!("{sibling}-{version}"));
        copy_tree(source, &destination)?;
        write_cargo_checksum(source, &destination, sibling, version)?;
    }

    let cargo_directory = simulation.join(".cargo");
    fs::create_dir_all(&cargo_directory)
        .map_err(|error| format!("could not create {}: {error}", cargo_directory.display()))?;
    fs::write(
        cargo_directory.join("config.toml"),
        "[source.crates-io]\nreplace-with = \"packaged-sources\"\n\
         \n[source.packaged-sources]\ndirectory = \"vendor\"\n",
    )
    .map_err(|error| format!("could not write vendored source configuration: {error}"))?;
    Ok(())
}

#[derive(Serialize)]
struct CargoChecksum {
    files: BTreeMap<String, String>,
    package: String,
}

fn write_cargo_checksum(
    packaged_directory: &Path,
    vendor_directory: &Path,
    package: &str,
    version: &str,
) -> Result<(), String> {
    let archive = packaged_directory
        .parent()
        .ok_or_else(|| {
            format!(
                "{} has no package output parent",
                packaged_directory.display()
            )
        })?
        .join(format!("{package}-{version}.crate"));
    let checksum = CargoChecksum {
        files: file_checksums(vendor_directory, vendor_directory)?,
        package: sha256(&archive)?,
    };
    let encoded = serde_json::to_vec(&checksum)
        .map_err(|error| format!("could not encode checksum for {package}: {error}"))?;
    fs::write(vendor_directory.join(".cargo-checksum.json"), encoded)
        .map_err(|error| format!("could not write checksum for {package}: {error}"))
}

fn file_checksums(root: &Path, directory: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut checksums = BTreeMap::new();
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if entry.file_name() == ".cargo-checksum.json" {
            continue;
        }
        if path.is_dir() {
            checksums.extend(file_checksums(root, &path)?);
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let relative = relative
                .components()
                .map(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .ok_or_else(|| format!("non-UTF-8 package path: {}", path.display()))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            checksums.insert(relative, sha256(&path)?);
        }
    }
    Ok(checksums)
}

fn sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn string_array(values: impl IntoIterator<Item = String>) -> Array {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    array
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("could not create {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source).map_err(|error| format!("{}: {error}", source.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() != "target" {
                copy_tree(&path, &target)?;
            }
        } else {
            fs::copy(&path, &target).map_err(|error| {
                format!(
                    "could not copy {} to {}: {error}",
                    path.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

fn product_version(root: &Path) -> Result<String, String> {
    let mut command = MetadataCommand::new();
    command
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--locked".into(), "--offline".into()]);
    let metadata = command
        .exec()
        .map_err(|error| format!("could not read Cargo metadata: {error}"))?;
    let mut version = None;
    for package in &metadata.packages {
        if PACKAGES
            .iter()
            .any(|product| product.name == package.name.as_str())
        {
            let current = package.version.to_string();
            if version.as_ref().is_some_and(|version| version != &current) {
                return Err("Concord product packages do not share one version".to_owned());
            }
            version = Some(current);
        }
    }
    version.ok_or_else(|| "Cargo metadata omitted Concord product packages".to_owned())
}

fn sibling_patches(
    siblings: &[(&str, &str)],
    packaged: &BTreeMap<&str, PathBuf>,
) -> Result<Vec<OsString>, String> {
    let mut arguments = Vec::new();
    for (_, sibling) in siblings {
        let path = packaged.get(sibling).ok_or_else(|| {
            format!("packaged dependency {sibling} must be verified before its consumer")
        })?;
        arguments.push(OsString::from("--config"));
        arguments.push(OsString::from(format!(
            "patch.crates-io.{sibling}.path={}",
            toml_path_literal(path)?
        )));
    }
    Ok(arguments)
}

fn package_target_directory(root: &Path) -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => root.join("target"),
    }
}

fn toml_path_literal(path: &Path) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))?
        .to_string_lossy()
        .into_owned();
    let canonical = if let Some(path) = canonical.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        canonical
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical)
            .to_owned()
    };
    Ok(Value::from(canonical.replace('\\', "/")).to_string())
}

fn verify_packaged_manifest(
    package: &Package<'_>,
    version: &str,
    directory: &Path,
) -> Result<(), String> {
    let manifest_path = directory.join("Cargo.toml");
    let original_path = directory.join("Cargo.toml.orig");
    let package_output = directory
        .parent()
        .ok_or_else(|| format!("{} has no package output parent", directory.display()))?;
    let archive_path = package_output.join(format!("{}-{version}.crate", package.name));
    for required in [&manifest_path, &original_path, &archive_path] {
        if !required.is_file() {
            return Err(format!(
                "{} packaging omitted {}",
                package.name,
                required.display()
            ));
        }
    }

    let text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
    let manifest = text
        .parse::<DocumentMut>()
        .map_err(|error| format!("{} is not valid TOML: {error}", manifest_path.display()))?;
    let package_table = manifest
        .get("package")
        .and_then(Item::as_table)
        .ok_or_else(|| {
            format!(
                "{} has no normalized package table",
                manifest_path.display()
            )
        })?;
    require_value(package_table, "name", package.name, &manifest_path)?;
    require_value(package_table, "version", version, &manifest_path)?;
    if package_table.get("publish").and_then(Item::as_bool) != Some(false) {
        return Err(format!(
            "{} must retain `publish = false`",
            manifest_path.display()
        ));
    }

    check_dependency_tables(&manifest, &manifest_path)?;
    let dependencies = manifest.get("dependencies").and_then(Item::as_table);
    for (alias, sibling) in package.siblings {
        let dependency = dependencies
            .and_then(|table| table.get(alias))
            .ok_or_else(|| {
                format!(
                    "{} omitted normalized sibling dependency {sibling}",
                    manifest_path.display()
                )
            })?;
        if dependency_field(dependency, "package") != Some(*sibling) {
            return Err(format!(
                "{} sibling alias {alias} does not name package {sibling}",
                manifest_path.display()
            ));
        }
        let exact = format!("={version}");
        if dependency_field(dependency, "version") != Some(exact.as_str()) {
            return Err(format!(
                "{} sibling {sibling} must retain exact version {exact}",
                manifest_path.display()
            ));
        }
    }
    Ok(())
}

fn require_value(table: &Table, key: &str, expected: &str, manifest: &Path) -> Result<(), String> {
    if table.get(key).and_then(Item::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(format!(
            "{} must contain `{key} = {expected:?}`",
            manifest.display()
        ))
    }
}

fn check_dependency_tables(manifest: &DocumentMut, path: &Path) -> Result<(), String> {
    visit_table(manifest.as_table(), path)
}

fn visit_table(table: &Table, path: &Path) -> Result<(), String> {
    for (key, item) in table {
        let dependencies = if matches!(
            key,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            item.as_table()
        } else {
            None
        };
        if let Some(dependencies) = dependencies {
            for (alias, dependency) in dependencies {
                if alias == "concord-test-support"
                    || dependency_field(dependency, "package") == Some("concord-test-support")
                {
                    return Err(format!(
                        "{} retained a concord-test-support dependency",
                        path.display()
                    ));
                }
                if dependency_field(dependency, "path").is_some() {
                    return Err(format!(
                        "{} retained a path for dependency {alias}",
                        path.display()
                    ));
                }
            }
        }
        if let Some(child) = item.as_table() {
            visit_table(child, path)?;
        }
    }
    Ok(())
}

fn dependency_field<'a>(item: &'a Item, field: &str) -> Option<&'a str> {
    if let Some(table) = item.as_table() {
        return table.get(field).and_then(Item::as_str);
    }
    item.as_inline_table()
        .and_then(|table: &InlineTable| table.get(field))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use toml_edit::DocumentMut;

    use super::check_dependency_tables;

    #[test]
    fn normalized_manifest_rejects_test_support_and_paths() {
        let support = r#"
[package]
name = "sample"
[dev-dependencies.concord_test_support]
package = "concord-test-support"
"#
        .parse::<DocumentMut>()
        .expect("support manifest");
        assert!(
            check_dependency_tables(&support, "Cargo.toml".as_ref())
                .expect_err("support must fail")
                .contains("concord-test-support")
        );

        let path = r#"
[package]
name = "sample"
[dependencies.sibling]
path = "../sibling"
"#
        .parse::<DocumentMut>()
        .expect("path manifest");
        assert!(
            check_dependency_tables(&path, "Cargo.toml".as_ref())
                .expect_err("path must fail")
                .contains("retained a path")
        );
    }
}
