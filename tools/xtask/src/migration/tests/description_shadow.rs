//! R3-01 reviewer-authored regressions (`regressions/description_shadow.patch`
//! of the R3 review), applied unchanged apart from this header, `cargo fmt`
//! and one marked fixture repair (the foreign-type control's receiver), plus
//! R4 additions at the end: the bound request's v3-only `description(&self)`
//! must not silently displace an application trait method of that name.
use super::*;

const DESCRIPTION_SHADOW_CODE: &str = "C3-MIGRATE-GENERATED-NAME-SHADOW";

const DESCRIPTION_V21: &str = r###"use concord::{api, Empty};
use std::sync::atomic::{AtomicUsize, Ordering};

static APPLICATION_DESCRIPTION_CALLS: AtomicUsize = AtomicUsize::new(0);

api! {
    client DescriptionApi {
        base "https://review.example.test/";
        timeout 5s;
        GET item() -> () via Empty {
            path ["item"];
        }
    }
}

pub trait AppDescription {
    fn description(&self) {
        APPLICATION_DESCRIPTION_CALLS.fetch_add(1, Ordering::SeqCst);
    }
}
impl AppDescription for description_api::ItemRequest {}
"###;

fn description_input(body: &str) -> String {
    format!(
        "{DESCRIPTION_V21}\npub fn application() {{\n    let client = DescriptionApi::builder().build().expect(\"fixture client\");\n    let request = client.item();\n{body}\n}}\n"
    )
}

fn require_description_shadow(body: &str, call: &str) {
    let source = description_input(body);
    let outcome = convert(&source);
    assert!(
        outcome.converted.contains(call),
        "custom call should remain unchanged"
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == DESCRIPTION_SHADOW_CODE
                && d.line == unique_line(&source, call)),
        "a newly generated bound description() must not displace the application trait without a diagnostic: {:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r3_description_method_shadow_is_reported() {
    require_description_shadow(
        "    let _ = request.description();",
        "request.description()",
    );
}

#[test]
fn review_r3_description_ufcs_shadow_is_reported() {
    require_description_shadow(
        "    let _ = description_api::ItemRequest::description(&request);",
        "description_api::ItemRequest::description(&request)",
    );
}

#[test]
fn review_r3_description_function_value_shadow_is_reported() {
    require_description_shadow(
        "    let describe = description_api::ItemRequest::description;\n    let _ = describe(&request);",
        "let describe = description_api::ItemRequest::description",
    );
}

#[test]
fn review_r3_description_opaque_macro_shadow_is_reported() {
    require_description_shadow(
        "    macro_rules! opaque { ($expression:expr) => { $expression }; }\n    let _ = opaque!(request.description());",
        "opaque!(request.description())",
    );
}

#[test]
fn review_r3_description_explicit_trait_call_is_not_shadowed() {
    let call = "<description_api::ItemRequest as AppDescription>::description(&request)";
    let source = description_input(&format!("    let _ = {call};"));
    let outcome = convert(&source);
    assert!(outcome.converted.contains(call));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|d| d.code.as_str() != DESCRIPTION_SHADOW_CODE),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r3_description_unbound_existing_method_is_not_shadowed() {
    let call = "DescriptionApi::unbound().item().description()";
    let source = description_input(&format!("    let _ = {call};"));
    let outcome = convert(&source);
    assert!(outcome.converted.contains(call));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|d| d.code.as_str() != DESCRIPTION_SHADOW_CODE),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r3_description_foreign_inherent_method_is_untouched() {
    // FIXTURE REPAIR (R4): the proposal called `Foreign.description()` on the
    // unit-struct value expression `Foreign`. The tool does not resolve a
    // unit-struct value path to its type, so that receiver is *unresolved*
    // and, like every other v3-only name (`Foreign.control_summary()` at
    // 2dd373a), is conservatively reported as a possible shadow (see
    // `description_on_an_unresolved_unit_struct_value_stays_conservative`).
    // The intent -- a crate-local foreign type with its own inherent
    // `description` is not reported -- is kept with a receiver the tool
    // resolves to that type (a typed binding).
    let source = format!(
        "{DESCRIPTION_V21}\nstruct Foreign;\nimpl Foreign {{ fn description(&self) -> u8 {{ 7 }} }}\npub fn foreign() -> u8 {{ let foreign: Foreign = Foreign; foreign.description() }}\n"
    );
    let outcome = convert(&source);
    assert!(outcome.converted.contains("foreign.description()"));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|d| d.code.as_str() != DESCRIPTION_SHADOW_CODE),
        "{:#?}",
        outcome.diagnostics
    );
}

// ----- R4 additions ----------------------------------------------------------

/// The unrepaired reviewer form: `Foreign` as a unit-struct value expression
/// is an unresolved receiver, which keeps the conservative unresolved-receiver
/// behavior of every v3-only name (a possible shadow, reported, not rewritten).
#[test]
fn description_on_an_unresolved_unit_struct_value_stays_conservative() {
    let call = "Foreign.description()";
    let source = format!(
        "{DESCRIPTION_V21}\nstruct Foreign;\nimpl Foreign {{ fn description(&self) -> u8 {{ 7 }} }}\npub fn foreign() -> u8 {{ {call} }}\n"
    );
    let outcome = convert(&source);
    assert!(outcome.converted.contains(call));
    let reported = outcome
        .diagnostics
        .iter()
        .filter(|d| d.code.as_str() == DESCRIPTION_SHADOW_CODE)
        .collect::<Vec<_>>();
    assert_eq!(reported.len(), 1, "{reported:#?}");
    assert_eq!(reported[0].line, unique_line(&source, call));
    assert!(
        reported[0].message.contains("may reach") && reported[0].message.contains("not resolved"),
        "{}",
        reported[0].message
    );
}

/// `description` with an argument cannot select the zero-argument generated
/// method: it fails to compile in v3 instead of changing behavior silently.
#[test]
fn description_with_arguments_is_not_a_silent_shadow() {
    let source = description_input("    let _ = request.description(1);");
    let outcome = convert(&source);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|d| d.code.as_str() != DESCRIPTION_SHADOW_CODE),
        "{:#?}",
        outcome.diagnostics
    );
}

/// Through a reference, auto-deref reaches the `&self` generated method too.
#[test]
fn description_through_a_reference_is_reported() {
    require_description_shadow(
        "    let borrowed = &request;\n    let _ = borrowed.description();",
        "borrowed.description()",
    );
}

/// The builder of a client with a custom credential provider: v3 names its
/// setter after the credential (v2.1: `<credential>_provider`), a new
/// inherent builder method the parity guard found missing from the inventory.
#[test]
fn custom_provider_builder_setter_shadow_is_reported() {
    let source = r#"use concord::{api, Empty};
api! {
    client Delegating {
        base "https://review.example.test/";
        timeout 5s;
        credentials {
            credential delegated = custom crate::Provider;
        }
        GET item() -> () via Empty {
            path ["item"];
            auth { bearer delegated; }
        }
    }
}
pub struct Provider;
pub trait AppDelegated {
    fn delegated(self, provider: Provider) -> Self;
}
impl AppDelegated for DelegatingBuilder {
    fn delegated(self, provider: Provider) -> Self { self.delegated_provider(provider) }
}
pub fn configure(builder: DelegatingBuilder) -> DelegatingBuilder {
    builder.delegated(Provider)
}
"#;
    let outcome = convert(source);
    let call = "builder.delegated(Provider)";
    assert!(outcome.converted.contains(call), "{}", outcome.converted);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == DESCRIPTION_SHADOW_CODE
                && d.line == unique_line(source, call)),
        "{:#?}",
        outcome.diagnostics
    );
}
