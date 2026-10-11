#![forbid(unsafe_code)]

mod command;
mod contract;
mod doc_audit;
mod doc_rules;
mod docs_check;
mod fixtures;
mod limit_drift;
mod migration;
mod msrv;
mod policy;
mod release_audit;
mod source_export;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) == Some("migrate-v3") {
        return migration::run_command(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("contract") {
        return contract::run_command(&arguments[1..]);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = workspace_root()?;
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.first().map(String::as_str) {
        Some("fixtures") => {
            parse_options(&arguments[1..])?;
            fixtures::run_all(&root)
        }
        Some("msrv-ui") => {
            parse_options(&arguments[1..])?;
            msrv::run(&root);
            Ok(())
        }
        Some("policy") => {
            parse_options(&arguments[1..])?;
            policy::run(&root)
        }
        Some("docs-check") => {
            parse_options(&arguments[1..])?;
            docs_check::run(&root)
        }
        Some("doc-audit") => {
            parse_options(&arguments[1..])?;
            doc_audit::run(&root)
        }
        Some("release-audit") => {
            parse_options(&arguments[1..])?;
            release_audit::run(&root)
        }
        Some("source-export") => {
            parse_options(&arguments[1..])?;
            source_export::run(&root)
        }
        _ => Err(usage()),
    }
}

fn parse_options(arguments: &[String]) -> Result<(), String> {
    for argument in arguments {
        match argument.as_str() {
            "--locked" => {}
            _ => return Err(usage()),
        }
    }
    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "xtask manifest is not under the workspace tools directory".to_owned())?
        .canonicalize()
        .map_err(|error| format!("could not resolve the workspace root: {error}"))
}

fn usage() -> String {
    format!(
        "usage: cargo run -p concord-xtask -- <docs-check [--locked] | {} | fixtures [--locked] | {} | release-audit [--locked] | source-export [--locked] | {} | msrv-ui [--locked] | policy [--locked]>",
        doc_audit::USAGE,
        migration::USAGE,
        contract::USAGE
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_options, usage};

    #[test]
    fn fixed_command_options_are_bounded() {
        parse_options(&["--locked".into()]).expect("locked");
        assert!(parse_options(&["--write".into()]).is_err());
    }

    #[test]
    fn usage_lists_the_rendered_documentation_audit() {
        assert!(usage().contains("| doc-audit [--locked] |"), "{}", usage());
    }
}
