//! Cross-platform documentation and active-language truth checks.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use regex::Regex;

const RELEASE_STATUS_OWNER: &str = "<!-- concord-release-status-owner -->";
const RUST_LANGUAGE: &str = "rust";
const DOMAIN_TREE_SOURCE: &str = "crates/concord-examples/src/getting_started/domain_tree.rs";
const MINIMAL_SOURCE: &str = "crates/concord-examples/src/getting_started/minimal.rs";
const CATALOG_V3_SOURCE: &str = "crates/concord-examples/src/contracts/catalog_v3.rs";
const CANONICAL_SOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "README.md",
        DOMAIN_TREE_SOURCE,
        "domain_tree_contract",
        RUST_LANGUAGE,
    ),
    (
        "README.md",
        DOMAIN_TREE_SOURCE,
        "domain_tree_await_request",
        "rust",
    ),
    ("docs/quick_start.md", MINIMAL_SOURCE, "first_api", "rust"),
    (
        "docs/quick_start.md",
        MINIMAL_SOURCE,
        "first_request",
        "rust",
    ),
    (
        "docs/quick_start.md",
        DOMAIN_TREE_SOURCE,
        "domain_tree_contract",
        RUST_LANGUAGE,
    ),
    (
        "docs/quick_start.md",
        DOMAIN_TREE_SOURCE,
        "domain_tree_await_request",
        "rust",
    ),
    (
        "docs/overview.md",
        DOMAIN_TREE_SOURCE,
        "domain_tree_contract",
        RUST_LANGUAGE,
    ),
    // §22: the canonical extended Catalog is the coverage reference, not the
    // first-contact advertisement, so it lives in the modeling guide.
    (
        "docs/modeling_apis.md",
        CATALOG_V3_SOURCE,
        "catalog_v3_declaration",
        RUST_LANGUAGE,
    ),
    (
        "docs/riot_walkthrough.md",
        "crates/concord-examples/src/riot/api/riot.rs",
        "riot_client",
        RUST_LANGUAGE,
    ),
    (
        "docs/riot_walkthrough.md",
        "crates/concord-examples/src/riot/api/riot.rs",
        "riot_match_ids_endpoint",
        RUST_LANGUAGE,
    ),
    (
        "docs/riot_walkthrough.md",
        "crates/concord-examples/src/riot/usage.rs",
        "riot_paginated_request",
        "rust",
    ),
];
const ACTIVE_MAINTAINER_DOCS: &[&str] = &[
    "dev_doc/release_gate_status.md",
    "dev_doc/claim_evidence_ledger.md",
    "dev_doc/decision_record.md",
    "dev_doc/adrs/README.md",
    "dev_doc/adrs/0001-representation-ownership.md",
    "dev_doc/adrs/0002-canonical-request-atoms.md",
    "dev_doc/adrs/0003-query-header-patch-language.md",
    "dev_doc/adrs/0005-pagination-controller-ownership.md",
    "dev_doc/adrs/0007-observation-without-policy-authority.md",
    "dev_doc/adrs/0011-public-stream-carriers.md",
    "dev_doc/adrs/0012-freeze-exact-optional-control-surface.md",
    "dev_doc/adrs/0013-observer-panic-diagnostics.md",
    "dev_doc/owner_release_decisions.md",
];
const NEGATIVE_SPACE_FIXTURES: &[&str] = &[
    "crates/concord/tests/ui/fail/syntax/spec_rejections.rs",
    "crates/concord/tests/ui/fail/syntax/ghost_syntax.rs",
    "crates/concord/tests/ui/fail/syntax/endpoint_accept_removed.rs",
    "crates/concord/tests/ui/fail/syntax/optional_static_secret_deferred.rs",
    "crates/concord/tests/ui/fail/syntax/retry_dsl_removed.rs",
    "crates/concord/tests/ui/fail/pagination/numeric_page_limit_absent.rs",
    "crates/concord/tests/ui/fail/pagination/pagination_syntax.rs",
    "crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.rs",
    "crates/concord/tests/ui/fail/capabilities/with_optional_control_absent.rs",
    "crates/concord/tests/ui/fail/capabilities/generated_accept_override_absent.rs",
    "crates/concord/tests/ui/fail/capabilities/runtime_accept_override_absent.rs",
];
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

/// Clauses of the §3.2 grammar each parser fallback diagnostic must name, so
/// an author who writes an unsupported entry is shown every legal choice.
const CLIENT_CLAUSES: &[&str] = &[
    "`base`",
    "`values`",
    "`credentials`",
    "`rate_limits`",
    "`request`",
    "`response`",
    "`subdomain`",
    "`path`",
    "`query`",
    "`headers`",
    "`auth`",
    "`timeout`",
    "`rate_limit`",
];
const SCOPE_CLAUSES: &[&str] = &[
    "`request`",
    "`response`",
    "`subdomain`",
    "`path`",
    "`query`",
    "`headers`",
    "`auth`",
    "`timeout`",
    "`rate_limit`",
];
const ENDPOINT_CLAUSES: &[&str] = &[
    "`path`",
    "`query`",
    "`headers`",
    "`auth`",
    "`timeout`",
    "`rate_limit`",
    "`id`",
    "`status`",
    "`effect`",
    "`absent on`",
    "`paginate`",
    "`expose accepted_bytes`",
];

/// Every terminal keyword of the canonical v3 grammar (§3.2, §11.1, §12.1,
/// §14.1). The parser must declare each one and `docs/spec.md` must quote
/// each one in its grammar.
const V3_KEYWORDS: &[&str] = &[
    "absent",
    "accepted_bytes",
    "allow",
    "api_key",
    "append",
    "auth",
    "authorization",
    "base",
    "basic",
    "bearer",
    "body",
    "challenge",
    "client",
    "concat",
    "cost",
    "credential",
    "credentials",
    "cursor",
    "custom",
    "each",
    "effect",
    "endpoint",
    "explicit",
    "expose",
    "header",
    "headers",
    "id",
    "idempotent",
    "init",
    "key",
    "METHOD",
    "never",
    "non_idempotent",
    "off",
    "on",
    "origin",
    "paginate",
    "path",
    "per",
    "policy",
    "query",
    "rate_limit",
    "rate_limits",
    "remove",
    "replace",
    "request",
    "response",
    "safe",
    "scope",
    "secret",
    "status",
    "subdomain",
    "success",
    "timeout",
    "unauthorized",
    "unauthorized_or_forbidden",
    "using",
    "values",
    "via",
    "when_present",
    "windows",
];

/// Retired v2 keywords the parser only recognizes to emit a migration
/// diagnostic; the normative grammar must never quote them as terminals.
const RETIRED_V2_KEYWORDS: &[&str] = &["vars", "host", "fmt", "retry"];

/// The v3 value-control matrix (§4.3) and credential controls (§11.3) as the
/// macro builds them: `(source file, required construction)`.
const GENERATED_NAME_CONSTRUCTIONS: &[(&str, &str)] = &[
    (
        "crates/concord-macros/src/semantic/names.rs",
        "prefixed_ident(\"set\", &value.name)",
    ),
    (
        "crates/concord-macros/src/semantic/names.rs",
        "prefixed_ident(\"replace\", &value.name)",
    ),
    ("crates/concord-macros/src/semantic/names.rs", "\"if_some\""),
    (
        "crates/concord-macros/src/semantic/names.rs",
        "prefixed_ident(\"omit\", &value.name)",
    ),
    (
        "crates/concord-macros/src/semantic/names.rs",
        "prefixed_ident(\"reset\", &value.name)",
    ),
    (
        "crates/concord-macros/src/semantic/auth.rs",
        "prefixed_ident(\"clear\", &declaration.name)",
    ),
    (
        "crates/concord-macros/src/semantic/auth.rs",
        "prefixed_ident(\"replace\", &declaration.name)",
    ),
    (
        "crates/concord-macros/src/semantic/auth.rs",
        "prefixed_ident(\"store_as\", &declaration.name)",
    ),
];

/// Retired v2 surfaces and the UI fixture proving the v3 product rejects each
/// one with a precise diagnostic: `(fixture, fixture needle, stderr needle)`.
const RETIRED_V2_DIAGNOSTICS: &[(&str, &str, &str)] = &[
    (
        "crates/concord/tests/ui/fail/syntax/retired_v2_forms.rs",
        "vars { tenant: String; }",
        "[C3-MIGRATE] `vars { ... }` was removed",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/retired_v2_forms.rs",
        "host [\"api\"];",
        "[C3-MIGRATE] `host [...]` was removed",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/retired_v2_forms.rs",
        "path [\"items\", id];",
        "[C3-MIGRATE] v2 path atom lists were replaced by a relative template string",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/retired_v2_forms.rs",
        "-> String using Text;",
        "[C3-MIGRATE] result `using Representation` was replaced by `via Representation`",
    ),
    (
        "crates/concord/tests/ui/fail/names/redundant_current_owner_qualification.rs",
        "{endpoint.id}",
        "[C3-MIGRATE] `endpoint.id` is redundant qualification of the current endpoint",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/default_expression_restrictions.rs",
        "query value: String = default;",
        "[C3-MIGRATE] `= default` was removed; place `#[default]` on the value declaration",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/retry_dsl_removed.rs",
        "retry reqwest_default;",
        "the `retry` DSL clause was removed",
    ),
    (
        "crates/concord/tests/ui/fail/pagination/numeric_page_limit_absent.rs",
        ".paginate(10_u32)",
        "no method named `paginate` found",
    ),
    (
        "crates/concord/tests/ui/fail/syntax/optional_static_secret_deferred.rs",
        "secret optional_api_key?;",
        "[C3-AUTH] optional static secrets are not supported",
    ),
];

/// §11.1: the parser's rejection of `secret NAME?;`.
const OPTIONAL_SECRET_REJECTION: &str = "optional static secrets are not supported";
const OPTIONAL_SECRET_FIXTURE: &str =
    "crates/concord/tests/ui/fail/syntax/optional_static_secret_deferred.rs";

/// §7.2: names the facade must re-export at its root.
const FACADE_ROOT_EXPORTS: &[&str] = &[
    "ApiError",
    "ClientBuildError",
    "ControlError",
    "IntoValue",
    "RequestControls",
    "CallOptions",
    "ControlSummary",
    "CancellationToken",
    "SecretString",
    "StoreOutcome",
    "DispatchKnowledge",
    "OperationId",
    "ExecutionMeta",
    "Bytes",
    "ByteStream",
    "Json",
    "Binary",
    "Text",
    "Empty",
    "Discard",
];
/// §7.2: detailed role modules of the facade.
const FACADE_ROLE_MODULES: &[&str] = &[
    "body",
    "response",
    "pagination",
    "auth",
    "rate_limit",
    "retry",
    "inspection",
];

/// Markdown heading of the README feature matrix checked against the facade
/// manifest.
const FEATURE_MATRIX_HEADING: &str = "Cargo features";

/// Anchors of fences that show construction/control internals; the tutorial
/// must show an ordinary awaited result before any of them (§19.2).
const TUTORIAL_INTERNAL_ANCHORS: &[&str] = &[
    "domain_tree_planning",
    "domain_tree_controls",
    "domain_tree_pagination",
    "runtime_endpoint",
    "runtime_endpoint_client",
    "execute_helper",
];
const TUTORIAL_INTERNAL_MARKERS: &[&str] = &[
    "unbound()",
    "RuntimeEndpoint",
    "with_options(",
    "cancel_with(",
    "control_summary(",
];
const TUTORIAL_AWAIT_ANCHORS: &[&str] = &["first_request", "domain_tree_await_request"];

#[derive(Default)]
struct Counts {
    dsl: usize,
    pseudocode: usize,
    anchors: usize,
    links: usize,
    fragments: usize,
    retired_files: usize,
    canonical_sources: usize,
}

#[derive(Clone)]
struct SourceReference {
    path: String,
    anchor: String,
    line: usize,
}

struct Fence {
    language: String,
    class: FenceClass,
    start: usize,
    body: String,
    source: Option<SourceReference>,
}

/// Rendering language and verification scope are independent: GitHub renders
/// every Rust/DSL example as Rust, while comments retain its evidence class.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum FenceClass {
    #[default]
    Code,
    CompiledDsl,
    Pseudocode,
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
    check_canonical_sources(root, &mut counts, &mut errors)?;
    check_tutorial_order(root, &mut errors)?;
    check_retired_language(root, &markdown_files, &mut counts, &mut errors)?;
    check_release_truth(root, &markdown_files, &mut errors)?;
    check_language_projection_agreement(root, &mut errors)?;
    check_facade_exports(root, &mut errors)?;
    check_feature_matrix(root, &mut errors)?;
    check_tool_agreement(root, &mut errors)?;
    let rules = crate::doc_rules::check(root, &mut errors)?;

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
        "validated {} machine-owned canonical source anchors and the tutorial order",
        counts.canonical_sources
    );
    println!(
        "validated retired public language across {} active files with explicit context allowlists",
        counts.retired_files
    );
    println!("validated one release-status owner and parser/spec/generated-name agreement");
    println!("validated facade root exports, the Cargo feature matrix and contract tool agreement");
    println!(
        "validated the EXT-01 extension contract of {} public traits, IDENTITY-01 endpoint-ID wording across {} product source files and the LIMIT-02 runtime limits section",
        rules.traits, rules.identity_files
    );
    Ok(())
}

fn check_markdown(
    root: &Path,
    path: &Path,
    normalized_fixture: &[String],
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
            && fence.class == FenceClass::Code
            && matches!(fence.language.as_str(), "" | RUST_LANGUAGE)
        {
            errors.push(format!(
                "{location}: an unanchored DSL excerpt needs a concord-dsl or concord-pseudocode comment before its Rust fence"
            ));
        }
        if matches!(fence.language.as_str(), "concord" | "concord-pseudocode")
            || (fence.class != FenceClass::Code && fence.language != RUST_LANGUAGE)
        {
            errors.push(format!(
                "{location}: Rust and DSL examples must use the `rust` highlighting language"
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
        } else if fence.class == FenceClass::CompiledDsl {
            counts.dsl += 1;
            if fence.body.trim().is_empty() {
                errors.push(format!("{location}: empty DSL fence"));
            } else if !source_contains(normalized_fixture, &fence.body) {
                errors.push(format!(
                    "{location}: normalized DSL block is absent from crates/concord-examples/src/docs_spec.rs"
                ));
            }
        } else if fence.class == FenceClass::Pseudocode {
            counts.pseudocode += 1;
            if fence.body.trim().is_empty() {
                errors.push(format!("{location}: empty DSL pseudocode fence"));
            }
        } else if fence.language == RUST_LANGUAGE {
            errors.push(format!(
                "{location}: normative Rust fence must be source-anchored to compiled code"
            ));
        }

        if !is_retired_allowlisted(&relative_path) {
            for finding in retired_fence_findings(fence) {
                errors.push(format!("{location}: {finding}"));
            }
            if is_user_documentation(&relative_path) && fence.class != FenceClass::Code {
                for finding in retired_v3_dsl_findings(&fence.body) {
                    errors.push(format!("{location}: {finding}"));
                }
            }
        }
    }

    for target in markdown_links(&content) {
        if target.is_empty() || has_uri_scheme(&target) || target.starts_with("//") {
            continue;
        }
        let (file_part, fragment) = target
            .split_once('#')
            .map_or((target.as_str(), None), |(file, fragment)| {
                (file, Some(fragment))
            });
        let Some(parent) = path.parent() else {
            errors.push(format!("{relative_path}: cannot resolve link {target}"));
            continue;
        };
        let resolved = if file_part.is_empty() {
            normalize_lexically(path)
        } else {
            normalize_lexically(&parent.join(file_part))
        };
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
    let mut pending_class: Option<(FenceClass, usize)> = None;

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if language.is_none() {
            let class = match line.trim() {
                "<!-- concord-dsl -->" => Some(FenceClass::CompiledDsl),
                "<!-- concord-pseudocode -->" => Some(FenceClass::Pseudocode),
                _ => None,
            };
            if let Some(class) = class {
                if pending_class.replace((class, line_number)).is_some() {
                    errors.push(format!(
                        "{relative_path}:{line_number}: duplicate DSL classification marker"
                    ));
                }
                continue;
            }
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
            if let Some((_, marker_line)) = pending_class {
                if !line.trim().is_empty() {
                    errors.push(format!(
                        "{relative_path}:{marker_line}: DSL classification marker must immediately precede a fence"
                    ));
                    pending_class = None;
                }
            }
            continue;
        }

        if is_fence_closing(line) {
            fences.push(Fence {
                language: language.take().expect("open fence language"),
                class: pending_class
                    .take()
                    .map_or(FenceClass::Code, |value| value.0),
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
    if let Some((_, marker_line)) = pending_class {
        errors.push(format!(
            "{relative_path}:{marker_line}: DSL classification marker has no fence"
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
        "TRACE ",
        "CONNECT ",
        "METHOD(",
        "values {",
        "credentials {",
        "request ",
        "response ",
        "subdomain ",
        "path ",
        "query ",
        "headers ",
        "auth ",
        "timeout ",
        "status ",
        "effect ",
        "paginate via ",
        "rate_limits {",
        "rate_limit ",
        "when_present ",
        "append ",
        "remove ",
        "replace ",
        "concat [",
        "expose accepted_bytes",
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

/// DOC-01: the tutorial builds a real typed client and shows an ordinary
/// awaited result before unbound/runtime/control internals.
fn check_tutorial_order(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let path = "docs/quick_start.md";
    let content = read(&root.join(path))?;
    let mut ignored = Vec::new();
    let fences = parse_fences(path, &content, &mut ignored);
    for error in tutorial_order_errors(path, &fences) {
        errors.push(error);
    }
    Ok(())
}

fn tutorial_order_errors(path: &str, fences: &[Fence]) -> Vec<String> {
    let first_await = fences.iter().position(|fence| {
        fence.source.as_ref().is_some_and(|source| {
            TUTORIAL_AWAIT_ANCHORS
                .iter()
                .any(|anchor| source.anchor == *anchor)
        })
    });
    let first_internal = fences.iter().position(|fence| {
        fence.source.as_ref().is_some_and(|source| {
            TUTORIAL_INTERNAL_ANCHORS
                .iter()
                .any(|anchor| source.anchor == *anchor)
        }) || TUTORIAL_INTERNAL_MARKERS
            .iter()
            .any(|marker| fence.body.contains(marker))
    });
    match (first_await, first_internal) {
        (None, _) => vec![format!(
            "{path}: the tutorial must show an anchored ordinary `.await` ({})",
            TUTORIAL_AWAIT_ANCHORS.join(" or ")
        )],
        (Some(first_await), Some(first_internal)) if first_internal < first_await => {
            vec![format!(
                "{path}:{}: construction/control internals appear before the first ordinary `.await` result at line {}",
                fences[first_internal].start, fences[first_await].start
            )]
        }
        _ => Vec::new(),
    }
}

fn check_canonical_sources(
    root: &Path,
    counts: &mut Counts,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let mut parsed = HashMap::<String, Vec<Fence>>::new();
    for (document, source_path, anchor, language) in CANONICAL_SOURCES {
        if !parsed.contains_key(*document) {
            let content = read(&root.join(document))?;
            parsed.insert(
                (*document).to_owned(),
                parse_fences(document, &content, errors),
            );
        }
        let found = parsed
            .get(*document)
            .expect("canonical document was parsed")
            .iter()
            .any(|fence| {
                fence.language == *language
                    && fence.source.as_ref().is_some_and(|source| {
                        source.path == *source_path && source.anchor == *anchor
                    })
            });
        if found {
            counts.canonical_sources += 1;
        } else {
            errors.push(format!(
                "{document}: canonical example must contain a `{language}` fence anchored to {source_path}#{anchor}"
            ));
        }
    }

    for document in ["README.md", "docs/quick_start.md", "docs/overview.md"] {
        let full_contract = parsed
            .get(document)
            .expect("front-door canonical document was parsed")
            .iter()
            .find(|fence| {
                fence.source.as_ref().is_some_and(|source| {
                    source.path == DOMAIN_TREE_SOURCE && source.anchor == "domain_tree_contract"
                })
            });
        if !full_contract.is_some_and(|fence| {
            fence.body.trim_start().starts_with("api!") && fence.body.contains("client CatalogApi")
        }) {
            errors.push(format!(
                "{document}: canonical front-door declaration must be the complete compiled CatalogApi contract"
            ));
        }
    }
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
        if is_user_documentation(&key) {
            for finding in retired_v3_surface_findings(&content, true) {
                errors.push(format!("{key}: {finding}"));
            }
        } else if key.ends_with(".rs") {
            for finding in retired_v3_surface_findings(&content, false) {
                errors.push(format!("{key}: {finding}"));
            }
        }
        counts.retired_files += 1;
    }
    Ok(())
}

/// README and `docs/` are the user documentation this checker holds to the
/// complete v3 retirement list; maintainer documents keep their own rules.
fn is_user_documentation(relative_path: &str) -> bool {
    relative_path == "README.md"
        || (relative_path.starts_with("docs/") && relative_path.ends_with(".md"))
}

/// A line that explicitly names the retired version or the migration
/// contract is migration context, not an active claim.
fn is_migration_context(line: &str) -> bool {
    line.contains("v2.1")
        || line.contains("v2.0")
        || line.contains("§25")
        || line.contains("C3-MIGRATE")
        || Regex::new(r"\bv2\b")
            .expect("migration context regex")
            .is_match(line)
}

/// v3 retired caller and DSL surfaces (§14.2, §16, §25.2), checked line by
/// line. Explicit migration context is exempt: a line naming the retired
/// version (see [`is_migration_context`]) and, in Markdown, every line of a
/// section headed "Migrating from v2…" or "Migration from v2…". In Markdown
/// the caller-surface patterns apply to every line and the DSL-shaped patterns
/// to inline code spans.
fn retired_v3_surface_findings(content: &str, markdown: bool) -> Vec<String> {
    let surface = [
        (
            r"\.paginate\s*\(",
            "retired `.paginate(...)` terminal; use `pages`, `collect_prefix` or `collect_all`",
        ),
        (
            r"\bPageLimit\b",
            "retired `PageLimit` selector; choose an explicit traversal terminal",
        ),
        (r"\bPaginatedRequest\b", "retired `PaginatedRequest` type"),
        (
            r"\.try_bind\s*\(",
            "retired runtime `try_bind`; bind with `owner.bind(endpoint)`",
        ),
        (
            r"\.body_(?:codec|spec)\s*::",
            "retired runtime body builder; use `body_repr::<B, T>(input)`",
        ),
    ]
    .map(|(pattern, message)| (Regex::new(pattern).expect("v3 surface regex"), message));
    let dsl = [
        (r"^vars\s*\{", "retired `vars` declaration; write `values`"),
        (r"^host\s*\[", "retired `host` clause; write `subdomain`"),
        (
            r"^path\s+(?:replace\s+)?\[",
            "retired path atom list; write a relative `path` template",
        ),
        (
            r"\busing\s+[A-Z]",
            "retired `using` representation; write `via`",
        ),
        (
            r"=\s*default\s*(?:[;,)]|$)",
            "retired `= default` initializer; write `#[default]`",
        ),
    ]
    .map(|(pattern, message)| (Regex::new(pattern).expect("v3 DSL regex"), message));

    let mut findings = Vec::new();
    let mut exempt_level: Option<usize> = None;
    let mut in_fence = false;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let mut dsl_candidates = Vec::new();
        if markdown {
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if !in_fence {
                if let Some((level, title)) = heading(trimmed) {
                    if exempt_level.is_some_and(|open| level <= open) {
                        exempt_level = None;
                    }
                    if is_migration_heading(title) {
                        exempt_level = Some(level);
                    }
                }
            }
            if exempt_level.is_some() {
                continue;
            }
            // DSL fences are checked as a whole by `retired_v3_dsl_findings`;
            // `text` fences may show display provenance such as the
            // `endpoint.category` origin of an expanded contract view.
            if !in_fence {
                dsl_candidates.extend(inline_code_spans(line));
            }
        }
        if is_migration_context(line) {
            continue;
        }
        for (regex, message) in &surface {
            if regex.is_match(line) {
                findings.push(format!("line {}: {message}", index + 1));
            }
        }
        for candidate in &dsl_candidates {
            for (regex, message) in &dsl {
                if regex.is_match(candidate.trim()) {
                    findings.push(format!("line {}: {message}", index + 1));
                }
            }
            if redundant_endpoint_qualification(candidate) {
                findings.push(format!(
                    "line {}: retired `endpoint.x` qualification; write the bare local name",
                    index + 1
                ));
            }
        }
    }
    findings
}

/// A heading that opens an explicit v2 migration section, optionally after a
/// section number (`## 17. Migration from v2.1`).
fn is_migration_heading(title: &str) -> bool {
    let title = title
        .trim_start_matches(|character: char| character.is_ascii_digit() || character == '.')
        .trim_start()
        .to_ascii_lowercase();
    title.starts_with("migrating from v2") || title.starts_with("migration from v2")
}

/// Retired v3 DSL forms inside an explicitly classified Rust fence. String
/// literal contents are ignored, so a legacy rate label such as
/// `"endpoint.item_id"` is data, not a reference.
fn retired_v3_dsl_findings(body: &str) -> Vec<String> {
    let code = strip_string_literals(body);
    let patterns = [
        (
            r"(?m)^\s*path\s+(?:replace\s+)?\[",
            "retired path atom list; write a relative `path` template",
        ),
        (
            r"=\s*default\s*[;,)]",
            "retired `= default` initializer; write `#[default]`",
        ),
        (
            r"(?m)^\s*retry\b",
            "retired `retry` DSL clause; install a `RetryPolicy` at runtime",
        ),
        (r"\bvars\s*\.", "retired `vars.*` reference"),
    ];
    let mut findings = patterns
        .into_iter()
        .filter(|(pattern, _)| {
            Regex::new(pattern)
                .expect("retired v3 DSL regex")
                .is_match(&code)
        })
        .map(|(_, message)| message.to_owned())
        .collect::<Vec<_>>();
    if code.lines().any(redundant_endpoint_qualification)
        || Regex::new(r"\{\s*endpoint\s*\.")
            .expect("template hole regex")
            .is_match(body)
    {
        findings.push("retired `endpoint.x` qualification; write the bare local name".to_owned());
    }
    findings
}

/// `endpoint.name` used as a value reference (not a Rust method call such as
/// `endpoint.description()`).
fn redundant_endpoint_qualification(text: &str) -> bool {
    let regex = Regex::new(r"\bendpoint\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)")
        .expect("endpoint qualification regex");
    regex.captures_iter(text).any(|captures| {
        let end = captures.get(0).expect("whole match").end();
        !text[end..].trim_start().starts_with('(')
    })
}

fn strip_string_literals(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut in_string = false;
    let mut escaped = false;
    for character in body.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
                result.push('"');
            } else if character == '\n' {
                result.push('\n');
            }
        } else {
            if character == '"' {
                in_string = true;
            }
            result.push(character);
        }
    }
    result
}

fn inline_code_spans(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    for character in line.chars() {
        if character == '`' {
            if in_code {
                spans.push(std::mem::take(&mut current));
            }
            in_code = !in_code;
        } else if in_code {
            current.push(character);
        }
    }
    spans
}

/// `(level, title)` of an ATX Markdown heading line.
fn heading(trimmed: &str) -> Option<(usize, &str)> {
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&hashes) || !trimmed[hashes..].starts_with(' ') {
        return None;
    }
    Some((hashes, trimmed[hashes..].trim()))
}

fn retired_fence_findings(fence: &Fence) -> Vec<String> {
    let mut findings = retired_surface_findings(&fence.body);
    if fence.class != FenceClass::Code {
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
            (r"(?m)^\s*timeout\s+off\s*;", "retired `timeout off` clause"),
            (
                r"(?m)^\s*rate_limit\s+use\b",
                "retired `rate_limit use` clause",
            ),
            (r"(?m)^\s*by\s*\[", "retired rate-limit `by` clause"),
            (
                r"(?m)^\s*paginate\s+(?:with\b|[A-Z_][A-Za-z0-9_:<>]*)",
                "retired pagination declaration",
            ),
            (r"\?=|\+=", "retired punctuation patch operator"),
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
        if retired_using_form(&fence.body) {
            findings.push("retired body/result `using` form".to_owned());
        }
    }
    findings
}

/// v3 keeps `using` only as a rate window source (spec §12.1 `window_source`):
/// `rate_limit "<id>" key [...] (cost n)? using <set>;`, or a standalone
/// `using <set>;` line inside a policy or inline attachment (a snake_case set
/// name, which keeps a split-line v2 `using Json;` retired). Every other
/// `using` is the retired v2 body/result representation form.
fn retired_using_form(body: &str) -> bool {
    let token = Regex::new(r"\busing\b").expect("retired DSL regex");
    let window_source = Regex::new(
        r#"^\s*(?:rate_limit\s+"[^"]*"\s+key\s*\[[^\]]*\]\s*(?:cost\s+[0-9][0-9_]*\s+)?using\s+[A-Za-z_][A-Za-z0-9_]*|using\s+[a-z_][a-z0-9_]*)\s*;\s*(?://.*)?$"#,
    )
    .expect("window source regex");
    body.lines()
        .any(|line| token.is_match(line) && !window_source.is_match(line))
}

fn retired_surface_findings(content: &str) -> Vec<String> {
    let patterns = [
        (
            r#"\.accept\s*\(\s*(?:\"|String::|[A-Za-z_])|\.accept_off\s*\("#,
            "retired endpoint/runtime `accept` override",
        ),
        (r"\bresponse_bytes\s*\(", "retired `response_bytes` API"),
        (r"\bSafeRequestPlan\b", "retired `SafeRequestPlan` type"),
        (r"\bResponseBodySpec\b", "retired `ResponseBodySpec` type"),
        (r"\.paginate\s*\(\s*[0-9]", "numeric pagination coercion"),
        (r"\.with_x\s*\(", "retired `with_*` optional-value control"),
        (
            r"\.with_<value>\s*\(",
            "retired `with_*` optional-value control",
        ),
    ];
    let mut findings = Vec::new();
    if retired_clear_control(content) {
        findings.push("retired `clear_*` control".to_owned());
    }
    findings.extend(
        patterns
            .into_iter()
            .filter(|(pattern, _)| {
                Regex::new(pattern)
                    .expect("retired surface regex")
                    .is_match(content)
            })
            .map(|(_, message)| message.to_owned()),
    );
    findings
}

/// v3 keeps `clear_*` only as the local credential control
/// `client.credentials().clear_x()` (§11.3); every other `.clear_*(` call is
/// the retired v2 optional-value control.
fn retired_clear_control(content: &str) -> bool {
    Regex::new(r"\.clear_[A-Za-z_][A-Za-z0-9_]*\s*\(")
        .expect("retired clear regex")
        .find_iter(content)
        .any(|found| {
            !content[..found.start()]
                .trim_end()
                .ends_with("credentials()")
        })
}

fn is_retired_allowlisted(relative_path: &str) -> bool {
    relative_path.starts_with("dev_doc/history/")
        || relative_path.starts_with("crates/concord/tests/ui/fail/")
        || relative_path.starts_with("crates/concord-macros/src/syntax/")
        || relative_path == "dev_doc/documentation_contract.md"
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
    for error in version_banner_errors(
        version,
        &read(&root.join("README.md"))?,
        &read(&root.join("docs/README.md"))?,
    ) {
        errors.push(error);
    }
    for path in markdown_files {
        let key = relative(root, path);
        if !is_user_documentation(&key) || is_retired_allowlisted(&key) {
            continue;
        }
        for stale in stale_version_mentions(version, &read(path)?) {
            errors.push(format!(
                "{key}: stale version `{stale}` in active user documentation (workspace is `{version}`)"
            ));
        }
    }
    if version.contains("beta") {
        if !status.contains("integration candidate / beta")
            || !status.contains("Publication:** disabled")
            || !status.contains("**Beta integration decision:**")
            || !status.contains("**Stable publication decision:**")
        {
            errors.push(
                "beta workspace must declare integration lifecycle, disabled publication, and separate beta/stable decisions"
                    .to_owned(),
            );
        }
    } else if !version.contains('-')
        && !status.contains("stable local release candidate accepted for merge")
    {
        errors.push(format!(
            "stable {version} requires the final accepted lifecycle text"
        ));
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
    check_claim_states(root, errors)?;
    check_active_maintainer_truth(root, errors)?;
    check_negative_space_fixtures(root, errors);
    Ok(())
}

fn check_claim_states(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "Observed",
        "Inferred",
        "Assumed",
        "Desired",
        "Candidate",
        "Selected",
        "Supported",
        "Disconfirmed",
        "Unknown",
        "Deferred",
    ];

    let ledger = read(&root.join("dev_doc/claim_evidence_ledger.md"))?;
    let mut claims = 0;
    for (index, line) in ledger.lines().enumerate() {
        let columns = line.split('|').map(str::trim).collect::<Vec<_>>();
        let Some(id) = columns.get(1).filter(|id| {
            id.strip_prefix('C').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        }) else {
            continue;
        };
        claims += 1;
        let state = columns.get(3).copied().unwrap_or_default();
        if !ALLOWED.contains(&state) {
            errors.push(format!(
                "dev_doc/claim_evidence_ledger.md:{}: claim {id} uses non-canonical state `{state}`",
                index + 1
            ));
        }
    }
    if claims == 0 {
        errors.push("claim/evidence ledger contains no numbered claims".to_owned());
    }
    Ok(())
}

fn check_active_maintainer_truth(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    for relative_path in ACTIVE_MAINTAINER_DOCS {
        let content = read(&root.join(relative_path))?;
        if !content.contains("**Status:**")
            && !content.contains("<!-- concord-release-status-owner -->")
        {
            errors.push(format!(
                "{relative_path}: active maintainer document must carry an explicit status"
            ));
        }
        for stale in [
            "NOT MERGEABLE while Gate 0",
            "trait and real-API gates pending",
            "trait ergonomics gate pending",
            "exact-candidate rerun pending",
            "Stream erasure/performance remains Gate 8 work",
            "stable merge is forbidden",
        ] {
            if content.contains(stale) {
                errors.push(format!(
                    "{relative_path}: stale active maintainer claim remains: `{stale}`"
                ));
            }
        }
        for stale_platform in [
            "Windows, Linux, and macOS is mandatory",
            "Windows, Linux, and macOS are mandatory",
            "required macOS",
            "macOS execution required",
        ] {
            if content.contains(stale_platform) {
                errors.push(format!(
                    "{relative_path}: macOS is still described as a mandatory gate"
                ));
            }
        }
    }

    for relative_path in [
        "dev_doc/release_verification_2_1.md",
        "dev_doc/evidence/package_verification.md",
        "dev_doc/evidence/beta_candidate_0f72e865.md",
    ] {
        let content = read(&root.join(relative_path))?;
        let header = content.lines().take(12).collect::<Vec<_>>().join("\n");
        if !header.contains("Historical") && !header.contains("Superseded") {
            errors.push(format!(
                "{relative_path}: prior evidence must be explicitly historical or superseded near its heading"
            ));
        }
    }

    let status = read(&root.join("dev_doc/release_gate_status.md"))?;
    let gates = read(&root.join("docs/evidence_and_release_gates.md"))?;
    let workflow = read(&root.join(".github/workflows/ci.yml"))?;
    for (relative_path, content) in [
        ("dev_doc/release_gate_status.md", status.as_str()),
        ("docs/evidence_and_release_gates.md", gates.as_str()),
    ] {
        if !content.contains("Windows and Linux are supported and mandatory")
            || !content.contains("macOS is not currently a supported or release-gated platform")
        {
            errors.push(format!(
                "{relative_path}: supported-platform policy must require only Windows and Linux"
            ));
        }
    }
    if workflow.contains("macos-latest")
        || !workflow.contains("os: [ubuntu-latest, windows-latest]")
    {
        errors.push(
            ".github/workflows/ci.yml: required platform matrix must contain only Ubuntu and Windows"
                .to_owned(),
        );
    }
    Ok(())
}

fn check_negative_space_fixtures(root: &Path, errors: &mut Vec<String>) {
    for relative_path in NEGATIVE_SPACE_FIXTURES {
        if !root.join(relative_path).is_file() {
            errors.push(format!(
                "missing no-retrocompatibility fixture {relative_path}"
            ));
        }
    }
}

fn check_language_projection_agreement(
    root: &Path,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    check_parser_clause_agreement(root, errors)?;
    check_grammar_keyword_agreement(root, errors)?;
    check_optional_secret_agreement(root, errors)?;
    check_generated_name_agreement(root, errors)?;
    check_rate_dimension_agreement(root, errors)?;
    check_retired_v2_diagnostics(root, errors)?;
    let spec = read(&root.join("docs/spec.md"))?;

    // §14.2 / §25.2: the traversal terminals replaced `.paginate(PageLimit)`;
    // the normative pages name the v3 terminals instead.
    let pagination = read(&root.join("docs/pagination.md"))?;
    for terminal in [".pages(", ".collect_prefix(", ".collect_all("] {
        if !pagination.contains(terminal) && !spec.contains(terminal) {
            errors.push(format!(
                "normative docs must show the v3 traversal terminal `{terminal}`"
            ));
        }
    }
    if !spec.contains("request explicit;")
        || !spec.contains("response explicit;")
        || !spec.contains("Representation defaults")
    {
        errors.push(
            "spec must retain representation inheritance and explicit-fence examples".to_owned(),
        );
    }

    for directory in [
        "crates/concord/src",
        "crates/concord-core/src",
        "crates/concord-extras/src",
        "crates/concord-macros/src/emit",
        "crates/concord-macros/src/semantic",
    ] {
        for path in collect_files(&root.join(directory), "rs")? {
            let source = read(&path)?;
            for finding in retired_implementation_symbols(&source) {
                errors.push(format!(
                    "{}: retired public symbol `{finding}` remains in active implementation source",
                    relative(root, &path)
                ));
            }
        }
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

/// Earlier Concord releases that active user documentation must not present
/// as the current product.
const RETIRED_CONCORD_VERSIONS: &[&str] = &["2.1.0-beta.1", "2.0.0"];

/// The README and documentation index titles name the workspace major.
fn version_banner_errors(version: &str, readme: &str, index: &str) -> Vec<String> {
    let major = version.split('.').next().unwrap_or(version);
    let banner = Regex::new(&format!(r"\bConcord v{}\b", regex::escape(major)))
        .expect("version banner regex");
    let mut errors = Vec::new();
    for (label, content) in [("README.md", readme), ("docs/README.md", index)] {
        let title = content.lines().find_map(|line| {
            heading(line.trim_start())
                .filter(|(level, _)| *level == 1)
                .map(|(_, title)| title)
        });
        match title {
            Some(title) if banner.is_match(title) => {}
            Some(title) => errors.push(format!(
                "{label}: title `{title}` must name `Concord v{major}` for workspace `{version}`"
            )),
            None => errors.push(format!(
                "{label}: missing a top-level title naming `Concord v{major}`"
            )),
        }
    }
    errors
}

/// Retired Concord versions mentioned outside explicit migration context.
fn stale_version_mentions(version: &str, content: &str) -> Vec<&'static str> {
    let mut found = RETIRED_CONCORD_VERSIONS
        .iter()
        .copied()
        .filter(|retired| *retired != version)
        .filter(|retired| {
            content
                .lines()
                .any(|line| !is_migration_context(line) && line.contains(retired))
        })
        .collect::<Vec<_>>();
    found.dedup();
    found
}

/// The parser's fallback diagnostics name every legal clause of the §3.2
/// grammar for the owner kind.
fn check_parser_clause_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let declaration = read(&root.join("crates/concord-macros/src/syntax/declaration.rs"))?;
    for (message, clauses) in [
        ("unsupported client entry", CLIENT_CLAUSES),
        ("unsupported scope entry", SCOPE_CLAUSES),
        ("unsupported endpoint clause", ENDPOINT_CLAUSES),
    ] {
        for error in fallback_clause_errors(&declaration, message, clauses) {
            errors.push(error);
        }
    }
    for snapshot in [
        "crates/concord/tests/ui/fail/syntax/second_client.stderr",
        "crates/concord/tests/ui/fail/syntax/malformed.stderr",
        "crates/concord/tests/ui/fail/syntax/unsupported_endpoint_clause.stderr",
    ] {
        if !root.join(snapshot).is_file() {
            errors.push(format!("missing parser fallback UI snapshot {snapshot}"));
        }
    }
    Ok(())
}

fn fallback_clause_errors(source: &str, message: &str, clauses: &[&str]) -> Vec<String> {
    let Some(diagnostic) = source.lines().find(|line| line.contains(message)) else {
        return vec![format!("parser fallback `{message}` is missing")];
    };
    let missing = clauses
        .iter()
        .filter(|clause| !diagnostic.contains(*clause))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "parser fallback `{message}` does not name all current legal clauses (missing {})",
            missing.join(", ")
        )]
    }
}

/// Every v3 grammar keyword is a parser keyword and a quoted terminal of the
/// normative grammar in `docs/spec.md`; retired v2 keywords are not.
fn check_grammar_keyword_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let keywords = read(&root.join("crates/concord-macros/src/syntax/keyword.rs"))?;
    let spec = read(&root.join("docs/spec.md"))?;
    for error in grammar_keyword_errors(&keywords, &spec) {
        errors.push(error);
    }
    Ok(())
}

fn grammar_keyword_errors(keyword_source: &str, spec: &str) -> Vec<String> {
    let declared = Regex::new(r"custom_keyword!\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)")
        .expect("keyword regex")
        .captures_iter(keyword_source)
        .map(|captures| captures[1].to_owned())
        .collect::<HashSet<_>>();
    let mut errors = Vec::new();
    for keyword in V3_KEYWORDS {
        if !declared.contains(*keyword) {
            errors.push(format!(
                "crates/concord-macros/src/syntax/keyword.rs: v3 grammar keyword `{keyword}` is not declared"
            ));
        }
        if !spec.contains(&format!("\"{keyword}\"")) {
            errors.push(format!(
                "docs/spec.md: the grammar does not quote v3 terminal \"{keyword}\""
            ));
        }
    }
    for retired in RETIRED_V2_KEYWORDS {
        if spec.contains(&format!("\"{retired}\"")) {
            errors.push(format!(
                "docs/spec.md: the grammar quotes retired v2 keyword \"{retired}\""
            ));
        }
    }
    errors
}

/// §11.1: `secret NAME?;` is rejected by the parser, pinned by a focused UI
/// fixture, and never admitted by the normative docs.
fn check_optional_secret_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let credentials = read(&root.join("crates/concord-macros/src/syntax/credentials.rs"))?;
    let fixture_path = root.join(OPTIONAL_SECRET_FIXTURE);
    let fixture = read(&fixture_path).unwrap_or_default();
    let snapshot = read(&fixture_path.with_extension("stderr")).unwrap_or_default();
    if !credentials.contains(OPTIONAL_SECRET_REJECTION)
        || !fixture.contains("secret optional_api_key?;")
        || !snapshot.contains("[C3-AUTH]")
        || !snapshot.contains(OPTIONAL_SECRET_REJECTION)
    {
        errors.push(
            "optional static secret rejection must agree across parser and focused UI fixture"
                .to_owned(),
        );
    }
    let active_optional_secret = Regex::new(r"(?m)^\s*secret\s+[A-Za-z_][A-Za-z0-9_]*\s*\?\s*;")
        .expect("optional secret regex");
    for document in ["docs/spec.md", "docs/auth.md"] {
        let content = read(&root.join(document))?;
        if active_optional_secret.is_match(&content) {
            errors.push(format!(
                "{document} still admits optional static secret syntax"
            ));
        }
    }
    if !read(&root.join("docs/auth.md"))?
        .to_ascii_lowercase()
        .contains(OPTIONAL_SECRET_REJECTION)
    {
        errors.push(format!(
            "docs/auth.md must state that {OPTIONAL_SECRET_REJECTION}"
        ));
    }
    Ok(())
}

/// The macro builds exactly the v3 value-control and credential-control
/// names, no retired alias construction remains, and the generated core
/// method names reserved by the macro are documented.
fn check_generated_name_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    for (path, construction) in GENERATED_NAME_CONSTRUCTIONS {
        if !read(&root.join(path))?.contains(construction) {
            errors.push(format!(
                "{path}: generated-name construction `{construction}` is missing"
            ));
        }
    }
    for path in collect_files(&root.join("crates/concord-macros/src"), "rs")? {
        let source = read(&path)?;
        if source.contains("format!(\"with_")
            || source.contains("format_ident!(\"with_")
            || source.contains("prefixed_ident(\"with\"")
            || source.contains("format!(\"clear_")
            || source.contains("format_ident!(\"clear_")
        {
            errors.push(format!(
                "{}: generated optional-control alias construction remains",
                relative(root, &path)
            ));
        }
    }
    for fixture in [
        "crates/concord/tests/ui/pass/language/with_namespace_released.rs",
        "crates/concord/tests/ui/fail/capabilities/with_optional_control_absent.rs",
        "crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.rs",
    ] {
        if !root.join(fixture).is_file() {
            errors.push(format!(
                "missing generated-name agreement fixture {fixture}"
            ));
        }
    }
    let names = read(&root.join("crates/concord-macros/src/semantic/names.rs"))?;
    let generated_client = read(&root.join("docs/generated_client.md"))?;
    for constant in ["CLIENT_METHODS", "BUILDER_METHODS", "REQUEST_METHODS"] {
        let methods = const_string_list(&names, constant);
        if methods.is_empty() {
            errors.push(format!(
                "crates/concord-macros/src/semantic/names.rs: reserved method list `{constant}` was not found"
            ));
        }
        for method in methods {
            if matches!(method.as_str(), "clone" | "into_future" | "default") {
                continue;
            }
            if !generated_client.contains(&format!("`{method}`"))
                && !generated_client.contains(&format!("{method}("))
            {
                errors.push(format!(
                    "docs/generated_client.md does not document generated core method `{method}` ({constant})"
                ));
            }
        }
    }
    Ok(())
}

/// The quoted strings of `const NAME: &[&str] = &[ ... ];` in `source`.
fn const_string_list(source: &str, name: &str) -> Vec<String> {
    let Some(start) = source.find(&format!("const {name}: &[&str] = &[")) else {
        return Vec::new();
    };
    let rest = &source[start..];
    let Some(end) = rest.find("];") else {
        return Vec::new();
    };
    Regex::new(r#""([^"]+)""#)
        .expect("string list regex")
        .captures_iter(&rest[..end])
        .map(|captures| captures[1].to_owned())
        .collect()
}

/// Rate diagnostics, generated docs and core schema name `origin`, never the
/// retired `host` dimension.
fn check_rate_dimension_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let syntax = read(&root.join("crates/concord-macros/src/syntax/rate_limits.rs"))?;
    let generated_docs = read(&root.join("crates/concord-macros/src/emit/docs.rs"))?;
    let schema = read(&root.join("crates/concord-core/src/rate_limit/schema.rs"))?;
    let snapshot = read(&root.join(
        "crates/concord/tests/ui/fail/rate_limits/rate_limit_unsupported_bare_dimensions.stderr",
    ))
    .unwrap_or_default();
    if !syntax.contains("a key dimension is `origin`, `endpoint`")
        || !generated_docs.contains("BoundDim::Origin => \"origin\".to_owned()")
        || !schema.contains("pub const ORIGIN_LABEL: &str = \"origin\";")
        || snapshot.contains("only `host`")
    {
        errors.push(
            "active rate diagnostics and descriptions must name `origin`, not retired `host`"
                .to_owned(),
        );
    }
    Ok(())
}

/// Every retired v2 surface the documentation calls removed is rejected by
/// the product with a precise diagnostic (MIGRATE-03).
fn check_retired_v2_diagnostics(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    for (fixture, needle, diagnostic) in RETIRED_V2_DIAGNOSTICS {
        let path = root.join(fixture);
        if !path.is_file() {
            errors.push(format!("missing retired-v2 diagnostic fixture {fixture}"));
            continue;
        }
        let source = read(&path)?;
        let snapshot = read(&path.with_extension("stderr")).unwrap_or_default();
        if !source.contains(needle) || !snapshot.contains(diagnostic) {
            errors.push(format!(
                "{fixture}: retired v2 form `{needle}` must be rejected with `{diagnostic}`"
            ));
        }
    }
    // §16.2 / RUNTIME-02: v2.1 dynamic credential selection is removed; a
    // runtime endpoint can only remove inherited placements.
    let runtime = read(&root.join("crates/concord-core/src/request/runtime_endpoint/mod.rs"))?;
    let authentication =
        Regex::new(r"pub fn ([A-Za-z0-9_]*(?:auth|credential)[A-Za-z0-9_]*)\s*[<(]")
            .expect("runtime auth regex")
            .captures_iter(&runtime)
            .map(|captures| captures[1].to_owned())
            .collect::<Vec<_>>();
    if authentication != ["auth_off", "auth_remove"] {
        errors.push(format!(
            "runtime endpoints must offer only `auth_off`/`auth_remove` authentication operations; found {authentication:?}"
        ));
    }
    Ok(())
}

/// Retired v2 public symbols in implementation source. `response_bytes` is
/// retired only as the v2 terminal method; `cap_response_bytes` and
/// `acquired_response_bytes` are v3 API.
fn retired_implementation_symbols(source: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    for retired in ["ResponseBodySpec", "SafeRequestPlan"] {
        if source.contains(retired) {
            found.push(retired);
        }
    }
    if Regex::new(r"(?:\bfn\s+|\.)response_bytes\s*[<(]")
        .expect("response bytes regex")
        .is_match(source)
    {
        found.push("response_bytes()");
    }
    found
}

/// §7.2: the facade root re-exports the required convenience names and role
/// modules, and the generated-client guide lists the same root exports.
fn check_facade_exports(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let source = read(&root.join("crates/concord/src/lib.rs"))?;
    let file = syn::parse_file(&source)
        .map_err(|error| format!("could not parse crates/concord/src/lib.rs: {error}"))?;
    let mut root_names = HashSet::new();
    let mut modules = HashMap::<String, HashSet<String>>::new();
    let mut api_macro = false;
    for item in &file.items {
        match item {
            syn::Item::Use(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                collect_use_names(&item.tree, &mut root_names);
            }
            syn::Item::Mod(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let mut names = HashSet::new();
                if let Some((_, items)) = &item.content {
                    for inner in items {
                        if let syn::Item::Use(inner) = inner {
                            if matches!(inner.vis, syn::Visibility::Public(_)) {
                                collect_use_names(&inner.tree, &mut names);
                            }
                        }
                    }
                }
                modules.insert(item.ident.to_string(), names);
            }
            syn::Item::Macro(item)
                if item.ident.as_ref().is_some_and(|ident| ident == "api")
                    && item
                        .attrs
                        .iter()
                        .any(|attribute| attribute.path().is_ident("macro_export")) =>
            {
                api_macro = true;
            }
            _ => {}
        }
    }
    if !api_macro {
        errors.push(
            "crates/concord/src/lib.rs: the facade must export `api!` at its root".to_owned(),
        );
    }
    for name in FACADE_ROOT_EXPORTS {
        if !root_names.contains(*name) {
            errors.push(format!(
                "crates/concord/src/lib.rs: required root re-export `{name}` is missing (§7.2)"
            ));
        }
    }
    for module in FACADE_ROLE_MODULES {
        if !modules.contains_key(*module) {
            errors.push(format!(
                "crates/concord/src/lib.rs: role module `{module}` is missing (§7.2)"
            ));
        }
    }
    if !modules
        .get("rate_limit")
        .is_some_and(|names| names.contains("LocalRateLimiter"))
    {
        errors.push(
            "crates/concord/src/lib.rs: `rate_limit::LocalRateLimiter` must be reachable (§7.2)"
                .to_owned(),
        );
    }
    let guide = read(&root.join("docs/generated_client.md"))?;
    for name in FACADE_ROOT_EXPORTS {
        if !guide.contains(&format!("`{name}`")) {
            errors.push(format!(
                "docs/generated_client.md does not list facade root export `{name}`"
            ));
        }
    }
    Ok(())
}

fn collect_use_names(tree: &syn::UseTree, names: &mut HashSet<String>) {
    match tree {
        syn::UseTree::Path(path) => collect_use_names(&path.tree, names),
        syn::UseTree::Name(name) => {
            names.insert(name.ident.to_string());
        }
        syn::UseTree::Rename(rename) => {
            names.insert(rename.rename.to_string());
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                collect_use_names(tree, names);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

/// V3-O05: the README feature matrix lists exactly the facade's Cargo
/// features and marks exactly its default set.
fn check_feature_matrix(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let manifest = read(&root.join("crates/concord/Cargo.toml"))?;
    let (features, defaults) = manifest_features(&manifest);
    let readme = read(&root.join("README.md"))?;
    match feature_table(&readme, FEATURE_MATRIX_HEADING) {
        Some(rows) => {
            for error in feature_matrix_errors(&features, &defaults, &rows) {
                errors.push(format!("README.md: {error}"));
            }
        }
        None => errors.push(format!(
            "README.md: missing the `{FEATURE_MATRIX_HEADING}` table checked against crates/concord/Cargo.toml"
        )),
    }
    Ok(())
}

/// `(features, default features)` of a manifest's `[features]` table.
fn manifest_features(manifest: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut features = BTreeSet::new();
    let mut defaults = BTreeSet::new();
    let quoted = Regex::new(r#""([^"]+)""#).expect("quoted regex");
    for line in manifest
        .lines()
        .skip_while(|line| line.trim() != "[features]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
    {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name == "default" {
            defaults.extend(
                quoted
                    .captures_iter(value)
                    .map(|captures| captures[1].to_owned()),
            );
        } else if !name.is_empty() && !name.starts_with('#') {
            features.insert(name.to_owned());
        }
    }
    (features, defaults)
}

/// `(feature, default)` rows of the Markdown table under `title`.
fn feature_table(markdown: &str, title: &str) -> Option<Vec<(String, bool)>> {
    let mut lines = markdown.lines();
    lines.find(|line| heading(line.trim_start()).is_some_and(|(_, text)| text == title))?;
    let mut rows = Vec::new();
    let mut started = false;
    for line in lines {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            if started || heading(trimmed).is_some() {
                break;
            }
            continue;
        }
        started = true;
        let columns = trimmed.split('|').map(str::trim).collect::<Vec<_>>();
        let Some(name) = columns
            .get(1)
            .and_then(|cell| cell.strip_prefix('`'))
            .and_then(|cell| cell.strip_suffix('`'))
        else {
            continue;
        };
        let default = columns
            .get(2)
            .is_some_and(|cell| cell.eq_ignore_ascii_case("yes"));
        rows.push((name.to_owned(), default));
    }
    Some(rows)
}

fn feature_matrix_errors(
    features: &BTreeSet<String>,
    defaults: &BTreeSet<String>,
    rows: &[(String, bool)],
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut documented = BTreeSet::new();
    for (name, default) in rows {
        if !documented.insert(name.clone()) {
            errors.push(format!("feature `{name}` is listed more than once"));
        }
        if !features.contains(name) {
            errors.push(format!(
                "feature `{name}` is documented but not declared by crates/concord/Cargo.toml"
            ));
        }
        if *default != defaults.contains(name) {
            errors.push(format!(
                "feature `{name}` default marking disagrees with crates/concord/Cargo.toml"
            ));
        }
    }
    for name in features.difference(&documented) {
        errors.push(format!(
            "feature `{name}` of crates/concord/Cargo.toml is not documented"
        ));
    }
    errors
}

/// The user docs show the exact V3 contract tool usage and format.
fn check_tool_agreement(root: &Path, errors: &mut Vec<String>) -> Result<(), String> {
    let observation = read(&root.join("docs/planning_and_observation.md"))?;
    let contract_usage = format!("cargo run -p concord-xtask -- {}", crate::contract::USAGE);
    if !observation.contains(&contract_usage) {
        errors.push(format!(
            "docs/planning_and_observation.md must show the exact tool usage `{contract_usage}`"
        ));
    }
    if !observation.contains(concord_inspect::CONTRACT_FORMAT) {
        errors.push(format!(
            "docs/planning_and_observation.md must name the `{}` format",
            concord_inspect::CONTRACT_FORMAT
        ));
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

pub(crate) fn collect_files(directory: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
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

// Ignore Rust trivia between tokens, never bytes within a literal. Keeping a
// vector also prevents substring matches from starting inside another token.
fn normalize_source(source: &str) -> Vec<String> {
    fn append(stream: proc_macro2::TokenStream, out: &mut Vec<String>) {
        use proc_macro2::{Delimiter, TokenTree};
        let mut tokens = stream.into_iter().peekable();
        while let Some(token) = tokens.next() {
            match token {
                TokenTree::Group(group) => {
                    let delimiter = match group.delimiter() {
                        Delimiter::Parenthesis => "()",
                        Delimiter::Brace => "{}",
                        Delimiter::Bracket => "[]",
                        Delimiter::None => "none",
                    };
                    out.push(format!("open:{delimiter}"));
                    append(group.stream(), out);
                    out.push(format!("close:{delimiter}"));
                }
                TokenTree::Ident(ident) => out.push(format!("ident:{ident}")),
                TokenTree::Literal(literal) => out.push(format!("literal:{literal}")),
                TokenTree::Punct(punct) => {
                    // Joint spacing matters for compound Rust punctuation
                    // and the lifetime introducer. Other adjacency, e.g.
                    // &'a vs & 'a or ::< vs :: <, is formatting trivia.
                    let joint = punct.spacing() == proc_macro2::Spacing::Joint
                        && match tokens.peek() {
                            Some(TokenTree::Punct(next)) => matches!(
                                (punct.as_char(), next.as_char()),
                                (':', ':')
                                    | ('-', '>')
                                    | ('=', '>')
                                    | ('=', '=')
                                    | ('!', '=')
                                    | ('<', '=')
                                    | ('>', '=')
                                    | ('&', '&')
                                    | ('|', '|')
                                    | ('<', '<')
                                    | ('>', '>')
                                    | ('+', '=')
                                    | ('-', '=')
                                    | ('*', '=')
                                    | ('/', '=')
                                    | ('%', '=')
                                    | ('^', '=')
                                    | ('&', '=')
                                    | ('|', '=')
                                    | ('.', '.')
                                    | ('.', '=')
                            ),
                            Some(TokenTree::Ident(_)) => punct.as_char() == '\'',
                            _ => false,
                        };
                    out.push(format!("punct:{}:{joint}", punct.as_char()));
                }
            }
        }
    }
    match source.parse::<proc_macro2::TokenStream>() {
        Ok(stream) => {
            let mut tokens = Vec::new();
            append(stream, &mut tokens);
            tokens
        }
        // A partial fragment that cannot tokenize gets exact-byte comparison;
        // malformed text must never become equal through whitespace erasure.
        Err(_) => vec![format!("unparsed:{source}")],
    }
}

fn source_contains(fixture: &[String], source: &str) -> bool {
    let candidate = normalize_source(source);
    !candidate.is_empty()
        && fixture
            .windows(candidate.len())
            .any(|window| window == candidate)
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

pub(crate) fn relative(root: &Path, path: &Path) -> String {
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
            language: "rust".to_owned(),
            class: if language == "concord-pseudocode" {
                FenceClass::Pseudocode
            } else if language == "concord" {
                FenceClass::CompiledDsl
            } else {
                FenceClass::Code
            },
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
        for retired in [
            "POST create() -> Item using Json;",
            "timeout off;",
            "rate_limit use shared;",
            "by [origin];",
            "paginate with OffsetPages;",
            "\"q\" += endpoint.value;",
        ] {
            assert!(!retired_fence_findings(&fence("concord", retired)).is_empty());
        }
    }

    /// Regression: the v3 window-source `using` (§12.1) is current syntax, while
    /// every v2 body/result `using` representation form stays retired.
    #[test]
    fn window_source_using_is_current_and_representation_using_is_retired() {
        let retired_using = "retired body/result `using` form".to_owned();
        for current in [
            "rate_limit \"concord.riot.21\" key [origin] using standard;",
            "rate_limit \"catalog.write\" key [origin, \"tenant\" = tenant] cost 2 using standard;",
            "policy reads(tenant) {\n    id \"catalog.read\";\n    key [origin];\n    using standard;\n}",
            "rate_limit {\n    id \"catalog.read\";\n    key [origin];\n    using standard; // numbers only\n}",
        ] {
            assert!(
                !retired_fence_findings(&fence("concord", current)).contains(&retired_using),
                "flagged: {current}"
            );
        }
        for retired in [
            "POST create() -> Item using Json;",
            "POST create(body body: Item using Json) -> Item;",
            "GET item()\n    -> Item\n    using Json;",
            "rate_limit \"concord.riot.21\" key [origin] using standard; GET x() -> T using Json;",
        ] {
            assert!(
                retired_fence_findings(&fence("concord-pseudocode", retired))
                    .contains(&retired_using),
                "not flagged: {retired}"
            );
        }
    }

    #[test]
    fn current_dsl_first_lines_are_classified() {
        for current in [
            "api! {",
            "client Api {",
            "scope items() {",
            "GET item() -> Item;",
            "POST item() -> Item;",
            "PUT item() -> Item;",
            "PATCH item() -> Item;",
            "DELETE item() -> Item;",
            "HEAD item() -> Item;",
            "OPTIONS item() -> Item;",
            "METHOD(\"PROPFIND\") item() -> Item;",
            "values {",
            "credentials {",
            "request Json;",
            "response Json;",
            "subdomain replace [\"api\"];",
            "path [\"items\"];",
            "query {",
            "headers {",
            "auth {",
            "timeout 5s;",
            "status success;",
            "effect safe;",
            "paginate via OffsetPages {",
            "rate_limits {",
            "rate_limit shared();",
            "when_present \"q\" = endpoint.q;",
            "append \"q\" = each endpoint.q;",
            "remove \"q\";",
            "replace {",
            "concat [\"item-\", endpoint.id]",
            "expose accepted_bytes;",
        ] {
            assert!(looks_like_dsl(current), "not classified: {current}");
        }
        assert!(!looks_like_dsl("paginate with OffsetPages;"));
    }

    #[test]
    fn retired_language_allowlist_is_explicit_and_path_bounded() {
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

    #[test]
    fn rust_highlighting_keeps_dsl_classification_and_source_evidence() {
        let root = std::env::temp_dir().join(format!(
            "concord-rust-fence-evidence-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture directory");
        let path = root.join("sample.md");
        let compiled = normalize_source("query { \"q\" = term; }");
        for (markdown, expected_error) in [
            (
                "<!-- concord-dsl -->\n```rust\nquery { \"q\" = term; }\n```\n",
                None,
            ),
            (
                "<!-- concord-dsl -->\n```rust\nquery { \"q\" = wrong; }\n```\n",
                Some("normalized DSL block is absent"),
            ),
            (
                "<!-- concord-pseudocode -->\n```rust\nGET illustrative() -> T;\n```\n",
                None,
            ),
            (
                "```rust\nquery { \"q\" = term; }\n```\n",
                Some("needs a concord-dsl or concord-pseudocode comment"),
            ),
            (
                "```rust\nlet value = 7;\n```\n",
                Some("normative Rust fence must be source-anchored"),
            ),
            (
                "<!-- concord-dsl -->\n```concord\nquery { \"q\" = term; }\n```\n",
                Some("must use the `rust` highlighting language"),
            ),
            (
                "<!-- concord-dsl -->\n```text\nquery { \"q\" = term; }\n```\n",
                Some("must use the `rust` highlighting language"),
            ),
        ] {
            fs::write(&path, markdown).expect("fixture Markdown");
            let mut errors = Vec::new();
            check_markdown(
                &root,
                &path,
                &compiled,
                &mut HashMap::new(),
                &mut Counts::default(),
                &mut errors,
            )
            .expect("Markdown check");
            match expected_error {
                None => assert!(errors.is_empty(), "{errors:?}"),
                Some(expected) => assert!(
                    errors.iter().any(|error| error.contains(expected)),
                    "missing {expected:?}: {errors:?}"
                ),
            }
        }
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn dsl_classification_markers_are_scoped_to_one_fence() {
        let markdown = "<!-- concord-source: crates/concord-examples/src/docs_spec.rs#query -->\n<!-- concord-dsl -->\n```rust\nquery {}\n```\n```rust\nordinary();\n```\n";
        let mut errors = Vec::new();
        let fences = parse_fences("sample.md", markdown, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(fences[0].class == FenceClass::CompiledDsl);
        assert!(fences[0].source.is_some());
        assert!(fences[1].class == FenceClass::Code);
        assert!(fences[1].source.is_none());
        for malformed in [
            "<!-- concord-dsl -->\nprose before the fence\n```rust\nquery {}\n```\n",
            "<!-- concord-pseudocode -->\n",
            "<!-- concord-dsl -->\n<!-- concord-pseudocode -->\n```rust\nquery {}\n```\n",
        ] {
            let mut errors = Vec::new();
            parse_fences("sample.md", malformed, &mut errors);
            assert!(!errors.is_empty(), "accepted malformed marker: {malformed}");
        }
    }

    fn anchored(anchor: &str, start: usize) -> Fence {
        Fence {
            language: "rust".to_owned(),
            class: FenceClass::Code,
            start,
            body: String::new(),
            source: Some(SourceReference {
                path: "crates/concord-examples/src/getting_started/minimal.rs".to_owned(),
                anchor: anchor.to_owned(),
                line: start - 1,
            }),
        }
    }

    /// §11.3: v3 keeps `clear_*` only as the local credential control on
    /// `client.credentials()`; every other `.clear_*(` stays retired.
    #[test]
    fn credential_clear_is_current_and_value_clear_is_retired() {
        for current in [
            "client.credentials().clear_session()",
            "client\n    .credentials()\n    .clear_session()?;",
        ] {
            assert!(
                !retired_surface_findings(current)
                    .contains(&"retired `clear_*` control".to_owned()),
                "flagged: {current}"
            );
        }
        for retired in ["request.clear_locale()", "builder\n    .clear_tenant()"] {
            assert!(
                retired_surface_findings(retired).contains(&"retired `clear_*` control".to_owned()),
                "not flagged: {retired}"
            );
        }
    }

    /// §14.2 / §25.2: `.paginate(PageLimit::…)` was retained by v2.1 but is a
    /// retired surface in v3, outside explicit migration context.
    #[test]
    fn v3_surface_retirements_respect_migration_context() {
        let retired = "let prefix = request.paginate(PageLimit::Prefix(2)).await?;";
        let findings = retired_v3_surface_findings(retired, false);
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("`.paginate(...)`"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("`PageLimit`"))
        );
        assert!(
            retired_v3_surface_findings(
                "// v2.1 `.paginate(PageLimit::Prefix(2))` is `.collect_prefix(2)`",
                false
            )
            .is_empty()
        );
        assert!(
            retired_v3_surface_findings("// §25.2: `.paginate(Prefix(n))` becomes", false)
                .is_empty()
        );
        for retired in [
            "let bound = endpoint.try_bind(&client)?;",
            "builder.body_codec::<Codec>(input)",
            // Split so the release-audit placeholder scan does not match this fixture text.
            concat!("let request: PaginatedRequest<Item> = ", "todo", "!();"),
        ] {
            assert!(
                !retired_v3_surface_findings(retired, false).is_empty(),
                "not flagged: {retired}"
            );
        }
        let markdown = "# Guide\n\nUse `request.collect_all(n)`.\n\n## Migrating from v2.1\n\n| `endpoint.try_bind(&owner)` | `owner.bind(endpoint)` |\n\n## Current\n\n`endpoint.try_bind(&owner)`\n";
        let findings = retired_v3_surface_findings(markdown, true);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].starts_with("line 11:"), "{findings:?}");

        assert!(is_migration_heading("Migrating from v2.1"));
        assert!(is_migration_heading(
            "17. Migration from v2.1 (retired forms)"
        ));
        assert!(!is_migration_heading("Migration notes"));
        assert!(!is_migration_heading("Current v3 surface"));
    }

    #[test]
    fn v3_markdown_dsl_spans_and_fence_lines_are_checked() {
        for retired in [
            "Write `path [\"items\", id]` for a route.",
            "Declare `vars { tenant: String; }` first.",
            "Use `host [\"api\"]` for labels.",
            "Bodies use `using Json`.",
            "Initialize with `locale: String = default;`.",
            "Reference `endpoint.id` in the template.",
            "```text\nlet pages = request.paginate(PageLimit::Prefix(2));\n```",
        ] {
            assert!(
                !retired_v3_surface_findings(retired, true).is_empty(),
                "not flagged: {retired}"
            );
        }
        for current in [
            "Call `endpoint.description()` to read the static facts.",
            "Write `path \"items/{id}\";` and `#[default] tags: Vec<String>`.",
            "Rate windows use `using standard;` inside a policy.",
            // An expanded contract view shows display provenance, not syntax.
            "```text\n    category <- endpoint.category, omitted when absent\n```",
        ] {
            assert!(
                retired_v3_surface_findings(current, true).is_empty(),
                "flagged: {current}"
            );
        }
    }

    #[test]
    fn v3_dsl_fences_reject_retired_forms_but_not_string_data() {
        for retired in [
            "GET item(id: String) -> Item {\n    path [\"items\", id];\n}",
            "values { query value: String = default; }",
            "client Api {\n    retry reqwest_default;\n}",
            "GET item(id: String) -> Item {\n    path \"items/{endpoint.id}\";\n}",
            "headers { \"X-Id\" = endpoint.id; }",
            "headers { \"X-Tenant\" = vars.tenant; }",
        ] {
            assert!(
                !retired_v3_dsl_findings(retired).is_empty(),
                "not flagged: {retired}"
            );
        }
        for current in [
            "key [origin, endpoint, \"endpoint.item_id\" = item_id];",
            "credential session = endpoint authenticate;",
            "#[default] query(\"tag\") each tags: Vec<String>;",
            "path replace \"legacy/{client.api_version}\";",
        ] {
            assert!(
                retired_v3_dsl_findings(current).is_empty(),
                "flagged: {current}"
            );
        }
    }

    #[test]
    fn tutorial_must_await_before_internals() {
        let ordered = [anchored("first_api", 3), anchored("first_request", 9), {
            let mut fence = anchored("domain_tree_planning", 20);
            fence.body = "let endpoint = CatalogApi::unbound();".to_owned();
            fence
        }];
        assert!(tutorial_order_errors("docs/quick_start.md", &ordered).is_empty());

        let reversed = [
            anchored("first_api", 3),
            anchored("runtime_endpoint", 9),
            anchored("domain_tree_await_request", 20),
        ];
        let errors = tutorial_order_errors("docs/quick_start.md", &reversed);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("docs/quick_start.md:9"), "{errors:?}");

        let mut marker = fence("text", "request.with_options(&options).await");
        marker.start = 5;
        let errors = tutorial_order_errors(
            "docs/quick_start.md",
            &[marker, anchored("first_request", 12)],
        );
        assert_eq!(errors.len(), 1, "{errors:?}");

        let errors = tutorial_order_errors("docs/quick_start.md", &[anchored("first_api", 3)]);
        assert!(
            errors[0].contains("anchored ordinary `.await`"),
            "{errors:?}"
        );
    }

    #[test]
    fn version_banners_follow_the_workspace_major() {
        assert!(
            version_banner_errors(
                "3.0.0-beta.1",
                "# Concord v3\n\nText",
                "intro\n# Concord v3 documentation"
            )
            .is_empty()
        );
        let errors = version_banner_errors(
            "3.0.0-beta.1",
            "# Concord v2.1\n",
            "## Concord v3 documentation\n",
        );
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("README.md: title `Concord v2.1`"));
        assert!(errors[1].contains("docs/README.md: missing a top-level title"));
        assert!(
            !version_banner_errors("3.0.0-beta.1", "# Concord v30\n", "# Concord v3\n").is_empty()
        );
    }

    #[test]
    fn stale_versions_are_reported_outside_migration_context() {
        assert_eq!(
            stale_version_mentions("3.0.0-beta.1", "This is `2.1.0-beta.1`."),
            ["2.1.0-beta.1"]
        );
        assert!(
            stale_version_mentions(
                "3.0.0-beta.1",
                "Upgrading from Concord v2.1 (`2.1.0-beta.1`) is covered separately."
            )
            .is_empty()
        );
        assert!(stale_version_mentions("2.1.0-beta.1", "This is `2.1.0-beta.1`.").is_empty());
    }

    #[test]
    fn parser_fallbacks_name_every_grammar_clause() {
        let declaration = include_str!("../../../crates/concord-macros/src/syntax/declaration.rs");
        for (message, clauses) in [
            ("unsupported client entry", CLIENT_CLAUSES),
            ("unsupported scope entry", SCOPE_CLAUSES),
            ("unsupported endpoint clause", ENDPOINT_CLAUSES),
        ] {
            assert!(
                fallback_clause_errors(declaration, message, clauses).is_empty(),
                "{message}"
            );
        }
        let stale = "\"unsupported endpoint clause; expected `path`, `query`\"";
        let errors = fallback_clause_errors(stale, "unsupported endpoint clause", ENDPOINT_CLAUSES);
        assert!(errors[0].contains("`absent on`"), "{errors:?}");
        assert!(
            fallback_clause_errors("", "unsupported scope entry", SCOPE_CLAUSES)[0]
                .contains("is missing")
        );
    }

    #[test]
    fn grammar_keywords_agree_with_the_parser_and_the_spec_grammar() {
        let keywords = include_str!("../../../crates/concord-macros/src/syntax/keyword.rs");
        let complete_spec = V3_KEYWORDS
            .iter()
            .map(|keyword| format!("\"{keyword}\""))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(grammar_keyword_errors(keywords, &complete_spec).is_empty());

        let incomplete = complete_spec.replace("\"when_present\"", "") + " \"vars\"";
        let errors = grammar_keyword_errors(keywords, &incomplete);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("\"when_present\""))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("retired v2 keyword \"vars\""))
        );

        let missing_keyword = keywords.replace("syn::custom_keyword!(windows);", "");
        assert!(
            grammar_keyword_errors(&missing_keyword, &complete_spec)
                .iter()
                .any(|error| error.contains("`windows` is not declared"))
        );
    }

    #[test]
    fn reserved_generated_methods_are_parsed_from_the_macro() {
        let names = include_str!("../../../crates/concord-macros/src/semantic/names.rs");
        let request = const_string_list(names, "REQUEST_METHODS");
        for method in [
            "timeout",
            "reset_timeout",
            "cancel_with",
            "with_options",
            "plan",
            "response",
        ] {
            assert!(request.iter().any(|name| name == method), "{method}");
        }
        assert!(const_string_list(names, "CLIENT_METHODS").contains(&"unbound".to_owned()));
        assert!(
            const_string_list(names, "BUILDER_METHODS").contains(&"configure_runtime".to_owned())
        );
        assert!(const_string_list(names, "MISSING_METHODS").is_empty());
        for (path, construction) in GENERATED_NAME_CONSTRUCTIONS {
            let source = match *path {
                "crates/concord-macros/src/semantic/names.rs" => names,
                "crates/concord-macros/src/semantic/auth.rs" => {
                    include_str!("../../../crates/concord-macros/src/semantic/auth.rs")
                }
                other => panic!("unexpected source {other}"),
            };
            assert!(source.contains(construction), "{construction}");
        }
    }

    #[test]
    fn retired_implementation_symbols_distinguish_v3_byte_caps() {
        assert!(retired_implementation_symbols("pub fn cap_response_bytes(self) {}").is_empty());
        assert!(retired_implementation_symbols("summary.acquired_response_bytes()").is_empty());
        assert!(retired_implementation_symbols("self.response_bytes\n").is_empty());
        assert_eq!(
            retired_implementation_symbols("pub fn response_bytes(self) -> X"),
            ["response_bytes()"]
        );
        assert_eq!(
            retired_implementation_symbols("request.response_bytes().await"),
            ["response_bytes()"]
        );
        assert_eq!(
            retired_implementation_symbols("let plan: SafeRequestPlan;"),
            ["SafeRequestPlan"]
        );
    }

    #[test]
    fn feature_matrix_matches_the_manifest() {
        let manifest = "[package]\nname = \"concord\"\n\n[features]\ndefault = [\"tls\", \"http2\"]\ntls = []\nhttp2 = []\njson = [\"x/json\"]\n\n[dependencies]\n";
        let (features, defaults) = manifest_features(manifest);
        assert_eq!(
            features.iter().map(String::as_str).collect::<Vec<_>>(),
            ["http2", "json", "tls"]
        );
        assert_eq!(
            defaults.iter().map(String::as_str).collect::<Vec<_>>(),
            ["http2", "tls"]
        );
        let readme = "# T\n\n## Cargo features\n\n| Feature | Default | Adds |\n|---|---|---|\n| `tls` | yes | TLS |\n| `http2` | yes | h2 |\n| `json` | no | Json |\n\nAfter.\n";
        let rows = feature_table(readme, "Cargo features").expect("table");
        assert!(feature_matrix_errors(&features, &defaults, &rows).is_empty());

        let drifted = readme
            .replace(
                "| `json` | no | Json |",
                "| `json` | yes | Json |\n| `csv` | no | CSV |",
            )
            .replace("| `http2` | yes | h2 |\n", "");
        let rows = feature_table(&drifted, "Cargo features").expect("table");
        let errors = feature_matrix_errors(&features, &defaults, &rows);
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(feature_table("# T\n", "Cargo features").is_none());

        let real = include_str!("../../../crates/concord/Cargo.toml");
        let (features, defaults) = manifest_features(real);
        assert!(features.contains("rate-limit") && features.contains("multipart"));
        assert!(defaults.contains("default-tls") && !defaults.contains("json"));
    }

    #[test]
    fn user_documentation_scope_is_readme_and_docs() {
        assert!(is_user_documentation("README.md"));
        assert!(is_user_documentation("docs/auth.md"));
        assert!(!is_user_documentation("FINAL.md"));
        assert!(!is_user_documentation("dev_doc/internals.md"));
        assert!(!is_user_documentation("docs/diagram.svg"));
    }
}

#[cfg(test)]
mod s5_literal_comparison_regressions {
    use super::*;

    #[test]
    fn literal_bytes_and_token_boundaries_survive_comparison() {
        assert_eq!(
            normalize_source("fn f() { foo ( 1 ); }"),
            normalize_source("fn f(){foo(1);}")
        );
        for (before, after) in [
            (r#"let s = "a b";"#, r#"let s = "ab";"#),
            (r##"let s = r#"a b"#;"##, r##"let s = r#"ab"#;"##),
            ("let c = ' ';", "let c = 'x';"),
            ("let xy = 1;", "let x y = 1;"),
            ("x >= y", "x > = y"),
        ] {
            assert_ne!(normalize_source(before), normalize_source(after));
        }
        assert!(!source_contains(&normalize_source("foobar"), "foo"));
        assert!(source_contains(
            &normalize_source("api! { GET f() -> (); }"),
            "GET f() -> ();"
        ));
        assert_ne!(
            normalize_source("'unterminated a b"),
            normalize_source("'unterminated ab")
        );
    }
}

#[cfg(test)]
mod s5_doc_trust_probes {
    use super::*;
    fn root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "s5-doc-trust-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("new temporary docs root");
        dir.canonicalize().expect("root")
    }
    #[test]
    fn s5_self_fragment_negative_is_checked() {
        let root = root("fragment");
        let path = root.join("probe.md");
        std::fs::write(
            &path,
            "# Existing\n\n[positive](#existing)\n[negative](#missing)\n",
        )
        .expect("probe");
        let mut errors = Vec::new();
        let mut counts = Counts::default();
        check_markdown(
            &root,
            &path,
            &[],
            &mut HashMap::new(),
            &mut counts,
            &mut errors,
        )
        .expect("run checker");
        std::fs::remove_file(&path).expect("remove owned file");
        std::fs::remove_dir(&root).expect("remove owned empty directory");
        println!(
            "S5_SELF_FRAGMENT_ERRORS={errors:?} checked_fragments={}",
            counts.fragments
        );
        assert!(
            errors.iter().any(|e| e.contains("missing")),
            "same-document missing fragments must be rejected"
        );
    }
    #[test]
    fn s5_source_anchor_literal_whitespace_is_not_erased() {
        let root = root("literal");
        let source_dir = root.join("crates/concord-examples/src");
        std::fs::create_dir_all(&source_dir).expect("owned source directories");
        let source = source_dir.join("probe.rs");
        std::fs::write(
            &source,
            "// ANCHOR: example\nfn example() { let _ = \"a b\"; }\n// ANCHOR_END: example\n",
        )
        .expect("source");
        let path = root.join("probe.md");
        let write_fence = |literal: &str| {
            std::fs::write(&path,format!("<!-- concord-source: crates/concord-examples/src/probe.rs#example -->\n```rust\nfn example() {{ let _ = \"{literal}\"; }}\n```\n")).expect("fence")
        };
        let run = || {
            let mut errors = Vec::new();
            check_markdown(
                &root,
                &path,
                &[],
                &mut HashMap::new(),
                &mut Counts::default(),
                &mut errors,
            )
            .expect("run checker");
            errors
        };
        write_fence("a b");
        assert!(run().is_empty(), "exact source anchor positive control");
        write_fence("ab");
        let errors = run();
        std::fs::remove_file(&source).expect("remove owned source");
        std::fs::remove_file(&path).expect("remove owned doc");
        for dir in [
            &source_dir,
            &root.join("crates/concord-examples"),
            &root.join("crates"),
            &root,
        ] {
            std::fs::remove_dir(dir).expect("remove owned empty directory");
        }
        println!("S5_LITERAL_ANCHOR_ERRORS={errors:?}");
        assert!(
            errors.iter().any(|e| e.contains("differs")),
            "changing a string literal in an anchored fence must fail"
        );
    }
}

#[cfg(test)]
mod s5_docs_token_neighbors {
    use super::*;
    #[test]
    fn token_trivia_and_literal_controls() {
        assert_eq!(
            normalize_source("foo /* harmless comment */ (1)"),
            normalize_source("foo(1)")
        );
        assert_eq!(
            normalize_source("fn f<'a>(s: &'a str) {}"),
            normalize_source("fn f < 'a > (s: & 'a str){}")
        );
        for (a, b) in [
            (r#"b"a b""#, r#"b"ab""#),
            (r##"br#"a b"#"##, r##"br#"ab"#"##),
            ("' '", "'x'"),
            ("foo bar", "foobar"),
            ("/* unterminated a b", "/* unterminated ab"),
        ] {
            assert_ne!(normalize_source(a), normalize_source(b));
        }
    }
}
