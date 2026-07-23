//! Shared subprocess support for tests whose contract is Cargo metadata,
//! dependency topology, feature selection, or downstream compilation.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path};
use std::process::{Command, Output};

const DIAGNOSTIC_LIMIT: usize = 64 * 1024;

fn fixture_target_dir(workspace_root: &Path, target_key: &str) -> std::path::PathBuf {
    let target_key = Path::new(target_key);
    assert!(
        target_key
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "Cargo fixture target key must contain only normal relative components"
    );
    workspace_root.join("target/fixtures").join(target_key)
}

/// Runs one locked, offline Cargo command with an isolated target directory.
///
/// This helper is intentionally confined to the workspace-internal test-support
/// crate. `target_key` must be a relative sequence of normal path components
/// so a test cannot accidentally redirect build output outside the workspace.
#[must_use]
pub fn run<I, S>(workspace_root: &Path, target_key: &str, subcommand: &str, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let target_dir = fixture_target_dir(workspace_root, target_key);
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(workspace_root)
        .arg(subcommand)
        .args(["--offline", "--locked", "--color", "never"])
        .args(arguments)
        .env("CARGO_TARGET_DIR", target_dir);
    command.output().unwrap_or_else(|error| {
        panic!("failed to run Cargo {subcommand} for fixture target `{target_key:?}`: {error}")
    })
}

/// Runs locked, offline Cargo Nextest with an isolated target directory.
///
/// Cargo's built-in flags follow `nextest run`, unlike built-in Cargo
/// subcommands where [`run`] places them immediately after the subcommand.
/// The standalone invocation uses its own default profile rather than an
/// outer Nextest runner's inherited profile selection.
#[must_use]
pub fn run_nextest<I, S>(workspace_root: &Path, target_key: &str, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let target_dir = fixture_target_dir(workspace_root, target_key);
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(workspace_root)
        .args([
            "nextest",
            "run",
            "--offline",
            "--locked",
            "--color",
            "never",
        ])
        .args(arguments)
        .env_remove("NEXTEST_PROFILE")
        .env("CARGO_TARGET_DIR", target_dir);
    command.output().unwrap_or_else(|error| {
        panic!("failed to run Cargo Nextest for fixture target `{target_key}`: {error}")
    })
}

/// Checks one standalone fixture manifest with the shared locked/offline
/// policy and an isolated target directory.
#[must_use]
pub fn check_manifest(
    workspace_root: &Path,
    target_key: &str,
    manifest: &Path,
    arguments: &[&str],
) -> Output {
    let mut cargo_arguments = vec![
        OsString::from("--manifest-path"),
        manifest.as_os_str().to_owned(),
    ];
    cargo_arguments.extend(arguments.iter().map(OsString::from));
    run(workspace_root, target_key, "check", cargo_arguments)
}

/// Produces bounded, lossy diagnostics without reproducing command arguments.
#[must_use]
pub fn diagnostics(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        bounded_lossy(&output.stdout),
        bounded_lossy(&output.stderr)
    )
}

fn bounded_lossy(bytes: &[u8]) -> String {
    let end = bytes.len().min(DIAGNOSTIC_LIMIT);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    if end != bytes.len() {
        text.push_str("\n[diagnostic output truncated]");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_diagnostics_do_not_reproduce_unbounded_output() {
        let bytes = vec![b'x'; DIAGNOSTIC_LIMIT + 1];
        let rendered = bounded_lossy(&bytes);
        assert!(rendered.ends_with("[diagnostic output truncated]"));
        assert!(rendered.len() < bytes.len() + 64);
    }
}
