//! Shared subprocess support for tests whose contract is Cargo metadata,
//! dependency topology, feature selection, or downstream compilation.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path};
use std::process::{Command, Output};

use serde_json::Value;

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

/// Compiles the listed fixture binaries and verifies compiler-independent
/// semantic diagnostics.
///
/// Expectations use one line per binary:
/// `binary|fragment one;;fragment two|primary/source/path.rs`. Blank lines and
/// lines beginning with `#` are ignored. Forbidden diagnostic tokens use one
/// literal value per line. Each binary is compiled separately so a diagnostic
/// from one source cannot accidentally satisfy another source's expectation.
///
/// # Panics
///
/// Panics when an expectation file is malformed, a fixture unexpectedly
/// compiles, an expected diagnostic or primary span is absent, or a forbidden
/// token appears in a compiler error.
pub fn assert_semantic_compile_failures(
    workspace_root: &Path,
    target_key: &str,
    manifest: &Path,
    package: &str,
    expectations: &Path,
    forbidden_tokens: &Path,
) {
    let cases = parse_expectations(expectations);
    let forbidden = parse_forbidden_tokens(forbidden_tokens);
    assert!(
        !cases.is_empty(),
        "semantic UI expectations must not be empty"
    );
    assert!(
        !forbidden.is_empty(),
        "semantic UI forbidden-token list must not be empty"
    );

    for case in cases {
        let output = check_manifest(
            workspace_root,
            target_key,
            manifest,
            &[
                "-p",
                package,
                "--no-default-features",
                "--bin",
                &case.binary,
                "--message-format=json",
            ],
        );
        assert!(
            !output.status.success(),
            "semantic UI binary `{}` unexpectedly compiled",
            case.binary
        );

        let diagnostics = compiler_error_diagnostics(&output.stdout);
        assert!(
            !diagnostics.is_empty(),
            "semantic UI binary `{}` failed without a compiler JSON error: {}",
            case.binary,
            sanitized_diagnostics(&output, &forbidden)
        );

        for diagnostic in &diagnostics {
            let text = diagnostic_text(diagnostic);
            for token in &forbidden {
                assert!(
                    !text.contains(token),
                    "semantic UI binary `{}` exposed a forbidden diagnostic token",
                    case.binary
                );
            }
        }

        let matching = diagnostics.iter().find(|diagnostic| {
            let text = diagnostic_text(diagnostic);
            case.fragments
                .iter()
                .all(|fragment| text.contains(fragment))
        });
        let diagnostic = matching.unwrap_or_else(|| {
            panic!(
                "semantic UI binary `{}` omitted expected message fragments {:?}: {}",
                case.binary,
                case.fragments,
                sanitized_compiler_errors(&diagnostics, &forbidden)
            )
        });
        assert!(
            has_primary_span(diagnostic, &case.primary_path_suffix),
            "semantic UI binary `{}` matched its message but omitted primary span suffix `{}`: {}",
            case.binary,
            case.primary_path_suffix,
            sanitized_compiler_errors(std::slice::from_ref(diagnostic), &forbidden)
        );
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SemanticExpectation {
    binary: String,
    fragments: Vec<String>,
    primary_path_suffix: String,
}

fn parse_expectations(path: &Path) -> Vec<SemanticExpectation> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut fields = line.splitn(3, '|');
            let binary = fields.next().unwrap_or_default().trim();
            let fragments = fields.next().unwrap_or_default().trim();
            let primary_path_suffix = fields.next().unwrap_or_default().trim();
            assert!(
                !binary.is_empty() && !fragments.is_empty() && !primary_path_suffix.is_empty(),
                "{}:{} must use binary|fragment;;fragment|primary/path.rs",
                path.display(),
                index + 1
            );
            let fragments = fragments
                .split(";;")
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(
                fragments.iter().all(|fragment| !fragment.is_empty()),
                "{}:{} contains an empty diagnostic fragment",
                path.display(),
                index + 1
            );
            Some(SemanticExpectation {
                binary: binary.to_owned(),
                fragments,
                primary_path_suffix: normalize_path(primary_path_suffix),
            })
        })
        .collect()
}

fn parse_forbidden_tokens(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn compiler_error_diagnostics(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| message.get("reason").and_then(Value::as_str) == Some("compiler-message"))
        .filter_map(|message| message.get("message").cloned())
        .filter(|diagnostic| diagnostic.get("level").and_then(Value::as_str) == Some("error"))
        .collect()
}

fn diagnostic_text(diagnostic: &Value) -> String {
    let mut text = String::new();
    append_json_string(&mut text, diagnostic.get("message"));
    append_json_string(
        &mut text,
        diagnostic.get("code").and_then(|code| code.get("code")),
    );
    append_json_string(&mut text, diagnostic.get("rendered"));
    if let Some(children) = diagnostic.get("children").and_then(Value::as_array) {
        for child in children {
            append_json_string(&mut text, child.get("message"));
            append_json_string(&mut text, child.get("rendered"));
        }
    }
    text
}

fn append_json_string(output: &mut String, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str) {
        output.push_str(value);
        output.push('\n');
    }
}

fn has_primary_span(diagnostic: &Value, expected_suffix: &str) -> bool {
    diagnostic
        .get("spans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|span| {
            span.get("is_primary").and_then(Value::as_bool) == Some(true)
                && span
                    .get("file_name")
                    .and_then(Value::as_str)
                    .is_some_and(|file| normalize_path(file).ends_with(expected_suffix))
        })
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn sanitized_compiler_errors(diagnostics: &[Value], forbidden: &[String]) -> String {
    let mut text = diagnostics
        .iter()
        .map(diagnostic_text)
        .collect::<Vec<_>>()
        .join("\n");
    sanitize(&mut text, forbidden);
    bounded_text(&text)
}

fn sanitized_diagnostics(output: &Output, forbidden: &[String]) -> String {
    let mut text = diagnostics(output);
    sanitize(&mut text, forbidden);
    bounded_text(&text)
}

fn sanitize(text: &mut String, forbidden: &[String]) {
    for token in forbidden {
        *text = text.replace(token, "[redacted diagnostic token]");
    }
}

fn bounded_text(text: &str) -> String {
    if text.len() <= DIAGNOSTIC_LIMIT {
        return text.to_owned();
    }
    let mut end = DIAGNOSTIC_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[diagnostic output truncated]", &text[..end])
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

    #[test]
    fn compiler_json_matching_requires_message_and_primary_source_span() {
        let source = br#"{"reason":"compiler-message","message":{"message":"no method named `paginate`","code":{"code":"E0599"},"level":"error","spans":[{"file_name":"tests/ui/no_paginate.rs","is_primary":true}],"children":[],"rendered":"error[E0599]: no method named `paginate`"}}"#;
        let diagnostics = compiler_error_diagnostics(source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostic_text(&diagnostics[0]).contains("no method named `paginate`"));
        assert!(has_primary_span(&diagnostics[0], "tests/ui/no_paginate.rs"));
        assert!(!has_primary_span(&diagnostics[0], "tests/ui/other.rs"));
    }

    #[test]
    fn diagnostic_sanitization_removes_forbidden_tokens() {
        let diagnostics = vec![serde_json::json!({
            "message": "internal __private sentinel",
            "level": "error",
            "spans": [],
            "children": [],
        })];
        let rendered = sanitized_compiler_errors(&diagnostics, &["__private".to_owned()]);
        assert!(!rendered.contains("__private"));
        assert!(rendered.contains("[redacted diagnostic token]"));
    }
}
