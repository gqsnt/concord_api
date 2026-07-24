#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
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
    "features",
    "policy",
    "package",
    "deny",
    "perf-smoke",
    "perf-scale",
    "verify-local",
    "verify-msrv",
    "verify-package",
    "verify-release-local",
];

const REQUIRED_WORKFLOW_JOBS: &[&str] = &[
    "format_metadata",
    "feature_checks",
    "clippy",
    "tests",
    "lifecycle_security_bounds",
    "macro_ui",
    "msrv",
    "extras",
    "fixtures",
    "examples_docs",
    "policy",
    "package",
    "platform_smoke",
    "performance",
    "release_clean",
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
        "crates/concord-core/src/endpoint.rs",
        "MAX_ENDPOINT_ID_BYTES",
        "Endpoint contract identity",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_AUTH_PLACEMENT_NAME_BYTES",
        "Authentication placement name",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_CREDENTIAL_SECRET_BYTES",
        "Raw credential secret",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_BASIC_USERNAME_BYTES",
        "Basic authentication username",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_BASIC_PASSWORD_BYTES",
        "Basic authentication password",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_AUTHORIZATION_VALUE_BYTES",
        "Native authentication header value",
        "bytes",
    ),
    (
        "crates/concord-core/src/auth/placement.rs",
        "MAX_AUTH_QUERY_VALUE_BYTES",
        "Encoded authentication query value",
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
    (
        "crates/concord-extras/src/auth/oauth2_client.rs",
        "MAX_OAUTH2_TOKEN_URL_BYTES",
        "OAuth2 token URL text",
        "bytes",
    ),
    (
        "crates/concord-extras/src/auth/oauth2_client.rs",
        "MAX_OAUTH2_CLIENT_ID_BYTES",
        "OAuth2 client identifier",
        "bytes",
    ),
    (
        "crates/concord-extras/src/auth/oauth2_client.rs",
        "MAX_OAUTH2_CLIENT_SECRET_BYTES",
        "OAuth2 client secret",
        "bytes",
    ),
    (
        "crates/concord-extras/src/auth/oauth2_client.rs",
        "MAX_OAUTH2_SCOPE_BYTES",
        "OAuth2 scope",
        "bytes",
    ),
    (
        "crates/concord-extras/src/auth/oauth2_client.rs",
        "MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES",
        "OAuth2 Basic Authorization value",
        "bytes",
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
        Some("package") => {
            if arguments.next().is_some() {
                return Err("usage: cargo run -p concord-xtask -- package".into());
            }
            verify_product_packages(&root)
        }
        Some("policy") => {
            if arguments.next().is_some() {
                return Err("usage: cargo run -p concord-xtask -- policy".into());
            }
            check_repository_policy(&root)?;
            println!("repository policy checks passed");
            Ok(())
        }
        _ => Err(
            "usage: cargo run -p concord-xtask -- <docs [--write] | fixtures | package | policy>"
                .into(),
        ),
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
    check_repository_policy(root)?;
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

#[derive(Default)]
struct JustRecipe {
    dependencies: BTreeSet<String>,
    commands: Vec<String>,
}

fn check_repository_policy(root: &Path) -> Result<(), String> {
    let manifest = read(&root.join("Cargo.toml"))?;
    if !manifest
        .lines()
        .any(|line| line.trim() == "exclude = [\"fixtures\"]")
    {
        return Err("root Cargo workspace must exclude the isolated `fixtures` workspace".into());
    }
    check_recipe_inventory(root)?;
    check_workflow_policy(root)
}

fn check_recipe_inventory(root: &Path) -> Result<(), String> {
    let justfile_path = root.join("justfile");
    let justfile = read(&justfile_path)?;
    let recipes = parse_justfile(&justfile);
    let missing = REQUIRED_RECIPES
        .iter()
        .filter(|recipe| !recipes.contains_key(**recipe))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "justfile is missing recipes: {}",
            missing.join(", ")
        ));
    }

    if recipes.contains_key("verify") {
        return Err(
            "justfile must use explicit verify scopes, not the retired `verify` gate".into(),
        );
    }

    for (name, recipe) in &recipes {
        for command in &recipe.commands {
            let command = command.strip_prefix('@').unwrap_or(command);
            if cargo_command_is_unlocked(command) {
                return Err(format!(
                    "just recipe `{name}` has an unlocked Cargo command: {command}"
                ));
            }
        }
    }

    require_recipe_dependencies(
        &recipes,
        "verify-local",
        &[
            "fmt-check",
            "check",
            "check-all",
            "lint",
            "test",
            "test-all",
            "ui",
            "fixtures",
            "examples",
            "docs",
            "security",
            "features",
            "policy",
        ],
    )?;
    require_recipe_dependencies(&recipes, "verify-msrv", &["test-msrv"])?;
    require_recipe_dependencies(&recipes, "verify-package", &["package"])?;
    require_recipe_dependencies(
        &recipes,
        "verify-release-local",
        &[
            "verify-local",
            "verify-msrv",
            "verify-package",
            "deny",
            "perf-smoke",
        ],
    )?;
    require_recipe_command(&recipes, "test-msrv", "concord --test ui_msrv_semantic")?;
    require_recipe_command(
        &recipes,
        "test-msrv",
        "concord-core --test ui_msrv_semantic",
    )?;
    require_recipe_command(&recipes, "policy", "concord-xtask -- policy")?;
    require_recipe_command(&recipes, "package", "concord-xtask -- package")?;
    Ok(())
}

fn parse_justfile(justfile: &str) -> BTreeMap<String, JustRecipe> {
    let mut recipes = BTreeMap::<String, JustRecipe>::new();
    let mut active = None::<String>;
    for line in justfile.lines() {
        if !line.chars().next().is_some_and(char::is_whitespace) {
            active = None;
            if line.is_empty() || line.starts_with('#') || line.starts_with("set ") {
                continue;
            }
            if let Some((name, dependencies)) = line.split_once(':') {
                let name = name.trim().to_owned();
                let recipe = recipes.entry(name.clone()).or_default();
                recipe.dependencies.extend(
                    dependencies
                        .split_whitespace()
                        .map(str::trim)
                        .filter(|dependency| !dependency.is_empty())
                        .map(str::to_owned),
                );
                active = Some(name);
            }
            continue;
        }
        let command = line.trim();
        if command.is_empty() || command.starts_with('#') {
            continue;
        }
        if let Some(name) = &active {
            recipes
                .get_mut(name)
                .expect("active just recipe exists")
                .commands
                .push(command.to_owned());
        }
    }
    recipes
}

fn cargo_command_is_unlocked(command: &str) -> bool {
    let command = command.trim();
    let command = command
        .strip_prefix("- run:")
        .or_else(|| command.strip_prefix("run:"))
        .map(str::trim_start)
        .unwrap_or(command);
    let command = command
        .strip_prefix('@')
        .map(str::trim_start)
        .unwrap_or(command);
    let Some(after_cargo) = command.strip_prefix("cargo ") else {
        return false;
    };
    let subcommand = after_cargo
        .strip_prefix('+')
        .and_then(|toolchain_and_command| toolchain_and_command.split_once(' '))
        .map_or(after_cargo, |(_, subcommand)| subcommand.trim_start());
    !subcommand.starts_with("fmt ")
        && !subcommand.starts_with("deny ")
        && !command.contains(" --locked")
}

fn require_recipe_dependencies(
    recipes: &BTreeMap<String, JustRecipe>,
    name: &str,
    expected: &[&str],
) -> Result<(), String> {
    let recipe = recipes
        .get(name)
        .ok_or_else(|| format!("justfile is missing recipe `{name}`"))?;
    let missing = expected
        .iter()
        .filter(|dependency| !recipe.dependencies.contains(**dependency))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "just recipe `{name}` does not own dependencies: {}",
            missing.join(", ")
        ))
    }
}

fn require_recipe_command(
    recipes: &BTreeMap<String, JustRecipe>,
    name: &str,
    expected: &str,
) -> Result<(), String> {
    let recipe = recipes
        .get(name)
        .ok_or_else(|| format!("justfile is missing recipe `{name}`"))?;
    if recipe
        .commands
        .iter()
        .any(|command| command.contains(expected))
    {
        Ok(())
    } else {
        Err(format!(
            "just recipe `{name}` does not execute command owner `{expected}`"
        ))
    }
}

fn check_workflow_policy(root: &Path) -> Result<(), String> {
    let path = root.join(".github/workflows/ci.yml");
    let workflow = read(&path)?;
    for required in [
        "push:",
        "pull_request:",
        "workflow_dispatch:",
        "permissions:",
        "contents: read",
    ] {
        if !workflow.lines().any(|line| line.trim() == required) {
            return Err(format!("{} is missing `{required}`", path.display()));
        }
    }

    let jobs = workflow_jobs(&workflow);
    let missing = REQUIRED_WORKFLOW_JOBS
        .iter()
        .filter(|job| !jobs.contains_key(**job))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "{} is missing required jobs: {}",
            path.display(),
            missing.join(", ")
        ));
    }
    for job in REQUIRED_WORKFLOW_JOBS {
        let body = jobs
            .get(*job)
            .expect("required workflow job was checked above");
        if !body
            .lines()
            .any(|line| line.trim_start().starts_with("timeout-minutes:"))
        {
            return Err(format!("workflow job `{job}` needs an explicit timeout"));
        }
    }

    for (index, line) in workflow.lines().enumerate() {
        let line = line.trim();
        let Some((_, action)) = line.split_once("uses:") else {
            continue;
        };
        let action = action.trim();
        if action.starts_with("./") {
            continue;
        }
        let Some((_, revision)) = action.rsplit_once('@') else {
            return Err(format!(
                "{}:{} has an action without a revision",
                path.display(),
                index + 1
            ));
        };
        if revision.len() != 40
            || !revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!(
                "{}:{} must pin `{action}` to a lowercase 40-hex revision",
                path.display(),
                index + 1
            ));
        }
    }

    for required in [
        "${{ runner.os }}",
        "${{ hashFiles('**/Cargo.lock') }}",
        "cargo test -p concord --test ui --no-default-features --locked",
        "cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked",
        "cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked",
        "cargo run --locked -p concord-xtask -- fixtures",
        "cargo run --locked -p concord-xtask -- package",
        "cargo run --locked -p concord-xtask -- policy",
        "git diff --check",
        "git diff --exit-code",
        "git status --porcelain",
    ] {
        if !workflow.contains(required) {
            return Err(format!(
                "{} does not own required policy text `{required}`",
                path.display()
            ));
        }
    }

    for (index, line) in workflow.lines().enumerate() {
        let command = line.trim();
        if cargo_command_is_unlocked(command) {
            return Err(format!(
                "{}:{} has an unlocked Cargo command: {command}",
                path.display(),
                index + 1
            ));
        }
    }
    Ok(())
}

fn workflow_jobs(workflow: &str) -> BTreeMap<String, String> {
    let mut jobs = BTreeMap::<String, String>::new();
    let mut active = None::<String>;
    let mut in_jobs = false;
    for line in workflow.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if let Some(candidate) = line.strip_prefix("  ") {
            if !candidate.starts_with(' ') && candidate.ends_with(':') {
                let name = candidate.trim_end_matches(':');
                if name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                {
                    jobs.entry(name.to_owned()).or_default();
                    active = Some(name.to_owned());
                    continue;
                }
            }
        }
        if let Some(name) = &active {
            let body = jobs.get_mut(name).expect("active workflow job exists");
            body.push_str(line);
            body.push('\n');
        }
    }
    jobs
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
    let mut local_files = 0_usize;
    let mut local_anchors = 0_usize;
    let mut external_urls = 0_usize;
    let mut anchor_cache = BTreeMap::<PathBuf, BTreeSet<String>>::new();
    for path in paths {
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
                if target.is_empty() {
                    continue;
                }
                if target.contains("://") || target.starts_with("mailto:") {
                    external_urls += 1;
                    continue;
                }
                let target = target
                    .split(" \"")
                    .next()
                    .unwrap_or(target)
                    .split(" '")
                    .next()
                    .unwrap_or(target);
                let (file_target, anchor) = target
                    .split_once('#')
                    .map_or((target, None), |(file, anchor)| (file, Some(anchor)));
                let resolved = if file_target.is_empty() {
                    path.clone()
                } else if let Some(repository_path) = file_target.strip_prefix('/') {
                    root.join(repository_path)
                } else {
                    path.parent().unwrap_or(root).join(file_target)
                };
                if !resolved.exists() {
                    return Err(format!(
                        "{}:{} links to missing `{file_target}`",
                        path.display(),
                        index + 1
                    ));
                }
                local_files += usize::from(!file_target.is_empty());
                if let Some(anchor) = anchor.filter(|anchor| !anchor.is_empty()) {
                    if resolved
                        .extension()
                        .is_some_and(|extension| extension == "md")
                    {
                        let anchors = match anchor_cache.get(&resolved) {
                            Some(anchors) => anchors,
                            None => {
                                let target_text = read(&resolved)?;
                                anchor_cache
                                    .entry(resolved.clone())
                                    .or_insert_with(|| markdown_anchors(&target_text))
                            }
                        };
                        if !anchors.contains(anchor) {
                            return Err(format!(
                                "{}:{} links to missing anchor `#{anchor}` in {}",
                                path.display(),
                                index + 1,
                                resolved.display()
                            ));
                        }
                        local_anchors += 1;
                    }
                }
            }
        }
    }
    println!(
        "markdown links checked: {local_files} local file links, {local_anchors} local anchors; {external_urls} external URLs were syntax-counted but not fetched"
    );
    Ok(())
}

fn markdown_anchors(text: &str) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    let mut occurrences = BTreeMap::<String, usize>::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let trimmed = line.trim_start();
        let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&hashes)
            && trimmed
                .as_bytes()
                .get(hashes)
                .is_some_and(u8::is_ascii_whitespace)
        {
            let heading = trimmed[hashes..].trim().trim_end_matches('#').trim();
            let base = github_heading_anchor(heading);
            if !base.is_empty() {
                let occurrence = occurrences.entry(base.clone()).or_default();
                let anchor = if *occurrence == 0 {
                    base
                } else {
                    format!("{base}-{}", *occurrence)
                };
                *occurrence += 1;
                anchors.insert(anchor);
            }
        }
        collect_explicit_html_anchors(trimmed, &mut anchors);
    }
    anchors
}

fn github_heading_anchor(heading: &str) -> String {
    heading
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                Some(character)
            } else if character.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn collect_explicit_html_anchors(line: &str, anchors: &mut BTreeSet<String>) {
    for prefix in ["id=\"", "name=\"", "id='", "name='"] {
        let quote = if prefix.ends_with('"') { '"' } else { '\'' };
        let mut rest = line;
        while let Some(start) = rest.find(prefix) {
            rest = &rest[start + prefix.len()..];
            let Some(end) = rest.find(quote) else {
                break;
            };
            if end > 0 {
                anchors.insert(rest[..end].to_owned());
            }
            rest = &rest[end + 1..];
        }
    }
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
            "--",
            "--test-threads=1",
        ],
    )
}

fn verify_product_packages(root: &Path) -> Result<(), String> {
    struct Package<'a> {
        name: &'a str,
        patch_dependencies: &'a [&'a str],
        sibling_dependencies: &'a [(&'a str, &'a str)],
        patch_test_support: bool,
    }

    let packages = [
        Package {
            name: "concord-core",
            patch_dependencies: &[],
            sibling_dependencies: &[],
            patch_test_support: true,
        },
        Package {
            name: "concord-macros",
            patch_dependencies: &[],
            sibling_dependencies: &[],
            patch_test_support: false,
        },
        Package {
            name: "concord-extras",
            patch_dependencies: &["concord-core"],
            sibling_dependencies: &[("concord_core", "concord-core")],
            patch_test_support: false,
        },
        Package {
            name: "concord",
            patch_dependencies: &["concord-core", "concord-macros", "concord-extras"],
            sibling_dependencies: &[
                ("concord_core", "concord-core"),
                ("concord_macros", "concord-macros"),
                ("concord_extras", "concord-extras"),
            ],
            patch_test_support: true,
        },
    ];
    let target = package_target_directory(root);
    let package_output = target.join("package");
    let product_version = workspace_product_version(root)?;
    let mut packaged = BTreeMap::<&str, PathBuf>::new();

    for package in packages {
        let mut arguments = Vec::<String>::new();
        if package.patch_test_support {
            push_path_patch(
                &mut arguments,
                "concord-test-support",
                &root.join("crates/concord-test-support"),
            )?;
        }
        for dependency in package.patch_dependencies {
            push_path_patch(
                &mut arguments,
                dependency,
                &root.join("crates").join(dependency),
            )?;
        }
        arguments.extend([
            "package".into(),
            "--locked".into(),
            "--allow-dirty".into(),
            "-p".into(),
            package.name.into(),
        ]);
        run_command_owned(root, "cargo", &arguments)?;

        let directory = package_output.join(format!("{}-{product_version}", package.name));
        verify_packaged_manifest(
            package.name,
            &product_version,
            &directory,
            package.sibling_dependencies,
        )?;

        let mut verification_arguments = Vec::<String>::new();
        if package.patch_test_support {
            push_path_patch(
                &mut verification_arguments,
                "concord-test-support",
                &root.join("crates/concord-test-support"),
            )?;
        }
        for dependency in package.patch_dependencies {
            let path = packaged.get(dependency).ok_or_else(|| {
                format!(
                    "{} requires packaged dependency {dependency} before its verification",
                    package.name
                )
            })?;
            push_path_patch(&mut verification_arguments, dependency, path)?;
        }
        verification_arguments.extend([
            "check".into(),
            "--manifest-path".into(),
            directory.join("Cargo.toml").display().to_string(),
            "--lib".into(),
            "--all-features".into(),
            "--locked".into(),
        ]);
        run_command_owned(root, "cargo", &verification_arguments)?;
        packaged.insert(package.name, directory);
    }

    println!(
        "package verification passed in order: concord-core, concord-macros, concord-extras, concord"
    );
    Ok(())
}

fn workspace_product_version(root: &Path) -> Result<String, String> {
    let manifest_path = root.join("Cargo.toml");
    let manifest = read(&manifest_path)?;
    let package = toml_section(&manifest, "workspace.package").ok_or_else(|| {
        format!(
            "{} has no workspace package section",
            manifest_path.display()
        )
    })?;
    package
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .filter(|version| !version.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "{} has no literal workspace package version",
                manifest_path.display()
            )
        })
}

fn push_path_patch(
    arguments: &mut Vec<String>,
    dependency: &str,
    path: &Path,
) -> Result<(), String> {
    arguments.push("--config".into());
    arguments.push(format!(
        "patch.crates-io.{dependency}.path={}",
        toml_path_literal(path)?
    ));
    Ok(())
}

fn package_target_directory(root: &Path) -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => root.join("target"),
    }
}

fn toml_path_literal(path: &Path) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))?
        .to_string_lossy()
        .into_owned();
    let canonical = if let Some(path) = canonical.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        canonical
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical)
            .to_owned()
    };
    let path = canonical.replace('\\', "/");
    if path.contains('"') || path.contains('\n') || path.contains('\r') {
        return Err(format!(
            "package patch path cannot be encoded safely: {path}"
        ));
    }
    Ok(format!("\"{path}\""))
}

fn verify_packaged_manifest(
    package: &str,
    version: &str,
    directory: &Path,
    sibling_dependencies: &[(&str, &str)],
) -> Result<(), String> {
    let manifest_path = directory.join("Cargo.toml");
    let original_path = directory.join("Cargo.toml.orig");
    let package_output = directory
        .parent()
        .ok_or_else(|| format!("{} has no package output parent", directory.display()))?;
    let archive_path = package_output.join(format!("{package}-{version}.crate"));
    for required in [&manifest_path, &original_path, &archive_path] {
        if !required.is_file() {
            return Err(format!(
                "{package} packaging omitted {}",
                required.display()
            ));
        }
    }

    let manifest = read(&manifest_path)?;
    let package_section = toml_section(&manifest, "package").ok_or_else(|| {
        format!(
            "{} has no normalized package section",
            manifest_path.display()
        )
    })?;
    for expected in [
        format!("name = \"{package}\""),
        format!("version = \"{version}\""),
        "publish = false".into(),
    ] {
        if !package_section.lines().any(|line| line.trim() == expected) {
            return Err(format!(
                "{} omitted normalized package fact `{expected}`",
                manifest_path.display()
            ));
        }
    }

    if toml_section(&manifest, "dependencies.concord_test_support").is_some() {
        return Err(format!(
            "{package} packaged a normal dependency on concord-test-support"
        ));
    }
    for (alias, dependency) in sibling_dependencies {
        let section_name = format!("dependencies.{alias}");
        let section = toml_section(&manifest, &section_name).ok_or_else(|| {
            format!(
                "{} omitted normalized sibling dependency {dependency}",
                manifest_path.display()
            )
        })?;
        for expected in [
            format!("package = \"{dependency}\""),
            format!("version = \"={version}\""),
        ] {
            if !section.lines().any(|line| line.trim() == expected) {
                return Err(format!(
                    "{} sibling {dependency} omitted `{expected}`",
                    manifest_path.display()
                ));
            }
        }
    }

    let mut dependency_section = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            dependency_section = line.contains("dependencies");
        } else if dependency_section && line.starts_with("path =") {
            return Err(format!(
                "{} retained a path-only packaged dependency: {line}",
                manifest_path.display()
            ));
        }
    }
    Ok(())
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

fn run_command_owned(root: &Path, program: &str, arguments: &[String]) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::cargo_command_is_unlocked;

    #[test]
    fn locked_command_policy_covers_toolchains_and_workflow_run_steps() {
        assert!(cargo_command_is_unlocked(
            "cargo +stable clippy --workspace --all-targets"
        ));
        assert!(cargo_command_is_unlocked("- run: cargo test --workspace"));
        assert!(!cargo_command_is_unlocked(
            "cargo +stable clippy --workspace --locked -- -D warnings"
        ));
        assert!(!cargo_command_is_unlocked(
            "- run: cargo test --workspace --locked"
        ));
        assert!(!cargo_command_is_unlocked("cargo fmt --all -- --check"));
        assert!(!cargo_command_is_unlocked("cargo deny check"));
    }
}
