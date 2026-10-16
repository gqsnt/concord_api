//! Generated-contract scale probe (V3-O07).
//!
//! For each endpoint count (default 1, 10, 100, 500 and 1000) this writes a
//! standalone fixture crate with one `api!` client whose endpoints all attach
//! one named rate definition, then measures:
//!
//! * the macro expansion (lines, characters, whitespace-separated fragments,
//!   declaration- and function-shaped lines) and the time to produce it;
//! * a full debug build of the fixture and the resulting binary size;
//! * a representative local edit: the rebuild time after changing one
//!   endpoint's route (V3-O07 "representative edit costs");
//! * a representative parent edit: the rebuild time after additionally
//!   changing the root timeout that every endpoint inherits (spec §28.4
//!   "representative local/parent edits");
//! * cold and warm compile of the generated crate: the build after
//!   `cargo clean -p` of the fixture package with every dependency still
//!   built (cold), then an immediate no-op rebuild (warm);
//! * the size of the static structural description (`concord.contract/3`
//!   export for V3).
//!
//! Each measured rebuild follows a one-declaration source change from the
//! previous step: base -> local edit (endpoint 0's route) -> parent edit
//! (the root `timeout`), then cold and warm builds of the parent-edited
//! source. Fixture builds inherit `CARGO_INCREMENTAL` from the environment
//! (the value is printed); `--incremental` instead sets
//! `CARGO_INCREMENTAL=1` for every fixture command and uses a separate
//! `incremental-fixture` target directory so the two modes never
//! invalidate each other's dependency artifacts.
//!
//! This tool generates and measures the current V3 declaration language only.
//! Compare V3 commits with separate checkouts and target directories.
//!
//! Expansion uses `cargo expand` when available (a `target/tools` install or
//! one on `PATH`), otherwise the identical rustc pretty-printer that
//! cargo-expand drives (`cargo rustc --profile check -- -Zunpretty=expanded`
//! with `RUSTC_BOOTSTRAP=1`, as cargo-expand itself does on stable); with that
//! fallback the fixture builds use the same `RUSTC_BOOTSTRAP` value so build
//! scripts keyed on it are not rerun between steps. The selected method is
//! printed: counts are only comparable within one method.
//!
//! The fixture shares `CARGO_TARGET_DIR` when it is set (so dependency
//! artifacts are reused) and otherwise uses `target/perf/expansion-scale`.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::Instant;

const DEFAULT_COUNTS: &[usize] = &[1, 10, 100, 500, 1_000];
const FIXTURE_PACKAGE: &str = "concord-expansion-scale";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("expansion-scale: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Options {
    counts: Vec<usize>,
    /// Build the fixture with `CARGO_INCREMENTAL=1` in its own target dir.
    incremental: bool,
}

/// Which source change the fixture carries (each step adds one change).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Edit {
    /// The measured base declaration.
    None,
    /// Endpoint 0's route changed (a local, one-endpoint edit).
    Local,
    /// The local edit plus a changed root `timeout` that every endpoint
    /// inherits (a parent edit).
    LocalAndParent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpandMethod {
    CargoExpand,
    RustcUnpretty,
}

impl ExpandMethod {
    const fn label(self) -> &'static str {
        match self {
            Self::CargoExpand => "cargo-expand",
            Self::RustcUnpretty => "rustc -Zunpretty=expanded (RUSTC_BOOTSTRAP=1)",
        }
    }
}

fn run() -> Result<(), String> {
    let root = workspace_root()?;
    let options = parse_options(&env::args().skip(1).collect::<Vec<_>>())?;
    let concord_root = &root;
    if !concord_root.join("crates/concord/Cargo.toml").is_file() {
        return Err(format!(
            "{} is not a Concord checkout (no crates/concord/Cargo.toml)",
            concord_root.display()
        ));
    }
    let scratch = root.join("target/perf/expansion-scale");
    let fixture = scratch.join("fixture-v3");
    let target =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| scratch.join("build"), PathBuf::from);
    let target = if options.incremental {
        target.join("incremental-fixture")
    } else {
        target
    };
    let incremental = options.incremental.then_some("1");
    fs::create_dir_all(fixture.join("src")).map_err(|error| error.to_string())?;
    fs::write(fixture.join("Cargo.toml"), fixture_manifest(concord_root))
        .map_err(|error| error.to_string())?;
    // Pin the fixture to the measured checkout's resolved dependency versions.
    let lock = concord_root.join("Cargo.lock");
    if lock.is_file() {
        fs::copy(&lock, fixture.join("Cargo.lock")).map_err(|error| error.to_string())?;
    }
    let method = expand_method(&root);
    let manifest = fixture.join("Cargo.toml");

    // Unreported warm-up: build dependencies in both profiles.
    fs::write(fixture.join("src/main.rs"), fixture_source(1, Edit::None))
        .map_err(|error| error.to_string())?;
    expand(method, &root, &manifest, &target, incremental)?;
    build(method, &manifest, &target, incremental)?;

    println!(
        "dialect: V3; concord: {}; expansion: {}; builds: debug{}; fixture CARGO_INCREMENTAL={}",
        concord_root.display(),
        method.label(),
        if method == ExpandMethod::RustcUnpretty {
            " with RUSTC_BOOTSTRAP=1 (dependency build scripts may select other code paths; compare only within this method)"
        } else {
            ""
        },
        incremental.map_or_else(
            || env::var("CARGO_INCREMENTAL").unwrap_or_else(|_| "<unset>".to_owned()),
            str::to_owned
        )
    );
    println!(
        "characters exclude leading indentation: a pretty-printed method chain nests one level per call, so raw widths grow with nesting depth, not code size"
    );
    println!(
        "columns: build = rebuild after the previous scale point's source; edit = local edit (endpoint 0's route); parent edit = root timeout changed on top of the local edit; cold = build after `cargo clean -p {FIXTURE_PACKAGE}` (dependencies still built); warm = immediate no-op rebuild"
    );
    println!(
        "| endpoints | lines | characters | fragments | declarations | methods | expand ms | build ms | edit ms | binary bytes | description bytes | parent edit ms | cold ms | warm ms |"
    );
    println!("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for count in options.counts {
        fs::write(
            fixture.join("src/main.rs"),
            fixture_source(count, Edit::None),
        )
        .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let expansion = expand(method, &root, &manifest, &target, incremental)?;
        let expand_ms = started.elapsed().as_millis();

        let started = Instant::now();
        build(method, &manifest, &target, incremental)?;
        let build_ms = started.elapsed().as_millis();
        let binary = target.join(format!(
            "debug/{FIXTURE_PACKAGE}{}",
            env::consts::EXE_SUFFIX
        ));
        let binary_bytes = fs::metadata(&binary)
            .map_err(|error| format!("could not inspect {}: {error}", binary.display()))?
            .len();
        let description_bytes = description_bytes(&binary)?;

        // Representative local edit: one endpoint's route changes; everything
        // is rebuilt from the edited declaration.
        fs::write(
            fixture.join("src/main.rs"),
            fixture_source(count, Edit::Local),
        )
        .map_err(|error| error.to_string())?;
        let started = Instant::now();
        build(method, &manifest, &target, incremental)?;
        let edit_ms = started.elapsed().as_millis();

        // Representative parent edit: the root timeout that every endpoint
        // inherits changes (one more declaration on top of the local edit).
        fs::write(
            fixture.join("src/main.rs"),
            fixture_source(count, Edit::LocalAndParent),
        )
        .map_err(|error| error.to_string())?;
        let started = Instant::now();
        build(method, &manifest, &target, incremental)?;
        let parent_edit_ms = started.elapsed().as_millis();

        // Cold: the generated package's own artifacts are removed while every
        // dependency stays built. Warm: an immediate no-op rebuild.
        clean_fixture(method, &manifest, &target, incremental)?;
        let started = Instant::now();
        build(method, &manifest, &target, incremental)?;
        let cold_ms = started.elapsed().as_millis();
        let started = Instant::now();
        build(method, &manifest, &target, incremental)?;
        let warm_ms = started.elapsed().as_millis();

        println!(
            "| {count} | {} | {} | {} | {} | {} | {expand_ms} | {build_ms} | {edit_ms} | {binary_bytes} | {description_bytes} | {parent_edit_ms} | {cold_ms} | {warm_ms} |",
            expansion.lines().count(),
            code_characters(&expansion),
            expansion.split_whitespace().count(),
            expansion
                .lines()
                .filter(|line| declaration_shaped(line.trim_start()))
                .count(),
            expansion
                .lines()
                .filter(|line| function_shaped(line.trim_start()))
                .count(),
        );
    }
    Ok(())
}

fn parse_options(arguments: &[String]) -> Result<Options, String> {
    let mut counts = Vec::new();
    let mut incremental = false;
    for argument in arguments {
        if argument == "--incremental" {
            if incremental {
                return Err("--incremental was given more than once".into());
            }
            incremental = true;
            continue;
        }
        let count = argument
            .parse::<usize>()
            .map_err(|_| format!("`{argument}` is not an endpoint count"))?;
        if !(1..=1_000).contains(&count) {
            return Err(format!(
                "endpoint count {count} is outside the supported 1..=1000 range"
            ));
        }
        counts.push(count);
    }
    if counts.is_empty() {
        counts = DEFAULT_COUNTS.to_vec();
    }
    Ok(Options {
        counts,
        incremental,
    })
}

fn checked(output: std::io::Result<Output>, what: &str) -> Result<Output, String> {
    let output = output.map_err(|error| format!("could not execute {what}: {error}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{what} failed with {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn cargo_expand_binary(root: &Path) -> Option<Command> {
    let local = root.join(format!(
        "target/tools/cargo-expand/bin/cargo-expand{}",
        env::consts::EXE_SUFFIX
    ));
    if local.is_file() {
        let mut command = Command::new(local);
        command.arg("expand");
        return Some(command);
    }
    let available = Command::new("cargo")
        .args(["expand", "--version"])
        .output()
        .is_ok_and(|output| output.status.success());
    available.then(|| {
        let mut command = Command::new("cargo");
        command.arg("expand");
        command
    })
}

fn expand_method(root: &Path) -> ExpandMethod {
    if cargo_expand_binary(root).is_some() {
        ExpandMethod::CargoExpand
    } else {
        ExpandMethod::RustcUnpretty
    }
}

/// Applies the fixture's `CARGO_INCREMENTAL` selection to one command.
fn apply_incremental(command: &mut Command, incremental: Option<&str>) {
    if let Some(value) = incremental {
        command.env("CARGO_INCREMENTAL", value);
    }
}

fn expand(
    method: ExpandMethod,
    root: &Path,
    manifest: &Path,
    target: &Path,
    incremental: Option<&str>,
) -> Result<String, String> {
    let manifest = manifest.to_string_lossy();
    let mut command = match method {
        ExpandMethod::CargoExpand => {
            let mut command = cargo_expand_binary(root).ok_or("cargo-expand disappeared")?;
            command.args(["--manifest-path", &manifest, "--bin", FIXTURE_PACKAGE]);
            command
        }
        ExpandMethod::RustcUnpretty => {
            let mut command = Command::new("cargo");
            command
                .args([
                    "rustc",
                    "--profile",
                    "check",
                    "--manifest-path",
                    &manifest,
                    "--bin",
                    FIXTURE_PACKAGE,
                    "--",
                    "-Zunpretty=expanded",
                ])
                .env("RUSTC_BOOTSTRAP", "1");
            command
        }
    };
    apply_incremental(&mut command, incremental);
    let output = checked(
        command
            .env("CARGO_TARGET_DIR", target)
            .env("NO_COLOR", "1")
            .output(),
        "fixture expansion",
    )?;
    String::from_utf8(output.stdout).map_err(|error| format!("expansion is not UTF-8: {error}"))
}

fn build(
    method: ExpandMethod,
    manifest: &Path,
    target: &Path,
    incremental: Option<&str>,
) -> Result<(), String> {
    fixture_cargo(
        method,
        &["build"],
        manifest,
        target,
        incremental,
        "fixture build",
    )
}

/// Removes only the fixture package's own artifacts (`cargo clean -p`), so
/// the next build compiles the generated crate from scratch while every
/// dependency stays built.
fn clean_fixture(
    method: ExpandMethod,
    manifest: &Path,
    target: &Path,
    incremental: Option<&str>,
) -> Result<(), String> {
    fixture_cargo(
        method,
        &["clean", "-p", FIXTURE_PACKAGE],
        manifest,
        target,
        incremental,
        "fixture clean",
    )
}

fn fixture_cargo(
    method: ExpandMethod,
    arguments: &[&str],
    manifest: &Path,
    target: &Path,
    incremental: Option<&str>,
    what: &str,
) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command
        .args(arguments)
        .args(["--manifest-path", &manifest.to_string_lossy()])
        .env("CARGO_TARGET_DIR", target)
        .env("NO_COLOR", "1");
    if method == ExpandMethod::RustcUnpretty {
        // Build scripts (proc-macro2 and others) rerun when RUSTC_BOOTSTRAP
        // changes; keeping one value for expansion and build stops every
        // measurement from rebuilding the dependency graph.
        command.env("RUSTC_BOOTSTRAP", "1");
    }
    apply_incremental(&mut command, incremental);
    checked(command.output(), what).map(drop)
}

fn description_bytes(binary: &Path) -> Result<usize, String> {
    let output = checked(Command::new(binary).output(), "scale fixture")?;
    String::from_utf8(output.stdout)
        .map_err(|error| format!("scale fixture emitted non-UTF-8 output: {error}"))?
        .trim()
        .parse::<usize>()
        .map_err(|error| format!("invalid description size: {error}"))
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

fn fixture_manifest(concord_root: &Path) -> String {
    let crate_path = |name: &str| {
        concord_root
            .join("crates")
            .join(name)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let concord = crate_path("concord");
    let inspect = format!(
        "concord-inspect = {{ path = \"{}\" }}\n",
        crate_path("concord-inspect")
    );
    format!(
        r#"[package]
name = "{FIXTURE_PACKAGE}"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
concord = {{ path = "{concord}", default-features = false }}
{inspect}"#
    )
}

/// The fixture source for `count` endpoints. [`Edit::Local`] changes
/// endpoint 0's route (the representative one-endpoint edit);
/// [`Edit::LocalAndParent`] additionally changes the root `timeout` every
/// endpoint inherits (the representative parent edit).
fn fixture_source(count: usize, edit: Edit) -> String {
    let source = fixture_source_v3(count, edit != Edit::None);
    if edit == Edit::LocalAndParent {
        source.replacen(ROOT_TIMEOUT, EDITED_ROOT_TIMEOUT, 1)
    } else {
        source
    }
}

/// The root timeout declaration of the V3 fixture, inherited by
/// every endpoint, and its parent-edited form.
const ROOT_TIMEOUT: &str = "        timeout 30s;\n";
const EDITED_ROOT_TIMEOUT: &str = "        timeout 31s;\n";

fn fixture_source_v3(count: usize, edited: bool) -> String {
    let mut source = String::from(
        r#"#![recursion_limit = "4096"]

use concord::{Binary, Bytes, api};

api! {
    client ScaleApi {
        base "https://scale.example.test/";
        timeout 30s;
        rate_limits {
            policy shared_reads() {
                id "concord.expansion_scale.1";
                key [origin, endpoint];
                cost 1;
                allow 1_000_000 per 1s;
            }
        }

"#,
    );
    for index in 0..count {
        let route = if edited && index == 0 {
            "edited/{value}"
        } else {
            "{value}"
        };
        // The `endpoint` key dimension consumes the operational ID, which v3
        // requires to be pinned explicitly.
        source.push_str(&format!(
            "        GET endpoint_{index:04}(value: u64) -> Bytes via Binary {{\n            id \"scale.endpoint_{index:04}\";\n            path \"{route}\";\n            rate_limit shared_reads();\n        }}\n"
        ));
    }
    source.push_str(
        "    }\n}\n\nfn main() {\n    println!(\"{}\", concord_inspect::export_string(ScaleApi::contract()).len());\n}\n",
    );
    source
}

/// Characters of the expansion excluding each line's leading indentation.
fn code_characters(expansion: &str) -> usize {
    expansion
        .lines()
        .map(|line| line.trim_start().chars().count())
        .sum()
}

fn function_shaped(line: &str) -> bool {
    let line = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .unwrap_or(line);
    line.starts_with("fn ") || line.starts_with("async fn ")
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
        let source = fixture_source(3, Edit::None);
        assert_eq!(source.matches("shared_reads()").count(), 4);
        assert_eq!(source.matches("rate_limit shared_reads();").count(), 3);
        assert_eq!(source.matches("GET endpoint_").count(), 3);
        assert_eq!(source.matches("path \"{value}\";").count(), 3);
        assert!(source.contains("id \"scale.endpoint_0002\";"));
        assert!(source.contains("concord_inspect::export_string(ScaleApi::contract())"));
        assert!(
            !source.contains("endpoint.value"),
            "v3 has no endpoint qualification"
        );
    }

    fn changed_lines(before: &str, after: &str) -> Vec<(String, String)> {
        assert_eq!(before.lines().count(), after.lines().count());
        before
            .lines()
            .zip(after.lines())
            .filter(|(before, after)| before != after)
            .map(|(before, after)| (before.trim().to_owned(), after.trim().to_owned()))
            .collect()
    }

    #[test]
    fn representative_edit_changes_exactly_one_route() {
        let base = fixture_source(10, Edit::None);
        let edited = fixture_source(10, Edit::Local);
        let changed = changed_lines(&base, &edited);
        assert_eq!(changed.len(), 1);
        assert!(changed[0].1.contains("edited"), "{changed:?}");
    }

    #[test]
    fn parent_edit_changes_only_the_inherited_root_timeout() {
        let local = fixture_source(10, Edit::Local);
        let parent = fixture_source(10, Edit::LocalAndParent);
        assert_eq!(
            changed_lines(&local, &parent),
            [("timeout 30s;".to_owned(), "timeout 31s;".to_owned())],
        );
        assert_eq!(parent.matches("timeout ").count(), 1);
        assert_eq!(local.matches(ROOT_TIMEOUT).count(), 1);
    }

    #[test]
    fn fixture_uses_current_contract_inspection() {
        let source = fixture_source(2, Edit::None);
        assert!(source.contains("concord_inspect::export_string(ScaleApi::contract())"));
        let manifest = fixture_manifest(Path::new("/v3"));
        assert!(manifest.contains("concord-inspect = { path = \"/v3/crates/concord-inspect\" }"));
    }

    #[test]
    fn code_characters_ignore_indentation() {
        assert_eq!(code_characters("fn a() {\n        b()\n    }\n"), 8 + 3 + 1);
    }

    #[test]
    fn default_scale_points_cover_required_endpoint_counts() {
        assert_eq!(DEFAULT_COUNTS, [1, 10, 100, 500, 1_000]);
        let options = parse_options(&[]).expect("defaults");
        assert_eq!(options.counts, DEFAULT_COUNTS);
        assert!(!options.incremental);
    }

    #[test]
    fn options_validate_current_scale_counts() {
        let arguments = ["1", "10"].map(str::to_owned);
        let options = parse_options(&arguments).expect("options");
        assert_eq!(options.counts, [1, 10]);
        assert!(parse_options(&["0".to_owned()]).is_err());
        assert!(parse_options(&["1001".to_owned()]).is_err());
        assert!(parse_options(&["--baseline-v2-1".to_owned()]).is_err());

        let arguments = ["--incremental", "100", "100"].map(str::to_owned);
        let options = parse_options(&arguments).expect("incremental options");
        assert!(options.incremental);
        assert_eq!(options.counts, [100, 100]);
        let twice = ["--incremental", "--incremental"].map(str::to_owned);
        assert!(parse_options(&twice).is_err());
    }
}
