#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const GENERATED_FEATURES_START: &str = "<!-- BEGIN GENERATED FEATURES -->";
const GENERATED_FEATURES_END: &str = "<!-- END GENERATED FEATURES -->";
const GENERATED_LIMITS_START: &str = "<!-- BEGIN GENERATED LIMITS -->";
const GENERATED_LIMITS_END: &str = "<!-- END GENERATED LIMITS -->";

const REQUIRED_RECIPES: &[&str] = &[
    "fmt",
    "check",
    "check-all",
    "lint",
    "test",
    "test-all",
    "test-msrv",
    "ui",
    "fixtures",
    "examples",
    "docs",
    "security",
    "perf-smoke",
    "perf-scale",
    "verify",
];

const LIMITS: &[(&str, &str, &str, &str)] = &[
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_REQUEST_BODY_BYTES",
        "Request body",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_RESPONSE_BODY_BYTES",
        "Buffered successful response",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_STREAM_RESPONSE_BYTES",
        "Streaming response",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_STATUS_BODY_BYTES",
        "Captured terminal status body",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_PROVIDER_REQUEST_BYTES",
        "Credential-provider request body",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_PROVIDER_RESPONSE_BYTES",
        "Credential-provider response body",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_PAGINATION_MAX_PAGES",
        "Pagination pages",
        "pages",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_PAGINATION_MAX_ITEMS",
        "Pagination accumulated items",
        "items",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_PAGINATION_IDENTITY_BYTES",
        "Retained pagination identities",
        "bytes",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS",
        "Authentication-failure cooldown cap",
        "seconds",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS",
        "Server-directed rate-limit cooldown cap",
        "seconds",
    ),
    (
        "crates/concord-core/src/runtime/defaults.rs",
        "DEFAULT_RATE_LIMIT_KEY_BYTES",
        "Default limiter-prepared rate-limit key",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_FORMATTED_PUBLIC_VALUE_BYTES",
        "One formatted public value",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RUNTIME_ENDPOINT_COMPONENTS",
        "Runtime endpoint top-level structure",
        "components",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES",
        "Runtime endpoint aggregate public shape",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS",
        "Runtime endpoint authentication placements",
        "placements",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_ID_BYTES",
        "Rate-limit rule ID",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_KEY_PARTS",
        "Rate-limit key",
        "parts",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_KEY_PART_NAME_BYTES",
        "Rate-limit key-part name",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_KEY_BYTES",
        "Rate-limit hard aggregate key",
        "bytes",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_WINDOWS_PER_RULE",
        "Rate-limit rule",
        "windows",
    ),
    (
        "crates/concord-core/src/limits.rs",
        "MAX_RATE_LIMIT_RULES_PER_PLAN",
        "Rate-limit plan",
        "rules",
    ),
];

const MACRO_LIMIT_MIRRORS: &[(&str, &str)] = &[
    (
        "MAX_RATE_LIMIT_ID_BYTES",
        "crates/concord-macros/src/lib.rs",
    ),
    (
        "MAX_RATE_LIMIT_WINDOWS_PER_RULE",
        "crates/concord-macros/src/lib.rs",
    ),
    (
        "MAX_RATE_LIMIT_RULES_PER_PLAN",
        "crates/concord-macros/src/lib.rs",
    ),
];

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
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("docs") => {
            let write = match arguments.next().as_deref() {
                None => false,
                Some("--write") => true,
                Some(_) => {
                    return Err("usage: cargo run -p concord-xtask -- docs [--write]".into());
                }
            };
            if arguments.next().is_some() {
                return Err("usage: cargo run -p concord-xtask -- docs [--write]".into());
            }
            check_docs(&root, write)
        }
        Some("fixtures") => {
            if arguments.next().is_some() {
                return Err("usage: cargo run -p concord-xtask -- fixtures".into());
            }
            run_fixtures(&root)
        }
        _ => Err("usage: cargo run -p concord-xtask -- <docs [--write] | fixtures>".into()),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let mut path = env::current_dir().map_err(|error| error.to_string())?;
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

fn check_docs(root: &Path, write: bool) -> Result<(), String> {
    check_macro_limit_mirrors(root)?;
    synchronize_generated_section(
        &root.join("docs/reference/features.md"),
        GENERATED_FEATURES_START,
        GENERATED_FEATURES_END,
        &generated_feature_table(root)?,
        write,
    )?;
    synchronize_generated_section(
        &root.join("docs/reference/limits.md"),
        GENERATED_LIMITS_START,
        GENERATED_LIMITS_END,
        &generated_limit_table(root)?,
        write,
    )?;
    check_recipe_inventory(root)?;
    check_example_proofs(root)?;
    check_user_vocabulary(root)?;
    check_markdown_links(root)?;
    println!("documentation checks passed");
    Ok(())
}

fn generated_feature_table(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("crates/concord/Cargo.toml");
    let manifest = read(&manifest_path)?;
    let section = toml_section(&manifest, "features")
        .ok_or_else(|| format!("{} has no [features] section", manifest_path.display()))?;
    let entries = section
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=')
                .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        })
        .collect::<Vec<_>>();
    let defaults = entries
        .iter()
        .find(|(name, _)| name == "default")
        .map(|(_, value)| quoted_values(value))
        .unwrap_or_default();

    let mut table =
        String::from("| Feature | Enabled by `default` | Capability |\n|---|---:|---|\n");
    for (name, _) in entries {
        let enabled = if name == "default" {
            "meta"
        } else if defaults.contains(&name) {
            "yes"
        } else {
            "no"
        };
        let description = feature_description(&name)
            .ok_or_else(|| format!("feature `{name}` needs a public documentation description"))?;
        table.push_str(&format!("| `{name}` | {enabled} | {description} |\n"));
    }
    Ok(table)
}

fn feature_description(name: &str) -> Option<&'static str> {
    match name {
        "default" => Some("Default TLS, HTTP/2, and process-local governor"),
        "default-tls" => Some("HTTPS with the ordinary default TLS backend"),
        "http2" => Some("HTTP/2 transport support"),
        "rate-limit-governor" => Some("Process-local generated rate-plan enforcement"),
        "json" => Some("`Json<T>` request and response codec"),
        "oauth2-client" => Some("OAuth2 client-credentials provider helper"),
        "records" => Some("Generic bounded record streaming"),
        "csv" => Some("CSV record decoder (also enables `records`)"),
        "ndjson" => Some("NDJSON record decoder (also enables `records`)"),
        "multipart" => Some("Managed multipart request construction"),
        "gzip" => Some("Gzip response decompression"),
        "brotli" => Some("Brotli response decompression"),
        "deflate" => Some("Deflate response decompression"),
        "dangerous-raw-response" => Some("Bounded undecoded successful-response terminal"),
        "dangerous-custom-retry" => Some("Custom native transport retry policy"),
        "dangerous-dev-tools" => Some("Internal development instrumentation"),
        _ => None,
    }
}

fn generated_limit_table(root: &Path) -> Result<String, String> {
    let mut table = String::from("| Bound | Value | Unit |\n|---|---:|---|\n");
    for (relative_source, constant, label, unit) in LIMITS {
        let number = integer_constant(root, relative_source, constant)?;
        table.push_str(&format!("| {label} | {} | {unit} |\n", grouped(number)));
    }
    Ok(table)
}

fn check_macro_limit_mirrors(root: &Path) -> Result<(), String> {
    for (constant, macro_source) in MACRO_LIMIT_MIRRORS {
        let core = integer_constant(root, "crates/concord-core/src/limits.rs", constant)?;
        let mirrored = integer_constant(root, macro_source, constant)?;
        if core != mirrored {
            return Err(format!(
                "{constant} is {core} in core but {mirrored} in {macro_source}"
            ));
        }
    }
    Ok(())
}

fn integer_constant(root: &Path, relative_source: &str, constant: &str) -> Result<u64, String> {
    let source_path = root.join(relative_source);
    let source = read(&source_path)?;
    let public_prefix = format!("pub const {constant}:");
    let private_prefix = format!("const {constant}:");
    let line = source
        .lines()
        .find(|line| {
            let line = line.trim_start();
            line.starts_with(&public_prefix) || line.starts_with(&private_prefix)
        })
        .ok_or_else(|| format!("missing {constant} in {}", source_path.display()))?;
    let value = line
        .split_once('=')
        .and_then(|(_, value)| value.split_once(';').map(|(value, _)| value.trim()))
        .ok_or_else(|| format!("could not parse {constant} in {}", source_path.display()))?;
    value
        .replace('_', "")
        .parse::<u64>()
        .map_err(|_| format!("{constant} must remain one integer literal"))
}

fn synchronize_generated_section(
    path: &Path,
    start: &str,
    end: &str,
    generated: &str,
    write: bool,
) -> Result<(), String> {
    let current = read(path)?;
    let expected = replace_section(&current, start, end, generated)?;
    if current == expected {
        return Ok(());
    }
    if write {
        fs::write(path, expected).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("updated {}", path.display());
        Ok(())
    } else {
        Err(format!(
            "{} is stale; run `cargo run -p concord-xtask -- docs --write`",
            path.display()
        ))
    }
}

fn replace_section(
    current: &str,
    start: &str,
    end: &str,
    generated: &str,
) -> Result<String, String> {
    let start_offset = current
        .find(start)
        .ok_or_else(|| format!("missing marker {start}"))?;
    let content_start = start_offset + start.len();
    let relative_end = current[content_start..]
        .find(end)
        .ok_or_else(|| format!("missing marker {end}"))?;
    let content_end = content_start + relative_end;
    let mut output = String::with_capacity(current.len() + generated.len());
    output.push_str(&current[..content_start]);
    output.push('\n');
    output.push_str(generated.trim_end());
    output.push('\n');
    output.push_str(&current[content_end..]);
    Ok(output)
}

fn check_recipe_inventory(root: &Path) -> Result<(), String> {
    let justfile = read(&root.join("justfile"))?;
    let recipes = justfile
        .lines()
        .filter_map(|line| {
            if line.chars().next().is_some_and(char::is_whitespace)
                || line.starts_with('#')
                || line.starts_with("set ")
            {
                return None;
            }
            line.split_once(':').map(|(name, _)| name.trim().to_owned())
        })
        .collect::<BTreeSet<_>>();
    let missing = REQUIRED_RECIPES
        .iter()
        .filter(|recipe| !recipes.contains(**recipe))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "justfile is missing recipes: {}",
            missing.join(", ")
        ))
    }
}

fn check_example_proofs(root: &Path) -> Result<(), String> {
    let guide = root.join("docs/guide");
    for path in markdown_files(&guide)? {
        let text = read(&path)?;
        if !text.contains("crates/concord-examples/src/")
            || !text.contains("crates/concord-examples/tests/")
        {
            return Err(format!(
                "{} must link to an importable example and a proving test",
                path.display()
            ));
        }
    }
    Ok(())
}

fn check_user_vocabulary(root: &Path) -> Result<(), String> {
    let mut paths = vec![root.join("README.md"), root.join("docs/README.md")];
    for directory in ["guide", "concepts", "recipes", "reference"] {
        paths.extend(markdown_files(&root.join("docs").join(directory))?);
    }
    for path in paths {
        let text = read(&path)?;
        for retired in [
            "PreparedEndpoint",
            "concord_core",
            "concord_macros",
            "concord_extras",
        ] {
            if text.contains(retired) {
                return Err(format!(
                    "{} contains implementation or retired term `{retired}`",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn check_markdown_links(root: &Path) -> Result<(), String> {
    let mut paths = vec![root.join("README.md")];
    paths.extend(markdown_files(&root.join("docs"))?);
    for path in paths {
        if path
            .components()
            .any(|component| component.as_os_str() == "evidence")
        {
            continue;
        }
        let text = read(&path)?;
        let mut fenced = false;
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            for target in markdown_link_targets(line) {
                let target = target.trim_matches(['<', '>']);
                if target.is_empty()
                    || target.starts_with('#')
                    || target.contains("://")
                    || target.starts_with("mailto:")
                {
                    continue;
                }
                let target = target
                    .split('#')
                    .next()
                    .unwrap_or(target)
                    .split(" \"")
                    .next()
                    .unwrap_or(target);
                let resolved = path.parent().unwrap_or(root).join(target);
                if !resolved.exists() {
                    return Err(format!(
                        "{}:{} links to missing `{target}`",
                        path.display(),
                        index + 1
                    ));
                }
            }
        }
    }
    Ok(())
}

fn markdown_link_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find(')') else {
            break;
        };
        targets.push(&rest[..close]);
        rest = &rest[close + 1..];
    }
    targets
}

fn markdown_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_markdown(directory, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect_markdown(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn run_fixtures(root: &Path) -> Result<(), String> {
    run_command_quiet(
        root,
        "cargo",
        &[
            "metadata",
            "--manifest-path",
            "fixtures/Cargo.toml",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ],
    )?;
    run_command(
        root,
        "cargo",
        &[
            "test",
            "-p",
            "concord",
            "--test",
            "public_compile",
            "--no-default-features",
            "--locked",
        ],
    )
}

fn run_command_quiet(root: &Path, program: &str, arguments: &[&str]) -> Result<(), String> {
    println!("running {program} {}", arguments.join(" "));
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} exited with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_command(root: &Path, program: &str, arguments: &[&str]) -> Result<(), String> {
    println!("running {program} {}", arguments.join(" "));
    let status = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .status()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

fn toml_section<'a>(manifest: &'a str, section: &str) -> Option<&'a str> {
    let header = format!("[{section}]");
    let start = manifest.find(&header)? + header.len();
    let tail = &manifest[start..];
    let end = tail
        .lines()
        .scan(0_usize, |offset, line| {
            let current = *offset;
            *offset += line.len() + 1;
            Some((current, line))
        })
        .find_map(|(offset, line)| line.trim_start().starts_with('[').then_some(offset))
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn quoted_values(value: &str) -> BTreeSet<String> {
    value
        .split('"')
        .enumerate()
        .filter_map(|(index, value)| (index % 2 == 1).then_some(value.to_owned()))
        .collect()
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}
