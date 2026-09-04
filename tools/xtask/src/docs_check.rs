//! Cross-platform documentation and active-language truth checks.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use regex::Regex;

const RELEASE_STATUS_OWNER: &str = "<!-- concord-release-status-owner -->";
const DSL_LANGUAGE: &str = "concord";
const DSL_PSEUDOCODE_LANGUAGE: &str = "concord-pseudocode";
const ANCHOR_ROOTS: &[&str] = &[
    "crates/concord-examples/src",
    "crates/concord-examples/tests",
    "crates/concord/tests",
];
const REMOVED_DOCS: &[&str] = &[
    "docs/advanced_endpoints.md",
    "docs/bodies_and_responses.md",
    "docs/retry_and_rate_limit.md",
    "docs/customization.md",
];

#[derive(Default)]
struct Counts {
    dsl: usize,
    pseudocode: usize,
    anchors: usize,
    links: usize,
    fragments: usize,
    retired_files: usize,
}

#[derive(Clone)]
struct SourceReference {
    path: String,
    anchor: String,
    line: usize,
}

struct Fence {
    language: String,
    start: usize,
    body: String,
    source: Option<SourceReference>,
}

struct HeadingIndex {
    slugs: HashSet<String>,
    duplicates: HashSet<String>,
}

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut counts = Counts::default();
    let fixture_path = root.join("crates/concord-examples/src/docs_spec.rs");
    let fixture = read(&fixture_path)?;
    let normalized_fixture = normalize_source(&fixture);
    let markdown_files = active_markdown_files(root)?;
    let mut heading_cache = HashMap::<PathBuf, HeadingIndex>::new();

    for path in &markdown_files {
        check_markdown(
            root,
            path,
            &normalized_fixture,
            &mut heading_cache,
            &mut counts,
            &mut errors,
        )?;
    }

    check_documentation_inventory(root, &mut counts, &mut errors)?;
    check_retired_language(root, &markdown_files, &mut counts, &mut errors)?;
    check_release_truth(root, &markdown_files, &mut errors)?;
    check_language_projection_agreement(root, &mut errors)?;

    if !errors.is_empty() {
        errors.sort();
        errors.dedup();
        return Err(format!(
            "documentation truth check failed:\n{}",
            errors
                .into_iter()
                .map(|error| format!("- {error}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    println!(
        "validated {} DSL fences against {}",
        counts.dsl,
        relative(root, &fixture_path)
    );
    println!(
        "validated {} explicitly classified DSL pseudocode fences",
        counts.pseudocode
    );
    println!(
        "validated {} source anchors under {}",
        counts.anchors,
        ANCHOR_ROOTS.join(", ")
    );
    println!(
        "validated {} local Markdown links and {} heading fragments",
        counts.links, counts.fragments
    );
    println!("validated canonical docs/README.md inventory coverage");
    println!(
        "validated retired public language across {} active files with explicit context allowlists",
        counts.retired_files
    );
    println!("validated one release-status owner and parser/spec/generated-name agreement");
    Ok(())
}

fn check_markdown(
    root: &Path,
    path: &Path,
    normalized_fixture: &str,
    heading_cache: &mut HashMap<PathBuf, HeadingIndex>,
    counts: &mut Counts,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let content = read(path)?;
    let relative_path = relative(root, path);
    let fences = parse_fences(&relative_path, &content, errors);

    for fence in &fences {
        let location = format!("{}:{}", relative_path, fence.start);
        let first_code_line = fence
            .body
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        if fence.language.is_empty() {
            errors.push(format!(
                "{location}: fenced block must declare its language/classification"
            ));
        }
        if looks_like_dsl(first_code_line)
            && fence.source.is_none()
            && matches!(fence.language.as_str(), "" | "rust")
        {
            errors.push(format!(
                "{location}: DSL fence must use `{DSL_LANGUAGE}` or `{DSL_PSEUDOCODE_LANGUAGE}`"
            ));
        }

        if let Some(source) = &fence.source {
            let anchor = format!("{}#{}", source.path, source.anchor);
            match anchored_source(root, source) {
                Ok(expected) => {
                    if normalize_source(&fence.body) != normalize_source(&expected) {
                        errors.push(format!("{location}: fenced source differs from {anchor}"));
                    }
                    counts.anchors += 1;
                }
                Err(error) => errors.push(format!(
                    "{}:{}: anchor {anchor}: {error}",
                    relative_path, source.line
                )),
            }
        } else if fence.language == "rust" {
            errors.push(format!(
                "{location}: normative Rust fence must be source-anchored to compiled code"
            ));
        } else if fence.language == DSL_LANGUAGE {
            counts.dsl += 1;
            if fence.body.trim().is_empty() {
                errors.push(format!("{location}: empty DSL fence"));
            } else if !normalized_fixture.contains(&normalize_source(&fence.body)) {
                errors.push(format!(
                    "{location}: normalized DSL block is absent from crates/concord-examples/src/docs_spec.rs"
                ));
            }
        } else if fence.language == DSL_PSEUDOCODE_LANGUAGE {
            counts.pseudocode += 1;
            if fence.body.trim().is_empty() {
                errors.push(format!("{location}: empty DSL pseudocode fence"));
            }
        }

        if !is_retired_allowlisted(&relative_path) {
            for finding in retired_fence_findings(fence) {
                errors.push(format!("{location}: {finding}"));
            }
        }
    }

    for target in markdown_links(&content) {
        if target.is_empty()
            || target.starts_with('#')
            || has_uri_scheme(&target)
            || target.starts_with("//")
        {
            continue;
        }
        let (file_part, fragment) = target
            .split_once('#')
            .map_or((target.as_str(), None), |(file, fragment)| {
                (file, Some(fragment))
            });
        if file_part.is_empty() {
            continue;
        }
        let Some(parent) = path.parent() else {
            errors.push(format!("{relative_path}: cannot resolve link {target}"));
            continue;
        };
        let resolved = normalize_lexically(&parent.join(file_part));
        if !resolved.starts_with(root) {
            errors.push(format!(
                "{relative_path}: link escapes repository root: {target}"
            ));
            continue;
        }
        if !resolved.exists() {
            errors.push(format!(
                "{relative_path}: missing local link target: {target}"
            ));
            continue;
        }
        counts.links += 1;
        if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
            if resolved.extension().and_then(|value| value.to_str()) == Some("md") {
                if !heading_cache.contains_key(&resolved) {
                    heading_cache.insert(resolved.clone(), headings(&read(&resolved)?));
                }
                let index = heading_cache
                    .get(&resolved)
                    .expect("heading cache was populated");
                if index.duplicates.contains(fragment) {
                    errors.push(format!(
                        "{relative_path}: ambiguous fragment `#{fragment}` in {}",
                        relative(root, &resolved)
                    ));
                } else if !index.slugs.contains(fragment) {
                    errors.push(format!(
                        "{relative_path}: {} has no heading for fragment `#{fragment}`",
                        relative(root, &resolved)
                    ));
                } else {
                    counts.fragments += 1;
                }
            }
        }
    }

    if relative_path == "README.md" || relative_path.starts_with("docs/") {
        for removed in REMOVED_DOCS {
            let file_name = Path::new(removed)
                .file_name()
                .and_then(|value| value.to_str())
                .expect("removed documentation name");
            if content.contains(removed) || content.contains(file_name) {
                errors.push(format!(
                    "{relative_path}: references removed documentation file {removed}"
                ));
            }
        }
    }
    Ok(())
}

fn parse_fences(relative_path: &str, content: &str, errors: &mut Vec<String>) -> Vec<Fence> {
    let mut fences = Vec::new();
    let mut language: Option<String> = None;
    let mut start = 0;
    let mut body = Vec::new();
    let mut pending_source: Option<SourceReference> = None;

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if language.is_none() {
            if let Some(source) = parse_source_reference(line, line_number) {
                pending_source = Some(source);
                continue;
            }
            if let Some(opening_language) = fence_opening(line) {
                language = Some(opening_language);
                start = line_number + 1;
                body.clear();
                continue;
            }
            if let Some(source) = &pending_source {
                if !line.trim().is_empty() {
                    errors.push(format!(
                        "{relative_path}:{}: concord-source marker must immediately precede a fence",
                        source.line
                    ));
                    pending_source = None;
                }
            }
            continue;
        }

        if is_fence_closing(line) {
            fences.push(Fence {
                language: language.take().expect("open fence language"),
                start,
                body: body.join("\n"),
                source: pending_source.take(),
            });
        } else {
            body.push(line.to_owned());
        }
    }

    if language.is_some() {
        errors.push(format!(
            "{relative_path}:{}: unclosed fenced block",
            start.saturating_sub(1)
        ));
    }
    if let Some(source) = pending_source {
        errors.push(format!(
            "{relative_path}:{}: concord-source marker has no fence",
            source.line
        ));
    }
    fences
}

fn parse_source_reference(line: &str, line_number: usize) -> Option<SourceReference> {
    let trimmed = line.trim();
    let body = trimmed
        .strip_prefix("<!-- concord-source:")?
        .strip_suffix("-->")?
        .trim();
    let (path, anchor) = body.split_once('#')?;
    if path.trim().is_empty()
        || anchor.trim().is_empty()
        || !anchor
            .trim()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    Some(SourceReference {
        path: path.trim().to_owned(),
        anchor: anchor.trim().to_owned(),
        line: line_number,
    })
}

fn fence_opening(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let language = trimmed.strip_prefix("```")?;
    if language.contains('`')
        || !language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    Some(language.to_owned())
}

fn is_fence_closing(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == b'`')
}

fn looks_like_dsl(line: &str) -> bool {
    let line = line.trim_start();
    [
        "api!",
        "client ",
        "scope ",
        "GET ",
        "POST ",
        "PUT ",
        "PATCH ",
        "DELETE ",
        "HEAD ",
        "OPTIONS ",
        "METHOD(",
        "values {",
        "credentials {",
        "subdomain ",
        "path ",
        "query ",
        "headers ",
        "auth ",
        "timeout ",
        "paginate with ",
        "rate_limits {",
        "rate_limit ",
        "concat [",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn anchored_source(root: &Path, source: &SourceReference) -> Result<String, String> {
    let relative_path = Path::new(&source.path);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("source path must be a repository-relative non-traversing path".to_owned());
    }
    if relative_path.extension().and_then(|value| value.to_str()) != Some("rs") {
        return Err("source path must name a .rs file".to_owned());
    }
    if !ANCHOR_ROOTS
        .iter()
        .any(|allowed| relative_path.starts_with(allowed))
    {
        return Err(format!(
            "source path is outside approved roots ({})",
            ANCHOR_ROOTS.join(", ")
        ));
    }
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err("source file does not exist".to_owned());
    }
    let content = read(&path)?;
    let mut stack = Vec::<(String, usize)>::new();
    let mut regions = HashMap::<String, (usize, usize)>::new();
    let lines = content.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("// ANCHOR:").map(str::trim) {
            if name.is_empty()
                || regions.contains_key(name)
                || stack.iter().any(|(open, _)| open == name)
            {
                return Err(format!("duplicate or invalid anchor `{name}`"));
            }
            stack.push((name.to_owned(), index));
        } else if let Some(name) = trimmed.strip_prefix("// ANCHOR_END:").map(str::trim) {
            let Some((open, start)) = stack.pop() else {
                return Err(format!("anchor `{name}` closes without an opening"));
            };
            if open != name {
                return Err(format!(
                    "anchor regions overlap: `{name}` closes while `{open}` is open"
                ));
            }
            regions.insert(open, (start, index));
        }
    }
    if let Some((open, _)) = stack.last() {
        return Err(format!("unclosed anchor `{open}`"));
    }
    let Some((start, end)) = regions.get(&source.anchor).copied() else {
        return Err(format!("anchor `{}` is not defined", source.anchor));
    };
    let selected = lines[start + 1..end]
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("// ANCHOR:") && !trimmed.starts_with("// ANCHOR_END:")
        })
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if selected.trim().is_empty() {
        return Err(format!("anchor `{}` selects no source", source.anchor));
    }
    Ok(selected)
}

fn check_documentation_inventory(
    root: &Path,
    counts: &mut Counts,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let index_path = root.join("docs/README.md");
    if !index_path.is_file() {
        errors.push("docs/README.md: canonical documentation index is missing".to_owned());
        return Ok(());
    }
    let targets = markdown_links(&read(&index_path)?)
        .into_iter()
        .filter_map(|target| {
            if target.is_empty() || target.starts_with('#') || has_uri_scheme(&target) {
                return None;
            }
            let file = target
                .split_once('#')
                .map_or(target.as_str(), |(file, _)| file);
            Some(normalize_lexically(&root.join("docs").join(file)))
        })
        .collect::<Vec<_>>();
    for document in collect_files(&root.join("docs"), "md")? {
        if document.file_name().and_then(|name| name.to_str()) == Some("README.md") {
            continue;
        }
        let count = targets.iter().filter(|target| *target == &document).count();
        if count != 1 {
            errors.push(format!(
                "docs/README.md: {} must be linked exactly once; found {count}",
                document
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<non-UTF-8 file>")
            ));
        }
    }
    counts.links += 0;
    Ok(())
}

fn check_retired_language(
    root: &Path,
    markdown_files: &[PathBuf],
    counts: &mut Counts,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let mut active_files = markdown_files
        .iter()
        .filter(|path| {
            let key = relative(root, path);
            key == "README.md" || key == "FINAL.md" || key.starts_with("docs/")
        })
        .cloned()
        .collect::<Vec<_>>();
    active_files.push(root.join("crates/concord/src/lib.rs"));
    for directory in [
        "crates/concord-examples/src",
        "crates/concord-examples/tests",
        "fixtures",
    ] {
        active_files.extend(collect_files(&root.join(directory), "rs")?);
    }
    active_files.sort();
    active_files.dedup();

    for path in active_files {
        let key = relative(root, &path);
        if is_retired_allowlisted(&key) {
            continue;
        }
        let content = read(&path)?;
        for finding in retired_surface_findings(&content) {
            errors.push(format!("{key}: {finding}"));
        }
        counts.retired_files += 1;
    }
    Ok(())
}

fn retired_fence_findings(fence: &Fence) -> Vec<String> {
    let mut findings = retired_surface_findings(&fence.body);
    if matches!(
        fence.language.as_str(),
        DSL_LANGUAGE | DSL_PSEUDOCODE_LANGUAGE | "text"
    ) {
        let patterns = [
            (r"(?m)^\s*vars\s*\{", "retired `vars` declaration"),
            (r"\bscope\s*\.", "retired generic `scope.*` owner lookup"),
            (r"\bep\s*\.", "retired `ep.*` owner lookup"),
            (
                r"(?m)^\s*host(?:\s|\[|off\b)",
                "retired active `host` clause",
            ),
            (r"\[\s*host(?:\s*[,\]])", "retired `host` rate dimension"),
            (r"\bfmt\s*\[", "retired `fmt` formatter"),
            (r"#\[body\]", "retired `#[body]` marker"),
            (
                r"(?m)^\s*accept(?:\s+off)?\s*;",
                "retired endpoint `accept` override",
            ),
            (
                r"\bsecret\s+[A-Za-z_][A-Za-z0-9_]*\s*\?\s*;",
                "optional static secret syntax is deferred",
            ),
            (
                r"->\s*(?:Json|Binary|Text|Csv|Ndjson)\s*(?:<|\b)",
                "retired representation wrapper in result position",
            ),
        ];
        for (pattern, message) in patterns {
            if Regex::new(pattern)
                .expect("retired DSL regex")
                .is_match(&fence.body)
            {
                findings.push(message.to_owned());
            }
        }
    }
    findings
}

fn retired_surface_findings(content: &str) -> Vec<String> {
    let patterns = [
        (
            r"\.clear_[A-Za-z_][A-Za-z0-9_]*\s*\(",
            "retired `clear_*` control",
        ),
        (
            r#"\.accept\s*\(\s*(?:\"|String::|[A-Za-z_])|\.accept_off\s*\("#,
            "retired endpoint/runtime `accept` override",
        ),
        (r"\bresponse_bytes\s*\(", "retired `response_bytes` API"),
        (r"\bSafeRequestPlan\b", "retired `SafeRequestPlan` type"),
        (r"\.paginate\s*\(\s*[0-9]", "numeric pagination coercion"),
        (r"\.with_x\s*\(", "retired `with_*` optional-value control"),
        (
            r"\.with_<value>\s*\(",
            "retired `with_*` optional-value control",
        ),
    ];
    patterns
        .into_iter()
        .filter(|(pattern, _)| {
            Regex::new(pattern)
                .expect("retired surface regex")
                .is_match(content)
        })
        .map(|(_, message)| message.to_owned())
        .collect()
}

fn is_retired_allowlisted(relative_path: &str) -> bool {
    relative_path == "docs/migration_v2_1.md"
        || relative_path.starts_with("dev_doc/history/")
        || relative_path.starts_with("crates/concord/tests/ui/fail/")
        || relative_path.starts_with("crates/concord-macros/src/syntax/")
        || relative_path == "dev_doc/documentation_contract.md"
        || relative_path == "tools/xtask/src/migration.rs"
        || relative_path == "tools/xtask/src/docs_check.rs"
}

fn check_release_truth(
    root: &Path,
    markdown_files: &[PathBuf],
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let owners = markdown_files
        .iter()
        .filter_map(|path| {
            read(path)
                .ok()
                .filter(|content| content.contains(RELEASE_STATUS_OWNER))
                .map(|_| relative(root, path))
        })
        .collect::<Vec<_>>();
    if owners != ["dev_doc/release_gate_status.md"] {
        errors.push(format!(
            "release status owner marker must occur only in dev_doc/release_gate_status.md; found {owners:?}"
        ));
    }

    let workspace = read(&root.join("Cargo.toml"))?;
    let version = workspace
        .lines()
        .skip_while(|line| line.trim() != "[workspace.package]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
        .find_map(|line| {
            line.trim()
                .strip_prefix("version")?
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .strip_prefix('"')?
                .strip_suffix('"')
        })
        .ok_or_else(|| "workspace package version is missing".to_owned())?;
    let status = read(&root.join("dev_doc/release_gate_status.md"))?;
    for (path, label) in [
        ("README.md", "root README"),
        ("FINAL.md", "status summary"),
        ("docs/README.md", "documentation index"),
        ("dev_doc/release_gate_status.md", "canonical release status"),
    ] {
        let content = read(&root.join(path))?;
        if !content.contains(version) {
            errors.push(format!(
                "{label} does not match workspace version `{version}`"
            ));
        }
    }
    if version.contains("beta") {
        if !status.contains("integration candidate / beta")
            || !status.contains("Publication:** disabled")
        {
            errors.push(
                "beta workspace must declare integration-candidate lifecycle and disabled publication"
                    .to_owned(),
            );
        }
    } else if version == "2.1.0"
        && !status.contains("stable local release candidate accepted for merge")
    {
        errors.push("stable 2.1.0 requires the final accepted lifecycle text".to_owned());
    }

    for manifest in [
        "crates/concord/Cargo.toml",
        "crates/concord-core/Cargo.toml",
        "crates/concord-macros/Cargo.toml",
        "crates/concord-extras/Cargo.toml",
    ] {
        if !read(&root.join(manifest))?.lines().any(|line| {
            let compact = line.split_whitespace().collect::<String>();
            compact == "publish=false"
        }) {
            errors.push(format!(
                "{manifest}: intended public package must set publish = false"
            ));
        }
    }
    Ok(())
}

fn check_language_projection_agreement(
    root: &Path,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let credentials = read(&root.join("crates/concord-macros/src/syntax/credentials.rs"))?;
    let spec = read(&root.join("docs/spec.md"))?;
    let optional_secret_fixture =
        root.join("crates/concord/tests/ui/fail/syntax/optional_static_secret_deferred.rs");
    if !credentials.contains("optional static secrets are deferred for v2.1")
        || !optional_secret_fixture.is_file()
        || !read(&optional_secret_fixture)?.contains("secret optional_api_key?;")
    {
        errors.push(
            "optional static secret rejection must agree across parser and focused UI fixture"
                .to_owned(),
        );
    }
    let active_optional_secret = Regex::new(r"(?m)^\s*secret\s+[A-Za-z_][A-Za-z0-9_]*\s*\?\s*;")
        .expect("optional secret regex");
    if active_optional_secret.is_match(&spec) {
        errors.push("docs/spec.md still admits optional static secret syntax".to_owned());
    }

    let symbols = read(&root.join("crates/concord-macros/src/semantic/symbols.rs"))?;
    let auth = read(&root.join("crates/concord-macros/src/semantic/auth.rs"))?;
    if symbols.contains("format!(\"with_") || auth.contains("format!(\"with_") {
        errors.push("semantic generated-name model still reserves `with_*`".to_owned());
    }
    for fixture in [
        "crates/concord/tests/ui/pass/language/with_namespace_released.rs",
        "crates/concord/tests/ui/fail/capabilities/with_optional_control_absent.rs",
    ] {
        if !root.join(fixture).is_file() {
            errors.push(format!(
                "missing generated-name agreement fixture {fixture}"
            ));
        }
    }

    let rate_syntax = read(&root.join("crates/concord-macros/src/syntax/rate_limits.rs"))?;
    let rate_snapshot = read(&root.join(
        "crates/concord/tests/ui/fail/rate_limits/rate_limit_unsupported_bare_dimensions.stderr",
    ))
    .unwrap_or_default();
    if !rate_syntax.contains("`origin`, `endpoint`, or declared bare formal names")
        || rate_snapshot.contains("only `host`")
    {
        errors.push("active rate diagnostic must name `origin`, not retired `host`".to_owned());
    }
    if !spec.contains("PageLimit::")
        && !read(&root.join("docs/pagination.md"))?.contains("PageLimit::")
    {
        errors.push("normative docs must contain an explicit PageLimit example".to_owned());
    }
    if !spec.contains("request explicit;")
        || !spec.contains("response explicit;")
        || !spec.contains("Representation defaults")
    {
        errors.push(
            "spec must retain representation inheritance and explicit-fence examples".to_owned(),
        );
    }

    let facade_manifest = read(&root.join("crates/concord/Cargo.toml"))?;
    let facade_name = package_name(&facade_manifest)
        .ok_or_else(|| "facade package name is missing".to_owned())?;
    let readme = read(&root.join("README.md"))?;
    if !readme.contains("Publication is disabled") || !facade_name.starts_with("concord") {
        errors.push("README/package identity projection is inconsistent".to_owned());
    }
    let quick_start = read(&root.join("docs/quick_start.md"))?;
    if facade_name == "concord" {
        if !quick_start.contains("concord = {") {
            errors
                .push("quick start does not use the current façade package/import name".to_owned());
        }
    } else {
        let dependency = format!("concord = {{ package = \"{facade_name}\"");
        if !quick_start.contains(&dependency)
            || !facade_manifest.contains("[lib]")
            || !facade_manifest.contains("name = \"concord\"")
        {
            errors.push(
                "renamed façade must document a `concord` dependency alias and retain `[lib] name = \"concord\"`"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn package_name(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[package]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
        .find_map(|line| {
            line.trim()
                .strip_prefix("name")?
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .strip_prefix('"')?
                .strip_suffix('"')
        })
}

fn active_markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = vec![root.join("README.md"), root.join("FINAL.md")];
    files.extend(collect_files(&root.join("docs"), "md")?);
    files.extend(
        collect_files(&root.join("dev_doc"), "md")?
            .into_iter()
            .filter(|path| !relative(root, path).starts_with("dev_doc/history/")),
    );
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_files(directory: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut pending = vec![directory.to_path_buf()];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        let entries = fs::read_dir(&current)
            .map_err(|error| format!("could not enumerate {}: {error}", current.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("could not read {} entry: {error}", current.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some(extension)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn markdown_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let visible = remove_inline_code(line);
        let mut remainder = visible.as_str();
        while let Some(open) = remainder.find("](") {
            remainder = &remainder[open + 2..];
            let Some(close) = remainder.find(')') else {
                break;
            };
            let mut target = remainder[..close].trim();
            if target.starts_with('<') && target.ends_with('>') && target.len() >= 2 {
                target = &target[1..target.len() - 1];
            }
            links.push(target.to_owned());
            remainder = &remainder[close + 1..];
        }
    }
    links
}

fn remove_inline_code(line: &str) -> String {
    let mut result = String::new();
    let mut in_code = false;
    for character in line.chars() {
        if character == '`' {
            in_code = !in_code;
        } else if !in_code {
            result.push(character);
        }
    }
    result
}

fn headings(content: &str) -> HeadingIndex {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let trimmed = line.trim_start();
        let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        if !(1..=6).contains(&hashes) || !trimmed[hashes..].starts_with(' ') {
            continue;
        }
        let heading = trimmed[hashes..].trim().trim_end_matches('#').trim();
        let slug = heading_slug(heading);
        if !slug.is_empty() && !seen.insert(slug.clone()) {
            duplicates.insert(slug);
        }
    }
    HeadingIndex {
        slugs: seen,
        duplicates,
    }
}

fn heading_slug(heading: &str) -> String {
    let mut result = String::new();
    let mut in_link_target = false;
    let mut previous = '\0';
    for character in heading.chars() {
        if character == '(' && previous == ']' {
            in_link_target = true;
        } else if character == ')' && in_link_target {
            in_link_target = false;
        } else if !in_link_target {
            if character.is_alphanumeric() || matches!(character, '_' | '-') {
                result.extend(character.to_lowercase());
            } else if character == ' ' {
                result.push('-');
            }
        }
        previous = character;
    }
    result
}

fn has_uri_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_alphabetic()
            } else {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
            }
        })
}

fn normalize_source(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fence(language: &str, body: &str) -> Fence {
        Fence {
            language: language.to_owned(),
            start: 1,
            body: body.to_owned(),
            source: None,
        }
    }

    #[test]
    fn retired_language_detection_has_positive_and_negative_space() {
        assert!(
            retired_fence_findings(&fence("concord", "vars { id: String; }"))
                .contains(&"retired `vars` declaration".to_owned())
        );
        assert!(retired_surface_findings("request.paginate(12)").len() == 1);
        assert!(retired_surface_findings("request.paginate(PageLimit::Prefix(12))").is_empty());
        assert!(retired_surface_findings("client.with_locale().read()").is_empty());
        assert!(retired_surface_findings("builder.set_locale(Some(value))").is_empty());
        assert!(
            retired_fence_findings(&fence("concord", "credentials { secret token?; }"))
                .iter()
                .any(|finding| finding.contains("optional static secret"))
        );
    }

    #[test]
    fn retired_language_allowlist_is_explicit_and_path_bounded() {
        assert!(is_retired_allowlisted("docs/migration_v2_1.md"));
        assert!(is_retired_allowlisted("dev_doc/history/old_audit.md"));
        assert!(is_retired_allowlisted(
            "crates/concord/tests/ui/fail/retired.rs"
        ));
        assert!(!is_retired_allowlisted("docs/spec.md"));
        assert!(!is_retired_allowlisted(
            "crates/concord-examples/src/lib.rs"
        ));
    }

    #[test]
    fn markdown_link_scanner_excludes_code_contexts() {
        let source =
            "[kept](docs/spec.md) ` [inline](missing.md) `\n```text\n[fenced](missing.md)\n```";
        assert_eq!(markdown_links(source), ["docs/spec.md"]);
    }
}
