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
                concat!("panic!(\"", "TODO"),
                concat!("panic!(\"", "placeholder"),
                concat!("panic!(\"", "not implemented"),
                concat!("Temporary", "Public"),
                concat!("TEMP_", "PUBLIC"),
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

    lifecycle_decision(version, &status, &owner)
}

/// The v3 release line this tree may carry (spec DONE-02: `3.0.0-beta.N`,
/// unpublished until separately authorized).
const RELEASE_LINE: &str = "3.0.0";

/// Decides whether the workspace version is allowed by the authoritative
/// status and owner-decision documents.
fn lifecycle_decision(version: &str, status: &str, owner: &str) -> Result<(), String> {
    if !status.contains(&format!("`{version}`")) {
        return Err(format!(
            "dev_doc/release_gate_status.md does not name the exact workspace version `{version}`"
        ));
    }
    if version == RELEASE_LINE {
        if owner.contains("REQUIRED OWNER DECISION")
            || status.contains("Blocked")
            || status.contains("Provisional")
            || status.contains("Conditional")
        {
            return Err(format!(
                "stable {RELEASE_LINE} is forbidden while owner or mandatory gate decisions remain open"
            ));
        }
    } else if version
        .strip_prefix(RELEASE_LINE)
        .and_then(|rest| rest.strip_prefix("-beta."))
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        if !status.contains("integration candidate / beta")
            || !status.contains("Blocked — owner authority required")
        {
            return Err("beta lifecycle does not name its mandatory external blockers".to_owned());
        }
    } else {
        return Err(format!(
            "unsupported lifecycle version `{version}`; this tree carries {RELEASE_LINE}-beta.N or {RELEASE_LINE}"
        ));
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
    use super::{lifecycle_decision, quoted_assignment};

    #[test]
    fn manifest_assignment_parser_is_exact() {
        assert_eq!(
            quoted_assignment("version = \"3.0.0-beta.1\"", "version"),
            Some("3.0.0-beta.1")
        );
        assert_eq!(quoted_assignment("versioned = \"bad\"", "version"), None);
    }

    const BETA_STATUS: &str = "- **Workspace version:** `3.0.0-beta.1`\n\
        unpublished beta (integration candidate / beta lifecycle)\n\
        | Gate 0 | **Blocked — owner authority required** |";

    #[test]
    fn v3_beta_lifecycle_requires_the_exact_version_and_named_blockers() {
        lifecycle_decision("3.0.0-beta.1", BETA_STATUS, "").expect("v3 beta");
        let stale = BETA_STATUS.replace("3.0.0-beta.1", "2.1.0-beta.1");
        let error = lifecycle_decision("3.0.0-beta.1", &stale, "").expect_err("stale status");
        assert!(error.contains("exact workspace version"), "{error}");
        let unblocked = BETA_STATUS.replace("Blocked — owner authority required", "Open");
        assert!(lifecycle_decision("3.0.0-beta.1", &unblocked, "").is_err());
        for unsupported in ["2.1.0-beta.1", "3.0.0-rc.1", "3.0.0-beta.", "3.1.0-beta.1"] {
            let status = BETA_STATUS.replace("3.0.0-beta.1", unsupported);
            let error = lifecycle_decision(unsupported, &status, "").expect_err(unsupported);
            assert!(error.contains("unsupported lifecycle version"), "{error}");
        }
    }

    #[test]
    fn stable_v3_requires_every_decision_closed() {
        let status = BETA_STATUS.replace("3.0.0-beta.1", "3.0.0");
        let error = lifecycle_decision("3.0.0", &status, "").expect_err("open gates");
        assert!(error.contains("forbidden"), "{error}");
        lifecycle_decision("3.0.0", "Workspace version: `3.0.0`; all gates passed", "")
            .expect("closed gates");
        assert!(
            lifecycle_decision("3.0.0", "`3.0.0`", "REQUIRED OWNER DECISION: license").is_err()
        );
    }
}
