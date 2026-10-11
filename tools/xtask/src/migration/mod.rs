//! `migrate-v3`: a bounded, source-aware Concord v2.1 -> v3 migration tool.
//!
//! The tool parses every Rust file with `syn`, locates `api!` invocations by
//! macro path, parses each v2.1 declaration with a real grammar parser
//! ([`syntax`]), resolves the owner graph and records exact byte edits
//! ([`declaration`]). Caller code is migrated from the resolved declarations
//! of the same crate ([`callers`]). The default mode prints a unified diff;
//! `--apply` writes an isolated copy into an explicit output directory and
//! never writes into the input tree.

mod callers;
mod declaration;
mod edits;
mod report;
mod resolve;
mod rust_file;
mod syntax;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use callers::{CallerInput, DeclIndex};
use declaration::{DeclarationInput, DeclarationOutput};
use edits::{EditSet, LineIndex, unified_diff};
use report::{Code, Diagnostic, EndpointIdentity, RawDiagnostic, Severity, Summary};
use resolve::{Edition, Layout};
use rust_file::{
    Imports, MAX_NESTING, ParseFailure, find_api_invocations, parse_rust, stripped_prefix,
};

/// Conversion settings shared by every file of one run.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    /// Convert leading simple plain introductions of local values into v3
    /// direct query/header placements when no mixed operation is reordered.
    pub(crate) direct_placement: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            direct_placement: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Check,
    Diff,
    Apply,
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Diff => "diff",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug)]
struct Options {
    input: PathBuf,
    mode: Mode,
    output: Option<PathBuf>,
    report: Option<PathBuf>,
    json: bool,
    settings: Settings,
}

pub(crate) const USAGE: &str = "migrate-v3 --input <file-or-dir> [--check | --diff | --apply --output <dir>] [--json] [--report <file.json>] [--keep-explicit-blocks]";

/// Runs the command. Exit status: 0 when no unautomatable site remains, 1
/// when unautomatable sites remain, 2 for usage or I/O failures.
pub(crate) fn run_command(arguments: &[String]) -> ExitCode {
    ExitCode::from(run_status(arguments))
}

/// The numeric exit status of [`run_command`].
pub(crate) fn run_status(arguments: &[String]) -> u8 {
    match parse_options(arguments).and_then(|options| execute(&options)) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("xtask migrate-v3: {error}");
            2
        }
    }
}

fn parse_options(arguments: &[String]) -> Result<Options, String> {
    let mut input = None;
    let mut mode = None;
    let mut output = None;
    let mut report = None;
    let mut json = false;
    let mut settings = Settings::default();
    let mut iterator = arguments.iter();
    let set_mode = |mode_slot: &mut Option<Mode>, selected: Mode| {
        if mode_slot.replace(selected).is_some() {
            return Err(format!(
                "choose one of --check, --diff or --apply\nusage: {USAGE}"
            ));
        }
        Ok(())
    };
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--input" => input = Some(PathBuf::from(value(&mut iterator, "--input")?)),
            "--output" => output = Some(PathBuf::from(value(&mut iterator, "--output")?)),
            "--report" => report = Some(PathBuf::from(value(&mut iterator, "--report")?)),
            "--check" => set_mode(&mut mode, Mode::Check)?,
            "--diff" => set_mode(&mut mode, Mode::Diff)?,
            "--apply" => set_mode(&mut mode, Mode::Apply)?,
            "--json" => json = true,
            "--keep-explicit-blocks" => settings.direct_placement = false,
            "--locked" => {}
            other => return Err(format!("unknown argument `{other}`\nusage: {USAGE}")),
        }
    }
    let input = input.ok_or_else(|| format!("--input is required\nusage: {USAGE}"))?;
    let mode = mode.unwrap_or(Mode::Diff);
    match (mode, &output) {
        (Mode::Apply, None) => {
            return Err(format!(
                "--apply requires an explicit --output directory\nusage: {USAGE}"
            ));
        }
        (Mode::Check | Mode::Diff, Some(_)) => {
            return Err(format!(
                "--output is only valid with --apply\nusage: {USAGE}"
            ));
        }
        _ => {}
    }
    Ok(Options {
        input,
        mode,
        output,
        report,
        json,
        settings,
    })
}

fn value<'a>(
    iterator: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<&'a str, String> {
    iterator
        .next()
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value\nusage: {USAGE}"))
}

fn execute(options: &Options) -> Result<u8, String> {
    let migration = migrate(&options.input, &options.settings)?;
    let summary = migration.summary();
    let input_root = migration.root.clone();
    if let Some(report) = &options.report {
        refuse_inside_input(&input_root, report, "--report")?;
    }
    if let Some(output) = &options.output {
        validate_output(&input_root, output)?;
    }
    let json = serde_json::to_string_pretty(&migration.report_json(options.mode.label(), &summary))
        .map_err(|error| format!("could not serialize the report: {error}"))?;
    if let Some(report) = &options.report {
        fs::write(report, format!("{json}\n"))
            .map_err(|error| format!("could not write {}: {error}", report.display()))?;
    }
    let diagnostics = migration
        .diagnostics
        .iter()
        .map(Diagnostic::render)
        .collect::<Vec<_>>();
    match options.mode {
        Mode::Diff => {
            if options.json {
                println!("{json}");
            } else {
                print!("{}", migration.diff());
            }
            for diagnostic in &diagnostics {
                eprintln!("{diagnostic}");
            }
            eprintln!("{}", summary.render());
        }
        Mode::Check => {
            if options.json {
                println!("{json}");
            } else {
                for diagnostic in &diagnostics {
                    println!("{diagnostic}");
                }
                println!("{}", summary.render());
            }
        }
        Mode::Apply => {
            let output = options
                .output
                .as_ref()
                .expect("apply mode validated its output directory");
            let applied = migration.apply(output)?;
            if options.json {
                println!("{json}");
            } else {
                for diagnostic in &diagnostics {
                    println!("{diagnostic}");
                }
                println!("{}", summary.render());
                println!(
                    "migrate-v3: wrote an isolated copy of {} file(s) to {}",
                    applied.written,
                    output.display()
                );
            }
            for link in &applied.skipped_links {
                eprintln!(
                    "migrate-v3: did not copy symbolic link {link}; recreate it in the output if it is needed"
                );
            }
        }
    }
    Ok(u8::from(summary.errors > 0))
}

/// What `--apply` wrote.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    pub(crate) written: usize,
    /// Input-relative symbolic links that were not followed or copied.
    pub(crate) skipped_links: Vec<String>,
}

/// One examined Rust file.
pub(crate) struct FileOutcome {
    pub(crate) relative: String,
    pub(crate) original: String,
    pub(crate) converted: String,
    pub(crate) edits: usize,
}

/// The complete, side-effect-free result of one migration run.
pub(crate) struct Migration {
    pub(crate) root: PathBuf,
    input: PathBuf,
    pub(crate) files: Vec<FileOutcome>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) identities: Vec<EndpointIdentity>,
    pub(crate) declarations: usize,
}

impl Migration {
    pub(crate) fn summary(&self) -> Summary {
        let count = |severity| {
            self.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity() == severity)
                .count()
        };
        Summary {
            files_scanned: self.files.len(),
            files_changed: self
                .files
                .iter()
                .filter(|file| file.converted != file.original)
                .count(),
            declarations: self.declarations,
            edits: self.files.iter().map(|file| file.edits).sum(),
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            notes: count(Severity::Note),
        }
    }

    pub(crate) fn diff(&self) -> String {
        self.files
            .iter()
            .map(|file| unified_diff(&file.relative, &file.original, &file.converted))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn converted(&self, relative: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|file| file.relative == relative)
            .map(|file| file.converted.as_str())
    }

    fn report_json(&self, mode: &str, summary: &Summary) -> serde_json::Value {
        let changed = self
            .files
            .iter()
            .filter(|file| file.converted != file.original)
            .map(|file| (file.relative.clone(), file.edits))
            .collect::<Vec<_>>();
        report::report_json(
            &self.input.display().to_string(),
            mode,
            summary,
            &changed,
            &self.diagnostics,
            &self.identities,
        )
    }

    /// Writes an isolated copy of the input tree with converted files into
    /// `output`, which must be empty or absent and outside the input tree.
    pub(crate) fn apply(&self, output: &Path) -> Result<Applied, String> {
        validate_output(&self.root, output)?;
        fs::create_dir_all(output)
            .map_err(|error| format!("could not create {}: {error}", output.display()))?;
        let converted = self
            .files
            .iter()
            .map(|file| (file.relative.as_str(), file.converted.as_str()))
            .collect::<HashMap<_, _>>();
        let mut applied = Applied::default();
        if self.input.is_file() {
            let relative = self
                .files
                .first()
                .map(|file| file.relative.clone())
                .ok_or_else(|| "no input file was examined".to_owned())?;
            let target = output.join(&relative);
            fs::write(&target, converted[relative.as_str()])
                .map_err(|error| format!("could not write {}: {error}", target.display()))?;
            applied.written = 1;
            return Ok(applied);
        }
        copy_tree(&self.root, &self.root, output, &converted, &mut applied)?;
        Ok(applied)
    }
}

fn copy_tree(
    root: &Path,
    directory: &Path,
    output: &Path,
    converted: &HashMap<&str, &str>,
    applied: &mut Applied,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if skipped_directory(&path) && file_type.is_dir() {
            continue;
        }
        let relative = relative_display(root, &path);
        let target = output.join(path.strip_prefix(root).unwrap_or(&path));
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("could not create {}: {error}", target.display()))?;
            copy_tree(root, &path, output, converted, applied)?;
        } else if file_type.is_file() {
            match converted.get(relative.as_str()) {
                Some(text) => fs::write(&target, text),
                None => fs::copy(&path, &target).map(|_| ()),
            }
            .map_err(|error| format!("could not write {}: {error}", target.display()))?;
            applied.written += 1;
        } else if file_type.is_symlink() {
            applied.skipped_links.push(relative);
        }
    }
    Ok(())
}

fn skipped_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "target" || name == ".git")
}

/// Canonicalizes a path that may not exist yet by resolving its nearest
/// existing ancestor.
fn canonical_lenient(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("could not read the current directory: {error}"))?
            .join(path)
    };
    let mut existing = absolute.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(ToOwned::to_owned) else {
            break;
        };
        tail.push(name);
        if !existing.pop() {
            break;
        }
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", existing.display()))?;
    for component in tail.into_iter().rev() {
        resolved.push(component);
    }
    Ok(normalize(&resolved))
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn refuse_inside_input(root: &Path, path: &Path, flag: &str) -> Result<(), String> {
    let root = canonical_lenient(root)?;
    let candidate = canonical_lenient(path)?;
    if candidate.starts_with(&root) {
        return Err(format!(
            "refusing {flag} {}: it is inside the input tree {}; migration never writes into the input tree",
            path.display(),
            root.display()
        ));
    }
    Ok(())
}

/// The output must be outside the input tree, must not contain the input tree
/// and must be absent or an empty directory.
fn validate_output(root: &Path, output: &Path) -> Result<(), String> {
    let root = canonical_lenient(root)?;
    let candidate = canonical_lenient(output)?;
    if candidate.starts_with(&root) {
        return Err(format!(
            "refusing to write into the input tree: --output {} is inside or equal to {}",
            output.display(),
            root.display()
        ));
    }
    if root.starts_with(&candidate) {
        return Err(format!(
            "refusing --output {}: it contains the input tree {}",
            output.display(),
            root.display()
        ));
    }
    if candidate.exists() {
        if !candidate.is_dir() {
            return Err(format!("--output {} is not a directory", output.display()));
        }
        let occupied = fs::read_dir(&candidate)
            .map_err(|error| format!("could not read {}: {error}", output.display()))?
            .next()
            .is_some();
        if occupied {
            return Err(format!(
                "refusing --output {}: the directory is not empty",
                output.display()
            ));
        }
    }
    Ok(())
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Collects one package's Rust files, stopping at nested packages.
fn collect_crate_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            if !skipped_directory(&path) && !path.join("Cargo.toml").is_file() {
                collect_crate_files(&path, output)?;
            }
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            output.push(path);
        }
    }
    Ok(())
}

fn collect_rust_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            if !skipped_directory(&path) {
                collect_rust_files(&path, output)?;
            }
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            output.push(path);
        }
    }
    Ok(())
}

/// The nearest ancestor directory holding a `Cargo.toml`: one crate.
fn crate_directory(file: &Path) -> Option<PathBuf> {
    let mut directory = file.parent()?.to_path_buf();
    loop {
        if directory.join("Cargo.toml").is_file() {
            return Some(directory);
        }
        if !directory.pop() {
            return None;
        }
    }
}

/// What a package's `Cargo.toml` declares for the crate model.
#[derive(Default)]
struct PackageTargets {
    /// The library's extern crate name (`[lib] name`, else the package name
    /// with `-` replaced by `_`).
    library: Option<String>,
    /// The library's source path (`[lib] path`, else the default
    /// `src/lib.rs`).
    library_path: Option<PathBuf>,
    /// The source paths of `[[bin]]`, `[[test]]`, `[[example]]` and
    /// `[[bench]]` targets.
    target_paths: Vec<PathBuf>,
    /// The package's edition: `package.edition` (no edition is Cargo's
    /// default, 2015), also through `edition.workspace = true`; unknown when
    /// the manifest cannot be read as TOML or does not determine it.
    edition: Edition,
    /// Why the manifest does not establish that the Concord crate names
    /// denote the genuine Concord packages (see [`concord_identity`]);
    /// `None` when it does, or when there is no readable manifest.
    concord_refusal: Option<String>,
    /// The Concord crate names the manifest proves to be the genuine Concord
    /// packages (a declared dependency, or the package's own library).
    concord_declared: Vec<String>,
    /// The package's own library name, when the package is the Concord
    /// package of that name (`extern crate self as concord;` is then
    /// genuine).
    concord_own: Option<String>,
}

/// A string value of a TOML table.
fn toml_string<'t>(table: &'t toml::Table, key: &str) -> Option<&'t str> {
    table.get(key).and_then(toml::Value::as_str)
}

/// A manifest path as diagnostics spell it: relative to the package directory
/// (`Cargo.toml`, `../Cargo.toml`, ...), so reasons do not depend on where
/// the input tree is.
fn manifest_label(package: &Path, path: &Path) -> String {
    if let Ok(inside) = path.strip_prefix(package) {
        return format!("`{}`", relative_display(Path::new(""), inside));
    }
    let above = path
        .parent()
        .and_then(|directory| package.strip_prefix(directory).ok())
        .map(|below| below.components().count());
    match above {
        Some(levels) => format!("`{}Cargo.toml`", "../".repeat(levels)),
        None => format!("`{}`", path.display()),
    }
}

/// Reads the `Cargo.toml` at `path` (of or above `package`) as a TOML table.
fn read_manifest(package: &Path, path: &Path) -> Result<toml::Table, String> {
    let label = manifest_label(package, path);
    let text = fs::read_to_string(path)
        .map_err(|error| format!("the manifest {label} could not be read ({error})"))?;
    text.parse::<toml::Table>()
        .map_err(|_| format!("the manifest {label} is not a valid TOML document"))
}

/// The edition an `edition` string names.
fn edition_named(value: &str, source: &str) -> Edition {
    match value {
        "2015" => Edition::Rust2015,
        "2018" | "2021" | "2024" => Edition::Rust2018OrLater,
        other => Edition::Unknown(format!(
            "{source} names the edition `{other}`, which the tool does not know"
        )),
    }
}

/// The workspace root of `package` (the package itself when it has a
/// `[workspace]` table, the directory `package.workspace` names, else the
/// nearest ancestor whose manifest has one): its directory and manifest;
/// `None` when there is none, the reason when it cannot be determined.
fn workspace_root(
    package: &Path,
    manifest: &toml::Table,
) -> Result<Option<(PathBuf, toml::Table)>, String> {
    if manifest.contains_key("workspace") {
        return Ok(Some((package.to_path_buf(), manifest.clone())));
    }
    let explicit = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("workspace"));
    match explicit {
        Some(toml::Value::String(relative)) => {
            let directory = package.join(relative);
            let root = read_manifest(package, &directory.join("Cargo.toml"))?;
            Ok(Some((directory, root)))
        }
        Some(_) => {
            Err("`package.workspace` is not a path, so the workspace root is unknown".to_owned())
        }
        None => {
            let mut directory = package.parent().map(Path::to_path_buf);
            while let Some(current) = directory {
                let candidate = current.join("Cargo.toml");
                if candidate.is_file() {
                    let table = read_manifest(package, &candidate)?;
                    if table.contains_key("workspace") {
                        return Ok(Some((current, table)));
                    }
                }
                directory = current.parent().map(Path::to_path_buf);
            }
            Ok(None)
        }
    }
}

/// The `[workspace.package] edition` of the workspace root of `package` (see
/// [`workspace_root`]), for `edition.workspace = true`.
fn workspace_edition(package: &Path, manifest: &toml::Table) -> Edition {
    let root = match workspace_root(package, manifest) {
        Ok(root) => root.map(|(_, root)| root),
        Err(reason) => return Edition::Unknown(reason),
    };
    let Some(root) = root else {
        return Edition::Unknown(
            "`edition.workspace = true` has no workspace root with a `[workspace]` table above the package".to_owned(),
        );
    };
    let edition = root
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("package"))
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("edition"));
    match edition {
        Some(toml::Value::String(value)) => edition_named(value, "the workspace root manifest"),
        _ => Edition::Unknown(
            "`edition.workspace = true`, but the workspace root declares no `[workspace.package] edition` string".to_owned(),
        ),
    }
}

/// The Concord packages and the one extern crate name under which the tool
/// recognizes each (see [`rust_file::is_concord_crate`]).
const CONCORD_PACKAGES: &[(&str, &str)] = &[
    ("concord", "concord"),
    ("concord-core", "concord_core"),
    ("concord-extras", "concord_extras"),
    ("concord-macros", "concord_macros"),
];

/// The extern crate name the tool recognizes for the Concord package named
/// `package` (Cargo treats `-` and `_` in package names alike); `None` for
/// any other package.
fn concord_extern_name(package: &str) -> Option<&'static str> {
    let normalized = package.replace('_', "-");
    CONCORD_PACKAGES
        .iter()
        .find(|(name, _)| *name == normalized)
        .map(|(_, extern_name)| *extern_name)
}

/// The dependency tables of a manifest, with the label a diagnostic names:
/// `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` (also in
/// their underscore spelling) and each of them under `[target.<cfg>]`.
fn dependency_tables(manifest: &toml::Table) -> Vec<(String, &toml::Table)> {
    const KINDS: &[&str] = &[
        "dependencies",
        "dev-dependencies",
        "dev_dependencies",
        "build-dependencies",
        "build_dependencies",
    ];
    let mut tables = Vec::new();
    for kind in KINDS {
        if let Some(table) = manifest.get(*kind).and_then(toml::Value::as_table) {
            tables.push((format!("[{kind}]"), table));
        }
    }
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, value) in targets {
            let Some(target_table) = value.as_table() else {
                continue;
            };
            for kind in KINDS {
                if let Some(table) = target_table.get(*kind).and_then(toml::Value::as_table) {
                    tables.push((format!("[target.'{target}'.{kind}]"), table));
                }
            }
        }
    }
    tables
}

/// Whether the manifest establishes that every crate name the tool treats as
/// Concord's (`concord`, `concord_core`, `concord_extras`, `concord_macros`)
/// denotes that genuine Concord package in this package, and that no Concord
/// package is reachable under another name. The identity of a dependency is
/// its package name (after a `package = ".."` rename, and read from the
/// workspace root's `[workspace.dependencies]` for `workspace = true`); a
/// `path` dependency must lead to a manifest whose `[package] name` is that
/// package. The package's own library counts like a dependency. `Err` holds
/// every reason, in manifest order, why the identity is foreign or unknown.
/// A registry or `git` dependency is identified by its package name alone.
fn concord_identity(
    package: &Path,
    manifest: &toml::Table,
    package_name: Option<&str>,
    library_name: Option<&str>,
    declared: &mut Vec<String>,
) -> Result<(), String> {
    let mut reasons = Vec::new();
    if let Some(own) = library_name.or(package_name) {
        let own = own.replace('-', "_");
        if rust_file::is_concord_crate(&own) {
            let genuine = package_name.and_then(concord_extern_name) == Some(own.as_str());
            if genuine {
                declared.push(own.clone());
            } else {
                reasons.push(format!(
                    "the package's own library is the crate `{own}`, but the package is `{}`, not the Concord package of that name",
                    package_name.unwrap_or("(unnamed)")
                ));
            }
        }
    }
    let mut root: Option<Result<Option<(PathBuf, toml::Table)>, String>> = None;
    for (label, table) in dependency_tables(manifest) {
        for (key, entry) in table {
            let extern_name = key.replace('-', "_");
            let concord_name = rust_file::is_concord_crate(&extern_name);
            // `workspace = true`: the entry of the workspace root.
            let inherited = entry
                .as_table()
                .and_then(|table| table.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true);
            let (base, entry) = if inherited {
                let workspace = root.get_or_insert_with(|| workspace_root(package, manifest));
                let found = match workspace {
                    Ok(Some((directory, root))) => root
                        .get("workspace")
                        .and_then(toml::Value::as_table)
                        .and_then(|workspace| workspace.get("dependencies"))
                        .and_then(toml::Value::as_table)
                        .and_then(|dependencies| dependencies.get(key))
                        .map(|entry| (directory.clone(), entry.clone()))
                        .ok_or_else(|| {
                            "the workspace root declares no such `[workspace.dependencies]` entry"
                                .to_owned()
                        }),
                    Ok(None) => Err(
                        "no workspace root with a `[workspace]` table is above the package"
                            .to_owned(),
                    ),
                    Err(reason) => Err(reason.clone()),
                };
                match found {
                    Ok(found) => found,
                    Err(reason) => {
                        if concord_name {
                            reasons.push(format!(
                                "the dependency `{key}` in `{label}` inherits `workspace = true`, but {reason}, so its package is unknown"
                            ));
                        }
                        continue;
                    }
                }
            } else {
                (package.to_path_buf(), entry.clone())
            };
            let details = entry.as_table();
            let dependency_package = match details.and_then(|table| table.get("package")) {
                None => key.as_str(),
                Some(toml::Value::String(renamed)) => renamed.as_str(),
                Some(_) => {
                    if concord_name {
                        reasons.push(format!(
                            "the dependency `{key}` in `{label}` has a `package` that is not a string, so its package is unknown"
                        ));
                    }
                    continue;
                }
            };
            match (concord_name, concord_extern_name(dependency_package)) {
                (false, None) => {}
                (false, Some(recognized)) => reasons.push(format!(
                    "the dependency `{key}` in `{label}` is the Concord package `{dependency_package}` under the crate name `{extern_name}`, which the tool does not recognize as Concord (only `{recognized}` names that package)"
                )),
                (true, None) => reasons.push(format!(
                    "the dependency `{key}` in `{label}` is the package `{dependency_package}`, not the Concord package the crate name `{extern_name}` denotes"
                )),
                (true, Some(recognized)) if recognized != extern_name => reasons.push(format!(
                    "the dependency `{key}` in `{label}` is the Concord package `{dependency_package}` under the crate name `{extern_name}`, which the tool recognizes as another Concord package (only `{recognized}` names `{dependency_package}`)"
                )),
                (true, Some(_)) => {
                    let path = details
                        .and_then(|table| table.get("path"))
                        .map(|path| path.as_str().ok_or(()));
                    let Some(path) = path else {
                        declared.push(extern_name.clone());
                        continue;
                    };
                    let Ok(path) = path else {
                        reasons.push(format!(
                            "the dependency `{key}` in `{label}` has a `path` that is not a string, so its package is unknown"
                        ));
                        continue;
                    };
                    // Named as written (relative to the workspace root for
                    // an inherited entry), so reasons do not depend on where
                    // the input tree is.
                    let written = format!("`{}/Cargo.toml`", path.trim_end_matches('/'));
                    let named = fs::read_to_string(base.join(path).join("Cargo.toml"))
                        .map_err(|_| format!("the manifest {written} could not be read"))
                        .and_then(|text| {
                            text.parse::<toml::Table>().map_err(|_| {
                                format!("the manifest {written} is not a valid TOML document")
                            })
                        })
                        .map(|target| {
                            target
                                .get("package")
                                .and_then(toml::Value::as_table)
                                .and_then(|table| toml_string(table, "name"))
                                .map(str::to_owned)
                        });
                    match named {
                        Ok(Some(name))
                            if concord_extern_name(&name) == Some(extern_name.as_str()) =>
                        {
                            declared.push(extern_name.clone());
                        }
                        Ok(Some(name)) => reasons.push(format!(
                            "the `path` dependency `{key}` in `{label}` leads to the package `{name}`, not the Concord package `{dependency_package}`"
                        )),
                        Ok(None) => reasons.push(format!(
                            "the `path` dependency `{key}` in `{label}` leads to a manifest without a package name, so its package is unknown"
                        )),
                        Err(reason) => reasons.push(format!(
                            "the `path` dependency `{key}` in `{label}` cannot be identified: {reason}"
                        )),
                    }
                }
            }
        }
    }
    reasons.extend(patch_reasons(package, manifest));
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons.join("; "))
    }
}

/// The package name a `[replace]` key (a package ID specification such as
/// `concord:3.0.0`, `concord@3.0.0` or `https://index#concord@3.0.0`) names.
fn replace_spec_name(spec: &str) -> &str {
    let tail = spec.rsplit('#').next().unwrap_or(spec);
    let tail = if tail.contains("://") {
        tail.trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(tail)
    } else {
        tail
    };
    tail.split(['@', ':']).next().unwrap_or(tail)
}

/// Focused review of R3 B01: a `[patch.<registry>]` or `[replace]` entry of
/// the package's manifest or of its workspace root's (where Cargo reads
/// them) that redirects a Concord package to another package, or to a `path`
/// whose package is not that Concord package, makes the Concord crate names
/// denote something else. A `git` or registry patch is identified by its
/// package name, like a `git` or registry dependency.
fn patch_reasons(package: &Path, manifest: &toml::Table) -> Vec<String> {
    let mut manifests = vec![(
        package.to_path_buf(),
        manifest_label(package, &package.join("Cargo.toml")),
        manifest.clone(),
    )];
    if let Ok(Some((root, table))) = workspace_root(package, manifest) {
        if root != package {
            let label = manifest_label(package, &root.join("Cargo.toml"));
            manifests.push((root, label, table));
        }
    }
    let mut reasons = Vec::new();
    for (base, label, table) in &manifests {
        let mut entries = Vec::new();
        if let Some(patches) = table.get("patch").and_then(toml::Value::as_table) {
            for (registry, patched) in patches {
                let Some(patched) = patched.as_table() else {
                    continue;
                };
                for (key, entry) in patched {
                    entries.push((
                        format!("[patch.{registry}]"),
                        key.as_str(),
                        key.as_str(),
                        entry,
                    ));
                }
            }
        }
        if let Some(replaced) = table.get("replace").and_then(toml::Value::as_table) {
            for (key, entry) in replaced {
                entries.push((
                    "[replace]".to_owned(),
                    key.as_str(),
                    replace_spec_name(key),
                    entry,
                ));
            }
        }
        for (section, key, patched, entry) in entries {
            let Some(extern_name) = concord_extern_name(patched) else {
                continue;
            };
            let place = format!("the entry `{key}` in `{section}` of {label}");
            let details = entry.as_table();
            match details.and_then(|table| table.get("package")) {
                Some(toml::Value::String(renamed))
                    if concord_extern_name(renamed) != Some(extern_name) =>
                {
                    reasons.push(format!(
                        "{place} redirects the Concord package `{patched}` to the package `{renamed}`"
                    ));
                    continue;
                }
                Some(toml::Value::String(_)) | None => {}
                Some(_) => {
                    reasons.push(format!(
                        "{place} has a `package` that is not a string, so the package it redirects the Concord package `{patched}` to is unknown"
                    ));
                    continue;
                }
            }
            let Some(path) = details.and_then(|table| table.get("path")) else {
                continue;
            };
            let Some(path) = path.as_str() else {
                reasons.push(format!(
                    "{place} has a `path` that is not a string, so the package it redirects the Concord package `{patched}` to is unknown"
                ));
                continue;
            };
            let written = format!("`{}/Cargo.toml`", path.trim_end_matches('/'));
            let named = fs::read_to_string(base.join(path).join("Cargo.toml"))
                .map_err(|_| format!("the manifest {written} could not be read"))
                .and_then(|text| {
                    text.parse::<toml::Table>()
                        .map_err(|_| format!("the manifest {written} is not a valid TOML document"))
                })
                .map(|target| {
                    target
                        .get("package")
                        .and_then(toml::Value::as_table)
                        .and_then(|table| toml_string(table, "name"))
                        .map(str::to_owned)
                });
            match named {
                Ok(Some(name)) if concord_extern_name(&name) == Some(extern_name) => {}
                Ok(Some(name)) => reasons.push(format!(
                    "{place} redirects the Concord package `{patched}` to the `path` `{path}`, whose package is `{name}`"
                )),
                Ok(None) => reasons.push(format!(
                    "{place} redirects the Concord package `{patched}` to the `path` `{path}`, whose manifest has no package name"
                )),
                Err(reason) => reasons.push(format!(
                    "{place} redirects the Concord package `{patched}` to the `path` `{path}`, which cannot be identified: {reason}"
                )),
            }
        }
    }
    reasons
}

/// Focused review of R3 B01: `extern crate other as concord;` makes a
/// Concord crate name denote another crate whatever the manifest declares,
/// and `extern crate concord as x;` makes `x` name `concord`, which only the
/// manifest can prove to be the Concord package. `extern crate self as
/// concord;` is the package's own library, genuine only in the Concord
/// package of that name (`own`). `(file, line, crate, alias)` come from the
/// crate's files; `package` is the package directory.
fn extern_crate_reasons(
    package: &Path,
    items: &[(PathBuf, usize, String, String)],
    declared: &[String],
    own_library: Option<&str>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    for (file, line, name, alias) in items {
        let written = if name == alias {
            format!("extern crate {name};")
        } else {
            format!("extern crate {name} as {alias};")
        };
        let place = format!("`{written}` ({}:{line})", relative_display(package, file));
        let own = name == "self";
        if rust_file::is_concord_crate(alias) && name != alias {
            if own && own_library == Some(alias.as_str()) {
                continue;
            }
            let concord = CONCORD_PACKAGES
                .iter()
                .find(|(_, extern_name)| extern_name == alias)
                .map_or(alias.as_str(), |(package, _)| package);
            let denoted = if own {
                "this package's own library".to_owned()
            } else {
                format!("the crate `{name}`")
            };
            reasons.push(format!(
                "{place} makes the crate name `{alias}` denote {denoted}, not the Concord package `{concord}`"
            ));
        } else if rust_file::is_concord_crate(name)
            && name != alias
            && !declared.iter().any(|known| known == name)
        {
            reasons.push(format!(
                "{place} names the crate `{name}`, which the manifest does not declare, so it is not proved to be the Concord package"
            ));
        }
    }
    reasons
}

/// The targets and edition a package's `Cargo.toml` declares, read as a TOML
/// document. A manifest that cannot be read or parsed, or that does not
/// determine the package name or the edition, makes the edition unknown (see
/// [`Edition::Unknown`]) instead of assuming edition 2015; an unknown name
/// leaves the library unreachable by name from the package's other targets.
fn package_targets(package: &Path) -> PackageTargets {
    let path = package.join("Cargo.toml");
    let manifest = match read_manifest(package, &path) {
        Ok(manifest) => manifest,
        Err(reason) => {
            return PackageTargets {
                edition: Edition::Unknown(reason),
                ..PackageTargets::default()
            };
        }
    };
    let Some(package_table) = manifest.get("package").and_then(toml::Value::as_table) else {
        return PackageTargets {
            edition: Edition::Unknown(
                "the manifest `Cargo.toml` declares no `[package]` table".to_owned(),
            ),
            ..PackageTargets::default()
        };
    };
    let package_name = toml_string(package_table, "name");
    let edition = match package_table.get("edition") {
        _ if package_name.is_none() => Edition::Unknown(
            "the manifest `Cargo.toml` declares no package name string".to_owned(),
        ),
        // Cargo's default edition.
        None => Edition::Rust2015,
        Some(toml::Value::String(value)) => edition_named(value, "the manifest `Cargo.toml`"),
        Some(toml::Value::Table(inherited))
            if inherited.get("workspace").and_then(toml::Value::as_bool) == Some(true) =>
        {
            workspace_edition(package, &manifest)
        }
        Some(_) => Edition::Unknown(
            "`package.edition` in the manifest `Cargo.toml` is neither an edition string nor `{ workspace = true }`".to_owned(),
        ),
    };
    let library_table = manifest.get("lib").and_then(toml::Value::as_table);
    let library_name = library_table.and_then(|table| toml_string(table, "name"));
    let library_path = library_table
        .and_then(|table| toml_string(table, "path"))
        .map(|relative| package.join(relative));
    let mut target_paths = Vec::new();
    for kind in ["bin", "test", "example", "bench"] {
        let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) else {
            continue;
        };
        for target in targets {
            if let Some(relative) = target
                .as_table()
                .and_then(|table| toml_string(table, "path"))
            {
                target_paths.push(package.join(relative));
            }
        }
    }
    let mut concord_declared = Vec::new();
    let concord_refusal = concord_identity(
        package,
        &manifest,
        package_name,
        library_name,
        &mut concord_declared,
    )
    .err();
    let library = library_name
        .or(package_name)
        .map(|name| name.replace('-', "_"));
    let concord_own = library
        .clone()
        .filter(|own| package_name.and_then(concord_extern_name) == Some(own.as_str()));
    PackageTargets {
        library,
        library_path,
        target_paths,
        edition,
        concord_refusal,
        concord_declared,
        concord_own,
    }
}

/// Indexes the declarations and name-resolution model of one crate from its
/// files (read-only). Files that fail to read or parse are skipped here and
/// reported when they are themselves migrated.
fn index_crate(layout: &Layout, files: &[PathBuf]) -> DeclIndex {
    let sources = files
        .iter()
        .filter_map(|path| {
            fs::read_to_string(path)
                .ok()
                .map(|source| (path.clone(), source))
        })
        .collect::<Vec<_>>();
    DeclIndex::build(layout, &sources)
}

/// The stack of the migration thread. Parsing and every syntax-tree walk
/// recurse per nesting level; a file nested deeper than
/// [`rust_file::MAX_NESTING`] is refused before it is walked, and this stack
/// holds the recursion up to that bound with a wide margin (the pages are
/// reserved, not committed).
const MIGRATION_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Runs `work` on a thread with [`MIGRATION_STACK_BYTES`] of stack, so deep
/// but bounded nesting is examined instead of overflowing the caller's stack.
/// A panic is propagated unchanged.
fn on_migration_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("migrate-v3".to_owned())
            .stack_size(MIGRATION_STACK_BYTES)
            .spawn_scoped(scope, work);
        match handle {
            Ok(handle) => handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            Err(error) => panic!("could not start the migration thread: {error}"),
        }
    })
}

/// Migrates a file or directory tree without writing anything.
pub(crate) fn migrate(input: &Path, settings: &Settings) -> Result<Migration, String> {
    on_migration_stack(|| migrate_on_this_thread(input, settings))
}

fn migrate_on_this_thread(input: &Path, settings: &Settings) -> Result<Migration, String> {
    let metadata = fs::metadata(input)
        .map_err(|error| format!("could not read --input {}: {error}", input.display()))?;
    let (root, files) = if metadata.is_file() {
        let root = input
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        (root, vec![input.to_path_buf()])
    } else {
        let mut files = Vec::new();
        collect_rust_files(input, &mut files)?;
        (input.to_path_buf(), files)
    };

    // Group input files by crate and index every declaration of that crate,
    // including crate files outside the input tree (read-only).
    let mut crates: BTreeMap<PathBuf, DeclIndex> = BTreeMap::new();
    let mut refusals: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut file_crate = Vec::with_capacity(files.len());
    for file in &files {
        let key = crate_directory(file).unwrap_or_else(|| root.clone());
        if !crates.contains_key(&key) {
            let mut crate_files = Vec::new();
            let package = key.join("Cargo.toml").is_file();
            if package {
                collect_crate_files(&key, &mut crate_files)?;
            } else {
                crate_files.extend(
                    files
                        .iter()
                        .filter(|candidate| crate_directory(candidate).is_none())
                        .cloned(),
                );
            }
            let mut targets = if package {
                package_targets(&key)
            } else {
                PackageTargets::default()
            };
            let concord_refusal = targets.concord_refusal.clone();
            let concord_declared = std::mem::take(&mut targets.concord_declared);
            let concord_own = targets.concord_own.take();
            let layout = Layout {
                package: package.then(|| key.clone()),
                library: targets.library,
                library_path: targets.library_path,
                target_paths: targets.target_paths,
                edition: targets.edition,
                base: key.clone(),
                // A lone file outside any package is its own crate root.
                standalone: !package && crate_files.len() == 1,
            };
            let index = index_crate(&layout, &crate_files);
            let mut reasons = concord_refusal.into_iter().collect::<Vec<_>>();
            if package {
                reasons.extend(extern_crate_reasons(
                    &key,
                    index.extern_crates(),
                    &concord_declared,
                    concord_own.as_deref(),
                ));
            }
            if !reasons.is_empty() {
                refusals.insert(key.clone(), reasons.join("; "));
            }
            crates.insert(key.clone(), index);
        }
        file_crate.push(key);
    }
    // A caller in one crate of the run may call a setter of a type generated
    // by another crate of the run; such calls are reported, never rewritten.
    let values = crates
        .iter()
        .map(|(key, index)| (key.clone(), index.value_names()))
        .collect::<Vec<_>>();
    let v3_names = crates
        .iter()
        .map(|(key, index)| {
            (
                key.clone(),
                index.v3_only_names(),
                index.v3_only_value_names(),
            )
        })
        .collect::<Vec<_>>();
    let removed_names = crates
        .iter()
        .map(|(key, index)| (key.clone(), index.v3_removed_names()))
        .collect::<Vec<_>>();
    for (key, index) in &mut crates {
        index.set_other_crate_values(
            values
                .iter()
                .filter(|(other, _)| other != key)
                .flat_map(|(_, names)| names.iter().cloned())
                .collect(),
        );
        let others = v3_names
            .iter()
            .filter(|(other, _, _)| other != key)
            .collect::<Vec<_>>();
        index.set_other_crate_v3_names(
            others
                .iter()
                .flat_map(|(_, names, _)| names.iter().cloned())
                .collect(),
            others
                .iter()
                .flat_map(|(_, _, values)| values.iter().cloned())
                .collect(),
        );
        index.set_other_crate_removed_names(
            removed_names
                .iter()
                .filter(|(other, _)| other != key)
                .flat_map(|(_, names)| names.iter().cloned())
                .collect(),
        );
    }

    let mut migration = Migration {
        root: root.clone(),
        input: input.to_path_buf(),
        files: Vec::new(),
        diagnostics: Vec::new(),
        identities: Vec::new(),
        declarations: 0,
    };
    for (file, key) in files.iter().zip(&file_crate) {
        let source = fs::read_to_string(file)
            .map_err(|error| format!("could not read {}: {error}", file.display()))?;
        let relative = if metadata.is_file() {
            file.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        } else {
            relative_display(&root, file)
        };
        let mut outcome = migrate_source_at(&relative, &source, &crates[key], Some(file), settings);
        if let Some(reason) = refusals.get(key) {
            outcome = refuse_foreign_dependency(&relative, &source, outcome, reason);
        }
        migration.declarations += outcome.declarations;
        migration.diagnostics.extend(outcome.diagnostics);
        migration.identities.extend(outcome.identities);
        migration.files.push(FileOutcome {
            relative,
            original: source,
            converted: outcome.converted,
            edits: outcome.edits,
        });
    }
    migration.diagnostics.sort_by(|left, right| {
        (&left.file, left.line, left.column, left.code).cmp(&(
            &right.file,
            right.line,
            right.column,
            right.code,
        ))
    });
    Ok(migration)
}

pub(crate) struct SourceOutcome {
    pub(crate) converted: String,
    pub(crate) edits: usize,
    pub(crate) declarations: usize,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) identities: Vec<EndpointIdentity>,
}

/// Migrates one Rust source text. `index` holds the crate's declarations; an
/// empty index makes the text a standalone crate of its own, migrated on the
/// migration stack like [`migrate`].
#[cfg(test)]
pub(crate) fn migrate_source(
    relative: &str,
    source: &str,
    index: &DeclIndex,
    settings: &Settings,
) -> SourceOutcome {
    if index.is_empty() {
        return on_migration_stack(|| {
            migrate_source_at(relative, source, &DeclIndex::default(), None, settings)
        });
    }
    migrate_source_at(relative, source, index, None, settings)
}

/// [`migrate_source`] for the crate file at `path`, which places the text in
/// the crate's module tree for caller resolution.
fn migrate_source_at(
    relative: &str,
    source: &str,
    index: &DeclIndex,
    path: Option<&Path>,
    settings: &Settings,
) -> SourceOutcome {
    let lines = LineIndex::new(source);
    let locate = |raw: RawDiagnostic| {
        let (line, column) = lines.line_col(source, raw.offset);
        Diagnostic {
            code: raw.code,
            file: relative.to_owned(),
            line,
            column,
            message: raw.message,
            suggestion: raw.suggestion,
        }
    };
    let parsed = match parse_rust(source) {
        Ok(parsed) => parsed,
        Err(failure) => {
            let diagnostic = match failure {
                ParseFailure::Syntax(error) => RawDiagnostic::new(
                    Code::ParseRust,
                    error.span().byte_range().start + stripped_prefix(source),
                    format!("could not parse Rust: {error}"),
                ),
                ParseFailure::Nesting { offset } => RawDiagnostic::new(
                    Code::NestingLimit,
                    offset,
                    format!(
                        "this delimiter or generic-argument bracket nests deeper than {MAX_NESTING} levels (delimiters and open `<` counted together), the bound the tool examines; the file was left unchanged and nothing in it was migrated. Reduce the nesting (for example by extracting functions, modules or type aliases) and rerun"
                    ),
                ),
            };
            return SourceOutcome {
                converted: source.to_owned(),
                edits: 0,
                declarations: 0,
                diagnostics: vec![locate(diagnostic)],
                identities: Vec::new(),
            };
        }
    };
    let mut imports = Imports::collect(&parsed.file);
    if imports.has_bare_api() {
        // A file its crate's index does not hold is its own crate root.
        let proof = path
            .and_then(|path| index.bare_api_proof(path))
            .unwrap_or_else(|| {
                let own_rules = imports.defines_api_macro_rules().then_some(relative);
                rust_file::bare_api_proof(&parsed.file.items, relative, own_rules)
            });
        imports.prove_bare_api(proof);
    }
    let mut edits = EditSet::default();
    let mut raw = Vec::new();
    for (offset, reason) in imports.unresolved_api_macros() {
        raw.push(RawDiagnostic::new(
            Code::ApiMacroUnresolved,
            offset + parsed.base,
            format!(
                "this `api!` invocation was not migrated because its macro path is not proved to be Concord's `api!`: {reason}. If it is Concord's declaration macro, invoke it as `api!` with `use concord::api;` (or `concord::api!`) in this module and rerun; otherwise it is not a Concord declaration"
            ),
        ));
    }
    let mut identities = Vec::new();
    let mut declarations = 0;
    let mut prelude_imports = HashMap::new();
    for invocation in find_api_invocations(&parsed, &imports) {
        match syntax::parse_api(invocation.tokens.clone(), parsed.base) {
            Ok(api) => {
                declarations += 1;
                let input = DeclarationInput {
                    source,
                    file: relative,
                    settings,
                    imports: &imports,
                    scope: invocation.scope.as_ref(),
                    offset: invocation.offset,
                };
                declaration::convert(
                    &api,
                    &input,
                    DeclarationOutput {
                        edits: &mut edits,
                        diagnostics: &mut raw,
                        identities: &mut identities,
                        prelude_imports: &mut prelude_imports,
                    },
                );
            }
            Err(error) => {
                if looks_like_v3(&invocation.tokens) {
                    raw.push(RawDiagnostic::new(
                        Code::AlreadyV3,
                        invocation.offset,
                        "this `api!` invocation already uses v3-only syntax (relative path template, direct placement or Init pagination) and was left unchanged",
                    ));
                } else {
                    let offset = error.span().byte_range().start + parsed.base;
                    raw.push(RawDiagnostic::new(
                        Code::ParseDeclaration,
                        offset.max(invocation.offset),
                        format!("not a valid v2.1 declaration: {error}"),
                    ));
                }
            }
        }
    }
    let crate_index;
    let (caller_index, location) = if index.is_empty() && declarations > 0 {
        // A standalone file: its own declarations and items form the crate.
        let own_path = PathBuf::from(relative);
        let layout = Layout {
            standalone: true,
            ..Layout::default()
        };
        crate_index = DeclIndex::build(&layout, &[(own_path.clone(), source.to_owned())]);
        (&crate_index, crate_index.file_scope(&own_path))
    } else {
        (index, path.and_then(|path| index.file_scope(path)))
    };
    callers::migrate(
        &parsed.file,
        &CallerInput {
            source,
            base: parsed.base,
            index: caller_index,
            imports: &imports,
            concord_file: declarations > 0 || imports.uses_concord(),
            location,
        },
        &mut edits,
        &mut raw,
    );
    let converted = edits.apply(source);
    let edit_count = edits.len();
    SourceOutcome {
        converted,
        edits: edit_count,
        declarations,
        diagnostics: raw.into_iter().map(locate).collect(),
        identities,
    }
}

/// Leaves a file of a package whose manifest does not establish the Concord
/// crate names' identity unchanged: when the tool would have edited or
/// reported anything in it (every such decision assumes that `concord::` and
/// the other Concord crate names are Concord's), the outcome is replaced by
/// one `C3-MIGRATE-DEPENDENCY-IDENTITY` diagnostic giving `reason`. A file
/// with nothing to migrate is not reported.
fn refuse_foreign_dependency(
    relative: &str,
    source: &str,
    outcome: SourceOutcome,
    reason: &str,
) -> SourceOutcome {
    if outcome.edits == 0 && outcome.declarations == 0 && outcome.diagnostics.is_empty() {
        return outcome;
    }
    let (line, column) = LineIndex::new(source).line_col(source, 0);
    SourceOutcome {
        converted: source.to_owned(),
        edits: 0,
        declarations: 0,
        diagnostics: vec![Diagnostic {
            code: Code::DependencyIdentity,
            file: relative.to_owned(),
            line,
            column,
            message: format!(
                "the package manifest does not establish that the Concord crate names (`concord`, `concord_core`, `concord_extras`, `concord_macros`) denote the genuine Concord packages: {reason}. Every migration decision in this file ({} edit(s), {} declaration(s) and {} diagnostic(s) it would have produced) depends on that identity, so the file was left unchanged. Declare the Concord packages under their own crate names (a `package = \"..\"` rename only between the matching names, such as `concord_core = {{ package = \"concord-core\" }}`) and rerun, or migrate the file by hand",
                outcome.edits,
                outcome.declarations,
                outcome.diagnostics.len()
            ),
            suggestion: None,
        }],
        identities: Vec::new(),
    }
}

/// Recognizes v3-only declaration syntax inside a token stream that failed the
/// v2.1 grammar: `path "template"`, `init` pagination, `absent on`, an
/// explicit rate key label, or a direct query/header placement.
fn looks_like_v3(tokens: &proc_macro2::TokenStream) -> bool {
    use proc_macro2::{Delimiter, TokenTree};
    let trees = tokens.clone().into_iter().collect::<Vec<_>>();
    let punct = |index: usize, character: char| matches!(trees.get(index), Some(TokenTree::Punct(punct)) if punct.as_char() == character);
    for (index, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Ident(ident) if ident == "key" => {
                if let Some(TokenTree::Group(group)) = trees.get(index + 1) {
                    let inner = group.stream().into_iter().collect::<Vec<_>>();
                    let labelled = inner.windows(2).any(|pair| {
                        matches!(&pair[0], TokenTree::Literal(_))
                            && matches!(&pair[1], TokenTree::Punct(punct) if punct.as_char() == '=')
                    });
                    if group.delimiter() == Delimiter::Bracket && labelled {
                        return true;
                    }
                }
            }
            TokenTree::Ident(ident) if ident == "query" || ident == "header" => {
                let mut next = index + 1;
                if matches!(trees.get(next), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
                {
                    next += 1;
                }
                if matches!(trees.get(next), Some(TokenTree::Ident(each)) if each == "each") {
                    next += 1;
                }
                if matches!(trees.get(next), Some(TokenTree::Ident(_)))
                    && (punct(next + 1, '?') || punct(next + 1, ':'))
                {
                    return true;
                }
            }
            TokenTree::Ident(ident) if ident == "path" => {
                if let Some(TokenTree::Literal(literal)) = trees.get(index + 1) {
                    if literal.to_string().starts_with('"') {
                        return true;
                    }
                }
                if let (Some(TokenTree::Ident(mode)), Some(TokenTree::Literal(literal))) =
                    (trees.get(index + 1), trees.get(index + 2))
                {
                    if mode == "replace" && literal.to_string().starts_with('"') {
                        return true;
                    }
                }
            }
            TokenTree::Ident(ident) if ident == "init" || ident == "absent" => {
                if matches!(trees.get(index + 1), Some(TokenTree::Ident(_))) {
                    return true;
                }
            }
            TokenTree::Group(group) if looks_like_v3(&group.stream()) => return true,
            _ => {}
        }
    }
    false
}
