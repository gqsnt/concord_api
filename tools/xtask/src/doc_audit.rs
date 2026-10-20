//! `doc-audit`: rendered-Rustdoc evidence for generated documentation
//! (DOC-01 §19.2, §2.3 and §3.5; acceptance row V3-O01).
//!
//! The macro's token tests read the `#[doc]` attributes the emitter produces;
//! DOC-01 requires inspecting what Rustdoc actually renders. This task runs
//! `cargo doc -p concord-examples --no-deps --all-features` (the target
//! directory comes from Cargo metadata, so `CARGO_TARGET_DIR` is respected),
//! fails on any Rustdoc warning for that package, and checks the rendered
//! pages of the canonical §22 Catalog
//! (`crates/concord-examples/src/contracts/catalog_v3.rs`): the `find_item`
//! constructor and request, a root endpoint, a traversal endpoint, a stream
//! endpoint and value-control setters.
//!
//! The canonical Catalog deprecates nothing, so the rendered deprecation
//! notes come from a small fixture client ([`FIXTURE`]) documented by the same
//! Rustdoc against the `concord` metadata produced by that build.
//!
//! Every Catalog endpoint constructor's rendered `Operation:` line (the
//! macro's `route_template` derivation) is checked against
//! [`CATALOG_OPERATIONS`]; with the `examples` feature, the unit test
//! `catalog_operation_lines_match_the_contract_url_templates` derives the same
//! lines from `CatalogApi::contract()` (`url_template`, method and ID), so the
//! method docs and the effective contract cannot disagree (EXPLAIN-01).
//!
//! Each checked section is reduced to text (tags removed, entities and
//! typographic punctuation normalized, whitespace collapsed) before matching.
//! Link expectations resolve the rendered `href` against the page and require
//! the target anchor to exist. A failure names the item, its page and every
//! missing fragment.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use cargo_metadata::MetadataCommand;
use serde_json::Value;

/// The command-line form of this task (after `cargo run -p concord-xtask --`).
pub(crate) const USAGE: &str = "doc-audit [--locked]";

/// Rendered module of the canonical Catalog, relative to the doc root.
const CATALOG: &str = "concord_examples/contracts/catalog_v3";
/// The Catalog's effective-contract method, relative to the doc root.
const CATALOG_CONTRACT: &str =
    "concord_examples/contracts/catalog_v3/struct.CatalogApi.html#method.contract";
/// Crate name of the rendered deprecation fixture.
const FIXTURE_CRATE: &str = "concord_doc_audit_fixture";
/// The fixture's effective-contract method, relative to its doc root.
const FIXTURE_CONTRACT: &str = "concord_doc_audit_fixture/struct.LegacyApi.html#method.contract";
/// A generated card must not advertise a recovery replay that an
/// endpoint-backed credential source cannot perform (§2.3).
const NO_RECOVERY: &[&str] = &["refresh", "recover"];

/// Deprecation fixture: a generated client whose endpoint, scope and value
/// are deprecated. It is documented with `-D warnings`, so its generated docs
/// (including the root-endpoint contract link) must render cleanly.
pub(crate) const FIXTURE: &str = r#"//! Deprecation rendering fixture of `cargo run -p concord-xtask -- doc-audit`.

use concord::{Binary, Bytes, api};

api! {
    /// Legacy provider client.
    client LegacyApi {
        base "https://legacy.example.test/";

        values {
            /// Retired filter, no longer sent.
            #[deprecated(note = "the provider ignores this filter")]
            legacy_filter?: String;
        }

        /// Returns the complete export.
        #[deprecated(since = "3.0.0", note = "use the paged feed instead")]
        GET export() -> Bytes via Binary {
            path "export";
        }

        /// Retired regional routing.
        #[deprecated(note = "regions are resolved by the provider")]
        scope regional(region: String) {
            path "regions/{region}";

            /// Regional status.
            GET status() -> Bytes via Binary {
                path "status";
            }
        }
    }
}
"#;

/// Which part of a rendered page one check reads.
#[derive(Clone, Copy, Debug)]
enum Section {
    /// A method's signature, stability notes and docblock on its owner's page.
    Method(&'static str),
    /// The page item's heading, declaration, stability notes and top docblock.
    Item,
}

/// One rendered item and what its documentation must show.
struct ItemCheck {
    /// Human-readable item name used in failures.
    item: &'static str,
    /// Page path relative to the doc root.
    page: String,
    section: Section,
    /// Text fragments that must all appear (after normalization).
    required: Vec<&'static str>,
    /// Link targets (doc-root relative, with fragment) the section must link.
    links: &'static [&'static str],
    /// Words that must not appear, compared case-insensitively.
    forbidden: &'static [&'static str],
    /// The author description that must precede the generated contract card.
    author_first: Option<&'static str>,
}

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let target = target_directory(root)?;
    let messages = document_examples(root)?;
    let mut failures = check_items(&target.join("doc"), &catalog_checks());
    failures.extend(check_items(
        &target.join("doc"),
        &catalog_operation_checks(),
    ));

    let concord = concord_metadata(&messages).ok_or_else(|| {
        "cargo doc reported no `concord` library metadata to document the deprecation fixture against"
            .to_owned()
    })?;
    let fixture_docs = render_fixture(&target, &concord)?;
    failures.extend(check_items(&fixture_docs, &fixture_checks()));

    if failures.is_empty() {
        println!(
            "doc-audit: rendered Rustdoc of the canonical Catalog and the deprecation fixture shows every required caller-contract fact"
        );
        Ok(())
    } else {
        Err(format!(
            "doc-audit found {} rendered-documentation gap(s):\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

fn target_directory(root: &Path) -> Result<PathBuf, String> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--locked".into(), "--offline".into()])
        .exec()
        .map_err(|error| format!("could not read Cargo metadata: {error}"))?;
    Ok(metadata.target_directory.into_std_path_buf())
}

/// Runs the documented `cargo doc` and returns its JSON messages. Cargo's
/// progress goes to the terminal; Rustdoc diagnostics arrive as JSON.
fn document_examples(root: &Path) -> Result<Vec<Value>, String> {
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "doc",
            "-p",
            "concord-examples",
            "--no-deps",
            "--all-features",
            "--locked",
            "--message-format=json",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("could not run cargo doc: {error}"))?;
    let messages = parse_messages(&output.stdout);
    let diagnostics = package_diagnostics(&messages, "concord_examples");
    if !output.status.success() {
        return Err(format!(
            "cargo doc -p concord-examples --no-deps --all-features failed ({}):\n{}",
            output.status,
            diagnostics.join("\n")
        ));
    }
    if !diagnostics.is_empty() {
        return Err(format!(
            "Rustdoc reported {} diagnostic(s) for concord-examples:\n{}",
            diagnostics.len(),
            diagnostics.join("\n")
        ));
    }
    Ok(messages)
}

fn parse_messages(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

/// Rendered warnings and errors that Rustdoc reported for one target.
fn package_diagnostics(messages: &[Value], target: &str) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message["reason"] == "compiler-message")
        .filter(|message| message["target"]["name"] == target)
        .filter(|message| {
            matches!(
                message["message"]["level"].as_str(),
                Some("warning" | "error")
            )
        })
        .map(|message| {
            message["message"]["rendered"]
                .as_str()
                .or_else(|| message["message"]["message"].as_str())
                .unwrap_or("<diagnostic without text>")
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// The `concord` library metadata (`.rmeta`, else `.rlib`) Cargo produced
/// while documenting the examples.
fn concord_metadata(messages: &[Value]) -> Option<PathBuf> {
    let filenames = messages
        .iter()
        .filter(|message| message["reason"] == "compiler-artifact")
        .find(|message| message["target"]["name"] == "concord")?["filenames"]
        .as_array()?;
    let paths: Vec<&str> = filenames.iter().filter_map(Value::as_str).collect();
    paths
        .iter()
        .find(|path| path.ends_with(".rmeta"))
        .or_else(|| paths.iter().find(|path| path.ends_with(".rlib")))
        .map(PathBuf::from)
}

/// Documents [`FIXTURE`] with Rustdoc (`RUSTDOC`, else `rustdoc`) against the
/// given `concord` metadata and returns the fixture's doc root.
fn render_fixture(target: &Path, concord: &Path) -> Result<PathBuf, String> {
    let directory = target.join("doc-audit");
    let docs = directory.join("doc");
    if docs.exists() {
        fs::remove_dir_all(&docs)
            .map_err(|error| format!("could not clear {}: {error}", docs.display()))?;
    }
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    let source = directory.join("fixture.rs");
    fs::write(&source, FIXTURE)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;
    let dependencies = concord
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", concord.display()))?;
    let mut search = OsString::from("dependency=");
    search.push(dependencies);
    let mut external = OsString::from("concord=");
    external.push(concord);
    let rustdoc = env::var_os("RUSTDOC").unwrap_or_else(|| OsString::from("rustdoc"));
    let output = Command::new(rustdoc)
        .args(["--edition", "2024", "--crate-type", "lib", "--crate-name"])
        .arg(FIXTURE_CRATE)
        .args(["-D", "warnings"])
        .arg(&source)
        .arg("-o")
        .arg(&docs)
        .arg("-L")
        .arg(search)
        .arg("--extern")
        .arg(external)
        .output()
        .map_err(|error| format!("could not run rustdoc: {error}"))?;
    if output.status.success() {
        Ok(docs)
    } else {
        Err(format!(
            "rustdoc could not render the deprecation fixture ({}):\n{}",
            output.status,
            bounded(&output)
        ))
    }
}

fn bounded(output: &Output) -> String {
    const LIMIT: usize = 16 * 1024;
    let text = String::from_utf8_lossy(&output.stderr);
    if text.len() <= LIMIT {
        return text.into_owned();
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[... truncated]", &text[..end])
}

// ---- expectations ----

/// The rendered page that documents an endpoint constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationPage {
    /// `CatalogApi` (root endpoints).
    Root,
    /// `catalog_api::regional::Regional`.
    Regional,
}

/// One Catalog endpoint constructor and the `Operation:` line its rendered
/// docs must show.
struct CatalogOperation {
    page: OperationPage,
    method: &'static str,
    id: &'static str,
    line: &'static str,
}

/// Every Catalog endpoint's rendered operation line (normalized text of the
/// generated card's first bullet).
const CATALOG_OPERATIONS: &[CatalogOperation] = &[
    CatalogOperation {
        page: OperationPage::Root,
        method: "authenticate",
        id: "catalog.auth.login",
        line: "Operation: POST https://api.catalog.example.test/root/{client.api_version}/auth/session (endpoint ID catalog.auth.login, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "item_summaries",
        id: "catalog.items.list",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items (endpoint ID catalog.items.list, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "item",
        id: "catalog.item.get",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items/{id} (endpoint ID catalog.item.get, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "find_item",
        id: "catalog.item.find",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items/{id} (endpoint ID catalog.item.find, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "update_item",
        id: "catalog.item.update",
        line: "Operation: PUT https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items/{id} (endpoint ID catalog.item.update, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "feed",
        id: "catalog.item.feed",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/feed (endpoint ID catalog.item.feed, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "export",
        id: "catalog.item.export",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/export (endpoint ID catalog.item.export, explicit)",
    },
    CatalogOperation {
        page: OperationPage::Regional,
        method: "inspect",
        id: "catalog.item.inspect",
        line: "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/inspect (endpoint ID catalog.item.inspect, explicit)",
    },
];

/// One check per [`CATALOG_OPERATIONS`] entry: the constructor's rendered
/// docs show exactly that operation line.
fn catalog_operation_checks() -> Vec<ItemCheck> {
    CATALOG_OPERATIONS
        .iter()
        .map(|operation| ItemCheck {
            item: operation.id,
            page: match operation.page {
                OperationPage::Root => format!("{CATALOG}/struct.CatalogApi.html"),
                OperationPage::Regional => {
                    format!("{CATALOG}/catalog_api/regional/struct.Regional.html")
                }
            },
            section: Section::Method(operation.method),
            required: vec![operation.line],
            links: &[],
            forbidden: &[],
            author_first: None,
        })
        .collect()
}

const FIND_ITEM_CARD: &[&str] = &[
    // Author description.
    "A head-only 404 is None; the body of that response is not polled.",
    // Effective route and explicit operational ID.
    "Operation: GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items/{id} (endpoint ID catalog.item.find, explicit)",
    // Result type and its representation.
    "Result: Option<Item> via Json; response representation inherited from CatalogApi",
    "Request body: none",
    "Accepted statuses: 200..=299",
    // Expected-absence status.
    "Expected absence: None on 404 (head-only; that response body is dropped unread)",
    "Effect: safe (method default)",
    // Effective timeout.
    "Timeout: 20 s",
    // Authentication source line.
    "Authentication: bearer session (challenge on 401), from CatalogApi::regional",
    "Rate policy catalog.tenant.read: key [origin, \"tenant\"], cost 1, from CatalogApi::regional",
    // The contract link text (the link itself is checked separately).
    "The complete effective contract is available from CatalogApi::contract().",
];

fn catalog_checks() -> Vec<ItemCheck> {
    let root = format!("{CATALOG}/struct.CatalogApi.html");
    let builder = format!("{CATALOG}/catalog_api/struct.CatalogApiBuilder.html");
    let regional = format!("{CATALOG}/catalog_api/regional/struct.Regional.html");
    vec![
        ItemCheck {
            item: "Regional::find_item (constructor)",
            page: regional.clone(),
            section: Section::Method("find_item"),
            required: [
                FIND_ITEM_CARD,
                &[
                    // Signature: required input and typed request.
                    "pub fn find_item(&self, id: impl IntoValue<ItemId>) -> FindItemRequest",
                    // Required inputs named on the method the caller invokes.
                    "Creates the inert CatalogApi::regional::find_item request from its required inputs id.",
                ],
            ]
            .concat(),
            links: &[CATALOG_CONTRACT],
            forbidden: NO_RECOVERY,
            author_first: Some("A head-only 404 is None"),
        },
        ItemCheck {
            item: "FindItemRequest (request type)",
            page: format!("{CATALOG}/catalog_api/regional/struct.FindItemRequest.html"),
            section: Section::Item,
            required: [
                FIND_ITEM_CARD,
                &[
                    "Named request for CatalogApi::regional::find_item. Holding it does not contact the server",
                    "id: ItemId (required)",
                ],
            ]
            .concat(),
            links: &[CATALOG_CONTRACT],
            forbidden: NO_RECOVERY,
            author_first: Some("A head-only 404 is None"),
        },
        ItemCheck {
            item: "CatalogApi::authenticate (root endpoint constructor)",
            page: root,
            section: Section::Method("authenticate"),
            required: vec![
                "pub fn authenticate(&self, input: LoginInput) -> AuthenticateRequest",
                "Explicit login. Use store_as_session to request local installation.",
                "from its required inputs body input.",
                "Request body: input: LoginInput via Json (required); request representation inherited from CatalogApi",
                "Effect: non-idempotent (declared)",
                "Authentication: header \"X-Bootstrap-Key\" = application_key, from CatalogApi",
                "store_as_session() requests local installation into credential session",
                "The complete effective contract is available from CatalogApi::contract().",
            ],
            links: &[CATALOG_CONTRACT],
            forbidden: NO_RECOVERY,
            author_first: Some("Explicit login."),
        },
        ItemCheck {
            item: "Regional::item_summaries (traversal endpoint constructor)",
            page: regional.clone(),
            section: Section::Method("item_summaries"),
            required: vec![
                "One decoded page; select an explicit traversal terminal for more.",
                "from its required inputs tags.",
                "Result: Vec<ItemSummary> via Json",
                "Pagination: pages, collect_prefix and collect_all are available; ordinary .await returns one page",
            ],
            links: &[CATALOG_CONTRACT],
            forbidden: &[],
            author_first: Some("One decoded page;"),
        },
        ItemCheck {
            item: "Regional::export (stream endpoint constructor)",
            page: regional,
            section: Section::Method("export"),
            required: vec![
                "pub fn export(&self) -> ExportRequest",
                "Returns a managed byte stream, not a completed export.",
                "Result: ByteStream via Binary; response representation selected by via on endpoint CatalogApi::regional::export",
                "Request body: none",
            ],
            links: &[CATALOG_CONTRACT],
            forbidden: &[],
            author_first: Some("Returns a managed byte stream"),
        },
        ItemCheck {
            item: "ItemSummariesRequest::replace_category (value control)",
            page: format!("{CATALOG}/catalog_api/regional/struct.ItemSummariesRequest.html"),
            section: Section::Method("replace_category"),
            required: vec![
                "pub fn replace_category(self, __concord_value: Option<Category>) -> Self",
                "Replaces the outer slot of category exactly: Some(v) is present, None is absent. For a nested Option payload, Some(None) is a present inner None.",
                "Owner: CatalogApi::regional::item_summaries.",
            ],
            links: &[],
            forbidden: &[],
            author_first: None,
        },
        ItemCheck {
            item: "CatalogApiBuilder::set_tenant (value control)",
            page: builder,
            section: Section::Method("set_tenant"),
            required: vec![
                "pub fn set_tenant(self, __concord_value: impl IntoValue<TenantId>) -> Self",
                "The tenant is sent, but excluded from automatic observation.",
                "Selects a present tenant payload (impl IntoValue<TenantId>: identity; borrowed strings only when the target is String).",
                "The value is sensitive: it is sent, but withheld from automatic plans, errors and observers.",
            ],
            links: &[],
            forbidden: &[],
            author_first: Some("The tenant is sent"),
        },
    ]
}

fn fixture_checks() -> Vec<ItemCheck> {
    let root = format!("{FIXTURE_CRATE}/struct.LegacyApi.html");
    vec![
        ItemCheck {
            item: "LegacyApi::export (deprecated endpoint constructor)",
            page: root.clone(),
            section: Section::Method("export"),
            required: vec![
                "pub fn export(&self) -> ExportRequest",
                "Deprecated since 3.0.0: use the paged feed instead",
                "Returns the complete export.",
                "Creates the inert LegacyApi::export request.",
                "The complete effective contract is available from LegacyApi::contract().",
            ],
            links: &[FIXTURE_CONTRACT],
            forbidden: &[],
            author_first: Some("Returns the complete export."),
        },
        ItemCheck {
            item: "ExportRequest (deprecated request type)",
            page: format!("{FIXTURE_CRATE}/legacy_api/struct.ExportRequest.html"),
            section: Section::Item,
            required: vec![
                "Deprecated since 3.0.0: use the paged feed instead",
                "Returns the complete export.",
            ],
            links: &[FIXTURE_CONTRACT],
            forbidden: &[],
            author_first: None,
        },
        ItemCheck {
            item: "LegacyApi::regional (deprecated scope navigation)",
            page: root.clone(),
            section: Section::Method("regional"),
            required: vec![
                "Deprecated: regions are resolved by the provider",
                "Retired regional routing.",
            ],
            links: &[],
            forbidden: &[],
            author_first: None,
        },
        ItemCheck {
            item: "LegacyApiBuilder::set_legacy_filter (deprecated value control)",
            page: format!("{FIXTURE_CRATE}/legacy_api/struct.LegacyApiBuilder.html"),
            section: Section::Method("set_legacy_filter"),
            required: vec![
                "Deprecated: the provider ignores this filter",
                "Retired filter, no longer sent.",
                "Selects a present legacy_filter payload",
            ],
            links: &[],
            forbidden: &[],
            author_first: None,
        },
        ItemCheck {
            item: "LegacyApi (client with a deprecated no-op value)",
            page: root,
            section: Section::Item,
            required: vec!["legacy_filter: String (optional, deprecated no-op value)"],
            links: &[],
            forbidden: &[],
            author_first: None,
        },
    ]
}

// ---- evaluation ----

fn check_items(doc_root: &Path, checks: &[ItemCheck]) -> Vec<String> {
    let mut failures = Vec::new();
    for check in checks {
        let path = doc_root.join(&check.page);
        let html = match fs::read_to_string(&path) {
            Ok(html) => html,
            Err(error) => {
                failures.push(format!(
                    "- `{}`: rendered page {} is unreadable: {error}",
                    check.item,
                    path.display()
                ));
                continue;
            }
        };
        failures.extend(evaluate(check, &html));
        let Some(section) = section(&html, check.section) else {
            continue;
        };
        for link in check.links {
            if let Some(failure) = link_target_failure(doc_root, check, section, link) {
                failures.push(failure);
            }
        }
    }
    failures
}

/// Checks one page's section text; link targets are verified separately.
fn evaluate(check: &ItemCheck, html: &str) -> Vec<String> {
    let Some(section) = section(html, check.section) else {
        return vec![format!(
            "- `{}`: {} has no rendered {:?} section",
            check.item, check.page, check.section
        )];
    };
    let text = html_to_text(section);
    let lower = text.to_lowercase();
    let mut failures = Vec::new();
    for needle in &check.required {
        if !text.contains(&normalize(needle)) {
            failures.push(format!(
                "- `{}` ({}) lacks `{needle}`",
                check.item, check.page
            ));
        }
    }
    for word in check.forbidden {
        if lower.contains(&word.to_lowercase()) {
            failures.push(format!(
                "- `{}` ({}) must not mention `{word}`",
                check.item, check.page
            ));
        }
    }
    if let Some(author) = check.author_first {
        let author_at = text.find(&normalize(author));
        let card_at = text.find("Operation:").or_else(|| text.find("Selects "));
        if let (Some(author_at), Some(card_at)) = (author_at, card_at) {
            if author_at > card_at {
                failures.push(format!(
                    "- `{}` ({}): the author description `{author}` must precede the generated text",
                    check.item, check.page
                ));
            }
        }
    }
    for link in check.links {
        let found = hrefs(section)
            .iter()
            .any(|href| resolve(&check.page, href).as_deref() == Some(*link));
        if !found {
            failures.push(format!(
                "- `{}` ({}) does not link `{link}`",
                check.item, check.page
            ));
        }
    }
    failures
}

/// A linked anchor must exist on its target page.
fn link_target_failure(
    doc_root: &Path,
    check: &ItemCheck,
    section: &str,
    link: &str,
) -> Option<String> {
    if !hrefs(section)
        .iter()
        .any(|href| resolve(&check.page, href).as_deref() == Some(link))
    {
        // Already reported by `evaluate`.
        return None;
    }
    let (page, fragment) = link.split_once('#').unwrap_or((link, ""));
    let path = doc_root.join(page);
    match fs::read_to_string(&path) {
        Ok(html) if fragment.is_empty() || html.contains(&format!("id=\"{fragment}\"")) => None,
        Ok(_) => Some(format!(
            "- `{}`: link target {} has no anchor `{fragment}`",
            check.item,
            path.display()
        )),
        Err(error) => Some(format!(
            "- `{}`: link target {} is unreadable: {error}",
            check.item,
            path.display()
        )),
    }
}

/// The HTML of one section of a Rustdoc page.
fn section(html: &str, section: Section) -> Option<&str> {
    match section {
        Section::Method(name) => {
            // Start after the method's opening `<section ...>` tag and end
            // before the next method or the end of this method's toggle.
            let marker = html.find(&format!("id=\"method.{name}\""))?;
            let start = marker + html[marker..].find('>')? + 1;
            let rest = &html[start..];
            let end = [rest.find("</details>"), rest.find("<section id=")]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(rest.len());
            Some(&rest[..end])
        }
        Section::Item => {
            let start = html.find("<div class=\"main-heading\">")?;
            let rest = &html[start..];
            let top = rest.find("class=\"toggle top-doc\"")?;
            let end = top + rest[top..].find("</details>")?;
            Some(&rest[..end])
        }
    }
}

/// Every `href` attribute value in a fragment of HTML.
fn hrefs(html: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("href=\"") {
        rest = &rest[start + "href=\"".len()..];
        let Some(end) = rest.find('"') else { break };
        found.push(&rest[..end]);
        rest = &rest[end..];
    }
    found
}

/// Resolves a relative `href` found on `page` to a doc-root-relative path
/// (with fragment). External and root-escaping links resolve to `None`.
fn resolve(page: &str, href: &str) -> Option<String> {
    if href.contains("://") || href.starts_with('/') {
        return None;
    }
    if let Some(fragment) = href.strip_prefix('#') {
        return Some(format!("{page}#{fragment}"));
    }
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (href, None),
    };
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let mut resolved = parts.join("/");
    if let Some(fragment) = fragment {
        resolved.push('#');
        resolved.push_str(fragment);
    }
    Some(resolved)
}

/// Reduces rendered HTML to comparable text: block tags become spaces, inline
/// tags disappear, entities are decoded and typographic punctuation and
/// whitespace are normalized.
pub(crate) fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open..];
            break;
        };
        if is_block_tag(&rest[open + 1..open + close]) {
            text.push(' ');
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    normalize(&decode_entities(&text))
}

fn is_block_tag(tag: &str) -> bool {
    const BLOCK: &[&str] = &[
        "p",
        "li",
        "ul",
        "ol",
        "dl",
        "dt",
        "dd",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "div",
        "section",
        "details",
        "summary",
        "br",
        "hr",
        "pre",
        "table",
        "tr",
        "td",
        "th",
        "blockquote",
    ];
    let name = tag
        .trim_start_matches('/')
        .split(|character: char| character.is_whitespace() || character == '/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    BLOCK.contains(&name.as_str())
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest[1..]
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| entity(&rest[1..=end]).map(|character| (character, end + 2)));
        match decoded {
            Some((character, consumed)) => {
                out.push(character);
                rest = &rest[consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Maps Rustdoc's typographic punctuation to ASCII and collapses whitespace.
pub(crate) fn normalize(text: &str) -> String {
    let mut ascii = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\u{201C}' | '\u{201D}' | '\u{201E}' => ascii.push('"'),
            '\u{2018}' | '\u{2019}' => ascii.push('\''),
            '\u{2026}' => ascii.push_str("..."),
            '\u{2013}' => ascii.push_str("--"),
            '\u{2014}' => ascii.push_str("---"),
            '\u{00A0}' => ascii.push(' '),
            other => ascii.push(other),
        }
    }
    ascii.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        CATALOG_OPERATIONS, FIND_ITEM_CARD, ItemCheck, Section, catalog_operation_checks,
        concord_metadata, decode_entities, evaluate, hrefs, html_to_text, normalize,
        package_diagnostics, parse_messages, resolve, section,
    };

    #[test]
    fn catalog_operation_checks_cover_each_endpoint_once() {
        let checks = catalog_operation_checks();
        assert_eq!(checks.len(), CATALOG_OPERATIONS.len());
        let mut ids = CATALOG_OPERATIONS
            .iter()
            .map(|operation| operation.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG_OPERATIONS.len(), "one line per endpoint");
        for operation in CATALOG_OPERATIONS {
            assert!(operation.line.starts_with("Operation: "));
            assert!(
                operation
                    .line
                    .ends_with(&format!("(endpoint ID {}, explicit)", operation.id))
                    || operation.line.ends_with(&format!(
                        "(endpoint ID {}, navigation-derived)",
                        operation.id
                    )),
                "{}",
                operation.line
            );
        }
        // The find_item card and the operation table state the same route.
        let find = CATALOG_OPERATIONS
            .iter()
            .find(|operation| operation.method == "find_item")
            .expect("find_item");
        assert!(FIND_ITEM_CARD.contains(&find.line));
    }

    /// EXPLAIN-01: the rendered method-doc routes (`route_template` in the
    /// macro, checked on rendered Rustdoc by `doc-audit`) equal the contract's
    /// independently derived `url_template`, method and ID for every Catalog
    /// endpoint.
    #[cfg(feature = "examples")]
    #[test]
    fn catalog_operation_lines_match_the_contract_url_templates() {
        use concord_examples::contracts::catalog_v3::CatalogApi;

        let contract = CatalogApi::contract();
        assert_eq!(contract.endpoints().len(), CATALOG_OPERATIONS.len());
        for operation in CATALOG_OPERATIONS {
            let endpoint = contract
                .endpoint(operation.id)
                .unwrap_or_else(|| panic!("{} is not a Catalog endpoint", operation.id));
            let derivation = match endpoint.identity().source().label() {
                "explicit" => "explicit",
                _ => "navigation-derived",
            };
            // Presentation rule, not a semantic difference: the method card
            // is written from the endpoint's point of view and shows its own
            // values bare (`{id}`), exactly as the v3 DSL names them, while
            // the whole-client contract qualifies every reference with its
            // owner (`{endpoint.id}`, as in the §18.5 static view). Ancestor
            // references (`{client.api_version}`, `{regional.region}`),
            // literals, labels, order and the base must agree verbatim.
            let template = contract.url_template(endpoint);
            assert!(
                !template.contains("{id}"),
                "the contract qualifies endpoint values: {template}"
            );
            let derived = format!(
                "Operation: {} {} (endpoint ID {}, {derivation})",
                endpoint.method(),
                template.replace("{endpoint.", "{"),
                endpoint.id()
            );
            assert_eq!(operation.line, derived, "{}", operation.id);
            assert_eq!(
                endpoint.name(),
                operation.method,
                "{}: documented on the constructor it names",
                operation.id
            );
            let owner = endpoint.owner();
            let expected_owner = match operation.page {
                super::OperationPage::Root => "CatalogApi",
                super::OperationPage::Regional => "CatalogApi::regional",
            };
            assert_eq!(owner, expected_owner, "{}", operation.id);
        }
    }

    #[test]
    fn html_to_text_strips_tags_decodes_entities_and_normalizes_punctuation() {
        let html = "<ul>\n<li>Result: <code>Option&lt;Item&gt;</code> via <code>Json</code></li>\
                    <li>key [origin, \u{201C}tenant\u{201D}], it\u{2019}s&nbsp;fine\u{2026}</li></ul>";
        assert_eq!(
            html_to_text(html),
            "Result: Option<Item> via Json key [origin, \"tenant\"], it's fine..."
        );
        // Block tags separate words; inline tags (including <wbr>) do not.
        assert_eq!(html_to_text("<p>one</p><p>two</p>"), "one two");
        assert_eq!(html_to_text("Find<wbr>Item<wbr>Request"), "FindItemRequest");
        assert_eq!(
            html_to_text("<h5 id=\"x\"><a href=\"#x\">\u{a7}</a>Contract</h5>"),
            "\u{a7}Contract"
        );
        // An unterminated tag is kept as text rather than swallowing the rest.
        assert_eq!(html_to_text("a < b"), "a < b");
    }

    #[test]
    fn entities_decode_named_and_numeric_forms_only() {
        assert_eq!(decode_entities("&#39;&#x27;&#X27;&amp;&quot;"), "'''&\"");
        assert_eq!(decode_entities("&bogus; & &#xZZ; &"), "&bogus; & &#xZZ; &");
        assert_eq!(normalize("  a \n\t b  "), "a b");
        assert_eq!(normalize("x \u{2013} y \u{2014} z"), "x -- y --- z");
    }

    const PAGE: &str = concat!(
        "<div class=\"main-heading\"><h1>Struct <span>Find<wbr>Item</span></h1></div>",
        "<span class=\"item-info\"><div class=\"stab deprecated\"><span>Deprecated since 3.0.0: gone</span></div></span>",
        "<details class=\"toggle top-doc\" open><summary class=\"hideme\"><span>Expand description</span></summary>",
        "<div class=\"docblock\"><p>Author text.</p>\n<p>Named request.</p></div></details>",
        "<details class=\"toggle method-toggle\" open><summary><section id=\"method.first\" class=\"method\">",
        "<h4 class=\"code-header\">pub fn <a href=\"#method.first\">first</a>(&amp;self) -&gt; Self</h4></section>",
        "<span class=\"item-info\"><div class=\"stab deprecated\"><span>Deprecated: old</span></div></span></summary>",
        "<div class=\"docblock\"><p>First docs; see <a href=\"../../struct.Api.html#method.contract\"><code>Api::contract()</code></a>.</p></div></details>",
        "<details class=\"toggle method-toggle\" open><summary><section id=\"method.second\" class=\"method\">",
        "<h4 class=\"code-header\">pub fn second()</h4></section></summary><div class=\"docblock\"><p>Second docs.</p></div></details>",
    );

    fn check(section: Section, required: &'static [&'static str]) -> ItemCheck {
        ItemCheck {
            item: "Api::first",
            page: "krate/api/scope/struct.Scope.html".to_owned(),
            section,
            required: required.to_vec(),
            links: &[],
            forbidden: &[],
            author_first: None,
        }
    }

    #[test]
    fn method_and_item_sections_are_scoped_to_one_item() {
        let first = html_to_text(section(PAGE, Section::Method("first")).expect("first"));
        assert_eq!(
            first,
            "pub fn first(&self) -> Self Deprecated: old First docs; see Api::contract()."
        );
        assert!(!first.contains("Second docs"));
        let second = html_to_text(section(PAGE, Section::Method("second")).expect("second"));
        assert!(second.ends_with("pub fn second() Second docs."), "{second}");
        assert!(section(PAGE, Section::Method("fir")).is_none());

        let item = html_to_text(section(PAGE, Section::Item).expect("item"));
        assert_eq!(
            item,
            "Struct FindItem Deprecated since 3.0.0: gone Expand description Author text. Named request."
        );
        assert!(section("<p>no heading</p>", Section::Item).is_none());
    }

    #[test]
    fn hrefs_resolve_against_the_page() {
        let page = "krate/api/scope/struct.Scope.html";
        assert_eq!(
            hrefs(section(PAGE, Section::Method("first")).expect("first")),
            ["#method.first", "../../struct.Api.html#method.contract"]
        );
        assert_eq!(
            resolve(page, "../../struct.Api.html#method.contract").as_deref(),
            Some("krate/struct.Api.html#method.contract")
        );
        assert_eq!(
            resolve(page, "#method.first").as_deref(),
            Some("krate/api/scope/struct.Scope.html#method.first")
        );
        assert_eq!(
            resolve(page, "./struct.Other.html").as_deref(),
            Some("krate/api/scope/struct.Other.html")
        );
        assert_eq!(resolve(page, "../../../../escape.html"), None);
        assert_eq!(resolve(page, "https://doc.rust-lang.org/std"), None);
    }

    #[test]
    fn evaluation_names_every_missing_fragment_forbidden_word_and_link() {
        assert!(
            evaluate(
                &check(
                    Section::Method("first"),
                    &[
                        "pub fn first(&self) -> Self",
                        "Deprecated: old",
                        "First docs;"
                    ]
                ),
                PAGE
            )
            .is_empty()
        );

        let mut strict = check(
            Section::Method("first"),
            &["Deprecated: old", "Second docs."],
        );
        strict.forbidden = &["FIRST DOCS"];
        strict.links = &[
            "krate/struct.Api.html#method.contract",
            "krate/struct.Api.html#method.other",
        ];
        strict.author_first = Some("First docs");
        let failures = evaluate(&strict, PAGE);
        assert_eq!(failures.len(), 3, "{failures:?}");
        assert!(
            failures[0].contains("`Api::first`") && failures[0].contains("lacks `Second docs.`")
        );
        assert!(failures[1].contains("must not mention `FIRST DOCS`"));
        assert!(failures[2].contains("does not link `krate/struct.Api.html#method.other`"));

        let missing = evaluate(&check(Section::Method("absent"), &["x"]), PAGE);
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("has no rendered Method(\"absent\") section"));

        // The author description must come before the generated card.
        let mut order = check(Section::Item, &[]);
        order.author_first = Some("Named request.");
        let html = PAGE.replace(
            "<p>Named request.</p>",
            "<p>Named request.</p><ul><li>Operation: GET</li></ul>",
        );
        assert!(evaluate(&order, &html).is_empty());
        let reversed = PAGE.replace("<p>Author text.</p>", "<ul><li>Operation: GET</li></ul>");
        order.author_first = Some("Named request.");
        assert_eq!(evaluate(&order, &reversed).len(), 1);
    }

    #[test]
    fn cargo_messages_yield_package_diagnostics_and_concord_metadata() {
        let stdout = concat!(
            "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"concord_core\"},\"filenames\":[\"/t/debug/deps/libconcord_core-1.rmeta\"]}\n",
            "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"concord\"},\"filenames\":[\"/t/debug/deps/libconcord-2.rlib\",\"/t/debug/deps/libconcord-2.rmeta\"]}\n",
            "{\"reason\":\"compiler-message\",\"target\":{\"name\":\"concord_examples\"},\"message\":{\"level\":\"warning\",\"rendered\":\"warning: unresolved link\\n\"}}\n",
            "{\"reason\":\"compiler-message\",\"target\":{\"name\":\"concord_examples\"},\"message\":{\"level\":\"note\",\"rendered\":\"note: context\"}}\n",
            "{\"reason\":\"compiler-message\",\"target\":{\"name\":\"other\"},\"message\":{\"level\":\"warning\",\"rendered\":\"warning: elsewhere\"}}\n",
            "not json\n",
            "{\"reason\":\"build-finished\",\"success\":true}\n",
        );
        let messages = parse_messages(stdout.as_bytes());
        assert_eq!(messages.len(), 6);
        assert_eq!(
            package_diagnostics(&messages, "concord_examples"),
            ["warning: unresolved link"]
        );
        assert_eq!(
            concord_metadata(&messages).as_deref(),
            Some(std::path::Path::new("/t/debug/deps/libconcord-2.rmeta"))
        );
        assert_eq!(concord_metadata(&messages[..1]), None);
    }
}
