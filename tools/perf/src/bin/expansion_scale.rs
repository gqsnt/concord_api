#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

const DEFAULT_COUNTS: &[usize] = &[1, 10, 100, 500, 1_000];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("expansion-scale: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = workspace_root()?;
    let counts = endpoint_counts()?;
    let fixture = root.join("target/perf/expansion-scale/fixture");
    let target = root.join("target/perf/expansion-scale/build");
    fs::create_dir_all(fixture.join("src")).map_err(|error| error.to_string())?;
    fs::write(fixture.join("Cargo.toml"), fixture_manifest(&root))
        .map_err(|error| error.to_string())?;

    println!("| endpoints | lines | characters | fragments | declaration-shaped | expand ms |");
    println!("|---:|---:|---:|---:|---:|---:|");
    for count in counts {
        fs::write(fixture.join("src/lib.rs"), fixture_source(count))
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let output = Command::new("cargo")
            .args([
                "expand",
                "--manifest-path",
                &fixture.join("Cargo.toml").to_string_lossy(),
                "--lib",
            ])
            .env("CARGO_TARGET_DIR", &target)
            .env("NO_COLOR", "1")
            .output()
            .map_err(|error| format!("could not execute cargo-expand: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo-expand failed for {count} endpoints with {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let expansion = String::from_utf8(output.stdout)
            .map_err(|error| format!("cargo-expand emitted non-UTF-8 output: {error}"))?;
        println!(
            "| {count} | {} | {} | {} | {} | {} |",
            expansion.lines().count(),
            expansion.chars().count(),
            expansion.split_whitespace().count(),
            expansion
                .lines()
                .filter(|line| declaration_shaped(line.trim_start()))
                .count(),
            started.elapsed().as_millis(),
        );
    }
    Ok(())
}

fn endpoint_counts() -> Result<Vec<usize>, String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.is_empty() {
        return Ok(DEFAULT_COUNTS.to_vec());
    }
    arguments
        .iter()
        .map(|argument| {
            let count = argument
                .parse::<usize>()
                .map_err(|_| format!("`{argument}` is not an endpoint count"))?;
            if !(1..=1_000).contains(&count) {
                return Err(format!(
                    "endpoint count {count} is outside the supported 1..=1000 range"
                ));
            }
            Ok(count)
        })
        .collect()
}

fn workspace_root() -> Result<PathBuf, String> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let manifest = path.join("Cargo.toml");
        if fs::read_to_string(&manifest)
            .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"))
        {
            return Ok(path);
        }
        if !path.pop() {
            return Err("could not locate the Concord workspace root".into());
        }
    }
}

fn fixture_manifest(root: &Path) -> String {
    let concord = root
        .join("crates/concord")
        .to_string_lossy()
        .replace('\\', "/");
    format!(
        r#"[package]
name = "concord-expansion-scale"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
concord = {{ path = "{concord}", default-features = false }}
"#
    )
}

fn fixture_source(count: usize) -> String {
    let mut source = String::from(
        r#"#![recursion_limit = "4096"]

use concord::{Bytes, api};

api! {
    client ScaleApi {
        base "https://scale.example.test";
        timeout 30s;
        rate_limits {
            policy shared_reads() { id "concord.expansion_scale.1"; cost 1;
                key [origin, endpoint];
                allow 1_000_000 per 1s;
            }
        }

"#,
    );
    for index in 0..count {
        source.push_str(&format!(
            "        GET endpoint_{index:04}(value: u64) -> Bytes via Binary {{\n            path [endpoint.value];\n            rate_limit shared_reads();\n        }}\n"
        ));
    }
    source.push_str("    }\n}\n");
    source
}

fn declaration_shaped(line: &str) -> bool {
    let line = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .unwrap_or(line);
    [
        "struct ", "enum ", "trait ", "impl ", "fn ", "type ", "const ", "mod ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_fixture_attaches_one_named_definition_to_every_endpoint() {
        let source = fixture_source(3);
        assert_eq!(source.matches("shared_reads()").count(), 4);
        assert_eq!(source.matches("rate_limit shared_reads();").count(), 3);
        assert_eq!(source.matches("GET endpoint_").count(), 3);
    }

    #[test]
    fn default_scale_points_cover_required_endpoint_counts() {
        assert_eq!(DEFAULT_COUNTS, [1, 10, 100, 500, 1_000]);
    }
}
