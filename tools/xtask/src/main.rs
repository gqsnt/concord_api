#![forbid(unsafe_code)]

mod command;
mod docs;
mod fixtures;
mod msrv;
mod package;
mod policy;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
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
        Some("docs") => {
            let write = parse_options(&arguments[1..], true)?;
            docs::run(&root, write)
        }
        Some("fixtures") => {
            parse_options(&arguments[1..], false)?;
            fixtures::run_all(&root)
        }
        Some("msrv-ui") => {
            parse_options(&arguments[1..], false)?;
            msrv::run(&root);
            Ok(())
        }
        Some("package") => {
            parse_options(&arguments[1..], false)?;
            package::run(&root)
        }
        Some("policy") => {
            parse_options(&arguments[1..], false)?;
            policy::run(&root)
        }
        _ => Err(usage()),
    }
}

fn parse_options(arguments: &[String], allow_write: bool) -> Result<bool, String> {
    let mut write = false;
    for argument in arguments {
        match argument.as_str() {
            "--locked" => {}
            "--write" if allow_write && !write => write = true,
            _ => return Err(usage()),
        }
    }
    Ok(write)
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
    "usage: cargo run -p concord-xtask -- <docs [--write] [--locked] | fixtures [--locked] | msrv-ui [--locked] | package [--locked] | policy [--locked]>".to_owned()
}

#[cfg(test)]
mod tests {
    use super::parse_options;

    #[test]
    fn fixed_command_options_are_bounded() {
        assert!(!parse_options(&["--locked".into()], false).expect("locked"));
        assert!(parse_options(&["--write".into(), "--locked".into()], true).expect("docs"));
        assert!(parse_options(&["--write".into()], false).is_err());
        assert!(parse_options(&["--write".into(), "--write".into()], true).is_err());
    }
}
