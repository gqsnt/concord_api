//! Exact-tree release truth and branch-hygiene checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn run(root: &Path) -> Result<(), String> {
    super::docs_check::run(root)?;
    let head = git_output(root, &["rev-parse", "HEAD"])?;
    let branch = git_output(root, &["branch", "--show-current"])?;
    let merge_base = git_output(root, &["merge-base", "main", "HEAD"])?;
    git_success(root, &["diff", "--check", &merge_base, "HEAD"])?;

    let status = git_output(root, &["status", "--porcelain", "--untracked-files=all"])?;
    if !status.is_empty() {
        return Err(format!(
            "exact-candidate audit requires a clean worktree; git status reported:\n{status}"
        ));
    }

    check_process_artifacts(root)?;
    check_source_sentinels(root)?;
    check_runtime_authority(root)?;
    check_lifecycle(root)?;

    println!("release-audit exact HEAD: {head}");
    println!("release-audit branch: {branch}");
    println!("branch diff hygiene: git diff --check {merge_base} HEAD passed");
    println!("worktree hygiene: git status --porcelain --untracked-files=all is empty");
    println!("publication safety, lifecycle, process-artifact, clock, and authority checks passed");
    Ok(())
}

fn check_process_artifacts(root: &Path) -> Result<(), String> {
    for relative in [
        "concord_v2_1",
        "PROMPT_FINISH_CONCORD_V2_1_AND_MERGE_LOCAL.md",
        "PROMPT_COMPLETE_CONCORD_V2_1_FINAL_RELEASE_AND_LOCAL_MERGE.md",
        "CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md",
        "concord_v2_1_second_release_readiness_audit.md",
    ] {
        if root.join(relative).exists() {
            return Err(format!(
                "obsolete root process artifact remains in the distributable workspace: {relative}"
            ));
        }
    }
    Ok(())
}

fn check_source_sentinels(root: &Path) -> Result<(), String> {
    let mut findings = Vec::new();
    for directory in ["crates", "tools"] {
        for path in collect_rust_files(&root.join(directory))? {
            let relative = relative(root, &path);
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("could not read {relative}: {error}"))?;
            for token in [concat!("todo", "!"), concat!("unimplemented", "!")] {
                if source.contains(token) {
                    findings.push(format!("{relative}: contains `{token}`"));
                }
            }
            for marker in [
                "panic!(\"TODO",
                "panic!(\"placeholder",
                "panic!(\"not implemented",
                "TemporaryPublic",
                "TEMP_PUBLIC",
            ] {
                if source.contains(marker) {
                    findings.push(format!(
                        "{relative}: contains placeholder marker `{marker}`"
                    ));
                }
            }
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!("placeholder scan failed:\n{}", findings.join("\n")))
    }
}

fn check_runtime_authority(root: &Path) -> Result<(), String> {
    let core = root.join("crates/concord-core/src");
    let mut ambient_reads = Vec::new();
    for path in collect_rust_files(&core)? {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if source.contains("SystemTime::now()")
            && relative(root, &path) != "crates/concord-core/src/runtime/wall_clock.rs"
        {
            ambient_reads.push(relative(root, &path));
        }
    }
    if !ambient_reads.is_empty() {
        return Err(format!(
            "ambient response wall-clock reads remain outside the default clock: {ambient_reads:?}"
        ));
    }

    let events = read(root, "crates/concord-core/src/hooks/events.rs")?;
    let limiter = read(root, "crates/concord-core/src/rate_limit/limiter.rs")?;
    if events.contains("pub observed_url:") || events.contains("pub headers:") {
        return Err("automatic observer events regained URL or header-map authority".to_owned());
    }
    if limiter.contains("pub observed_url:")
        || limiter.contains("pub host:")
        || limiter.contains("pub headers:")
    {
        return Err("rate limiter context regained complete URL/host/header authority".to_owned());
    }
    if !events.contains("pub target: RequestTarget")
        || !limiter.contains("pub logical_origin: Option<&'a str>")
    {
        return Err("bounded observer/limiter authority projections are missing".to_owned());
    }
    Ok(())
}

fn check_lifecycle(root: &Path) -> Result<(), String> {
    let workspace = read(root, "Cargo.toml")?;
    let version = workspace
        .lines()
        .skip_while(|line| line.trim() != "[workspace.package]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
        .find_map(|line| quoted_assignment(line, "version"))
        .ok_or_else(|| "workspace version is missing".to_owned())?;
    let status = read(root, "dev_doc/release_gate_status.md")?;
    let owner = read(root, "dev_doc/owner_release_decisions.md")?;

    for manifest in [
        "crates/concord/Cargo.toml",
        "crates/concord-core/Cargo.toml",
        "crates/concord-macros/Cargo.toml",
        "crates/concord-extras/Cargo.toml",
    ] {
        let content = read(root, manifest)?;
        if !content
            .lines()
            .any(|line| line.split_whitespace().collect::<String>() == "publish=false")
        {
            return Err(format!("{manifest}: publish = false is missing"));
        }
    }

    if version == "2.1.0" {
        if owner.contains("REQUIRED OWNER DECISION")
            || status.contains("Blocked")
            || status.contains("Provisional")
            || status.contains("Conditional")
        {
            return Err(
                "stable 2.1.0 is forbidden while owner or mandatory gate decisions remain open"
                    .to_owned(),
            );
        }
    } else if version.starts_with("2.1.0-beta.") {
        if !status.contains("integration candidate / beta")
            || !status.contains("Blocked — owner authority required")
        {
            return Err("beta lifecycle does not name its mandatory external blockers".to_owned());
        }
    } else {
        return Err(format!("unsupported v2.1 lifecycle version `{version}`"));
    }
    Ok(())
}

fn quoted_assignment<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.trim()
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
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

fn git_success(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run git {}: {error}", arguments.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed with {}:\n{}{}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn collect_rust_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![directory.to_path_buf()];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current)
            .map_err(|error| format!("could not enumerate {}: {error}", current.display()))?
        {
            let entry = entry.map_err(|error| {
                format!("could not inspect {} entry: {error}", current.display())
            })?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("rs")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn read(root: &Path, relative: &str) -> Result<String, String> {
    fs::read_to_string(root.join(relative))
        .map_err(|error| format!("could not read {relative}: {error}"))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::quoted_assignment;

    #[test]
    fn manifest_assignment_parser_is_exact() {
        assert_eq!(
            quoted_assignment("version = \"2.1.0-beta.1\"", "version"),
            Some("2.1.0-beta.1")
        );
        assert_eq!(quoted_assignment("versioned = \"bad\"", "version"), None);
    }
}
