//! R4-01: generated members v2.1 emitted and v3 does not. A call that reached
//! such a generated inherent method in v2.1 may, unchanged, reach an
//! application trait method of the same name in v3 (a trait method is
//! selected once no inherent method of that name exists). Each removed member
//! has a disposition (`migration::tests::surface_parity`):
//!
//! - the bound request's `debug(self, DebugLevel)`: a manual decision,
//!   `C3-MIGRATE-GENERATED-MEMBER-REMOVED`;
//! - the builder's custom-provider setter `<credential>_provider(self, P)`:
//!   a manual decision with the v3 name (not an exact rename: v2.1 took any
//!   provider type with the same `Material`, v3 takes the declared type);
//! - the paginated request's `paginate(self, PageLimit)`: the dedicated
//!   paginate rule (`C3-MIGRATE-PAGINATE-*`), never a second diagnostic.
use super::*;

const REMOVED: &str = "C3-MIGRATE-GENERATED-MEMBER-REMOVED";
const SHADOW: &str = "C3-MIGRATE-GENERATED-NAME-SHADOW";
const PAGINATE_UNRESOLVED: &str = "C3-MIGRATE-PAGINATE-UNRESOLVED";

const DEBUG_V21: &str = r#"use concord::{api, Empty};
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
"#;

fn debug_input(body: &str) -> String {
    format!(
        "{DEBUG_V21}\npub fn application() {{\n    let client = RemovedMemberApi::builder().build().expect(\"fixture client\");\n    let request = client.item();\n{body}\n}}\n"
    )
}

/// The removed-member diagnostics on the one line holding `fragment`.
fn removed_at<'o>(
    outcome: &'o SourceOutcome,
    source: &str,
    fragment: &str,
) -> Vec<&'o super::super::report::Diagnostic> {
    let line = unique_line(source, fragment);
    outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line && d.code.as_str() == REMOVED)
        .collect()
}

/// `fragment` stays unchanged and carries exactly one removed-member error
/// whose message is the proved form: it names the old role and member, says
/// v3 has no such inherent member, why the unchanged call may resolve
/// differently, and what to review.
fn assert_proved_removal(source: &str, fragment: &str, member: &str, role: &str) {
    let outcome = convert(source);
    assert!(
        outcome.converted.contains(fragment),
        "the call must stay unchanged:\n{}",
        outcome.converted
    );
    let found = removed_at(&outcome, source, fragment);
    assert_eq!(found.len(), 1, "{:#?}", outcome.diagnostics);
    let d = found[0];
    assert_eq!(d.severity(), Severity::Error);
    for needle in [
        format!("`{member}`"),
        role.to_owned(),
        "v2.1 generates".to_owned(),
        "v3 no longer generates".to_owned(),
        "no inherent method".to_owned(),
        "application trait".to_owned(),
        "Review".to_owned(),
    ] {
        assert!(
            d.message.contains(&needle),
            "missing `{needle}`: {}",
            d.message
        );
    }
    assert!(
        !d.message.contains("v3 generates `"),
        "must not reuse the addition-shadow wording: {}",
        d.message
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|other| other.code.as_str() != SHADOW),
        "{:#?}",
        outcome.diagnostics
    );
}

/// `fragment` stays unchanged and carries no removed-member diagnostic.
fn assert_no_removal(source: &str, fragment: &str) {
    let outcome = convert(source);
    assert!(outcome.converted.contains(fragment));
    assert!(
        removed_at(&outcome, source, fragment).is_empty(),
        "{:#?}",
        outcome.diagnostics
    );
}

// ----- debug: reported forms ------------------------------------------------

#[test]
fn removed_debug_method_call_is_a_proved_removal() {
    let source = debug_input("    let _request = request.debug(DebugLevel::None);");
    assert_proved_removal(
        &source,
        "request.debug(DebugLevel::None)",
        "debug",
        "bound request",
    );
}

#[test]
fn removed_debug_raw_identifier_is_a_proved_removal() {
    let source = debug_input("    let _request = request.r#debug(DebugLevel::None);");
    assert_proved_removal(
        &source,
        "request.r#debug(DebugLevel::None)",
        "debug",
        "bound request",
    );
}

#[test]
fn removed_debug_on_a_typed_binding_and_parameter_is_a_proved_removal() {
    let source = debug_input(
        "    let typed: removed_member_api::ItemRequest = client.item();\n    let _typed = typed.debug(DebugLevel::Basic);",
    );
    assert_proved_removal(
        &source,
        "typed.debug(DebugLevel::Basic)",
        "debug",
        "bound request",
    );
    let source = format!(
        "{DEBUG_V21}\npub fn parameter(request: removed_member_api::ItemRequest) -> removed_member_api::ItemRequest {{\n    request.debug(DebugLevel::Headers)\n}}\n"
    );
    assert_proved_removal(
        &source,
        "request.debug(DebugLevel::Headers)",
        "debug",
        "bound request",
    );
}

#[test]
fn removed_debug_through_a_box_is_a_proved_removal() {
    // A `self` method is reached through a `Box` (it moves out of it).
    let source = debug_input(
        "    let boxed: Box<removed_member_api::ItemRequest> = Box::new(client.item());\n    let _request = boxed.debug(DebugLevel::None);",
    );
    assert_proved_removal(
        &source,
        "boxed.debug(DebugLevel::None)",
        "debug",
        "bound request",
    );
    // An untyped `Box::new(..)` binding is not resolved by the model: it is
    // still reported, as a possible (not proved) removal.
    let fragment = "untyped.debug(DebugLevel::None)";
    let source = debug_input(&format!(
        "    let untyped = Box::new(client.item());\n    let _request = {fragment};"
    ));
    let outcome = convert(&source);
    let found = removed_at(&outcome, &source, fragment);
    assert_eq!(found.len(), 1, "{:#?}", outcome.diagnostics);
    assert!(
        found[0].message.contains("may reach"),
        "{}",
        found[0].message
    );
}

#[test]
fn removed_debug_type_qualified_forms_are_proved_removals() {
    for call in [
        "removed_member_api::ItemRequest::debug(request, DebugLevel::None)",
        "<removed_member_api::ItemRequest>::debug(request, DebugLevel::None)",
    ] {
        let source = debug_input(&format!("    let _request = {call};"));
        assert_proved_removal(&source, call, "debug", "bound request");
    }
}

#[test]
fn removed_debug_function_value_is_a_proved_removal() {
    let source = debug_input(
        "    let debug = removed_member_api::ItemRequest::debug;\n    let _request = debug(request, DebugLevel::None);",
    );
    assert_proved_removal(
        &source,
        "let debug = removed_member_api::ItemRequest::debug",
        "debug",
        "bound request",
    );
    // The local function value itself is not a generated member path.
    assert_no_removal(&source, "debug(request, DebugLevel::None)");
}

#[test]
fn removed_debug_inside_self_impl_is_a_proved_removal() {
    let source = format!(
        "{DEBUG_V21}\npub trait Quiet: Sized {{ fn quiet(self) -> Self; }}\nimpl Quiet for removed_member_api::ItemRequest {{\n    fn quiet(self) -> Self {{ Self::debug(self, DebugLevel::None) }}\n}}\n"
    );
    assert_proved_removal(
        &source,
        "Self::debug(self, DebugLevel::None)",
        "debug",
        "bound request",
    );
}

#[test]
fn removed_debug_in_an_opaque_macro_is_reported_as_unproved() {
    let fragment = "opaque!(request.debug(DebugLevel::None))";
    let source = debug_input(&format!(
        "    macro_rules! opaque {{ ($value:expr) => {{ $value }}; }}\n    let _request = {fragment};"
    ));
    let outcome = convert(&source);
    assert!(outcome.converted.contains(fragment));
    let found = removed_at(&outcome, &source, fragment);
    assert_eq!(found.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(found[0].severity(), Severity::Error);
    assert!(
        found[0].message.contains("may reach") && found[0].message.contains("does not expand"),
        "{}",
        found[0].message
    );
}

#[test]
fn removed_debug_on_an_unresolved_receiver_keeps_the_conservative_report() {
    // A generic receiver is not modelled: it is not proved to be, or not to
    // be, a generated request, so it is reported as possible, never as a
    // proved removal.
    let fragment = "value.debug(DebugLevel::None)";
    let source =
        format!("{DEBUG_V21}\npub fn generic<T: AppDebug>(value: T) -> T {{\n    {fragment}\n}}\n");
    let outcome = convert(&source);
    assert!(outcome.converted.contains(fragment));
    let found = removed_at(&outcome, &source, fragment);
    assert_eq!(found.len(), 1, "{:#?}", outcome.diagnostics);
    let message = &found[0].message;
    assert_eq!(found[0].severity(), Severity::Error);
    assert!(message.contains("may reach"), "{message}");
    assert!(!message.contains("v2.1 generates `debug` on"), "{message}");
}

#[test]
fn removed_member_check_mode_exits_with_migration_needed() {
    let manifest = "[package]\nname = \"removed\"\nversion = \"0.0.0\"\nedition = \"2024\"\n";
    let status = |body: &str| {
        let crate_dir = TempDir::new("r5-removed-member");
        crate_dir.write("Cargo.toml", manifest);
        crate_dir.write("src/lib.rs", &debug_input(body));
        run_status(&[
            "--input".to_owned(),
            crate_dir.0.display().to_string(),
            "--check".to_owned(),
        ])
    };
    // Control: the same crate without the removed-member call needs nothing.
    assert_eq!(status("    let _request = request;"), 0);
    assert_eq!(
        status("    let _request = request.debug(DebugLevel::None);"),
        1,
        "a removed-member error must fail --check"
    );
}

// ----- debug: not reported --------------------------------------------------

#[test]
fn trait_qualified_debug_calls_are_not_removals() {
    for call in [
        "<removed_member_api::ItemRequest as AppDebug>::debug(request, DebugLevel::None)",
        "AppDebug::debug(request, DebugLevel::None)",
    ] {
        let source = debug_input(&format!("    let _request = {call};"));
        assert_no_removal(&source, call);
    }
}

#[test]
fn runtime_config_builder_debug_still_exists_and_is_not_a_removal() {
    // v3 keeps `RuntimeConfigBuilder::debug(self, DebugLevel)`
    // (crates/concord-core/src/runtime/config.rs).
    let call = "runtime.debug(DebugLevel::Headers)";
    let source = format!(
        "{DEBUG_V21}\npub fn configured() {{\n    let _ = RemovedMemberApi::builder()\n        .configure_runtime(|runtime| {call})\n        .build();\n}}\n"
    );
    assert_no_removal(&source, call);
}

#[test]
fn debug_on_types_proved_not_generated_is_not_a_removal() {
    // A crate-local type with its own inherent `debug`, the unbound intent
    // (v2.1 generated no `debug` on it) and a wrong arity.
    let source = format!(
        "{DEBUG_V21}\nstruct Foreign;\nimpl Foreign {{ fn debug(self, _level: DebugLevel) -> Self {{ self }} }}\npub fn others() {{\n    let foreign: Foreign = Foreign;\n    let _ = foreign.debug(DebugLevel::None);\n    let _ = RemovedMemberApi::unbound().item().debug(DebugLevel::None);\n    let client = RemovedMemberApi::builder().build().expect(\"fixture client\");\n    let _ = client.item().debug();\n}}\n"
    );
    for call in [
        "foreign.debug(DebugLevel::None)",
        "RemovedMemberApi::unbound().item().debug(DebugLevel::None)",
        "client.item().debug()",
    ] {
        assert_no_removal(&source, call);
    }
}

// ----- custom-provider builder setter ----------------------------------------

const PROVIDER_V21: &str = r#"use concord::{api, Empty};
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
"#;

#[test]
fn removed_provider_setter_is_reported_with_its_v3_name() {
    for call in [
        "builder.delegated_provider(provider)",
        "DelegatingBuilder::delegated_provider(builder, provider)",
        "Delegating::builder().delegated_provider(provider)",
    ] {
        let source = format!(
            "{PROVIDER_V21}\npub fn configure(builder: DelegatingBuilder, provider: Provider) -> DelegatingBuilder {{\n    {call}\n}}\n"
        );
        assert_proved_removal(&source, call, "delegated_provider", "client builder");
        let outcome = convert(&source);
        let found = removed_at(&outcome, &source, call);
        let message = &found[0].message;
        assert!(
            message.contains("`delegated(provider)`") && message.contains("declared provider type"),
            "the v3 setter and why it is not renamed automatically: {message}"
        );
        assert!(
            found[0]
                .suggestion
                .as_deref()
                .is_some_and(|suggestion| suggestion.contains("delegated")
                    && !suggestion.contains("delegated_provider")),
            "{:?}",
            found[0].suggestion
        );
        // Not renamed: the argument type narrows in v3.
        assert!(!outcome.converted.contains(".delegated(provider)"));
    }
}

#[test]
fn trait_qualified_provider_setter_is_not_a_removal() {
    let call = "AppProviderSetter::delegated_provider(builder, provider)";
    let source = format!(
        "{PROVIDER_V21}\npub fn configure(builder: DelegatingBuilder, provider: Provider) -> DelegatingBuilder {{\n    {call}\n}}\n"
    );
    assert_no_removal(&source, call);
}

// ----- paginate: owned by the dedicated rule ----------------------------------

const PAGINATE_V21: &str = r#"use std::num::NonZeroU32;
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
pub trait AppPaginate: Sized {
    fn paginate(self, _limit: PageLimit) -> Self { self }
}
impl AppPaginate for pages::ListRequest {}
"#;

/// The paginate-rule, removed-member and shadow codes on `line` (a
/// remaining `PageLimit` reference on the same line is a separate report).
fn member_codes_at(outcome: &SourceOutcome, line: usize) -> Vec<&'static str> {
    outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.code.as_str())
        .filter(|code| {
            code.starts_with("C3-MIGRATE-PAGINATE") || *code == REMOVED || *code == SHADOW
        })
        .collect()
}

#[test]
fn paginate_rule_owns_the_removed_paginate_member() {
    // A site the paginate rule transforms gets its rule diagnostic only.
    let source = format!(
        "{PAGINATE_V21}\npub fn prefix(client: Pages) {{\n    let _ = client.list().paginate(PageLimit::Prefix(3));\n}}\n"
    );
    let outcome = convert(&source);
    assert!(
        outcome
            .converted
            .contains("client.list().collect_prefix(3)"),
        "{}",
        outcome.converted
    );
    let line = unique_line(&source, "client.list().paginate(PageLimit::Prefix(3))");
    let at_line = outcome
        .diagnostics
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        at_line,
        ["C3-MIGRATE-PAGINATE-PREFIX"],
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn unproved_paginate_keeps_one_paginate_unresolved_report() {
    let fragment = "value.paginate(PageLimit::Prefix(3))";
    let source = format!(
        "{PAGINATE_V21}\npub fn generic<T: AppPaginate>(value: T) -> T {{\n    {fragment}\n}}\n"
    );
    let outcome = convert(&source);
    assert!(outcome.converted.contains(fragment));
    let at_line = member_codes_at(&outcome, unique_line(&source, fragment));
    assert_eq!(at_line, [PAGINATE_UNRESOLVED], "{:#?}", outcome.diagnostics);
}

#[test]
fn type_qualified_paginate_is_reported_by_the_paginate_rule() {
    // The rule rewrites only `.paginate(PageLimit::..)` method calls; a
    // type-qualified call or function value of the removed generated
    // `paginate` is reported by the same rule, once, and left unchanged.
    for (body, fragment) in [
        (
            "    let _ = pages::ListRequest::paginate(client.list(), PageLimit::Prefix(3));",
            "pages::ListRequest::paginate(client.list(), PageLimit::Prefix(3))",
        ),
        (
            "    let traverse = pages::ListRequest::paginate;\n    let _ = traverse(client.list(), PageLimit::Prefix(3));",
            "let traverse = pages::ListRequest::paginate",
        ),
    ] {
        let source = format!("{PAGINATE_V21}\npub fn qualified(client: Pages) {{\n{body}\n}}\n");
        let outcome = convert(&source);
        assert!(
            outcome.converted.contains(fragment),
            "{}",
            outcome.converted
        );
        let at_line = member_codes_at(&outcome, unique_line(&source, fragment));
        assert_eq!(at_line, [PAGINATE_UNRESOLVED], "{:#?}", outcome.diagnostics);
    }
}

// ----- R5-01: removed members referenced inside unexpanded macros -------------

const PASS: &str = "    macro_rules! pass { ($value:expr) => { $value }; }";

/// One scanner-form coverage row: the disposition, the entry form, the
/// statements holding the site, the fragment on the site's line, and the
/// member codes expected exactly on that line (none for a control).
struct FormRow {
    disposition: &'static str,
    form: &'static str,
    body: &'static str,
    fragment: &'static str,
    expected: &'static [&'static str],
}

fn form_source(disposition: &str, body: &str) -> String {
    match disposition {
        "debug" => debug_input(&format!("{PASS}\n{body}")),
        "provider setter" => format!(
            "{PROVIDER_V21}\npub fn configure(builder: DelegatingBuilder, provider: Provider) {{\n{PASS}\n{body}\n}}\n"
        ),
        "paginate" => {
            format!("{PAGINATE_V21}\npub fn traverse(client: Pages) {{\n{PASS}\n{body}\n}}\n")
        }
        other => panic!("unknown disposition {other}"),
    }
}

/// Every supported removed-member disposition crossed with every entry form
/// the migrator must see: a direct call inside an unexpanded macro (method
/// and type-qualified), a qualified function value inside it, a grouped
/// `(Type::member)(..)` call inside it, and the function value outside
/// macros, plus controls. A function-value reference silently skipped by the
/// macro scanner fails its row.
#[test]
fn removed_member_scanner_form_coverage() {
    let rows = [
        // debug (manual decision)
        FormRow {
            disposition: "debug",
            form: "method call in macro",
            body: "    let _ = pass!(request.debug(DebugLevel::None));",
            fragment: "pass!(request.debug(DebugLevel::None))",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "qualified call in macro",
            body: "    let _ = pass!(removed_member_api::ItemRequest::debug(request, DebugLevel::None));",
            fragment: "pass!(removed_member_api::ItemRequest::debug(",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "qualified function value in macro",
            body: "    let apply = pass!(removed_member_api::ItemRequest::debug);\n    let _ = apply(request, DebugLevel::None);",
            fragment: "pass!(removed_member_api::ItemRequest::debug)",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "<T> function value in macro",
            body: "    let apply = pass!(<removed_member_api::ItemRequest>::debug);\n    let _ = apply(request, DebugLevel::None);",
            fragment: "pass!(<removed_member_api::ItemRequest>::debug)",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "grouped call in macro",
            body: "    let _ = pass!((removed_member_api::ItemRequest::debug)(request, DebugLevel::None));",
            fragment: "pass!((removed_member_api::ItemRequest::debug)(",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "function value outside macros",
            body: "    let apply = removed_member_api::ItemRequest::debug;\n    let _ = apply(request, DebugLevel::None);",
            fragment: "let apply = removed_member_api::ItemRequest::debug;",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "debug",
            form: "control: trait-qualified value in macro",
            body: "    let _apply = pass!(<removed_member_api::ItemRequest as AppDebug>::debug);",
            fragment: "as AppDebug>::debug)",
            expected: &[],
        },
        FormRow {
            disposition: "debug",
            form: "control: stringify!",
            body: "    let _ = stringify!(removed_member_api::ItemRequest::debug);",
            fragment: "stringify!(",
            expected: &[],
        },
        FormRow {
            disposition: "debug",
            form: "control: nested log::debug!",
            body: "    pass!(log::debug!(\"{:?}\", DebugLevel::None));",
            fragment: "pass!(log::debug!(",
            expected: &[],
        },
        // custom-provider builder setter (manual decision)
        FormRow {
            disposition: "provider setter",
            form: "method call in macro",
            body: "    let _ = pass!(builder.delegated_provider(provider));",
            fragment: "pass!(builder.delegated_provider(provider))",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "provider setter",
            form: "qualified call in macro",
            body: "    let _ = pass!(DelegatingBuilder::delegated_provider(builder, provider));",
            fragment: "pass!(DelegatingBuilder::delegated_provider(",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "provider setter",
            form: "qualified function value in macro",
            body: "    let apply = pass!(DelegatingBuilder::delegated_provider);\n    let _ = apply(builder, provider);",
            fragment: "pass!(DelegatingBuilder::delegated_provider)",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "provider setter",
            form: "grouped call in macro",
            body: "    let _ = pass!((DelegatingBuilder::delegated_provider)(builder, provider));",
            fragment: "pass!((DelegatingBuilder::delegated_provider)(",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "provider setter",
            form: "function value outside macros",
            body: "    let apply = DelegatingBuilder::delegated_provider;\n    let _ = apply(builder, provider);",
            fragment: "let apply = DelegatingBuilder::delegated_provider;",
            expected: &[REMOVED],
        },
        FormRow {
            disposition: "provider setter",
            form: "control: trait-qualified value in macro",
            body: "    let _apply = pass!(AppProviderSetter::delegated_provider);\n    let _ = (builder, provider);",
            fragment: "pass!(AppProviderSetter::delegated_provider)",
            expected: &[],
        },
        // paginate (dedicated paginate rule)
        FormRow {
            disposition: "paginate",
            form: "method call in macro",
            body: "    let _ = pass!(client.list().paginate(PageLimit::Prefix(3)));",
            fragment: "pass!(client.list().paginate(",
            expected: &[PAGINATE_UNRESOLVED],
        },
        FormRow {
            disposition: "paginate",
            form: "qualified call in macro",
            body: "    let _ = pass!(pages::ListRequest::paginate(client.list(), PageLimit::Prefix(3)));",
            fragment: "pass!(pages::ListRequest::paginate(",
            expected: &[PAGINATE_UNRESOLVED],
        },
        FormRow {
            disposition: "paginate",
            form: "qualified function value in macro",
            body: "    let apply = pass!(pages::ListRequest::paginate);\n    let _ = apply(client.list(), PageLimit::Prefix(3));",
            fragment: "pass!(pages::ListRequest::paginate)",
            expected: &[PAGINATE_UNRESOLVED],
        },
        FormRow {
            disposition: "paginate",
            form: "grouped call in macro",
            body: "    let _ = pass!((pages::ListRequest::paginate)(client.list(), PageLimit::Prefix(3)));",
            fragment: "pass!((pages::ListRequest::paginate)(",
            expected: &[PAGINATE_UNRESOLVED],
        },
        FormRow {
            disposition: "paginate",
            form: "function value outside macros",
            body: "    let apply = pages::ListRequest::paginate;\n    let _ = apply(client.list(), PageLimit::Prefix(3));",
            fragment: "let apply = pages::ListRequest::paginate;",
            expected: &[PAGINATE_UNRESOLVED],
        },
        FormRow {
            disposition: "paginate",
            form: "control: trait-qualified value in macro",
            body: "    let _apply = pass!(<pages::ListRequest as AppPaginate>::paginate);\n    let _ = client;",
            fragment: "as AppPaginate>::paginate)",
            expected: &[],
        },
    ];
    let mut failures = Vec::new();
    println!(
        "{:<16} | {:<40} | expected -> actual",
        "disposition", "form"
    );
    for row in &rows {
        let source = form_source(row.disposition, row.body);
        let outcome = convert(&source);
        let actual = member_codes_at(&outcome, unique_line(&source, row.fragment));
        // The member reference itself stays (the generated builder's type
        // path outside macros moves into the client's module).
        let unchanged = outcome.converted.contains(row.fragment)
            || outcome.converted.contains(
                &row.fragment
                    .replace("DelegatingBuilder", "delegating::DelegatingBuilder"),
            );
        println!(
            "{:<16} | {:<40} | {:?} -> {:?}{}",
            row.disposition,
            row.form,
            row.expected,
            actual,
            if unchanged { "" } else { " (REWRITTEN)" }
        );
        if actual != row.expected || !unchanged {
            failures.push(format!(
                "{} / {}: expected {:?}, got {:?}, unchanged: {unchanged}",
                row.disposition, row.form, row.expected, actual
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "scanner-form coverage failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn removed_member_macro_references_are_not_blanket_debug_tokens() {
    // A bare `debug` token, a field access `.debug` without a call (also as
    // a grouped callee), a module path segment named `debug` and a nested
    // macro named `debug` are not references to the removed generated method.
    for (body, fragment) in [
        (
            "    let debug = 1;\n    let _ = pass!(debug);",
            "pass!(debug)",
        ),
        ("    let _ = pass!(request.debug);", "pass!(request.debug)"),
        (
            "    let _ = pass!((request.debug)(DebugLevel::None));",
            "pass!((request.debug)(",
        ),
        (
            "    let _ = pass!(crate::debug::helper(request));",
            "pass!(crate::debug::helper",
        ),
        (
            "    pass!(tracing::debug!(\"x\"));",
            "pass!(tracing::debug!(",
        ),
    ] {
        let source = debug_input(&format!("{PASS}\n{body}"));
        let outcome = convert(&source);
        assert!(outcome.converted.contains(fragment));
        assert_eq!(
            member_codes_at(&outcome, unique_line(&source, fragment)),
            Vec::<&str>::new(),
            "{fragment}: {:#?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn removed_member_macro_function_value_keeps_the_conservative_wording() {
    // Inside an unexpanded macro a path is not a proved call: the report is
    // the possible ("may reach") form with the macro as the reason, also for
    // a raw identifier, a `Self::` path in an impl of the generated type and
    // a grouped path in a `macro_rules!` body.
    for (source, fragment) in [
        (
            debug_input(&format!(
                "{PASS}\n    let apply = pass!(removed_member_api::ItemRequest::r#debug);\n    let _ = apply(request, DebugLevel::None);"
            )),
            "pass!(removed_member_api::ItemRequest::r#debug)",
        ),
        (
            format!(
                "{DEBUG_V21}\n{PASS}\npub trait Quiet: Sized {{ fn quiet(self) -> Self; }}\nimpl Quiet for removed_member_api::ItemRequest {{\n    fn quiet(self) -> Self {{ let apply = pass!(Self::debug); apply(self, DebugLevel::None) }}\n}}\n"
            ),
            "pass!(Self::debug)",
        ),
        (
            format!(
                "{DEBUG_V21}\nmacro_rules! quiet {{ ($r:expr) => {{ (crate::removed_member_api::ItemRequest::debug)($r, DebugLevel::None) }}; }}\n"
            ),
            "(crate::removed_member_api::ItemRequest::debug)",
        ),
    ] {
        let outcome = convert(&source);
        assert!(outcome.converted.contains(fragment));
        let found = removed_at(&outcome, &source, fragment);
        assert_eq!(found.len(), 1, "{fragment}: {:#?}", outcome.diagnostics);
        assert_eq!(found[0].severity(), Severity::Error);
        assert!(
            found[0].message.contains("may reach") && found[0].message.contains("does not expand"),
            "{}",
            found[0].message
        );
    }
}
