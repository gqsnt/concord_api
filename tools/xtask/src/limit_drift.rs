//! LIMIT-01 (§20.1): the macro's copies of core limits cannot drift.
//!
//! `concord-macros` is a proc-macro crate and cannot depend on
//! `concord-core`, so it keeps copies of a few core limits for compile-time
//! diagnostics and generated docs: named constants (`MAX_AUTH_PLACEMENTS`,
//! `MAX_RATE_LIMIT_*`, head components/header count), inline numbers in small
//! validators (endpoint ID 128/63, static header/query name 256, method token
//! 128) and generated text ("30 s" default timeout). This check reads both
//! sides with `syn` and fails when any copy differs from the one core owner.
//! Core values are also compared with the compiled public constants where
//! they are exported, so the source reader cannot silently misread them.
//! `policy` runs it, and a unit test runs it against the repository.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use syn::visit::Visit;

const CORE_SRC: &str = "crates/concord-core/src";
const MACRO_SRC: &str = "crates/concord-macros/src";

/// Named macro copies that must exist and equal the core constant of the
/// same name.
const NAMED_COPIES: &[&str] = &[
    "MAX_AUTH_PLACEMENTS",
    "MAX_AUTH_PLACEMENT_NAME_BYTES",
    "MAX_AUTHORIZATION_SCHEME_BYTES",
    "MAX_RATE_LIMIT_ID_BYTES",
    "MAX_RATE_LIMIT_KEY_PART_NAME_BYTES",
    "MAX_RATE_LIMIT_KEY_PARTS",
    "MAX_RATE_LIMIT_WINDOWS_PER_POLICY",
    "MAX_RATE_LIMIT_POLICIES_PER_PLAN",
    "MAX_REQUEST_HEAD_COMPONENTS",
    "MAX_REQUEST_HEADER_COUNT",
];

/// One macro function that repeats core limits inline.
struct Site {
    /// Macro source file, relative to the repository root.
    file: &'static str,
    /// The function holding the copy.
    function: &'static str,
    /// Core constants whose values the non-trivial integer literals of the
    /// function's code must equal, in source order (empty: code unchecked).
    code: &'static [&'static str],
    /// Core constants that every number in the function's string literals
    /// must belong to (`1`, a range start, is always allowed).
    text: &'static [&'static str],
    /// Whether at least one textual number must be present.
    text_required: bool,
}

const SITES: &[Site] = &[
    Site {
        file: "crates/concord-macros/src/syntax/diagnostics.rs",
        function: "is_valid_endpoint_id",
        code: &["MAX_ENDPOINT_ID_BYTES", "MAX_ENDPOINT_ID_SEGMENT_BYTES"],
        text: &[],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/syntax/diagnostics.rs",
        function: "validate_endpoint_id",
        code: &[],
        text: &["MAX_ENDPOINT_ID_BYTES", "MAX_ENDPOINT_ID_SEGMENT_BYTES"],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/semantic/names.rs",
        function: "resolve_identities",
        code: &[],
        text: &["MAX_ENDPOINT_ID_BYTES", "MAX_ENDPOINT_ID_SEGMENT_BYTES"],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/syntax/diagnostics.rs",
        function: "validate_public_header_name",
        code: &["MAX_STATIC_KEY_BYTES"],
        text: &["MAX_STATIC_KEY_BYTES"],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/syntax/diagnostics.rs",
        function: "validate_query_key",
        code: &["MAX_STATIC_KEY_BYTES"],
        text: &["MAX_STATIC_KEY_BYTES"],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/syntax/declaration.rs",
        function: "parse_method",
        code: &["MAX_RUNTIME_ENDPOINT_METHOD_BYTES"],
        text: &["MAX_RUNTIME_ENDPOINT_METHOD_BYTES"],
        text_required: false,
    },
    Site {
        file: "crates/concord-macros/src/emit/docs.rs",
        function: "timeout_text",
        code: &[],
        text: &["DEFAULT_OPERATION_TIMEOUT_SECONDS"],
        text_required: true,
    },
];

/// Diagnostic texts outside small functions: `(file, pattern, constants)`.
/// Each capture group must equal the constant at the same position whenever
/// the text is present.
const MESSAGES: &[(&str, &str, &[&str])] = &[(
    "crates/concord-macros/src/syntax/rate_limits.rs",
    r"policy ID must be 1\.\.=(\d+)",
    &["MAX_RATE_LIMIT_ID_BYTES"],
)];

/// Core constants that are exported, with their compiled values.
fn compiled_core() -> BTreeMap<&'static str, u64> {
    use concord_core::contracts as core;
    let value = |number: usize| u64::try_from(number).expect("limit fits u64");
    BTreeMap::from([
        ("MAX_AUTH_PLACEMENTS", value(core::MAX_AUTH_PLACEMENTS)),
        (
            "MAX_AUTH_PLACEMENT_NAME_BYTES",
            value(core::MAX_AUTH_PLACEMENT_NAME_BYTES),
        ),
        (
            "MAX_AUTHORIZATION_SCHEME_BYTES",
            value(core::MAX_AUTHORIZATION_SCHEME_BYTES),
        ),
        (
            "MAX_RATE_LIMIT_ID_BYTES",
            value(core::MAX_RATE_LIMIT_ID_BYTES),
        ),
        (
            "MAX_RATE_LIMIT_KEY_PART_NAME_BYTES",
            value(core::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES),
        ),
        (
            "MAX_RATE_LIMIT_KEY_PARTS",
            value(core::MAX_RATE_LIMIT_KEY_PARTS),
        ),
        (
            "MAX_RATE_LIMIT_WINDOWS_PER_POLICY",
            value(core::MAX_RATE_LIMIT_WINDOWS_PER_POLICY),
        ),
        (
            "MAX_RATE_LIMIT_POLICIES_PER_PLAN",
            value(core::MAX_RATE_LIMIT_POLICIES_PER_PLAN),
        ),
        (
            "MAX_REQUEST_HEAD_COMPONENTS",
            value(core::MAX_REQUEST_HEAD_COMPONENTS),
        ),
        (
            "MAX_REQUEST_HEADER_COUNT",
            value(core::MAX_REQUEST_HEADER_COUNT),
        ),
        ("MAX_ENDPOINT_ID_BYTES", value(core::MAX_ENDPOINT_ID_BYTES)),
        (
            "MAX_RUNTIME_ENDPOINT_METHOD_BYTES",
            value(core::MAX_RUNTIME_ENDPOINT_METHOD_BYTES),
        ),
    ])
}

/// Runs the check for `policy`.
pub(crate) fn check(root: &Path) -> Result<usize, String> {
    let (findings, compared) = findings(root)?;
    if findings.is_empty() {
        Ok(compared)
    } else {
        Err(format!(
            "concord-macros copies of core limits drifted (LIMIT-01):\n{}",
            findings.join("\n")
        ))
    }
}

/// Every drift finding and the number of copies compared.
pub(crate) fn findings(root: &Path) -> Result<(Vec<String>, usize), String> {
    let core = source_constants(&root.join(CORE_SRC))?;
    let macros = source_constants(&root.join(MACRO_SRC))?;
    let mut findings = Vec::new();
    let mut compared = 0;

    for (name, compiled) in compiled_core() {
        match core.get(name) {
            Some(parsed) if *parsed == compiled => {}
            Some(parsed) => findings.push(format!(
                "- core source constant {name} reads {parsed} but the compiled value is {compiled}"
            )),
            None => findings.push(format!(
                "- exported core constant {name} was not found in {CORE_SRC}"
            )),
        }
    }

    let (named, named_compared) = named_copy_findings(&core, &macros);
    findings.extend(named);
    compared += named_compared;

    for site in SITES {
        let source = read(&root.join(site.file))?;
        let Some(numbers) = function_numbers(&source, site.function)
            .map_err(|error| format!("could not parse {}: {error}", site.file))?
        else {
            findings.push(format!(
                "- {}: function `{}` holding a limit copy was not found (update SITES)",
                site.file, site.function
            ));
            continue;
        };
        findings.extend(site_findings(site, &numbers, &core));
        compared += 1;
    }

    for (file, pattern, names) in MESSAGES {
        let source = read(&root.join(file))?;
        let pattern = Regex::new(pattern).expect("message pattern");
        for captures in pattern.captures_iter(&source) {
            compared += 1;
            for (index, name) in names.iter().enumerate() {
                let written = captures
                    .get(index + 1)
                    .and_then(|found| found.as_str().parse::<u64>().ok());
                let expected = core.get(*name).copied();
                if written != expected {
                    findings.push(format!(
                        "- {file}: `{}` states {written:?} but core {name} is {expected:?}",
                        &captures[0]
                    ));
                }
            }
        }
    }
    Ok((findings, compared))
}

/// Named macro constants that share a core constant's name must equal it,
/// and every expected copy must still exist.
fn named_copy_findings(
    core: &BTreeMap<String, u64>,
    macros: &BTreeMap<String, u64>,
) -> (Vec<String>, usize) {
    let mut findings = Vec::new();
    let mut compared = 0;
    for name in NAMED_COPIES {
        if !macros.contains_key(*name) {
            findings.push(format!(
                "- expected macro copy {name} is missing from {MACRO_SRC} (update NAMED_COPIES if it was removed deliberately)"
            ));
        }
    }
    for (name, value) in macros {
        let Some(core_value) = core.get(name.as_str()) else {
            continue;
        };
        compared += 1;
        if value != core_value {
            findings.push(format!(
                "- macro constant {name} = {value} differs from the core owner ({core_value})"
            ));
        }
    }
    (findings, compared)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// Integer literals of one function: code literals in source order and the
/// numbers written inside its string literals.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Numbers {
    pub(crate) code: Vec<u64>,
    pub(crate) text: Vec<u64>,
}

fn site_findings(site: &Site, numbers: &Numbers, core: &BTreeMap<String, u64>) -> Vec<String> {
    let mut findings = Vec::new();
    let value = |name: &str| core.get(name).copied();
    if !site.code.is_empty() {
        let expected = site.code.iter().map(|name| value(name)).collect::<Vec<_>>();
        let written = numbers
            .code
            .iter()
            .copied()
            .filter(|number| *number > 1)
            .map(Some)
            .collect::<Vec<_>>();
        if written != expected {
            findings.push(format!(
                "- {}: `{}` compares against {:?} but the core owners {:?} are {:?}",
                site.file, site.function, numbers.code, site.code, expected
            ));
        }
    }
    let allowed = site
        .text
        .iter()
        .filter_map(|name| value(name))
        .collect::<Vec<_>>();
    let mut message = String::new();
    for number in numbers.text.iter().filter(|number| **number != 1) {
        if !allowed.contains(number) {
            let _ = write!(message, "{number} ");
        }
    }
    if !message.is_empty() {
        findings.push(format!(
            "- {}: `{}` text states {}but the core owners {:?} are {allowed:?}",
            site.file, site.function, message, site.text
        ));
    }
    if site.text_required && numbers.text.iter().all(|number| !allowed.contains(number)) {
        findings.push(format!(
            "- {}: `{}` no longer states the core value of {:?}",
            site.file, site.function, site.text
        ));
    }
    findings
}

/// `name -> value` of every integer `const` item in a source tree, outside
/// `#[cfg(test)]` modules and test-only files (any path component naming
/// `test`). Values are integer literals or products of them.
pub(crate) fn source_constants(directory: &Path) -> Result<BTreeMap<String, u64>, String> {
    let mut constants = BTreeMap::new();
    for path in crate::docs_check::collect_files(directory, "rs")? {
        let test_only = path.strip_prefix(directory).is_ok_and(|relative| {
            relative
                .components()
                .any(|component| component.as_os_str().to_string_lossy().contains("test"))
        });
        if test_only {
            continue;
        }
        let source = read(&path)?;
        let file = syn::parse_file(&source)
            .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
        let mut visitor = Constants(&mut constants);
        visitor.visit_file(&file);
    }
    Ok(constants)
}

struct Constants<'a>(&'a mut BTreeMap<String, u64>);

impl<'ast> Visit<'ast> for Constants<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !cfg_test(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, _item: &'ast syn::ItemFn) {
        // Function-local constants are not limit owners.
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        if let Some(value) = integer(&item.expr) {
            self.0.entry(item.ident.to_string()).or_insert(value);
        }
    }
}

fn cfg_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string() == "test")
    })
}

fn integer(expression: &syn::Expr) -> Option<u64> {
    match expression {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(literal),
            ..
        }) => literal.base10_parse().ok(),
        syn::Expr::Paren(inner) => integer(&inner.expr),
        syn::Expr::Group(inner) => integer(&inner.expr),
        syn::Expr::Binary(binary) if matches!(binary.op, syn::BinOp::Mul(_)) => {
            integer(&binary.left)?.checked_mul(integer(&binary.right)?)
        }
        _ => None,
    }
}

/// The numbers of the named top-level function, or `None` when absent.
pub(crate) fn function_numbers(source: &str, function: &str) -> syn::Result<Option<Numbers>> {
    let file = syn::parse_file(source)?;
    let Some(item) = file.items.iter().find_map(|item| match item {
        syn::Item::Fn(item) if item.sig.ident == function => Some(item),
        _ => None,
    }) else {
        return Ok(None);
    };
    let mut numbers = Numbers::default();
    let mut visitor = Literals(&mut numbers);
    visitor.visit_block(&item.block);
    Ok(Some(numbers))
}

/// Decimal digit runs in text.
static TEXT_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+").expect("text number regex"));

/// Standalone decimal numbers written in text ("1-128 ASCII bytes",
/// "30 s"), excluding digits that belong to a word such as `UTF-8`, `u32`
/// or `h2`.
pub(crate) fn text_numbers(text: &str) -> Vec<u64> {
    let bytes = text.as_bytes();
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    TEXT_NUMBER
        .find_iter(text)
        .filter(|found| {
            let before = found.start().checked_sub(1).map(|index| bytes[index]);
            let after = bytes.get(found.end()).copied();
            let attached_before = match before {
                Some(byte) if word(byte) => true,
                // `UTF-8`: a hyphen joining a preceding word.
                Some(b'-') => found
                    .start()
                    .checked_sub(2)
                    .is_some_and(|index| bytes[index].is_ascii_alphabetic()),
                _ => false,
            };
            let attached_after =
                after.is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
            !attached_before && !attached_after
        })
        .filter_map(|found| found.as_str().parse::<u64>().ok())
        .collect()
}

struct Literals<'a>(&'a mut Numbers);

impl<'ast> Visit<'ast> for Literals<'_> {
    fn visit_lit_int(&mut self, literal: &'ast syn::LitInt) {
        if let Ok(value) = literal.base10_parse() {
            self.0.code.push(value);
        }
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.0.text.extend(text_numbers(&literal.value()));
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // `format!("...")` and friends: read the literal arguments too.
        let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(arguments) = mac.parse_body_with(parser) {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Numbers, SITES, findings, function_numbers, site_findings, source_constants};

    fn core() -> BTreeMap<String, u64> {
        BTreeMap::from([
            ("MAX_ENDPOINT_ID_BYTES".to_owned(), 128),
            ("MAX_ENDPOINT_ID_SEGMENT_BYTES".to_owned(), 63),
            ("DEFAULT_OPERATION_TIMEOUT_SECONDS".to_owned(), 30),
            ("MAX_STATIC_KEY_BYTES".to_owned(), 256),
        ])
    }

    fn site(function: &str) -> &'static super::Site {
        SITES
            .iter()
            .find(|site| site.function == function)
            .expect("site")
    }

    #[test]
    fn function_numbers_read_code_ranges_and_message_text() {
        let source = r#"
fn is_valid_endpoint_id(value: &str) -> bool {
    (1..=128).contains(&value.len()) && value.split('.').all(|s| (1..=63).contains(&s.len()))
}
fn timeout_text() -> String {
    format!("the client default ({} unless configured)", "30 s")
}
fn other() {}
"#;
        let ids = function_numbers(source, "is_valid_endpoint_id")
            .expect("parse")
            .expect("function");
        assert_eq!(
            ids,
            Numbers {
                code: vec![1, 128, 1, 63],
                text: vec![]
            }
        );
        let timeout = function_numbers(source, "timeout_text")
            .expect("parse")
            .expect("function");
        assert_eq!(timeout.text, [30]);
        assert!(
            function_numbers(source, "missing")
                .expect("parse")
                .is_none()
        );
    }

    #[test]
    fn named_copies_must_exist_and_equal_the_core_owner() {
        let mut core = BTreeMap::new();
        let mut macros = BTreeMap::new();
        for (index, name) in super::NAMED_COPIES.iter().enumerate() {
            let value = u64::try_from(index).expect("index") + 10;
            core.insert((*name).to_owned(), value);
            macros.insert((*name).to_owned(), value);
        }
        // Macro-only constants (no core owner) are not compared.
        macros.insert("MAX_SCOPE_DEPTH".to_owned(), 64);
        let (found, compared) = super::named_copy_findings(&core, &macros);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(compared, super::NAMED_COPIES.len());

        macros.insert("MAX_RATE_LIMIT_KEY_PARTS".to_owned(), 999);
        macros.remove("MAX_AUTH_PLACEMENTS");
        let (found, _) = super::named_copy_findings(&core, &macros);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found
                .iter()
                .any(|finding| finding.contains("MAX_AUTH_PLACEMENTS is missing"))
        );
        assert!(
            found
                .iter()
                .any(|finding| finding.contains("MAX_RATE_LIMIT_KEY_PARTS = 999 differs"))
        );
    }

    #[test]
    fn text_numbers_skip_digits_inside_words() {
        assert_eq!(
            super::text_numbers("1-128 ASCII bytes in dot-separated 1-63 byte segments"),
            [1, 128, 1, 63]
        );
        assert_eq!(
            super::text_numbers("a query key of at most 256 UTF-8 bytes"),
            [256]
        );
        assert_eq!(
            super::text_numbers("`u32`, h2 and x_1 are words; 30 s is not"),
            [30]
        );
    }

    #[test]
    fn drifted_inline_copies_are_reported() {
        let core = core();
        let agree = Numbers {
            code: vec![1, 128, 1, 63],
            text: vec![],
        };
        assert!(site_findings(site("is_valid_endpoint_id"), &agree, &core).is_empty());
        let drifted = Numbers {
            code: vec![1, 100, 1, 63],
            text: vec![],
        };
        let found = site_findings(site("is_valid_endpoint_id"), &drifted, &core);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("is_valid_endpoint_id"));

        let stale_text = Numbers {
            code: vec![],
            text: vec![1, 100, 1, 63],
        };
        let found = site_findings(site("validate_endpoint_id"), &stale_text, &core);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("states 100"));

        let timeout = Numbers {
            code: vec![],
            text: vec![45],
        };
        let found = site_findings(site("timeout_text"), &timeout, &core);
        assert_eq!(found.len(), 2, "{found:?}");
        let silent = Numbers::default();
        let found = site_findings(site("timeout_text"), &silent, &core);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("no longer states"));
    }

    #[test]
    fn source_constants_evaluate_literals_and_products_outside_tests() {
        let directory =
            std::env::temp_dir().join(format!("concord-limit-drift-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        std::fs::write(
            directory.join("lib.rs"),
            "pub const A: usize = 64 * 1_024;\nconst B: u32 = 7;\nconst D: Duration = Duration::from_secs(1);\nfn f() { const LOCAL: usize = 9; }\n#[cfg(test)]\nmod tests { const A: usize = 1; }\n",
        )
        .expect("write");
        let constants = source_constants(&directory).expect("constants");
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(
            constants,
            BTreeMap::from([("A".to_owned(), 65_536), ("B".to_owned(), 7)])
        );
    }

    /// The repository's macro copies equal their core owners.
    #[test]
    fn repository_macro_limit_copies_match_core() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let (found, compared) = findings(&root).expect("limit drift check");
        assert!(found.is_empty(), "{}", found.join("\n"));
        assert!(compared >= 17, "copies compared: {compared}");
    }
}
