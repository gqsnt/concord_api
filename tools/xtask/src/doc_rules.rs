//! Normative documentation rules checked from source by `docs-check`.
//!
//! * EXT-01 (§17.1): every public trait of `concord-core` and
//!   `concord-extras` documents ownership, concurrency, cancellation, input
//!   sensitivity, error behavior, resource obligations and negative
//!   capabilities in one consistent `# Extension contract` section.
//! * IDENTITY-01 (§12.4): public rustdoc of the product crates (and the text
//!   the macro emits into generated rustdoc) never calls an endpoint ID
//!   "stable": an operational endpoint ID is an explicit `id` pin or
//!   navigation-derived, and a derived ID changes on renames and moves.
//! * LIMIT-02 (§20.3): `docs/runtime.md` keeps the "What limits do not bound"
//!   statements, and no plugin sandbox, per-object resource tracker or hidden
//!   task scheduler module appears in the product crates.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use syn::visit::Visit;

/// Crates whose public traits must carry the EXT-01 contract section.
const EXTENSION_TRAIT_ROOTS: &[&str] = &["crates/concord-core/src", "crates/concord-extras/src"];

/// Heading of the EXT-01 section.
pub(crate) const CONTRACT_HEADING: &str = "# Extension contract";

/// The EXT-01 topics, as bold bullet labels in this exact order.
pub(crate) const CONTRACT_LABELS: &[&str] = &[
    "Ownership",
    "Concurrency",
    "Cancellation",
    "Input sensitivity",
    "Errors",
    "Resource obligations",
    "Negative capabilities",
];

/// Product crates whose rustdoc comments are scanned for IDENTITY-01.
const RUSTDOC_ROOTS: &[&str] = &[
    "crates/concord-core/src",
    "crates/concord-extras/src",
    "crates/concord/src",
    "crates/concord-macros/src",
];

/// The macro crate: its string literals become generated rustdoc.
const GENERATED_DOC_ROOT: &str = "crates/concord-macros/src";

/// The LIMIT-02 section and the statements it must keep (whitespace is
/// normalized before matching).
const LIMITS_DOC: &str = "docs/runtime.md";
const LIMITS_HEADING: &str = "What limits do not bound";
const LIMITS_STATEMENTS: &[&str] = &[
    "do not bound memory already owned by a caller's values",
    "allocation performed by a trusted decoder",
    "atom limits apply to encoded output",
    "Deadlines are checked around synchronous callbacks but cannot preempt them",
    "Native TCP/TLS stacks keep their own bounded buffers",
    "no plugin sandbox, per-object resource tracker or hidden task scheduler",
];
/// Product crates that must not grow the machinery LIMIT-02 rules out.
const PRODUCT_SOURCE_ROOTS: &[&str] = &[
    "crates/concord-core/src",
    "crates/concord-extras/src",
    "crates/concord/src",
    "crates/concord-macros/src",
];
const FORBIDDEN_MODULE_WORDS: &[&str] = &["sandbox", "scheduler", "resource_tracker"];

/// Runs every rule and appends one message per violation.
pub(crate) fn check(root: &Path, errors: &mut Vec<String>) -> Result<Counts, String> {
    let mut counts = Counts::default();
    for directory in EXTENSION_TRAIT_ROOTS {
        for path in crate::docs_check::collect_files(&root.join(directory), "rs")? {
            let source = read(&path)?;
            let relative = crate::docs_check::relative(root, &path);
            let traits = public_traits(&source)
                .map_err(|error| format!("could not parse {relative}: {error}"))?;
            for (name, docs) in traits {
                counts.traits += 1;
                for problem in contract_problems(&docs) {
                    errors.push(format!(
                        "{relative}: public trait `{name}` {problem} (EXT-01: use `{CONTRACT_HEADING}` with the bullets {})",
                        CONTRACT_LABELS
                            .iter()
                            .map(|label| format!("`* **{label}.**`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
    }
    for directory in RUSTDOC_ROOTS {
        for path in crate::docs_check::collect_files(&root.join(directory), "rs")? {
            let source = read(&path)?;
            let relative = crate::docs_check::relative(root, &path);
            let mut texts = doc_blocks(&source);
            if directory == &GENERATED_DOC_ROOT {
                texts.extend(
                    string_literals(&source)
                        .map_err(|error| format!("could not parse {relative}: {error}"))?,
                );
            }
            counts.identity_files += 1;
            for (line, text) in texts {
                for finding in stable_identity_findings(&text) {
                    errors.push(format!(
                        "{relative}:{line}: rustdoc calls an endpoint ID stable (`{finding}`); say \"operational endpoint ID (an explicit `id` pin or navigation-derived)\" (IDENTITY-01)"
                    ));
                }
            }
        }
    }
    let runtime = read(&root.join(LIMITS_DOC))?;
    for problem in limits_section_problems(&runtime) {
        errors.push(format!("{LIMITS_DOC}: {problem} (LIMIT-02)"));
    }
    for directory in PRODUCT_SOURCE_ROOTS {
        for path in crate::docs_check::collect_files(&root.join(directory), "rs")? {
            let relative = crate::docs_check::relative(root, &path);
            let source = read(&path)?;
            for word in forbidden_machinery(&relative, &source) {
                errors.push(format!(
                    "{relative}: `{word}` machinery is ruled out; extensions are trusted code with documented obligations (LIMIT-02)"
                ));
            }
        }
    }
    Ok(counts)
}

/// How much the rules examined, for the `docs-check` summary.
#[derive(Debug, Default)]
pub(crate) struct Counts {
    pub(crate) traits: usize,
    pub(crate) identity_files: usize,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))
}

// ----- EXT-01 -------------------------------------------------------------------

/// `(name, rustdoc text)` of every public, documented-API trait in a file:
/// `pub` traits at file level or inside `pub` inline modules, excluding
/// `#[doc(hidden)]` generated-code bridges and traits inside private inline
/// modules (sealing supertraits). Every file is a candidate because private
/// file modules re-export their public traits.
pub(crate) fn public_traits(source: &str) -> syn::Result<Vec<(String, String)>> {
    let file = syn::parse_file(source)?;
    let mut traits = Vec::new();
    collect_traits(&file.items, &mut traits);
    Ok(traits)
}

fn collect_traits(items: &[syn::Item], traits: &mut Vec<(String, String)>) {
    for item in items {
        match item {
            syn::Item::Trait(item)
                if matches!(item.vis, syn::Visibility::Public(_)) && !doc_hidden(&item.attrs) =>
            {
                traits.push((item.ident.to_string(), doc_text(&item.attrs)));
            }
            syn::Item::Mod(item)
                if matches!(item.vis, syn::Visibility::Public(_)) && !doc_hidden(&item.attrs) =>
            {
                if let Some((_, items)) = &item.content {
                    collect_traits(items, traits);
                }
            }
            _ => {}
        }
    }
}

fn doc_hidden(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("doc")
            && matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string().contains("hidden"))
    })
}

/// The rustdoc text of `///` lines / `#[doc = "..."]` attributes, one line
/// each, with the single conventional leading space removed.
fn doc_text(attributes: &[syn::Attribute]) -> String {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("doc"))
        .filter_map(|attribute| match &attribute.meta {
            syn::Meta::NameValue(syn::MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(text),
                        ..
                    }),
                ..
            }) => Some(text.value()),
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').map(str::to_owned).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What an EXT-01 section lacks; empty when the rustdoc is complete.
pub(crate) fn contract_problems(docs: &str) -> Vec<String> {
    let lines = docs.lines().collect::<Vec<_>>();
    let Some(start) = lines
        .iter()
        .position(|line| line.trim() == CONTRACT_HEADING)
    else {
        return vec![format!("rustdoc lacks the `{CONTRACT_HEADING}` section")];
    };
    // The section runs to the next level-1 heading.
    let section = lines[start + 1..]
        .iter()
        .take_while(|line| !line.starts_with("# "))
        .copied()
        .collect::<Vec<_>>();
    // Bullets with their continuation text.
    let mut bullets: Vec<(String, String)> = Vec::new();
    for line in &section {
        if let Some(rest) = line.strip_prefix("* **") {
            let (label, text) = match rest.split_once(".**") {
                Some((label, text)) => (label.to_owned(), text.trim().to_owned()),
                None => (rest.to_owned(), String::new()),
            };
            bullets.push((label, text));
        } else if let Some((_, text)) = bullets.last_mut() {
            let continuation = line.trim();
            if !continuation.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(continuation);
            }
        }
    }
    let labels = bullets
        .iter()
        .map(|(label, _)| label.as_str())
        .collect::<Vec<_>>();
    let mut problems = Vec::new();
    for label in CONTRACT_LABELS {
        if !labels.contains(label) {
            problems.push(format!(
                "rustdoc section `{CONTRACT_HEADING}` lacks `**{label}.**`"
            ));
        }
    }
    if problems.is_empty() && labels != CONTRACT_LABELS {
        problems.push(format!(
            "rustdoc section `{CONTRACT_HEADING}` must list exactly the seven topics in order; found {labels:?}"
        ));
    }
    for (label, text) in &bullets {
        if CONTRACT_LABELS.contains(&label.as_str()) && text.is_empty() {
            problems.push(format!(
                "rustdoc section `{CONTRACT_HEADING}` leaves `**{label}.**` empty"
            ));
        }
    }
    problems
}

// ----- IDENTITY-01 --------------------------------------------------------------

/// `(first line, joined text)` of every contiguous `///` or `//!` comment
/// block, whitespace-normalized so sentences spanning lines are matched.
pub(crate) fn doc_blocks(source: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, Vec<String>)> = None;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let text = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"));
        match (text, current.as_mut()) {
            (Some(text), Some((_, lines))) => lines.push(text.trim().to_owned()),
            (Some(text), None) => current = Some((index + 1, vec![text.trim().to_owned()])),
            (None, _) => {
                if let Some((start, lines)) = current.take() {
                    blocks.push((start, lines.join(" ")));
                }
            }
        }
    }
    if let Some((start, lines)) = current {
        blocks.push((start, lines.join(" ")));
    }
    blocks
}

/// `(line, value)` of every string literal outside `#[cfg(test)]` items.
fn string_literals(source: &str) -> syn::Result<Vec<(usize, String)>> {
    struct Literals(Vec<(usize, String)>);
    impl<'ast> Visit<'ast> for Literals {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if !cfg_test(&item.attrs) {
                syn::visit::visit_item_mod(self, item);
            }
        }
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            if !cfg_test(&item.attrs) {
                syn::visit::visit_item_fn(self, item);
            }
        }
        fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
            self.0.push((literal.span().start().line, literal.value()));
        }
    }
    let file = syn::parse_file(source)?;
    let mut literals = Literals(Vec::new());
    literals.visit_file(&file);
    Ok(literals.0)
}

fn cfg_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string() == "test")
    })
}

/// Phrases that present an endpoint ID as stable.
static STABLE_IDENTITY: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [
        // "stable [generated|operational|public ...] endpoint identity/ID"
        r"(?i)\bstable\b(?:[ ,]+[\w`'-]+){0,4}?[ ,]+(?:endpoint|operational|contract)[ -]+(?:identity|identities|ids?|identifiers?)\b",
        // "endpoint ID is/remains stable"
        r"(?i)\b(?:endpoint|operational)[ -]+(?:identity|identities|ids?)\b(?:[ ,]+[\w`'-]+){0,3}?[ ,]+(?:is|are|remains?|stays?)[ ]+stable\b",
    ]
    .map(|pattern| Regex::new(pattern).expect("stable identity regex"))
});

/// Every phrase of `text` that presents an endpoint ID as stable.
pub(crate) fn stable_identity_findings(text: &str) -> Vec<String> {
    if !text.to_ascii_lowercase().contains("stable") {
        return Vec::new();
    }
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    STABLE_IDENTITY
        .iter()
        .flat_map(|pattern| {
            pattern
                .find_iter(&normalized)
                .map(|found| found.as_str().to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}

// ----- LIMIT-02 -----------------------------------------------------------------

/// What the "What limits do not bound" section lacks.
pub(crate) fn limits_section_problems(markdown: &str) -> Vec<String> {
    let mut lines = markdown.lines();
    if lines
        .by_ref()
        .find(|line| line.trim_start_matches('#').trim() == LIMITS_HEADING && line.starts_with('#'))
        .is_none()
    {
        return vec![format!("missing the `{LIMITS_HEADING}` section")];
    }
    let section = lines
        .take_while(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let section = section.split_whitespace().collect::<Vec<_>>().join(" ");
    LIMITS_STATEMENTS
        .iter()
        .filter(|statement| !section.contains(*statement))
        .map(|statement| format!("section `{LIMITS_HEADING}` no longer states `{statement}`"))
        .collect()
}

/// A `mod NAME` declaration (inline or file module).
static MODULE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)").expect("module regex")
});

/// Forbidden machinery named by a product source file path or module.
pub(crate) fn forbidden_machinery(relative: &str, source: &str) -> Vec<&'static str> {
    let mut names = MODULE
        .captures_iter(source)
        .map(|captures| captures[1].to_ascii_lowercase())
        .collect::<Vec<_>>();
    if let Some(stem) = Path::new(relative)
        .file_stem()
        .and_then(|stem| stem.to_str())
    {
        names.push(stem.to_ascii_lowercase());
    }
    FORBIDDEN_MODULE_WORDS
        .iter()
        .copied()
        .filter(|word| names.iter().any(|name| name.contains(word)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CONTRACT_LABELS, contract_problems, doc_blocks, forbidden_machinery,
        limits_section_problems, public_traits, stable_identity_findings,
    };

    const COMPLETE: &str = "/// Summary.
///
/// # Extension contract
///
/// * **Ownership.** Borrowed.
/// * **Concurrency.** Shared
///   across threads.
/// * **Cancellation.** Synchronous.
/// * **Input sensitivity.** Sensitive.
/// * **Errors.** Value-free.
/// * **Resource obligations.** Bounded.
/// * **Negative capabilities.** None granted.
pub trait Complete {}
";

    fn docs_of(source: &str) -> Vec<(String, String)> {
        public_traits(source).expect("parse")
    }

    #[test]
    fn a_complete_contract_section_has_no_problems() {
        let traits = docs_of(COMPLETE);
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].0, "Complete");
        assert!(
            contract_problems(&traits[0].1).is_empty(),
            "{:?}",
            contract_problems(&traits[0].1)
        );
    }

    #[test]
    fn missing_empty_and_misordered_topics_are_reported() {
        let missing_heading = COMPLETE.replace("# Extension contract", "# Contract");
        let problems = contract_problems(&docs_of(&missing_heading)[0].1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("lacks the `# Extension contract` section"));

        for label in CONTRACT_LABELS {
            let removed = COMPLETE
                .lines()
                .filter(|line| !line.contains(&format!("**{label}.**")))
                .collect::<Vec<_>>()
                .join("\n");
            let problems = contract_problems(&docs_of(&removed)[0].1);
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.contains(&format!("lacks `**{label}.**`"))),
                "{label}: {problems:?}"
            );
        }

        let empty = COMPLETE.replace("* **Errors.** Value-free.", "* **Errors.**");
        let problems = contract_problems(&docs_of(&empty)[0].1);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("leaves `**Errors.**` empty"));

        let swapped = COMPLETE
            .replace("**Ownership.** Borrowed.", "**TEMP.** Borrowed.")
            .replace("**Concurrency.** Shared", "**Ownership.** Shared")
            .replace("**TEMP.** Borrowed.", "**Concurrency.** Borrowed.");
        let problems = contract_problems(&docs_of(&swapped)[0].1);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("exactly the seven topics in order"));

        // A later level-1 heading ends the section.
        let split = COMPLETE.replace(
            "/// * **Errors.** Value-free.",
            "///\n/// # Examples\n///\n/// * **Errors.** Value-free.",
        );
        let problems = contract_problems(&docs_of(&split)[0].1);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("lacks `**Errors.**`"))
        );
    }

    #[test]
    fn hidden_private_module_and_restricted_traits_are_not_public_api() {
        let source = "
/// Undocumented.
pub trait Bare {}
#[doc(hidden)]
pub trait Hidden {}
pub(crate) trait Crate {}
trait Private {}
mod private { pub trait Sealed {} }
pub(crate) mod restricted { pub trait Sealed {} }
pub mod open { /// Open.
pub trait Nested {} }
";
        let names = docs_of(source)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Bare", "Nested"]);
        assert!(!contract_problems(&docs_of(source)[0].1).is_empty());
    }

    #[test]
    fn stable_identity_wording_is_rejected_and_precise_wording_is_not() {
        for bad in [
            "Validated stable endpoint identity.",
            "Stable generated endpoint identity.",
            "Returns the typed stable operational endpoint identity.",
            "Stable generated or runtime endpoint identity.",
            "Returns the stable public contract identity.",
            "Returns the stable endpoint ID.",
            "Returns stable endpoint identity and inherited timeout metadata.",
            "The derived endpoint ID is stable across refactors.",
            "an endpoint\n  identity remains stable",
        ] {
            assert!(!stable_identity_findings(bad).is_empty(), "{bad}");
        }
        for good in [
            "Returns the operational endpoint ID (an explicit `id` pin or navigation-derived).",
            "A derived ID changes when a scope is renamed; it is not preserved across such refactors.",
            "Returns the stable lowercase label used by structural exports.",
            "A stable, explicit logical identifier for one rate-limit policy.",
            "Stable encoding contract name recorded in the policy schema.",
            "the controls and the page-stable endpoint template",
        ] {
            assert!(stable_identity_findings(good).is_empty(), "{good}");
        }
    }

    #[test]
    fn doc_blocks_join_lines_and_keep_the_first_line_number() {
        let source = "fn a() {}\n/// Returns the stable\n/// endpoint ID.\nfn b() {}\n//! Inner.\n";
        let blocks = doc_blocks(source);
        assert_eq!(
            blocks,
            [
                (2, "Returns the stable endpoint ID.".to_owned()),
                (5, "Inner.".to_owned())
            ]
        );
        assert_eq!(stable_identity_findings(&blocks[0].1).len(), 1);
    }

    #[test]
    fn limits_section_statements_are_required() {
        let complete = "# Runtime\n\n## What limits do not bound\n\nWire and resource ceilings do not bound memory already owned by a caller's\nvalues or allocation performed by a trusted decoder: atom limits apply to\nencoded output, not to arbitrary object graphs. Deadlines are checked around\nsynchronous callbacks but cannot preempt them. Native TCP/TLS stacks keep their\nown bounded buffers. Concord provides no plugin sandbox, per-object resource\ntracker or hidden task scheduler.\n\n## Features\n\nText.\n";
        assert!(
            limits_section_problems(complete).is_empty(),
            "{:?}",
            limits_section_problems(complete)
        );
        let weakened = complete.replace("but cannot preempt them", "and preempt them");
        let problems = limits_section_problems(&weakened);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("cannot preempt them"));
        // Statements after the next heading do not count.
        let moved = complete
            .replace(
                "## Features",
                "## Features\n\nNative TCP/TLS stacks keep their own bounded buffers.",
            )
            .replace(
                "Native TCP/TLS stacks keep their\nown bounded buffers. ",
                "",
            );
        assert_eq!(limits_section_problems(&moved).len(), 1);
        assert_eq!(
            limits_section_problems("# Runtime\n"),
            ["missing the `What limits do not bound` section"]
        );
    }

    #[test]
    fn sandbox_tracker_and_scheduler_modules_are_ruled_out() {
        assert!(
            forbidden_machinery(
                "crates/concord-core/src/lib.rs",
                "mod execute;\npub mod rate_limit;"
            )
            .is_empty()
        );
        assert_eq!(
            forbidden_machinery(
                "crates/concord-core/src/lib.rs",
                "pub(crate) mod plugin_sandbox;"
            ),
            ["sandbox"]
        );
        assert_eq!(
            forbidden_machinery("crates/concord-core/src/task_scheduler.rs", ""),
            ["scheduler"]
        );
        assert_eq!(
            forbidden_machinery("crates/concord-core/src/lib.rs", "mod resource_tracker {}"),
            ["resource_tracker"]
        );
    }

    /// The real repository satisfies every rule (the same check `docs-check`
    /// runs), so `cargo test -p concord-xtask` catches a regression.
    #[test]
    fn repository_sources_satisfy_ext01_identity01_and_limit02() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let mut errors = Vec::new();
        let counts = super::check(&root, &mut errors).expect("doc rules");
        assert!(errors.is_empty(), "{}", errors.join("\n"));
        assert!(
            counts.traits >= 29,
            "public traits examined: {}",
            counts.traits
        );
        assert!(counts.identity_files > 0);
    }
}
