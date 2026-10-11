//! R5-01 review: a removed generated member named as a function value inside
//! a macro the tool does not expand. Reconstructed from the review brief (the
//! reviewer's archive was not available): the scanner reported a removed
//! member inside opaque macro tokens only when `(..)` immediately followed
//! it, so `pass!(Type::debug)` and `pass!((Type::debug)(request, level))`
//! were silently left to reach an application trait method in v3.
use super::*;

const REMOVED: &str = "C3-MIGRATE-GENERATED-MEMBER-REMOVED";
const PAGINATE_UNRESOLVED: &str = "C3-MIGRATE-PAGINATE-UNRESOLVED";

const V21: &str = r#"use concord::{api, Empty};
use concord::observation::DebugLevel;
api! {
    client RemovedMemberApi {
        base "https://review.example.test/";
        timeout 5s;
        GET item() -> () via Empty { path ["item"]; }
    }
}
pub trait AppDebug: Sized {
    fn debug(self, _level: DebugLevel) -> Self { self }
}
impl AppDebug for removed_member_api::ItemRequest {}
struct Foreign;
impl Foreign { fn debug(self, _level: DebugLevel) -> Self { self } }
"#;

const PASS: &str = "macro_rules! pass { ($value:expr) => { $value }; }";

fn with_body(body: &str) -> String {
    format!(
        "{V21}\npub fn application() {{\n{PASS}\nlet client = RemovedMemberApi::builder().build().expect(\"fixture client\");\nlet request = client.item();\n{body}\n}}\n"
    )
}

/// The diagnostic codes on the one line holding `fragment`.
fn codes_at(outcome: &SourceOutcome, source: &str, fragment: &str) -> Vec<&'static str> {
    let line = unique_line(source, fragment);
    outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.code.as_str())
        .collect()
}

fn assert_reported(body: &str, fragment: &str) {
    let source = with_body(body);
    let outcome = convert(&source);
    assert!(
        outcome.converted.contains(fragment),
        "do not guess a replacement:\n{}",
        outcome.converted
    );
    let line = unique_line(&source, fragment);
    let found = outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line && d.code.as_str() == REMOVED)
        .collect::<Vec<_>>();
    assert_eq!(
        found.len(),
        1,
        "a removed generated member referenced inside a macro must not silently change dispatch: {:#?}",
        outcome.diagnostics
    );
    assert_eq!(found[0].severity(), Severity::Error);
    assert!(
        found[0].message.contains("`debug`") && found[0].message.contains("may reach"),
        "{}",
        found[0].message
    );
}

fn assert_silent(body: &str, fragment: &str) {
    let source = with_body(body);
    let outcome = convert(&source);
    assert!(outcome.converted.contains(fragment));
    assert_eq!(
        codes_at(&outcome, &source, fragment),
        Vec::<&str>::new(),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r5_removed_debug_function_value_in_opaque_macro_is_reported() {
    assert_reported(
        "let apply = pass!(removed_member_api::ItemRequest::debug);\nlet _request = apply(request, DebugLevel::None);",
        "pass!(removed_member_api::ItemRequest::debug)",
    );
}

#[test]
fn review_r5_grouped_removed_debug_call_in_opaque_macro_is_reported() {
    assert_reported(
        "let _request = pass!((removed_member_api::ItemRequest::debug)(request, DebugLevel::None));",
        "pass!((removed_member_api::ItemRequest::debug)(request, DebugLevel::None))",
    );
}

#[test]
fn review_r5_direct_removed_debug_calls_in_opaque_macro_stay_reported() {
    assert_reported(
        "let _request = pass!(request.debug(DebugLevel::None));",
        "pass!(request.debug(DebugLevel::None))",
    );
    assert_reported(
        "let _request = pass!(removed_member_api::ItemRequest::debug(request, DebugLevel::None));",
        "pass!(removed_member_api::ItemRequest::debug(request, DebugLevel::None))",
    );
}

#[test]
fn review_r5_function_value_outside_macros_stays_reported() {
    let body = "let apply = removed_member_api::ItemRequest::debug;\nlet _request = apply(request, DebugLevel::None);";
    let fragment = "let apply = removed_member_api::ItemRequest::debug";
    let source = with_body(body);
    let outcome = convert(&source);
    assert!(outcome.converted.contains(fragment));
    assert_eq!(
        codes_at(&outcome, &source, fragment),
        [REMOVED],
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r5_stringify_control_is_untouched_and_unreported() {
    assert_silent(
        "let _name = stringify!(removed_member_api::ItemRequest::debug);\nlet _ = request;",
        "stringify!(removed_member_api::ItemRequest::debug)",
    );
}

#[test]
fn review_r5_nested_log_debug_macro_is_not_reported() {
    assert_silent(
        "pass!(log::debug!(\"item {:?}\", DebugLevel::None));\nlet _ = request;",
        "pass!(log::debug!(",
    );
    assert_silent(
        "pass!(::log::debug!(\"plain\"));\nlet _ = request;",
        "pass!(::log::debug!(",
    );
}

#[test]
fn review_r5_trait_qualified_and_foreign_references_in_macro_are_not_reported() {
    for (body, fragment) in [
        (
            "let _apply = pass!(<removed_member_api::ItemRequest as AppDebug>::debug);",
            "pass!(<removed_member_api::ItemRequest as AppDebug>::debug)",
        ),
        (
            "let _request = pass!(<removed_member_api::ItemRequest as AppDebug>::debug(request, DebugLevel::None));",
            "pass!(<removed_member_api::ItemRequest as AppDebug>::debug(request, DebugLevel::None))",
        ),
        (
            "let _apply = pass!(AppDebug::debug);",
            "pass!(AppDebug::debug)",
        ),
        (
            "let _apply = pass!(Foreign::debug);",
            "pass!(Foreign::debug)",
        ),
    ] {
        assert_silent(&format!("{body}\nlet _ = request;"), fragment);
    }
}

#[test]
fn review_r5_provider_setter_value_in_macro_goes_through_the_shared_path() {
    let source = r#"use concord::{api, Empty};
api! {
    client Delegating {
        base "https://review.example.test/";
        timeout 5s;
        credentials { credential delegated = custom crate::Provider; }
        GET item() -> () via Empty {
            path ["item"];
            auth { bearer delegated; }
        }
    }
}
pub struct Provider;
pub trait AppProviderSetter: Sized {
    fn delegated_provider(self, _provider: Provider) -> Self { self }
}
impl AppProviderSetter for DelegatingBuilder {}
macro_rules! pass { ($value:expr) => { $value }; }
pub fn configure(builder: DelegatingBuilder, provider: Provider) -> DelegatingBuilder {
    let apply = pass!(DelegatingBuilder::delegated_provider);
    apply(builder, provider)
}
"#;
    let fragment = "pass!(DelegatingBuilder::delegated_provider)";
    let outcome = convert(source);
    assert!(outcome.converted.contains(fragment));
    let line = unique_line(source, fragment);
    let found = outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line && d.code.as_str() == REMOVED)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{:#?}", outcome.diagnostics);
    assert!(
        found[0].message.contains("delegated_provider"),
        "{}",
        found[0].message
    );
}

#[test]
fn review_r5_paginate_value_in_macro_is_owned_by_the_paginate_rule() {
    let source = r#"use std::num::NonZeroU32;
use concord::{PageLimit, api};
api! {
    client Pages {
        base "https://pages.example.test/";
        timeout 5s;
        GET list(offset: u64 = 0, count: NonZeroU32 = crate::PAGE) -> Vec<String> {
            query { "offset" = endpoint.offset; "count" = endpoint.count; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}
pub const PAGE: NonZeroU32 = NonZeroU32::MIN;
macro_rules! pass { ($value:expr) => { $value }; }
pub fn traverse(client: Pages) {
    let apply = pass!(pages::ListRequest::paginate);
    let _ = apply(client.list(), PageLimit::Prefix(3));
}
"#;
    let fragment = "pass!(pages::ListRequest::paginate)";
    let outcome = convert(source);
    assert!(outcome.converted.contains(fragment));
    assert_eq!(
        codes_at(&outcome, source, fragment),
        [PAGINATE_UNRESOLVED],
        "exactly the paginate rule's report, never a second generic diagnostic: {:#?}",
        outcome.diagnostics
    );
}
