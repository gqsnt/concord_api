//! Reviewer-proposed migration regressions. Not compiled or run by this review.
//! Install as tools/xtask/src/migration/tests/suffix_names.rs in a scratch tree.

use super::*;

const SUFFIX_API: &str = r#"use concord::{api, Empty};
use concord::atom::{AtomEncoder, AtomError, BoundedAtomWriter};
pub struct InnerOption;
impl AtomEncoder<Option<u8>> for InnerOption {
    fn write(value: &Option<u8>, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str("none"),
        }
    }
}
api! {
    client SuffixApi {
        base "https://review.example.test/";
        timeout 5s;
        GET item(x_if_some?: Option<u8>, ordinary?: Option<u8>) -> () via Empty {
            path ["item"];
            query {
                "x" = endpoint.x_if_some via InnerOption;
                "ordinary" = endpoint.ordinary via InnerOption;
            }
        }
    }
}
"#;

#[test]
fn review_suffix_direct_and_raw_exact_setters_migrate() {
    let outcome = convert(&format!(
        "{SUFFIX_API}{}",
        r#"
pub fn direct(client: &SuffixApi) { let _ = client.item().set_x_if_some(None); }
pub fn raw(client: &SuffixApi) { let _ = client.item().r#set_x_if_some(None); }
"#
    ));
    assert!(
        outcome.converted.contains(".replace_x_if_some(None)"),
        "a declared value suffix is not proof of a v3 conditional setter"
    );
    assert!(
        outcome.converted.contains(".r#replace_x_if_some(None)"),
        "raw exact setter must preserve the same slot operation"
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
}

#[test]
fn review_suffix_qualified_exact_setters_migrate() {
    let outcome = convert(&format!(
        "{SUFFIX_API}{}",
        r#"
pub fn qualified(request: suffix_api::ItemRequest) {
    let _ = suffix_api::ItemRequest::set_x_if_some(request, None);
}
pub fn qualified_type(request: suffix_api::ItemRequest) {
    let _ = <suffix_api::ItemRequest>::set_x_if_some(request, None);
}
"#
    ));
    assert!(
        outcome
            .converted
            .contains("ItemRequest::replace_x_if_some(request, None)")
    );
    assert!(
        outcome
            .converted
            .contains("ItemRequest>::replace_x_if_some(request, None)")
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
}

#[test]
fn review_suffix_setter_function_value_is_reported() {
    let outcome = convert(&format!(
        "{SUFFIX_API}{}",
        r#"
pub fn function_value(request: suffix_api::ItemRequest) {
    let exact = suffix_api::ItemRequest::set_x_if_some;
    let _ = exact(request, None);
}
"#
    ));
    assert_eq!(
        count(&outcome, Code::SetterUnresolved),
        1,
        "unrewritten exact-setter function values must not silently disappear"
    );
    assert!(outcome.converted.contains("ItemRequest::set_x_if_some;"));
    assert_eq!(count(&outcome, Code::SetterReplace), 0);
}

#[test]
fn review_suffix_opaque_macro_call_is_reported() {
    let outcome = convert(&format!(
        "{SUFFIX_API}{}",
        r#"
macro_rules! passthrough { ($e:expr) => { $e }; }
pub fn opaque(client: &SuffixApi) {
    let _ = passthrough!(client.item().set_x_if_some(None));
}
"#
    ));
    assert_eq!(
        count(&outcome, Code::SetterUnresolved),
        1,
        "the suffix must not suppress the unresolved-site diagnostic either"
    );
    assert!(outcome.converted.contains(".set_x_if_some(None)"));
    assert_eq!(count(&outcome, Code::SetterReplace), 0);
}

#[test]
fn review_suffix_ordinary_name_positive_control_migrates() {
    let outcome = convert(&format!(
        "{SUFFIX_API}{}",
        "pub fn ordinary(client: &SuffixApi) { let _ = client.item().set_ordinary(None); }\n"
    ));
    assert!(outcome.converted.contains(".replace_ordinary(None)"));
    assert_eq!(count(&outcome, Code::SetterReplace), 1);
}

#[test]
fn review_suffix_foreign_type_is_still_not_rewritten() {
    let callers = r#"
pub struct Foreign;
impl Foreign {
    pub fn set_x_if_some(self, _: Option<u8>) -> u32 { 1 }
    pub fn replace_x_if_some(self, _: Option<u8>) -> u32 { 2 }
}
pub fn foreign(request: Foreign) -> u32 { request.set_x_if_some(None) }
"#;
    let outcome = convert(&format!("{SUFFIX_API}{callers}"));
    assert!(outcome.converted.contains("request.set_x_if_some(None)"));
    assert_eq!(count(&outcome, Code::SetterReplace), 0);
}

// Implementer's focused pass (R3 B01): every declared owner kind, an optional
// body, a repeated suffix and a conditional-shaped call that is not the exact
// setter of any declared value. Candidates are classified by the v2.1
// declared value and its generated method role, never by the suffix.

/// Root (client builder) value, scope parameter, endpoint parameter and
/// optional endpoint body, each named with the `_if_some` suffix, plus a
/// repeated suffix and an ordinary value `plain` without a `plain_if_some`.
const SUFFIX_OWNERS_API: &str = r#"use concord::api;
api! {
    client Owners {
        base "https://review.example.test/";
        timeout 5s;
        values {
            trace_if_some?: String;
        }
        headers { "x-trace" = client.trace_if_some; }
        scope regional(zone_if_some?: String) {
            query { "zone" = regional.zone_if_some; }
            GET item(x_if_some_if_some?: String, plain?: String) -> String {
                query {
                    "x" = endpoint.x_if_some_if_some;
                    "plain" = endpoint.plain;
                }
            }
            POST upload(body payload_if_some?: String) -> String;
        }
    }
}
"#;

#[test]
fn review_suffix_root_and_scope_values_migrate_by_declared_role() {
    let outcome = convert(&format!(
        "{SUFFIX_OWNERS_API}{}",
        r#"
pub fn root() -> Owners {
    Owners::builder().set_trace_if_some(None).build().expect("client")
}
pub fn scope(client: &Owners) {
    let _ = client.regional().set_zone_if_some(None).item();
}
"#
    ));
    assert!(
        outcome
            .converted
            .contains("Owners::builder().replace_trace_if_some(None).build()"),
        "{}",
        outcome.converted
    );
    assert!(
        outcome
            .converted
            .contains("client.regional().replace_zone_if_some(None).item()"),
        "{}",
        outcome.converted
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 0);
}

#[test]
fn review_suffix_optional_body_and_repeated_suffix_migrate() {
    let outcome = convert(&format!(
        "{SUFFIX_OWNERS_API}{}",
        r#"
pub fn body(client: &Owners) {
    let _ = client.regional().upload().set_payload_if_some(None);
}
pub fn repeated(client: &Owners) {
    let _ = client.regional().item().set_x_if_some_if_some(None);
}
"#
    ));
    assert!(
        outcome
            .converted
            .contains("client.regional().upload().replace_payload_if_some(None)"),
        "{}",
        outcome.converted
    );
    assert!(
        outcome
            .converted
            .contains("client.regional().item().replace_x_if_some_if_some(None)"),
        "{}",
        outcome.converted
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 0);
}

/// v2.1 generated no `_if_some` method at all: for the v2.1 value `plain`
/// (no `plain_if_some` is declared) and for the shorter spelling of
/// `x_if_some_if_some`, a conditional-shaped call is not the exact setter of
/// any declared value. It is neither rewritten nor reported as a setter, on a
/// proved receiver, a qualified path, a function value or inside an opaque
/// macro.
#[test]
fn review_suffix_conditional_shaped_call_of_undeclared_value_is_untouched() {
    let callers = r#"
macro_rules! passthrough { ($e:expr) => { $e }; }
pub fn conditional(client: &Owners, request: owners::regional::ItemRequest) {
    let _ = client.regional().item().set_plain_if_some(Some(String::new()));
    let _ = client.regional().item().set_x_if_some(None);
    let _ = owners::regional::ItemRequest::set_plain_if_some(request, None);
    let exact = owners::regional::ItemRequest::set_plain_if_some;
    let _ = passthrough!(client.regional().item().set_plain_if_some(None));
    let _ = exact;
}
"#;
    let outcome = convert(&format!("{SUFFIX_OWNERS_API}{callers}"));
    assert!(
        outcome.converted.ends_with(callers),
        "caller code must be unchanged:\n{}",
        outcome.converted
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 0);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 0);
    assert_eq!(count(&outcome, Code::SetterAmbiguous), 0);
}

/// An unresolved receiver of a declared suffix value's exact setter is
/// reported (default deny), not guessed.
#[test]
fn review_suffix_unresolved_receiver_is_reported_not_rewritten() {
    let callers = r#"
pub fn opaque(request: impl Sized) { let _ = unknown_type(request).set_zone_if_some(None); }
"#;
    let outcome = convert(&format!("{SUFFIX_OWNERS_API}{callers}"));
    assert!(
        outcome.converted.ends_with(callers),
        "{}",
        outcome.converted
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 0);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 1);
}
