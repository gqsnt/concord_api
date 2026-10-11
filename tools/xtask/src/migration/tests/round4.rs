//! Third post-fix review of F03 (round 4): the classes the default-deny gate
//! of round 3 still let through, reason accuracy, determinism and robustness.
//!
//! Each third-review probe (`evidence/r2/postfix_review3/f03/postfix3_probe.rs`)
//! is a test here that checks the complete resulting caller text and every
//! diagnostic in it: a `let` annotation that coerces the initializer (D1), an
//! `async fn` call (D2), a field that is not visible where it is accessed
//! (D3), a crate-local trait implemented under an import rename (D4),
//! `#[cfg_attr(.., macro_use)]` crates (D5), `#[path]` on an inline module
//! (D6), a library whose trait model is incomplete seen from the package's
//! other targets (D7), and a bare `api!` without an import (D8). Unknown sites
//! stay unchanged with a diagnostic that names the true cause; genuine sites
//! still migrate. Probe lines that held two setter calls are split into one
//! call per line (the harness checks one diagnostic per unproved line).

use std::time::{Duration, Instant};

use super::*;

/// The third reviewer's probe declaration, verbatim.
const REVIEW3_API: &str = r#"use concord::{api, Empty};
api! {
    client Api {
        base "https://review.example.test/";
        timeout 5s;
        values {
            locale?: String;
        }
        query {
            "locale" = client.locale;
        }
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}
pub mod other {
    #[derive(Clone, Copy)]
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
// callers
"#;

/// The declaration of the workspace fixtures (a package library).
const LIBRARY_API: &str = r#"use concord::{Empty, api};

api! {
    client Api {
        base "https://review.example.test/";
        timeout 5s;
        values {
            locale?: String;
        }
        query {
            "locale" = client.locale;
        }
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}

pub mod other {
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
"#;

/// `source` with the declaration of [`LIBRARY_API`] in its exact v3 form (what
/// migrate-v3 writes for it), for comparing complete library texts.
fn migrated_declaration(source: &str) -> String {
    let v2 = r#"            locale?: String;
        }
        query {
            "locale" = client.locale;
        }
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
"#;
    let v3 = r#"            query locale?: String;
        }
        GET item(query x?: u32) -> () via Empty {
            path "item";
"#;
    assert_eq!(source.matches(v2).count(), 1, "one declaration");
    source.replace(v2, v3)
}

/// Checks that on the line holding each `fragment` of `source` some
/// diagnostic's message names `reason`.
fn assert_reasons(outcome: &SourceOutcome, source: &str, reasons: &[(&str, &str)]) {
    for (fragment, reason) in reasons {
        let line = unique_line(source, fragment);
        let messages = outcome
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.line == line)
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(
            messages.iter().any(|message| message.contains(reason)),
            "line {line} (`{fragment}`): no diagnostic names `{reason}`: {messages:#?}"
        );
    }
}

/// [`assert_probe`] followed by [`assert_reasons`].
fn assert_probe_reasons(
    api: &str,
    callers: &str,
    rewrites: &[(&str, &str)],
    extra: &[(&str, &str)],
    reasons: &[(&str, &str)],
) -> SourceOutcome {
    let outcome = assert_probe(api, callers, rewrites, extra);
    assert_reasons(&outcome, &format!("{api}{callers}"), reasons);
    outcome
}

/// The `(code, line, message)` of every diagnostic of `file` in a run.
fn file_diagnostics(
    migration: &super::super::Migration,
    file: &str,
) -> Vec<(String, usize, String)> {
    let mut found = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| {
            (
                diagnostic.code.as_str().to_owned(),
                diagnostic.line,
                diagnostic.message.clone(),
            )
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// A package with `Cargo.toml` named `name` in a fresh temporary directory.
fn package(label: &str, name: &str) -> TempDir {
    let workspace = TempDir::new(label);
    workspace.write(
        "Cargo.toml",
        &format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"),
    );
    workspace
}

// ----- D1: a declared type always wins over the initializer --------------------

/// Probe `postfix3_probe_annotation_coercion`: a `let` annotation the model
/// cannot resolve exactly (`&dyn Nav`, `Box<dyn DynExt>`, an alias of a trait
/// object, a partially inferred `Box<_>`) no longer falls back to the
/// initializer's type, because the annotation may coerce the value to another
/// type. An exact annotation and a plain `_` (which constrains nothing) still
/// migrate.
#[test]
fn round4_f03_third_review_annotation_coercion() {
    let callers = r#"
pub trait Nav {
    fn item(&self) -> other::ItemRequest;
}
impl Nav for Api {
    fn item(&self) -> other::ItemRequest { other::ItemRequest }
}
pub fn dyn_ref_annotation(client: &Api) -> u32 {
    let r: &dyn Nav = client;
    r.item().set_x(None) // PROBE:UNPROVED dyn_ref_annotation_navigation
}
pub trait DynExt {
    fn set_x(&self, v: Option<u32>) -> u32;
    fn replace_x(&self, v: Option<u32>) -> u32;
}
impl DynExt for api::ItemRequest {
    fn set_x(&self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(&self, _: Option<u32>) -> u32 { 2 }
}
pub fn dyn_box_annotation(boxed: Box<api::ItemRequest>) -> u32 {
    let b: Box<dyn DynExt> = boxed;
    b.set_x(None) // PROBE:UNPROVED dyn_box_annotation
}
pub fn known_annotation(client: &Api) {
    let r: &Api = client;
    let _ = r.item().set_x(None); // PROBE:GENUINE known_ref_annotation
}
pub type DynNav = dyn Nav;
pub fn alias_of_dyn_annotation(client: &Api) -> u32 {
    let r: &DynNav = client;
    r.item().set_x(None) // PROBE:UNPROVED alias_of_dyn_annotation
}
pub fn partial_annotation(boxed: Box<api::ItemRequest>) -> u32 {
    let b: Box<_> = boxed;
    b.set_x(None) // PROBE:UNPROVED partially_inferred_annotation
}
pub fn infer_annotation(client: &Api) {
    let r: _ = client.item();
    let _ = r.set_x(None); // PROBE:GENUINE plain_infer_annotation
}
pub fn closure_annotation(boxed: Box<api::ItemRequest>) -> u32 {
    let f = |b: Box<dyn DynExt>| b.set_x(None); // PROBE:UNPROVED closure_dyn_parameter
    f(boxed)
}
"#;
    assert_probe_reasons(
        REVIEW3_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED dyn_ref_annotation_navigation",
                "annotation",
            ),
            ("// PROBE:UNPROVED dyn_box_annotation", "annotation"),
            ("// PROBE:UNPROVED alias_of_dyn_annotation", "annotation"),
            (
                "// PROBE:UNPROVED partially_inferred_annotation",
                "annotation",
            ),
        ],
    );
}

// ----- D2: an `async fn` call is a future ----------------------------------------

/// Probe `postfix3_probe_async_fn_result`: calling an `async fn` produces an
/// opaque future, not the declared return type, so a blanket trait for
/// futures may provide the method.
#[test]
fn round4_f03_third_review_async_fn_result() {
    let callers = r#"
pub trait FutExt {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
impl<F: std::future::Future> FutExt for F {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
pub async fn make_request(client: &Api) -> api::ItemRequest { client.item() }
pub fn async_fn_result(client: &Api) -> u32 {
    make_request(client).set_x(None) // PROBE:UNPROVED async_fn_call_result
}
pub fn async_fn_path_result(client: &Api) -> u32 {
    self::make_request(client).set_x(None) // PROBE:UNPROVED async_fn_path_call_result
}
pub fn sync_make(client: &Api) -> api::ItemRequest { client.item() }
pub fn sync_fn_result(client: &Api) {
    let _ = sync_make(client).set_x(None); // PROBE:GENUINE sync_fn_call_result
}
"#;
    assert_probe_reasons(
        REVIEW3_API,
        callers,
        &[],
        &[],
        &[
            ("// PROBE:UNPROVED async_fn_call_result", "async fn"),
            ("// PROBE:UNPROVED async_fn_path_call_result", "async fn"),
        ],
    );
}

// ----- D3: field visibility --------------------------------------------------------

/// Probe `postfix3_probe_private_field_deref`: outside its module a private
/// field is skipped by field access and auto-deref may reach an accessible
/// field of the same name, so a field that is not provably visible at the
/// access is unknown. Every visibility form is modeled relative to the access.
#[test]
fn round4_f03_third_review_private_field() {
    let callers = r#"
pub mod holder {
    pub struct Holder {
        pub inner: crate::other::ItemRequest,
    }
    pub struct S {
        inner: crate::api::ItemRequest,
        pub other: Holder,
    }
    impl std::ops::Deref for S {
        type Target = Holder;
        fn deref(&self) -> &Holder { &self.other }
    }
    impl S {
        pub fn new(inner: crate::api::ItemRequest) -> Self {
            Self { inner, other: Holder { inner: crate::other::ItemRequest } }
        }
        pub fn own(self) -> u32 {
            let _ = self.inner.set_x(None); // PROBE:GENUINE private_field_inside_module
            0
        }
    }
}
pub fn private_field_outside(s: holder::S) -> u32 {
    s.inner.set_x(None) // PROBE:UNPROVED private_field_outside_module
}
pub mod outer {
    pub mod fields {
        pub struct V {
            pub(super) a: crate::api::ItemRequest,
            pub(in crate::outer) b: crate::api::ItemRequest,
            pub(self) c: crate::api::ItemRequest,
            pub(crate) d: crate::api::ItemRequest,
            e: crate::api::ItemRequest,
        }
        pub struct P(crate::api::ItemRequest, pub crate::api::ItemRequest);
        pub fn inside(v: V, p: P) {
            let _ = v.c.set_x(None); // PROBE:GENUINE pub_self_inside
            let _ = v.e.set_x(None); // PROBE:GENUINE private_inside
            let _ = p.0.set_x(None); // PROBE:GENUINE private_tuple_inside
            let _ = v.a.set_x(None); // PROBE:GENUINE pub_super_inside
        }
        pub mod child {
            pub fn descendant(v: super::V) {
                let _ = v.e.set_x(None); // PROBE:GENUINE private_in_descendant
            }
        }
    }
    pub fn in_parent(v: fields::V, p: fields::P) -> u32 {
        let _ = v.a.set_x(None); // PROBE:GENUINE pub_super_in_parent
        let _ = v.b.set_x(None); // PROBE:GENUINE pub_in_path_in_scope
        let _ = p.1.set_x(None); // PROBE:GENUINE public_tuple_field
        let _ = p.0; // private
        v.c.set_x(None) // PROBE:UNPROVED pub_self_outside
    }
    pub fn private_in_parent(v: fields::V) -> u32 {
        v.e.set_x(None) // PROBE:UNPROVED private_in_parent
    }
    pub fn private_tuple_in_parent(p: fields::P) -> u32 {
        p.0.set_x(None) // PROBE:UNPROVED private_tuple_in_parent
    }
}
pub fn at_root(v: outer::fields::V) -> u32 {
    let _ = v.d.set_x(None); // PROBE:GENUINE pub_crate_same_crate
    v.a.set_x(None) // PROBE:UNPROVED pub_super_outside
}
pub fn pub_in_path_outside(v: outer::fields::V) -> u32 {
    v.b.set_x(None) // PROBE:UNPROVED pub_in_path_outside
}
"#;
    assert_probe_reasons(
        REVIEW3_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED private_field_outside_module",
                "not visible",
            ),
            ("// PROBE:UNPROVED pub_self_outside", "not visible"),
            ("// PROBE:UNPROVED private_in_parent", "not visible"),
            ("// PROBE:UNPROVED private_tuple_in_parent", "not visible"),
            ("// PROBE:UNPROVED pub_super_outside", "not visible"),
            ("// PROBE:UNPROVED pub_in_path_outside", "not visible"),
        ],
    );
}

/// A `pub(crate)` field of the package library is not visible from the
/// package's integration test (another crate), where auto-deref may reach
/// another field of the same name.
#[test]
fn round4_f03_crate_visible_field_from_another_crate() {
    let workspace = package("round4-field-crate", "fieldvis");
    workspace.write(
        "src/lib.rs",
        &format!(
            "{LIBRARY_API}
pub struct Holder {{
    pub inner: other::ItemRequest,
}}
pub struct S {{
    pub(crate) inner: api::ItemRequest,
    pub open: api::ItemRequest,
    pub other: Holder,
}}
impl std::ops::Deref for S {{
    type Target = Holder;
    fn deref(&self) -> &Holder {{ &self.other }}
}}
pub fn same_crate(s: S) {{
    let _ = s.inner.set_x(None);
}}
"
        ),
    );
    let test = "pub fn crate_field(s: fieldvis::S) -> u32 {
    s.inner.set_x(None)
}
pub fn public_field(s: fieldvis::S) {
    let _ = s.open.set_x(None);
}
";
    workspace.write("tests/it.rs", test);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert!(
        migration
            .converted("src/lib.rs")
            .is_some_and(|text| text.contains("s.inner.replace_x(None)")),
        "the same-crate access migrates"
    );
    assert_eq!(
        migration.converted("tests/it.rs"),
        Some(
            test.replace("s.open.set_x(None)", "s.open.replace_x(None)")
                .as_str()
        )
    );
    let found = file_diagnostics(&migration, "tests/it.rs");
    assert_eq!(
        found
            .iter()
            .map(|(code, line, _)| (code.as_str(), *line))
            .collect::<Vec<_>>(),
        [
            ("C3-MIGRATE-SETTER-REPLACE", 5),
            ("C3-MIGRATE-SETTER-UNRESOLVED", 2)
        ],
        "{found:#?}"
    );
    assert!(found[1].2.contains("not visible"), "{found:#?}");
}

// ----- D4: the trait of an `impl` is resolved through imports ---------------------

const TRAITS_A_B: &str = r#"
pub mod traits_a {
    pub trait Ext {
        fn set_x(self, _: Option<u32>) -> u32 where Self: Sized { 1 }
        fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized { 2 }
    }
}
pub mod traits_b {
    pub trait Other {
        fn unrelated(&self) {}
    }
}
use traits_a::Ext as Other;
impl Other for Box<api::ItemRequest> {}
pub fn renamed_trait_box(r: Box<api::ItemRequest>) -> u32 {
    r.set_x(None) // PROBE:UNPROVED renamed_trait_box
}
"#;

/// Probes `postfix3_probe_renamed_trait_alone`, `postfix3_probe_renamed_trait`
/// and `postfix3_probe_renamed_trait_control`: the methods of a crate-local
/// trait implemented under an import rename are those of the trait the path
/// resolves to, not of another trait that has the written name.
#[test]
fn round4_f03_third_review_renamed_trait() {
    // `.._alone`, verbatim.
    assert_probe_reasons(
        REVIEW3_API,
        TRAITS_A_B,
        &[],
        &[],
        &[("// PROBE:UNPROVED renamed_trait_box", "`Ext`")],
    );
    // `postfix3_probe_renamed_trait`, verbatim (another trait implemented
    // for `Box<Request>` in the crate blocked it already).
    let combined = format!(
        "{TRAITS_A_B}pub mod direct {{
    pub trait Ext2 {{
        fn set_x(self, _: Option<u32>) -> u32 where Self: Sized {{ 1 }}
        fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized {{ 2 }}
    }}
    impl Ext2 for Box<crate::api::ItemRequest> {{}}
    pub fn direct_trait_box(r: Box<crate::api::ItemRequest>) -> u32 {{
        r.set_x(None) // PROBE:UNPROVED direct_trait_box_control
    }}
}}
"
    );
    assert_probe(REVIEW3_API, &combined, &[], &[]);
    // `.._control`, verbatim.
    let control = r#"
pub mod traits_a {
    pub trait Ext {
        fn set_x(self, _: Option<u32>) -> u32 where Self: Sized { 1 }
        fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized { 2 }
    }
}
use traits_a::Ext;
impl Ext for Box<api::ItemRequest> {}
pub fn named_trait_box(r: Box<api::ItemRequest>) -> u32 {
    r.set_x(None) // PROBE:UNPROVED named_trait_box
}
"#;
    assert_probe(REVIEW3_API, control, &[], &[]);
}

/// The trait path of an `impl` is resolved through `pub use` re-exports and
/// aliases (the written last segment names an unrelated trait here), and a
/// trait resolved exactly that provides no such method no longer blocks a
/// genuine call.
#[test]
fn round4_f03_impl_trait_paths_resolve_exactly() {
    let reexported = r#"
pub mod traits_c {
    pub trait Ext3 {
        fn set_x(self, _: Option<u32>) -> u32 where Self: Sized { 1 }
        fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized { 2 }
    }
}
pub mod traits_d {
    pub trait Alias {
        fn unrelated(&self) {}
    }
}
pub mod reexport {
    pub use crate::traits_c::Ext3 as Alias;
}
pub mod through_reexport {
    use crate::reexport::Alias;
    impl Alias for Box<crate::api::ItemRequest> {}
    pub fn call(r: Box<crate::api::ItemRequest>) -> u32 {
        r.set_x(None) // PROBE:UNPROVED trait_through_reexport_alias
    }
}
"#;
    assert_probe_reasons(
        REVIEW3_API,
        reexported,
        &[],
        &[],
        &[("// PROBE:UNPROVED trait_through_reexport_alias", "`Ext3`")],
    );
    // Two traits share a name; the implemented one provides no `set_x`.
    let exact = r#"
pub mod a {
    pub trait Ext {
        fn set_x(self, _: Option<u32>) -> u32 where Self: Sized { 1 }
        fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized { 2 }
    }
}
pub mod b {
    pub trait Ext {
        fn unrelated(&self) {}
    }
}
impl b::Ext for Box<api::ItemRequest> {}
pub fn same_name_other_trait(r: Box<api::ItemRequest>) {
    let _ = r.set_x(None); // PROBE:GENUINE same_named_trait_without_the_method
}
"#;
    assert_probe(REVIEW3_API, exact, &[], &[]);
}

// ----- D5: `#[cfg_attr(.., macro_use)]` -------------------------------------------

/// Probes `postfix3_probe_cfg_attr_macro_use` and `.._macro_use_control`: a
/// `#[cfg_attr]` that may apply `macro_use` makes the crate's macros replace
/// the standard prelude macros, so a standard name is not proved standard.
#[test]
fn round4_f03_third_review_cfg_attr_macro_use() {
    for attribute in [
        "#[macro_use]",
        "#[cfg_attr(feature = \"shim\", macro_use)]",
        "#[cfg_attr(unix, cfg_attr(feature = \"shim\", macro_use))]",
        "#[cfg_attr(feature = \"shim\", allow(unused), macro_use)]",
    ] {
        let callers = format!(
            r#"
{attribute}
extern crate assert_shim;
pub fn std_named_macro(r: api::ItemRequest) {{
    assert!(r.set_x(None) == 1); // PROBE:UNPROVED macro_use_assert
}}
"#
        );
        assert_probe_reasons(
            REVIEW3_API,
            &callers,
            &[],
            &[],
            &[("// PROBE:UNPROVED macro_use_assert", "`assert!`")],
        );
    }
    // A `cfg_attr` whose predicate is trivially false never applies.
    let inert = r#"
#[cfg_attr(any(), macro_use)]
extern crate assert_shim;
pub fn std_named_macro(r: api::ItemRequest) {
    assert!(r.set_x(None) == r); // PROBE:GENUINE trivially_false_cfg_attr
}
"#;
    assert_probe(REVIEW3_API, inert, &[], &[]);
}

// ----- D6: `#[path]` on an inline module ------------------------------------------

/// Third-review control `ctl21_layout`: `#[path]` on an inline module moves
/// the directory of the file modules declared inside it, exactly as rustc
/// resolves it (a mod-rs file: relative to its directory; a non-mod-rs file:
/// relative to its directory without the module's own name; a nested inline
/// module: relative to the enclosing inline module's directory). A stray file
/// at the default location is never read.
#[test]
fn round4_f03_third_review_inline_path_module() {
    let workspace = package("round4-inline-path", "layout");
    let lib = format!(
        "{LIBRARY_API}
#[path = \"thread_files\"]
pub mod thread {{
    pub mod local_data;
}}
pub mod nonmod;
pub mod nested;

pub fn inline_path_module(r: thread::local_data::R) -> u32 {{
    r.set_x(None) // unrelated: mod-rs
}}
pub fn non_mod_rs(r: nonmod::thread::local_data::R) -> u32 {{
    r.set_x(None) // unrelated: non-mod-rs
}}
pub fn nested_inline(r: nested::outer::inner::leaf::R) -> u32 {{
    r.set_x(None) // unrelated: nested inline
}}
pub fn genuine(r: thread::local_data::G) {{
    let _ = r.set_x(None); // genuine: path module
}}
"
    );
    workspace.write("src/lib.rs", &lib);
    let unrelated =
        "pub type R = crate::other::ItemRequest;\npub type G = crate::api::ItemRequest;\n";
    let stray = "pub type R = crate::api::ItemRequest;\npub type G = crate::other::ItemRequest;\n";
    workspace.write("src/thread_files/local_data.rs", unrelated);
    workspace.write("src/thread/local_data.rs", stray);
    workspace.write(
        "src/nonmod.rs",
        "#[path = \"nonmod_files\"]\npub mod thread {\n    pub mod local_data;\n}\n",
    );
    workspace.write("src/nonmod_files/local_data.rs", unrelated);
    workspace.write("src/nonmod/thread/local_data.rs", stray);
    workspace.write("src/nonmod/nonmod_files/local_data.rs", stray);
    workspace.write(
        "src/nested.rs",
        "pub mod outer {\n    #[path = \"deep\"]\n    pub mod inner {\n        pub mod leaf;\n    }\n}\n",
    );
    workspace.write("src/nested/outer/deep/leaf.rs", unrelated);
    workspace.write("src/nested/outer/inner/leaf.rs", stray);
    workspace.write("src/deep/leaf.rs", stray);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    let converted = migration.converted("src/lib.rs").expect("library");
    let expected = migrated_declaration(&lib).replace(
        "let _ = r.set_x(None); // genuine",
        "let _ = r.replace_x(None); // genuine",
    );
    assert!(
        converted == expected,
        "the library text differs from the expected decisions:\n{}",
        super::super::edits::unified_diff("src/lib.rs", &expected, converted)
    );
    let found = file_diagnostics(&migration, "src/lib.rs");
    let line = |fragment: &str| unique_line(&lib, fragment);
    let mut expected = vec![
        (
            "C3-MIGRATE-SETTER-REPLACE".to_owned(),
            line("// genuine: path module"),
        ),
        (
            "C3-MIGRATE-SETTER-UNRESOLVED".to_owned(),
            line("// unrelated: mod-rs"),
        ),
        (
            "C3-MIGRATE-SETTER-UNRESOLVED".to_owned(),
            line("// unrelated: non-mod-rs"),
        ),
        (
            "C3-MIGRATE-SETTER-UNRESOLVED".to_owned(),
            line("// unrelated: nested inline"),
        ),
    ];
    expected.sort();
    assert_eq!(
        found
            .iter()
            .map(|(code, line, _)| (code.clone(), *line))
            .collect::<Vec<_>>(),
        expected,
        "{found:#?}"
    );
    for (code, _, message) in &found {
        if code == "C3-MIGRATE-SETTER-UNRESOLVED" {
            assert!(
                message.contains("not generated by this crate"),
                "the receiver resolves through the file rustc reads: {message}"
            );
        }
    }
}

/// A `#[cfg_attr(.., path = ..)]` on an inline module makes the location of
/// the file modules inside it unknown: they are unresolved, and the crate's
/// trait model is incomplete.
#[test]
fn round4_f03_cfg_attr_path_on_inline_module_is_unknown() {
    let workspace = package("round4-inline-cfg-path", "layout");
    let lib = format!(
        "{LIBRARY_API}
#[cfg_attr(feature = \"alt\", path = \"alt_files\")]
pub mod thread {{
    pub mod local_data;
}}

pub fn gated_path(r: thread::local_data::G) -> u32 {{
    r.set_x(None) // unknown: gated inline path
}}
"
    );
    workspace.write("src/lib.rs", &lib);
    workspace.write(
        "src/thread/local_data.rs",
        "pub type G = crate::api::ItemRequest;\n",
    );
    workspace.write(
        "src/alt_files/local_data.rs",
        "pub type G = crate::other::ItemRequest;\n",
    );
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(
        migration.converted("src/lib.rs"),
        Some(migrated_declaration(&lib).as_str())
    );
    let found = file_diagnostics(&migration, "src/lib.rs");
    assert_eq!(
        found
            .iter()
            .map(|(code, line, _)| (code.as_str(), *line))
            .collect::<Vec<_>>(),
        [(
            "C3-MIGRATE-SETTER-UNRESOLVED",
            unique_line(&lib, "// unknown: gated inline path")
        )],
        "{found:#?}"
    );
}

// ----- D7: the package library's completeness -------------------------------------

/// Third-review control `ctl21_cross`: a trait implementation generated by a
/// macro in the package library may intercept a `Box<Request>` receiver in
/// the package's integration tests, examples, binaries and benches, which use
/// the library's generated types. The library's foreign-trait implementations
/// are seen from those targets too.
#[test]
fn round4_f03_third_review_library_incompleteness() {
    let workspace = package("round4-library", "ctl_cross");
    workspace.write(
        "src/lib.rs",
        &format!(
            "{LIBRARY_API}
pub trait Ext {{
    fn set_x(self, _: Option<u32>) -> u32 where Self: Sized {{ 1 }}
    fn replace_x(self, _: Option<u32>) -> u32 where Self: Sized {{ 2 }}
}}

macro_rules! ext_for {{
    ($t:ty) => {{ impl Ext for $t {{}} }};
}}
ext_for!(Box<api::ItemRequest>);
"
        ),
    );
    let caller = "use ctl_cross::Ext;

pub fn boxed_call(b: Box<ctl_cross::api::ItemRequest>) -> u32 {
    b.set_x(None)
}
";
    for target in [
        "tests/it.rs",
        "examples/ex.rs",
        "src/bin/tool.rs",
        "src/main.rs",
        "benches/bench.rs",
    ] {
        workspace.write(target, caller);
    }
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    for target in [
        "tests/it.rs",
        "examples/ex.rs",
        "src/bin/tool.rs",
        "src/main.rs",
        "benches/bench.rs",
    ] {
        assert_eq!(migration.converted(target), Some(caller), "{target}");
        let found = file_diagnostics(&migration, target);
        assert_eq!(found.len(), 1, "{target}: {found:#?}");
        assert_eq!(found[0].0, "C3-MIGRATE-SETTER-UNRESOLVED", "{target}");
        assert_eq!(found[0].1, 4, "{target}");
        assert!(
            found[0].2.contains("library") && found[0].2.contains("ext_for!"),
            "{target}: the reason names the library's macro: {}",
            found[0].2
        );
    }
}

/// Positive control: the library's generated types are migrated in the
/// package's integration test when the library's trait model is complete.
#[test]
fn round4_f03_complete_library_migrates_from_tests() {
    let workspace = package("round4-library-complete", "complete");
    workspace.write("src/lib.rs", LIBRARY_API);
    let caller = "pub fn boxed_call(b: Box<complete::api::ItemRequest>) {
    let _ = b.set_x(None);
}
";
    workspace.write("tests/it.rs", caller);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(
        migration.converted("tests/it.rs"),
        Some(caller.replace("set_x", "replace_x").as_str())
    );
}

// ----- D8: a bare `api!` without an import -------------------------------------------

/// A bare `api!` with nothing of that name imported in its file is Concord's
/// only through `#[macro_use] extern crate concord` at its crate root (edition
/// 2024 has no prelude `api!`); a crate-local `macro_rules! api` (in textual
/// scope, possibly through `#[macro_use] mod`) or a missing proof is reported
/// and the declaration is left unchanged.
#[test]
fn round4_f03_third_review_bare_api_macro() {
    let declaration = LIBRARY_API.replace("use concord::{Empty, api};", "use concord::Empty;");
    // (a) A standalone file: nothing proves the bare `api!`.
    let outcome = convert(&declaration);
    assert_eq!(outcome.converted, declaration);
    assert_eq!(codes(&outcome), ["C3-MIGRATE-API-MACRO-UNRESOLVED"]);
    assert_eq!(outcome.declarations, 0);

    // (b) Third-review control `ctl21_apimacro`: a crate-local `api!` in
    // textual scope through `#[macro_use] mod macros;`.
    let workspace = package("round4-api-local", "ctl_apimacro");
    workspace.write("src/lib.rs", "#[macro_use]\nmod macros;\npub mod decl;\n");
    workspace.write(
        "src/macros.rs",
        "macro_rules! api {\n    ($($body:tt)*) => {\n        pub const DECLARATION_TEXT: &str = stringify!($($body)*);\n        concord::api! { $($body)* }\n    };\n}\n",
    );
    workspace.write("src/decl.rs", &declaration);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(
        migration.converted("src/decl.rs"),
        Some(declaration.as_str())
    );
    let found = file_diagnostics(&migration, "src/decl.rs");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "C3-MIGRATE-API-MACRO-UNRESOLVED");
    assert!(found[0].2.contains("macro_rules! api"), "{found:#?}");
    assert_eq!(migration.declarations, 0);

    // (c) `#[macro_use] extern crate concord;` at the crate root proves it.
    let workspace = package("round4-api-macro-use", "macro_use_api");
    workspace.write(
        "src/lib.rs",
        "#[macro_use]\nextern crate concord;\npub mod decl;\n",
    );
    workspace.write("src/decl.rs", &declaration);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(migration.declarations, 1, "{:#?}", migration.diagnostics);
    assert!(
        migration
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.as_str() != "C3-MIGRATE-API-MACRO-UNRESOLVED"),
        "{:#?}",
        migration.diagnostics
    );

    // (d) ... unless a crate-local `macro_rules! api` exists as well, or
    // (e) the `macro_use` is conditional.
    for (label, root) in [
        (
            "local",
            "#[macro_use]\nextern crate concord;\nmacro_rules! api { ($($t:tt)*) => {}; }\npub mod decl;\n",
        ),
        (
            "conditional",
            "#[cfg_attr(feature = \"c\", macro_use)]\nextern crate concord;\npub mod decl;\n",
        ),
        (
            "other crate",
            "#[macro_use]\nextern crate concord;\n#[macro_use]\nextern crate other_macros;\npub mod decl;\n",
        ),
    ] {
        let workspace = package("round4-api-unproved", "unproved_api");
        workspace.write("src/lib.rs", root);
        workspace.write("src/decl.rs", &declaration);
        let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
        assert_eq!(migration.declarations, 0, "{label}");
        assert_eq!(
            migration.converted("src/decl.rs"),
            Some(declaration.as_str()),
            "{label}"
        );
        assert_eq!(
            file_diagnostics(&migration, "src/decl.rs")
                .iter()
                .map(|(code, _, _)| code.as_str())
                .collect::<Vec<_>>(),
            ["C3-MIGRATE-API-MACRO-UNRESOLVED"],
            "{label}"
        );
    }
}

// ----- deterministic, accurate reasons ---------------------------------------------

/// The third review ran the tool three times and got 23 messages that named a
/// different "first" reason (hash-map order). Every reason is chosen in a
/// stable order: repeated runs over a fixture with many candidate reasons are
/// byte-for-byte identical (diagnostics and report).
#[test]
fn round4_f03_reasons_are_deterministic() {
    let workspace = package("round4-determinism", "determinism");
    let mut lib = LIBRARY_API.to_owned();
    for index in 0..24 {
        lib.push_str(&format!("use ext_crate_{index}::Item{index};\n"));
    }
    lib.push_str(
        "pub fn value_navigation(client: Api) -> u32 {
    let _ = client.item().set_x(None);
    0
}
pub mod nested {
",
    );
    for index in 0..24 {
        lib.push_str(&format!(
            "    use super::other::ItemRequest as Local{index};\n"
        ));
        lib.push_str(&format!("    use more_crate_{index}::Thing{index};\n"));
    }
    lib.push_str(
        "    pub fn boxed(b: Box<crate::api::ItemRequest>) {
        let _ = b.set_x(None);
    }
}
",
    );
    workspace.write("src/lib.rs", &lib);
    for index in 0..6 {
        workspace.write(
            &format!("tests/t{index}.rs"),
            "pub fn t(client: determinism::Api) {\n    let _ = client.item().set_x(None);\n}\n",
        );
        workspace.write(
            &format!("src/unreached_{index}.rs"),
            "pub fn u(client: crate::Api) {\n    let _ = client.item().set_x(None);\n}\n",
        );
    }
    let render = || {
        let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
        let summary = migration.summary();
        let rendered = migration
            .diagnostics
            .iter()
            .map(super::super::report::Diagnostic::render)
            .collect::<Vec<_>>()
            .join("\n");
        let report = serde_json::to_string_pretty(&migration.report_json("check", &summary))
            .expect("report");
        (rendered, report)
    };
    let first = render();
    for run in 1..8 {
        let again = render();
        assert!(
            again.0 == first.0,
            "run {run} rendered different diagnostics:\n{}",
            super::super::edits::unified_diff("diagnostics", &first.0, &again.0)
        );
        assert!(again.1 == first.1, "run {run} wrote a different report");
    }
}

/// A runtime configuration setter whose receiver is unknown because the
/// client's `api!` is gated by `#[cfg]` names that cause (third review:
/// `pagination.rs:978` said only "receiver unresolved").
#[test]
fn round4_f03_nonzero_reason_names_the_cfg_gate() {
    let source = r#"use std::num::NonZeroUsize;
use concord::{Empty, api};

#[cfg(feature = "json")]
api! {
    client Gated {
        base "https://review.example.test/";
        timeout 5s;
        GET item() -> () via Empty {
            path ["item"];
        }
    }
}

pub fn build(limit: NonZeroUsize) {
    let _ = Gated::builder()
        .configure_runtime(move |runtime| runtime.request_body_limit(limit))
        .build();
}
"#;
    let outcome = convert(source);
    let line = unique_line(source, "request_body_limit(limit)");
    let messages = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.line == line
                && diagnostic.code.as_str() == "C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED"
        })
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 1, "{:#?}", outcome.diagnostics);
    assert!(
        messages[0].contains("#[cfg]"),
        "the reason names the gated declaration: {}",
        messages[0]
    );
}

/// Reasons on a package whose library gates a module by `#[cfg]`, shares a
/// module file between two test targets, implements a foreign trait in
/// another test target, and has files no crate root reaches (third review:
/// `buffered_records.rs:137`, `runtime/deterministic.rs:133`,
/// `current_language.rs:104`, `ui/pass/.../current_language.rs:57`).
#[test]
fn round4_f03_reasons_name_the_true_cause() {
    let workspace = package("round4-reasons", "reasons");
    workspace.write(
        "src/lib.rs",
        &format!(
            "{LIBRARY_API}
#[cfg(feature = \"records\")]
pub mod records {{
    pub fn validate() {{}}
}}
"
        ),
    );
    // A cfg-gated module of the library, imported by a test.
    let gated_import = "use reasons::records::validate;

pub fn value_navigation(client: reasons::Api) -> u32 {
    let _ = validate;
    client.item().set_x(None)
}
";
    workspace.write("tests/gated_import.rs", gated_import);
    // A module file shared by two test targets.
    workspace.write("tests/support/mod.rs", "pub fn helper() {}\n");
    workspace.write(
        "tests/first.rs",
        "mod support;\npub fn f() { support::helper(); }\n",
    );
    let second = "mod support;

pub fn value_navigation(client: reasons::Api) -> u32 {
    support::helper();
    client.item().set_x(None)
}
";
    workspace.write("tests/second.rs", second);
    // A foreign trait implemented for a cfg-gated type in another test.
    workspace.write(
        "tests/impls.rs",
        "#[cfg(feature = \"x\")]\nstruct Gated;\n#[cfg(feature = \"x\")]\nimpl external::Trait for Gated {}\n",
    );
    let unaffected = "pub fn value_navigation(client: reasons::Api) {
    let _ = client.item().set_x(None);
}
";
    workspace.write("tests/unaffected.rs", unaffected);
    // Files no crate root reaches (UI-test inputs).
    workspace.write("tests/ui/pass/aaa_first.rs", "fn main() {}\n");
    let ui = "pub fn value_navigation(client: reasons::Api) -> u32 {
    client.item().set_x(None)
}
";
    workspace.write("tests/ui/pass/zzz_input.rs", ui);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    let reason = |file: &str| {
        let found = file_diagnostics(&migration, file);
        assert_eq!(found.len(), 1, "{file}: {found:#?}");
        assert_eq!(found[0].0, "C3-MIGRATE-SETTER-UNRESOLVED", "{file}");
        found[0].2.clone()
    };
    let gated = reason("tests/gated_import.rs");
    assert!(
        gated.contains("#[cfg]") && gated.contains("records"),
        "the reason names the gated library module: {gated}"
    );
    let shared = reason("tests/second.rs");
    assert!(
        shared.contains("first.rs") && shared.contains("one crate"),
        "the reason names the one-crate limit of a shared module file: {shared}"
    );
    assert_eq!(
        migration.converted("tests/unaffected.rs"),
        Some(unaffected.replace("set_x", "replace_x").as_str()),
        "another test target's implementations do not affect this one: {:#?}",
        file_diagnostics(&migration, "tests/unaffected.rs")
    );
    let unreached = reason("tests/ui/pass/zzz_input.rs");
    assert!(
        unreached.contains("zzz_input.rs") && !unreached.contains("aaa_first.rs"),
        "the reason names the file itself: {unreached}"
    );
}

// ----- classes found by the systematic audit ---------------------------------------

/// Audit (paths through `extern crate`/prelude): a module named `concord`
/// (or an import or glob that may bind that name) where `concord::api!` is
/// written takes precedence over the extern prelude, so the path may name a
/// crate-local macro; the invocation is reported and not migrated.
#[test]
fn round4_f03_audit_local_module_named_concord_shadows_the_crate() {
    let declaration = LIBRARY_API.replace("use concord::{Empty, api};", "use concord::Empty;");
    let body = declaration
        .split_once("api! {")
        .map(|(_, body)| body)
        .expect("declaration body");
    for shadow in [
        "mod concord {\n    macro_rules! api { ($($t:tt)*) => {}; }\n    pub(crate) use api;\n}\n",
        "use crate::local as concord;\nmod local {\n    macro_rules! api { ($($t:tt)*) => {}; }\n    pub(crate) use api;\n}\n",
        "use crate::local::*;\nmod local {\n    pub mod concord {\n        macro_rules! api { ($($t:tt)*) => {}; }\n        pub(crate) use api;\n    }\n}\n",
    ] {
        let source = format!("{shadow}concord::api! {{{body}");
        let outcome = convert(&source);
        assert_eq!(outcome.converted, source, "{shadow}");
        assert_eq!(
            codes(&outcome),
            ["C3-MIGRATE-API-MACRO-UNRESOLVED"],
            "{shadow}: {:#?}",
            outcome.diagnostics
        );
        assert_eq!(outcome.declarations, 0, "{shadow}");
    }
    // Control: nothing named `concord` here.
    let source = format!("concord::api! {{{body}");
    let outcome = convert(&source);
    assert_eq!(outcome.declarations, 1, "{:#?}", outcome.diagnostics);
}

/// Audit (crate layout): a library whose `Cargo.toml` names its source file
/// (`[lib] path`) is modeled from that file; a stray `src/lib.rs` beside it
/// (which the compiler never reads) does not decide what the package's
/// other targets resolve through the library's name.
#[test]
fn round4_f03_audit_library_path_from_the_manifest() {
    let workspace = TempDir::new("round4-lib-path");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"libpath\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/real.rs\"\n",
    );
    workspace.write("src/lib.rs", LIBRARY_API);
    workspace.write(
        "src/real.rs",
        "pub mod other {\n    pub struct ItemRequest;\n    impl ItemRequest {\n        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }\n        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }\n    }\n}\npub mod api {\n    pub use crate::other::ItemRequest;\n}\n",
    );
    let test = "pub fn call(r: libpath::api::ItemRequest) -> u32 {\n    r.set_x(None)\n}\n";
    workspace.write("tests/it.rs", test);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(migration.converted("tests/it.rs"), Some(test));
    let found = file_diagnostics(&migration, "tests/it.rs");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "C3-MIGRATE-SETTER-UNRESOLVED");
    assert!(
        found[0].2.contains("not generated by this crate"),
        "the path resolves through the real library file: {}",
        found[0].2
    );
}

/// Audit (paths through `use`/`extern crate`): on edition 2015 (a
/// `Cargo.toml` with `edition = "2015"` or no edition) a `use` path is
/// relative to the crate root, not to the module; the same source on edition
/// 2024 resolves the module's own `api`.
#[test]
fn round4_f03_audit_edition_2015_use_paths_start_at_the_crate_root() {
    let lib = r#"extern crate concord;

pub mod gen {
    use concord::{Empty, api};
    api! {
        client Api {
            base "https://review.example.test/";
            timeout 5s;
            GET item(x?: u32) -> () via Empty {
                path ["item"];
                query { "x" = endpoint.x; }
            }
        }
    }
}

pub mod other {
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}

pub mod api {
    pub use crate::other::ItemRequest;
}

pub mod sub {
    pub mod api {
        pub use crate::gen::api::ItemRequest;
    }
    use api::ItemRequest;
    pub fn call(r: ItemRequest) -> u32 {
        r.set_x(None) // the call
    }
}
"#;
    for (edition, renamed) in [
        ("", false),
        ("edition = \"2015\"\n", false),
        ("edition = \"2024\"\n", true),
    ] {
        let workspace = TempDir::new("round4-edition");
        workspace.write(
            "Cargo.toml",
            &format!("[package]\nname = \"edition\"\nversion = \"0.0.0\"\n{edition}"),
        );
        workspace.write("src/lib.rs", lib);
        let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
        let converted = migration.converted("src/lib.rs").expect("library");
        let line = converted
            .lines()
            .find(|line| line.contains("// the call"))
            .expect("the call");
        assert_eq!(line.contains("replace_x"), renamed, "{edition:?}: {line}");
        if !renamed {
            let found = file_diagnostics(&migration, "src/lib.rs");
            let call = unique_line(lib, "// the call");
            assert!(
                found
                    .iter()
                    .any(|(code, at, message)| code == "C3-MIGRATE-SETTER-UNRESOLVED"
                        && *at == call
                        && message.contains("not generated by this crate")),
                "{edition:?}: {found:#?}"
            );
        }
    }
}

/// Audit (paths through globs): a glob import brings in only the items and
/// imports visible to the importing module; a `pub(super)` or
/// `pub(in path)` entry seen from outside its restriction is not imported,
/// so the other glob decides.
#[test]
fn round4_f03_audit_glob_imports_respect_restricted_visibility() {
    let callers = r#"
pub mod a {
    pub mod b {
        pub(super) use crate::api::ItemRequest;
    }
    pub mod c {
        pub(in crate::a) use crate::api::ItemRequest;
    }
}
pub mod through_super {
    use crate::a::b::*;
    use crate::other::*;
    pub fn call(r: ItemRequest) -> u32 {
        r.set_x(None) // PROBE:UNPROVED restricted_glob_entry_super
    }
}
pub mod through_in_path {
    use crate::a::c::*;
    use crate::other::*;
    pub fn call(r: ItemRequest) -> u32 {
        r.set_x(None) // PROBE:UNPROVED restricted_glob_entry_in_path
    }
}
pub mod a_inside {
    pub fn call() {}
}
"#;
    assert_probe_reasons(
        REVIEW3_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED restricted_glob_entry_super",
                "not generated by this crate",
            ),
            (
                "// PROBE:UNPROVED restricted_glob_entry_in_path",
                "not generated by this crate",
            ),
        ],
    );
}

// ----- robustness -------------------------------------------------------------------

/// Third-review robustness: a deeply nested generic type overflowed the stack
/// inside `syn` (5000 levels; `rustc` itself overflows there). Generic
/// argument nesting counts toward the nesting bound, so such a file is
/// reported and left unchanged instead of being parsed.
#[test]
fn round4_f03_generic_nesting_counts_toward_the_limit() {
    let depth = 300;
    let ty = format!("{}u32{}", "Option<".repeat(depth), ">".repeat(depth));
    let source = format!("{REVIEW3_API}pub fn deep(_: {ty}) {{}}\n");
    let outcome = convert(&source);
    assert_eq!(outcome.converted, source);
    assert_eq!(codes(&outcome), ["C3-MIGRATE-NESTING-LIMIT"]);
    // Comparisons are not nesting.
    let comparisons = format!(
        "{REVIEW3_API}pub fn many(a: u32, b: u32) -> bool {{ {} true }}\n",
        "a < b || ".repeat(400)
    );
    let outcome = convert(&comparisons);
    assert!(
        !codes(&outcome).contains(&"C3-MIGRATE-NESTING-LIMIT"),
        "{:#?}",
        outcome.diagnostics
    );
}

/// Third-review robustness: a 5000-call chain of generated setters took 22 s
/// (every call re-inferred its whole receiver chain). Receivers are memoized,
/// so the chain is examined in linear time.
#[test]
fn round4_f03_long_method_chains_are_linear() {
    let chain = ".set_x(None)".repeat(5000);
    let source =
        format!("{REVIEW3_API}pub fn f(client: &Api) {{ let _ = client.item(){chain}; }}\n");
    let started = Instant::now();
    let outcome = convert(&source);
    let elapsed = started.elapsed();
    assert_eq!(
        outcome.converted.matches(".replace_x(None)").count(),
        5000,
        "every generated setter of the chain migrates"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "a 5000-call chain took {elapsed:?}"
    );
}

// ----- compiled control -------------------------------------------------------------

const ROUND4_FIXTURE: &str = "round4_v2_1.rs.txt";
const ROUND4_GOLDEN: &str = "tests/migrated_v3_identity/round4.rs";
const ROUND4_MODULE_FILE: &str = "tests/migrated_v3_identity/round4_files/local_data.rs";

/// The exact tool output of `fixtures/migrate_v3/round4_v2_1.rs.txt`, migrated
/// as the module `round4` of a crate (with the `#[path]` module file rustc
/// reads, and a stray file at the default location declaring the opposite
/// types), is `tests/migrated_v3_identity/round4.rs`, which the `migrated_v3`
/// test target compiles through the real v3 `concord::api!` and executes: every
/// unrelated call keeps its original result while the genuine calls keep the
/// v2.1 exact-replacement meaning. The unrelated sites are exactly the
/// reported unautomatable ones.
///
/// Regenerate after an intentional tool change with
/// `CONCORD_MIGRATE_BLESS=1 cargo test -p concord-xtask round4_f03_golden`.
#[test]
fn round4_f03_golden_output_is_the_exact_tool_output() {
    let workspace = package("round4-golden", "round4");
    workspace.write("src/lib.rs", "pub mod round4;\n");
    let source = fixture(ROUND4_FIXTURE);
    workspace.write("src/round4.rs", &source);
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let module_file = fs::read_to_string(manifest.join(ROUND4_MODULE_FILE)).expect("module file");
    workspace.write("src/round4_files/local_data.rs", &module_file);
    workspace.write(
        "src/round4/thread/local_data.rs",
        "pub type R = super::super::api::ItemRequest;\npub type G = super::super::other::ItemRequest;\n",
    );
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    let converted = migration.converted("round4.rs").expect("migrated fixture");
    assert_eq!(
        migration.converted("round4_files/local_data.rs"),
        Some(module_file.as_str())
    );
    let line_of = |marker: &str| unique_line(&source, marker);
    let sites = |code: &str| {
        let mut lines = migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.file == "round4.rs" && diagnostic.code.as_str() == code)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let mut unrelated = [
        "// unrelated: dyn reference annotation",
        "// unrelated: dyn box annotation",
        "// unrelated: alias of dyn annotation",
        "// unrelated: async fn result",
        "// unrelated: private field",
        "// unrelated: pub(super) field",
        "// unrelated: renamed trait",
        "// unrelated: re-exported trait alias",
        "// unrelated: inline path module",
    ]
    .map(line_of);
    unrelated.sort_unstable();
    assert_eq!(
        sites("C3-MIGRATE-SETTER-UNRESOLVED"),
        unrelated,
        "{:#?}",
        migration.diagnostics
    );
    let mut genuine = [
        "// genuine: private field inside its module",
        "// genuine: exact annotation",
        "// genuine: plain infer annotation",
        "// genuine: sync fn result",
        "// genuine: path module type",
    ]
    .map(line_of);
    genuine.sort_unstable();
    assert_eq!(sites("C3-MIGRATE-SETTER-REPLACE"), genuine);
    let errors = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(super::super::report::Diagnostic::render)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), unrelated.len(), "{errors:#?}");
    let path = manifest.join(ROUND4_GOLDEN);
    if std::env::var_os("CONCORD_MIGRATE_BLESS").is_some() {
        fs::write(&path, converted).expect("bless golden");
        return;
    }
    let golden = fs::read_to_string(&path).expect("golden");
    assert!(
        golden == converted,
        "{ROUND4_GOLDEN} differs from the migrate-v3 output:\n{}",
        super::super::edits::unified_diff(ROUND4_GOLDEN, &golden, converted)
    );
}
