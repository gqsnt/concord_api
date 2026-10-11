//! Focused review of R3 B01 (D1, N1): names that only the v3 generator
//! defines.
//!
//! v3 generates inherent methods that v2.1 did not (`replace_x` and the
//! conditional `set_x_if_some` for a declared optional value `x`, the
//! conditional setter of an initialized or client value, `bind` and
//! `plan_runtime` on client and scope handles, `cancel_with`, `with_options`,
//! `control_summary`, `description` (R3-01) and, for a paginated endpoint,
//! `pages`, `collect_prefix` and `collect_all` on a request; the provider
//! setter of a custom credential on the builder). An application trait implemented for a
//! generated v2.1 type may already provide a method of such a name; in v3 the
//! inherent method wins method resolution, so an unchanged call would silently
//! call the generated method instead. Such calls are reported, never
//! rewritten, and never silent. A v2.1 declaration whose values produce
//! colliding v3 names is reported at the declaration.

use super::*;

/// The reviewer's D1 repro (`differential/ext_trait_shadow_v21.rs`) as a v2.1
/// source: `ClearOnNone::set_x_if_some` clears on `None`.
const EXT_TRAIT_SHADOW: &str = r#"//! v2.1 input: an application extension trait on the generated request that
//! hand-rolls a conditional setter named like the v3 generated one.
use concord::{api, Empty};
api! {
    client XOnly {
        base "https://review.example.test/";
        timeout 5s;
        GET item(x?: u8) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}
pub trait ClearOnNone {
    fn set_x_if_some(self, v: Option<u8>) -> Self;
}
impl ClearOnNone for x_only::ItemRequest {
    // Application semantics: `None` clears (outer absence), `Some` sets.
    fn set_x_if_some(self, v: Option<u8>) -> Self { self.set_x(v) }
}
pub fn application_trait_clears_on_none() -> String {
    let client = XOnly::builder().build().expect("fixture client");
    let plan = client.item().set_x(Some(3)).set_x_if_some(None).plan().expect("plan");
    plan.observed_url().to_owned()
}
"#;

const SHADOW: &str = "C3-MIGRATE-GENERATED-NAME-SHADOW";
const COLLISION: &str = "C3-MIGRATE-GENERATED-NAME-COLLISION";

/// `(line, message)` of every diagnostic with `code`, sorted by line.
fn sites(outcome: &SourceOutcome, code: &str) -> Vec<(usize, String)> {
    let mut found = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == code)
        .map(|diagnostic| {
            assert_eq!(
                diagnostic.severity(),
                Severity::Error,
                "{code} is an unautomatable site"
            );
            (diagnostic.line, diagnostic.message.clone())
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

fn lines(outcome: &SourceOutcome, code: &str) -> Vec<usize> {
    sites(outcome, code)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn review_d1_extension_trait_conditional_setter_is_reported_not_silent() {
    let outcome = convert(EXT_TRAIT_SHADOW);
    let call = ".set_x_if_some(None).plan()";
    assert!(
        outcome
            .converted
            .contains(&format!("client.item().replace_x(Some(3)){call}")),
        "the call is left unchanged (only the proved v2.1 setter is migrated):\n{}",
        outcome.converted
    );
    assert!(
        outcome
            .converted
            .contains("fn set_x_if_some(self, v: Option<u8>) -> Self { self.replace_x(v) }")
    );
    let found = sites(&outcome, SHADOW);
    assert_eq!(
        found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![unique_line(EXT_TRAIT_SHADOW, call)],
        "the call whose meaning v3 changes must be reported: {:#?}",
        outcome.diagnostics
    );
    let message = &found[0].1;
    for expected in [
        "`set_x_if_some`",
        "`ClearOnNone`",
        "conditional setter of the optional value `x`",
        "inherent",
        "left unchanged",
    ] {
        assert!(message.contains(expected), "`{expected}` in: {message}");
    }
}

#[test]
fn review_probe_extension_trait_replace_x_is_reported() {
    let source = format!(
        "{}{}",
        EXT_TRAIT_SHADOW
            .split("pub trait ClearOnNone")
            .next()
            .expect("declaration"),
        r#"pub trait Rep { fn replace_x(self, v: Option<u8>) -> u32; }
impl Rep for x_only::ItemRequest { fn replace_x(self, _: Option<u8>) -> u32 { 5 } }
pub fn ext(c: &XOnly) -> u32 { c.item().replace_x(None) }
"#
    );
    let outcome = convert(&source);
    assert!(outcome.converted.contains("c.item().replace_x(None)"));
    let found = sites(&outcome, SHADOW);
    assert_eq!(
        found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![unique_line(&source, "c.item().replace_x(None)")],
        "{:#?}",
        outcome.diagnostics
    );
    assert!(found[0].1.contains("`Rep`"), "{}", found[0].1);
    assert!(
        found[0]
            .1
            .contains("exact replacement of the optional value `x`"),
        "{}",
        found[0].1
    );
}

/// Every call form that may reach the v3 inherent method is reported: a
/// method call on a proved or unknown receiver, a type-qualified call
/// (`Type::m(..)` and `<Type>::m(..)` find inherent functions first, also
/// `Self::m`), a function value, and a macro the tool does not expand. A
/// trait-qualified call, a receiver proved not to be generated, and a call
/// whose arity the generated method cannot accept (a compile error in v3,
/// not a silent change) are not.
#[test]
fn generated_name_shadow_call_forms() {
    let declaration = EXT_TRAIT_SHADOW
        .split("pub fn application_trait_clears_on_none")
        .next()
        .expect("declaration");
    let callers = r#"pub mod unrelated {
    pub struct ItemRequest;
    impl ItemRequest { pub fn set_x_if_some(self, _: Option<u8>) -> u32 { 1 } }
}
macro_rules! custom { ($e:expr) => { $e }; }
pub fn method(r: x_only::ItemRequest) -> x_only::ItemRequest { r.set_x_if_some(None) } // shadow: method
pub fn boxed(r: Box<x_only::ItemRequest>) -> x_only::ItemRequest { r.set_x_if_some(None) } // shadow: boxed
pub fn shared(r: std::sync::Arc<x_only::ItemRequest>) -> u8 { r.set_x_if_some(None) } // a `self` method is not reached through `Arc`
pub fn typed(r: x_only::ItemRequest) -> x_only::ItemRequest { x_only::ItemRequest::set_x_if_some(r, None) } // shadow: type path
pub fn qualified(r: x_only::ItemRequest) -> x_only::ItemRequest { <x_only::ItemRequest>::set_x_if_some(r, None) } // shadow: qself type
pub fn value() -> fn(x_only::ItemRequest, Option<u8>) -> x_only::ItemRequest { x_only::ItemRequest::set_x_if_some } // shadow: function value
pub fn closure() { let _f = |r| ClearOnNone::set_x_if_some(r, None); let _g = |r: x_only::ItemRequest| (r,); let _h = |r| custom!(r); } // trait-qualified
pub fn unknown(items: Vec<x_only::ItemRequest>) { for r in items.into_iter().map(|r| r) { let _ = r.set_x_if_some(None); } } // shadow: unknown receiver
pub fn in_macro(r: x_only::ItemRequest) { let _ = custom!(r.set_x_if_some(None)); } // shadow: macro
pub fn trait_path(r: x_only::ItemRequest) -> x_only::ItemRequest { <x_only::ItemRequest as ClearOnNone>::set_x_if_some(r, None) } // trait-qualified
pub fn other(r: unrelated::ItemRequest) -> u32 { r.set_x_if_some(None) } // not generated
pub fn other_path(r: unrelated::ItemRequest) -> u32 { unrelated::ItemRequest::set_x_if_some(r, None) } // not generated
pub fn arity(items: Vec<u8>) { for r in items { let _ = r.set_x_if_some(None, 1); } } // arity
pub struct Holder;
impl Holder {
    pub fn inside(r: x_only::ItemRequest) -> x_only::ItemRequest { r.set_x_if_some(Some(1)) } // shadow: inside impl
}
"#;
    let source = format!("{declaration}{callers}");
    let outcome = convert(&source);
    let mut expected = [
        "// shadow: method",
        "// shadow: boxed",
        "// shadow: type path",
        "// shadow: qself type",
        "// shadow: function value",
        "// shadow: unknown receiver",
        "// shadow: macro",
        "// shadow: inside impl",
    ]
    .map(|marker| unique_line(&source, marker));
    expected.sort_unstable();
    assert_eq!(
        lines(&outcome, SHADOW),
        expected,
        "{:#?}",
        outcome.diagnostics
    );
    // Nothing is rewritten.
    for marker in [
        "r.set_x_if_some(None) } // shadow: method",
        "x_only::ItemRequest::set_x_if_some(r, None) } // shadow: type path",
        "x_only::ItemRequest::set_x_if_some } // shadow: function value",
        "custom!(r.set_x_if_some(None)); } // shadow: macro",
    ] {
        assert!(outcome.converted.contains(marker), "{marker}");
    }
}

/// v3-only names that are not value controls: `bind`/`plan_runtime` on the
/// client and scope handles, request controls, and the traversal terminals
/// of a paginated endpoint's request only.
#[test]
fn generated_name_shadow_covers_v3_core_methods() {
    let source = r#"use concord::{api, Empty};
api! {
    client Svc {
        base "https://review.example.test/";
        timeout 5s;
        GET item() -> () via Empty {
            path ["item"];
        }
    }
}
pub trait Ext {
    fn bind(&self, n: u8) -> u8 { n }
    fn cancel_with(self, n: u8) -> u8 where Self: Sized { n }
    fn pages(self, n: u8) -> u8 where Self: Sized { n }
}
impl Ext for Svc {}
impl Ext for svc::ItemRequest {}
pub fn calls(client: &Svc) {
    let _ = client.bind(1); // shadow: bind
    let _ = client.item().cancel_with(2); // shadow: cancel_with
    let _ = client.item().pages(3); // not paginated
}
"#;
    let outcome = convert(source);
    let mut expected =
        ["// shadow: bind", "// shadow: cancel_with"].map(|marker| unique_line(source, marker));
    expected.sort_unstable();
    assert_eq!(
        lines(&outcome, SHADOW),
        expected,
        "{:#?}",
        outcome.diagnostics
    );
}

/// N1: `x?` and `x_if_some?` are both legal in v2.1, but v3 generates
/// `set_x_if_some` for both (the conditional setter of `x`, and the setter of
/// `x_if_some`) and rejects the declaration. The tool reports it at the
/// declaration instead of silently producing a declaration v3 rejects.
#[test]
fn review_n1_colliding_v3_value_controls_are_reported_at_the_declaration() {
    let source = r#"use concord::{api, Empty};
api! {
    client Both {
        base "https://review.example.test/";
        timeout 5s;
        GET item(x?: u8, x_if_some?: u8) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; "y" = endpoint.x_if_some; }
        }
    }
}
"#;
    let outcome = convert(source);
    let found = sites(&outcome, COLLISION);
    assert_eq!(
        found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![unique_line(source, "GET item(")],
        "{:#?}",
        outcome.diagnostics
    );
    let message = &found[0].1;
    for expected in ["`set_x_if_some`", "`x`", "`x_if_some`", "endpoint `item`"] {
        assert!(message.contains(expected), "`{expected}` in: {message}");
    }
    // The declaration is still converted per the existing policy.
    assert!(
        outcome
            .converted
            .contains("GET item(query x?: u8, query(\"y\") x_if_some?: u8)"),
        "{}",
        outcome.converted
    );
    // A declaration without such a pair is not reported.
    let plain = convert(&source.replace("x_if_some", "y_value"));
    assert!(sites(&plain, COLLISION).is_empty());
}

const NAME_SHADOW_FIXTURE: &str = "name_shadow_v2_1.rs.txt";
const NAME_SHADOW_GOLDEN: &str = "tests/migrated_v3_identity/name_shadow.rs";

/// The compiled behavioral control: `tests/migrated_v3_identity/name_shadow.rs`
/// is the exact tool output for `fixtures/migrate_v3/name_shadow_v2_1.rs.txt`
/// (a package crate), compiled and run on v3 by `tests/migrated_v3.rs`. The
/// method call whose meaning v3 changes is reported and left for manual
/// migration; the trait-qualified call keeps its meaning and is not reported.
#[test]
fn generated_name_shadow_golden_output_is_the_exact_tool_output() {
    let workspace = TempDir::new("name-shadow-golden");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"name_shadow\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nconcord = \"3\"\n",
    );
    workspace.write("src/lib.rs", "pub mod name_shadow;\n");
    let source = fixture(NAME_SHADOW_FIXTURE);
    workspace.write("src/name_shadow.rs", &source);
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    let converted = migration
        .converted("name_shadow.rs")
        .expect("migrated fixture");
    let shadows = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == SHADOW)
        .map(|diagnostic| (diagnostic.file.as_str(), diagnostic.line))
        .collect::<Vec<_>>();
    assert_eq!(
        shadows,
        vec![(
            "name_shadow.rs",
            unique_line(&source, ".set_x_if_some(None).plan()")
        )],
        "{:#?}",
        migration.diagnostics
    );
    let errors = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .count();
    assert_eq!(errors, 1, "{:#?}", migration.diagnostics);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(NAME_SHADOW_GOLDEN);
    if std::env::var_os("CONCORD_MIGRATE_BLESS").is_some() {
        fs::write(&path, converted).expect("bless golden");
        return;
    }
    let golden = fs::read_to_string(&path).expect("golden");
    assert!(
        golden == converted,
        "{NAME_SHADOW_GOLDEN} differs from the migrate-v3 output:\n{}",
        super::super::edits::unified_diff(NAME_SHADOW_GOLDEN, &golden, converted)
    );
}

/// Another crate of the run declares the value: a call on that crate's type,
/// or on an unknown receiver, is reported; a call on a type proved not to be
/// generated is not, and a core method of another crate is reported only on
/// that crate's type.
#[test]
fn generated_name_shadow_across_crates_of_the_run() {
    let tree = TempDir::new("name-shadow-crates");
    let manifest = |name: &str, dependencies: &str| {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nconcord = \"3\"\n{dependencies}"
        )
    };
    tree.write("api/Cargo.toml", &manifest("api", ""));
    let declaration = EXT_TRAIT_SHADOW
        .split("pub trait ClearOnNone")
        .next()
        .expect("declaration");
    tree.write("api/src/lib.rs", declaration);
    tree.write(
        "app/Cargo.toml",
        &manifest("app", "api = { path = \"../api\" }\n"),
    );
    let source = r#"pub struct Local;
impl Local { pub fn set_x_if_some(self, _: Option<u8>) -> u8 { 1 } pub fn bind(&self, _: u8) {} }
pub fn external(r: api::x_only::ItemRequest) { let _ = r.set_x_if_some(None); } // shadow: external
pub fn unknown(items: Vec<u8>) { for r in items { let _ = r.set_x_if_some(None); } } // shadow: unknown
pub fn unknown_core(items: Vec<u8>) { for r in items { let _ = r.bind(1); } } // core
pub fn local(r: Local) -> u8 { r.set_x_if_some(None) } // local
"#;
    tree.write("app/src/lib.rs", source);
    let migration = migrate(&tree.0, &Settings::default()).expect("migration");
    let mut found = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == SHADOW)
        .map(|diagnostic| (diagnostic.file.as_str(), diagnostic.line))
        .collect::<Vec<_>>();
    found.sort_unstable();
    assert_eq!(
        found,
        vec![
            ("app/src/lib.rs", unique_line(source, "// shadow: external")),
            ("app/src/lib.rs", unique_line(source, "// shadow: unknown")),
        ],
        "{:#?}",
        migration.diagnostics
    );
}
