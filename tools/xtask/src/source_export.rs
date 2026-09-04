//! Clean, exact-commit source handoff creation and extraction audit.

use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path};
use std::process::Command;

const RETIRED_PROCESS_FILES: &[&str] = &[
    "CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md",
    "concord_v2_1_second_release_readiness_audit.md",
    "PROMPT_CONCORD_V2_1_FINAL_INTEGRATION_AND_LOCAL_BETA_MERGE.md",
    "concord_v2_1_final_integration_audit.md",
];

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let head = git_output(root, &["rev-parse", "HEAD"])?;
    let status = git_output(root, &["status", "--porcelain", "--untracked-files=all"])?;
    if !status.is_empty() {
        return Err(format!(
            "source export requires a clean exact commit; git status reported:\n{status}"
        ));
    }

    let short = head
        .get(..12)
        .ok_or_else(|| format!("unexpected Git object ID `{head}`"))?;
    let export_root = root.join("target/source-export");
    fs::create_dir_all(&export_root).map_err(|error| {
        format!(
            "could not create source-export directory {}: {error}",
            export_root.display()
        )
    })?;
    let unique = std::process::id();
    let archive_prefix = format!("concord-source-{short}/");
    let archive = export_root.join(format!("concord-source-{short}-{unique}.tar"));
    let manifest_root = export_root.join(format!("manifest-{unique}"));
    let extraction_root = export_root.join(format!("verify-{unique}"));
    fs::create_dir(&manifest_root).map_err(|error| {
        format!(
            "could not create manifest staging directory {}: {error}",
            manifest_root.display()
        )
    })?;
    fs::create_dir(&extraction_root).map_err(|error| {
        format!(
            "could not create extraction directory {}: {error}",
            extraction_root.display()
        )
    })?;

    let manifest = manifest_root.join("SOURCE_MANIFEST.txt");
    let manifest_body = format!(
        "Concord source export\ncommit={head}\nclean_tree=true\narchive_prefix={archive_prefix}\n"
    );
    fs::write(&manifest, &manifest_body)
        .map_err(|error| format!("could not write {}: {error}", manifest.display()))?;

    let archive_arguments = vec![
        OsString::from("archive"),
        OsString::from("--format=tar"),
        OsString::from(format!("--prefix={archive_prefix}")),
        OsString::from(format!("--add-file={}", manifest.display())),
        OsString::from("--output"),
        archive.as_os_str().to_owned(),
        OsString::from("HEAD"),
    ];
    command_success(root, "git", &archive_arguments)?;
    command_success(
        root,
        "tar",
        &[
            OsString::from("-xf"),
            archive.as_os_str().to_owned(),
            OsString::from("-C"),
            extraction_root.as_os_str().to_owned(),
        ],
    )?;

    let extracted = extraction_root.join(archive_prefix.trim_end_matches('/'));
    let extracted_manifest = extracted.join("SOURCE_MANIFEST.txt");
    let observed_manifest = fs::read_to_string(&extracted_manifest).map_err(|error| {
        format!(
            "source archive did not extract its manifest {}: {error}",
            extracted_manifest.display()
        )
    })?;
    if observed_manifest != manifest_body {
        return Err("source archive manifest differs after extraction".to_owned());
    }

    let entries = audit_extracted_tree(&extracted)?;
    let final_status = git_output(root, &["status", "--porcelain", "--untracked-files=all"])?;
    if !final_status.is_empty() {
        return Err(format!(
            "source export changed the candidate worktree status:\n{final_status}"
        ));
    }

    remove_generated_directory(&export_root, &manifest_root)?;
    remove_generated_directory(&export_root, &extraction_root)?;
    println!("source-export exact HEAD: {head}");
    println!("source-export archive: {}", archive.display());
    println!("source-export manifest: clean_tree=true, commit={head}");
    println!("source-export extracted and audited {entries} entries");
    println!("source-export excludes Git/build/temp/editor/package-staging/process artifacts");
    Ok(())
}

fn audit_extracted_tree(root: &Path) -> Result<usize, String> {
    if !root.is_dir() {
        return Err(format!(
            "source archive is missing its expected root {}",
            root.display()
        ));
    }
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("could not enumerate {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| {
                format!(
                    "could not inspect an entry under {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("could not relativize {}: {error}", path.display()))?;
            if let Some(reason) = forbidden_export_path(relative) {
                return Err(format!(
                    "source archive contains forbidden path {}: {reason}",
                    relative.display()
                ));
            }
            entries += 1;
            let kind = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if kind.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(entries)
}

fn forbidden_export_path(path: &Path) -> Option<&'static str> {
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Some("non-normal path component");
        };
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            ".git"
                | "target"
                | "wip"
                | "__pycache__"
                | ".idea"
                | ".vscode"
                | "package-verification"
                | "package-staging"
        ) {
            return Some("repository/build/editor/package staging directory");
        }
    }
    let file_name = path.file_name()?.to_string_lossy();
    if RETIRED_PROCESS_FILES.contains(&file_name.as_ref()) {
        return Some("retired external process kit");
    }
    if file_name == ".DS_Store"
        || file_name.ends_with('~')
        || ["tmp", "temp", "swp", "swo", "bak", "pyc"]
            .iter()
            .any(|extension| path.extension().is_some_and(|value| value == *extension))
    {
        return Some("temporary, generated, or editor file");
    }
    None
}

fn remove_generated_directory(export_root: &Path, path: &Path) -> Result<(), String> {
    if path == export_root || !path.starts_with(export_root) {
        return Err(format!(
            "refusing to remove unverified generated path {}",
            path.display()
        ));
    }
    fs::remove_dir_all(path).map_err(|error| {
        format!(
            "could not remove generated directory {}: {error}",
            path.display()
        )
    })
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run git {}: {error}", arguments.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn command_success(root: &Path, program: &str, arguments: &[OsString]) -> Result<(), String> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed with {}:\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::forbidden_export_path;
    use std::path::Path;

    #[test]
    fn export_path_filter_has_positive_and_negative_space() {
        for rejected in [
            "fixtures/target/debug/object.o",
            ".git/config",
            ".vscode/settings.json",
            "dev_doc/history/concord_v2_1_second_release_readiness_audit.md",
            "scratch.tmp",
        ] {
            assert!(
                forbidden_export_path(Path::new(rejected)).is_some(),
                "not rejected: {rejected}"
            );
        }
        for accepted in [
            "Cargo.toml",
            "crates/concord/src/lib.rs",
            "dev_doc/adrs/0001-representation-ownership.md",
        ] {
            assert_eq!(
                forbidden_export_path(Path::new(accepted)),
                None,
                "unexpected rejection: {accepted}"
            );
        }
    }
}
