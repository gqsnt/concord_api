//! `contract export|check|diff|list`: `concord.contract/3` tooling (§18.3).
//!
//! Exit status: `0` success (for `check`: no structural difference), `1` a
//! `check` found a structural difference, `2` usage, I/O or format errors.

mod registry;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use concord_inspect::{DiffReport, check, diff_documents, export_string, parse_document};

pub(crate) const USAGE: &str = "contract <export --target NAME --output FILE | check --target NAME --snapshot FILE | diff BEFORE AFTER [--format text|json] | list>";

#[derive(Debug, Eq, PartialEq)]
enum Format {
    Text,
    Json,
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Export {
        target: String,
        output: PathBuf,
    },
    Check {
        target: String,
        snapshot: PathBuf,
    },
    Diff {
        before: PathBuf,
        after: PathBuf,
        format: Format,
    },
    List,
}

/// Runs one `contract` subcommand.
pub(crate) fn run_command(arguments: &[String]) -> ExitCode {
    ExitCode::from(run_status(arguments))
}

/// The numeric exit status of [`run_command`].
pub(crate) fn run_status(arguments: &[String]) -> u8 {
    match parse(arguments).and_then(|command| execute(&command)) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("xtask contract: {error}");
            2
        }
    }
}

fn parse(arguments: &[String]) -> Result<Command, String> {
    let usage = || format!("usage: cargo run -p concord-xtask -- {USAGE}");
    let (subcommand, rest) = arguments.split_first().ok_or_else(usage)?;
    let mut target = None;
    let mut output = None;
    let mut snapshot = None;
    let mut format = None;
    let mut positional = Vec::new();
    let mut iterator = rest.iter();
    while let Some(argument) = iterator.next() {
        let mut value = |slot: &mut Option<String>| -> Result<(), String> {
            let next = iterator.next().ok_or_else(usage)?;
            if slot.replace(next.clone()).is_some() {
                return Err(format!("{argument} was given more than once"));
            }
            Ok(())
        };
        match argument.as_str() {
            "--target" => value(&mut target)?,
            "--output" => value(&mut output)?,
            "--snapshot" => value(&mut snapshot)?,
            "--format" => value(&mut format)?,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => positional.push(path.to_owned()),
        }
    }
    if matches!(subcommand.as_str(), "export" | "check") && format.is_some() {
        return Err(usage());
    }
    if subcommand == "diff" && (output.is_some() || snapshot.is_some()) {
        return Err(usage());
    }
    let format = match format.as_deref() {
        None | Some("text") => Format::Text,
        Some("json") => Format::Json,
        Some(_) => return Err("--format must be text or json".to_owned()),
    };
    let command = match subcommand.as_str() {
        "export" if positional.is_empty() && snapshot.is_none() => Command::Export {
            target: target.ok_or_else(usage)?,
            output: PathBuf::from(output.ok_or_else(usage)?),
        },
        "check" if positional.is_empty() && output.is_none() => Command::Check {
            target: target.ok_or_else(usage)?,
            snapshot: PathBuf::from(snapshot.ok_or_else(usage)?),
        },
        "diff" if positional.len() == 2 && target.is_none() => Command::Diff {
            before: PathBuf::from(&positional[0]),
            after: PathBuf::from(&positional[1]),
            format,
        },
        "list" if rest.is_empty() => Command::List,
        _ => return Err(usage()),
    };
    Ok(command)
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

fn target(name: &str) -> Result<registry::ContractTarget, String> {
    registry::resolve(name).ok_or_else(|| {
        let known = registry::targets()
            .iter()
            .map(|target| target.name)
            .collect::<Vec<_>>();
        format!(
            "unknown contract target `{name}`; registered: {}",
            known.join(", ")
        )
    })
}

fn execute(command: &Command) -> Result<u8, String> {
    match command {
        Command::List => {
            for target in registry::targets() {
                println!("{:<18} {}", target.name, target.description);
            }
            Ok(0)
        }
        Command::Export {
            target: name,
            output,
        } => {
            let resolved = target(name)?;
            let text = export_string((resolved.contract)());
            fs::write(output, text)
                .map_err(|error| format!("could not write {}: {error}", output.display()))?;
            println!(
                "exported {} ({}) to {}",
                resolved.name,
                concord_inspect::CONTRACT_FORMAT,
                output.display()
            );
            Ok(0)
        }
        Command::Check {
            target: name,
            snapshot,
        } => {
            let resolved = target(name)?;
            let snapshot_text = read(snapshot)?;
            let report = check((resolved.contract)(), &snapshot_text)
                .map_err(|error| format!("{}: {error}", snapshot.display()))?;
            Ok(check_status(resolved.name, &report))
        }
        Command::Diff {
            before,
            after,
            format,
        } => {
            let before_document = parse_document(&read(before)?)
                .map_err(|error| format!("{}: {error}", before.display()))?;
            let after_document = parse_document(&read(after)?)
                .map_err(|error| format!("{}: {error}", after.display()))?;
            let report = diff_documents(&before_document, &after_document)
                .map_err(|error| error.to_string())?;
            match format {
                Format::Text => print!("{}", report.to_text()),
                Format::Json => print!("{}", report.to_json_string()),
            }
            Ok(0)
        }
    }
}

fn check_status(name: &str, report: &DiffReport) -> u8 {
    if report.is_structurally_equal() {
        println!(
            "contract {name} matches the snapshot ({})",
            report.verdict().label()
        );
        for unresolved in report.unresolved() {
            println!(
                "note: unresolved [{}] {}: {}",
                unresolved.axis().label(),
                unresolved.location(),
                unresolved.reason()
            );
        }
        0
    } else {
        println!("contract {name} differs from the snapshot");
        print!("{}", report.to_text());
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn command_lines_are_parsed_exactly() {
        assert_eq!(
            parse(&arguments(&[
                "export",
                "--target",
                "fixture-catalog",
                "--output",
                "out.json"
            ])),
            Ok(Command::Export {
                target: "fixture-catalog".to_owned(),
                output: PathBuf::from("out.json"),
            })
        );
        assert_eq!(
            parse(&arguments(&[
                "check",
                "--target",
                "t",
                "--snapshot",
                "s.json"
            ])),
            Ok(Command::Check {
                target: "t".to_owned(),
                snapshot: PathBuf::from("s.json"),
            })
        );
        assert_eq!(
            parse(&arguments(&[
                "diff", "a.json", "b.json", "--format", "json"
            ])),
            Ok(Command::Diff {
                before: PathBuf::from("a.json"),
                after: PathBuf::from("b.json"),
                format: Format::Json,
            })
        );
        for input in [
            &["diff", "a.json", "b.json"][..],
            &["diff", "a.json", "b.json", "--format", "text"],
        ] {
            assert_eq!(
                parse(&arguments(input)),
                Ok(Command::Diff {
                    before: PathBuf::from("a.json"),
                    after: PathBuf::from("b.json"),
                    format: Format::Text,
                })
            );
        }
        assert_eq!(parse(&arguments(&["list"])), Ok(Command::List));
        for invalid in [
            &["export", "--target", "t"][..],
            &["check", "--snapshot", "s"],
            &["diff", "a.json"],
            &["diff", "a", "b", "--format", "yaml"],
            &["export", "--target", "a", "--target", "b", "--output", "o"],
            &["export", "--bogus", "x"],
            &["frobnicate"],
            &[],
        ] {
            assert!(
                parse(&arguments(invalid)).is_err(),
                "{invalid:?} must be rejected"
            );
        }
    }

    #[test]
    fn registry_resolves_the_fixture_target_without_network() {
        let target = registry::resolve("fixture-catalog").expect("fixture target");
        let contract = (target.contract)();
        assert_eq!(contract.name(), "CatalogApi");
        assert!(registry::resolve("missing").is_none());
        assert!(target_error_lists_registered("missing").contains("fixture-catalog"));
    }

    fn target_error_lists_registered(name: &str) -> String {
        match target(name) {
            Ok(_) => String::new(),
            Err(error) => error,
        }
    }

    #[test]
    fn check_status_is_nonzero_on_structural_difference() {
        let baseline = concord_inspect::fixtures::catalog(
            concord_inspect::fixtures::CatalogMutation::Baseline,
        );
        let snapshot = export_string(&baseline);
        let equal = check(&baseline, &snapshot).expect("check");
        assert_eq!(check_status("fixture-catalog", &equal), 0);
        let changed = concord_inspect::fixtures::catalog(
            concord_inspect::fixtures::CatalogMutation::RenameRateLabel,
        );
        let different = check(&changed, &snapshot).expect("check");
        assert_eq!(check_status("fixture-catalog", &different), 1);
        assert_eq!(
            run_status(&arguments(&[
                "check",
                "--target",
                "nope",
                "--snapshot",
                "x"
            ])),
            2
        );
    }
}
