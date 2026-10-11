//! Reviewer-authored migration regressions. NOT compiled or run here.
//! A removed inherent member can expose an application trait method. The
//! converter must diagnose the loss; it must not pick the user's replacement.
use super::*;

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
"#;

fn with_body(body: &str) -> String {
    format!(
        "{V21}\npub fn application() {{\nlet client = RemovedMemberApi::builder().build().expect(\"fixture client\");\nlet request = client.item();\n{body}\n}}\n"
    )
}

fn assert_reported(body: &str, call: &str) {
    let source = with_body(body);
    let outcome = convert(&source);
    assert!(
        outcome.converted.contains(call),
        "do not guess a replacement"
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.line == unique_line(&source, call)
                && d.severity() == Severity::Error
                && d.message.contains("debug")),
        "a removed generated method must not silently change method dispatch: {:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r4_removed_debug_method_is_reported() {
    assert_reported(
        "let _request = request.debug(DebugLevel::None);",
        "request.debug(DebugLevel::None)",
    );
}

#[test]
fn review_r4_removed_debug_raw_method_is_reported() {
    assert_reported(
        "let _request = request.r#debug(DebugLevel::None);",
        "request.r#debug(DebugLevel::None)",
    );
}

#[test]
fn review_r4_removed_debug_ufcs_is_reported() {
    assert_reported(
        "let _request = removed_member_api::ItemRequest::debug(request, DebugLevel::None);",
        "removed_member_api::ItemRequest::debug(request, DebugLevel::None)",
    );
}

#[test]
fn review_r4_removed_debug_function_value_is_reported() {
    assert_reported(
        "let debug = removed_member_api::ItemRequest::debug;\nlet _request = debug(request, DebugLevel::None);",
        "let debug = removed_member_api::ItemRequest::debug",
    );
}

#[test]
fn review_r4_removed_debug_in_opaque_macro_is_reported() {
    assert_reported(
        "macro_rules! opaque { ($value:expr) => { $value }; }\nlet _request = opaque!(request.debug(DebugLevel::None));",
        "opaque!(request.debug(DebugLevel::None))",
    );
}

#[test]
fn review_r4_explicit_application_trait_call_is_not_reported() {
    let call = "<removed_member_api::ItemRequest as AppDebug>::debug(request, DebugLevel::None)";
    let source = with_body(&format!("let _request = {call};"));
    let outcome = convert(&source);
    assert!(outcome.converted.contains(call));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|d| d.line != unique_line(&source, call)),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn review_r4_foreign_debug_and_existing_runtime_debug_stay_untouched() {
    let source = format!(
        r#"{V21}
struct Foreign;
impl Foreign {{ fn debug(self, _level: DebugLevel) -> Self {{ self }} }}
pub fn foreign() {{
    let foreign: Foreign = Foreign;
    let _ = foreign.debug(DebugLevel::None);
}}
pub fn configured_client() {{
    let _ = RemovedMemberApi::builder()
        .configure_runtime(|runtime| runtime.debug(DebugLevel::None))
        .build().expect("fixture client");
}}
"#
    );
    let outcome = convert(&source);
    for call in [
        "foreign.debug(DebugLevel::None)",
        "runtime.debug(DebugLevel::None)",
    ] {
        assert!(outcome.converted.contains(call));
        assert!(
            outcome
                .diagnostics
                .iter()
                .all(|d| d.line != unique_line(&source, call)),
            "{:#?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn review_r4_removed_provider_setter_side_of_rename_is_accounted_for() {
    // Token-level migration fixture, as in R4's existing provider shadow test.
    // It does not purport to implement the runtime CredentialProvider role.
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
pub fn configure(builder: DelegatingBuilder, provider: Provider) -> DelegatingBuilder {
    builder.delegated_provider(provider)
}
"#;
    let call = "builder.delegated_provider(provider)";
    let outcome = convert(source);
    // A selected, justified rename may be automated; otherwise explicitly report
    // the removed-member choice. Leaving this call silently unchanged is not enough.
    let safely_renamed = outcome.converted.contains("builder.delegated(provider)");
    let reported = outcome.diagnostics.iter().any(|d| {
        d.line == unique_line(source, call)
            && d.severity() == Severity::Error
            && d.message.contains("delegated_provider")
    });
    assert!(
        safely_renamed || reported,
        "the old side of a generated provider-setter rename was silently lost: {:#?}",
        outcome.diagnostics
    );
}
