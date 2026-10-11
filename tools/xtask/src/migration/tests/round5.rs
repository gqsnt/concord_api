//! Fourth post-fix review of F03 (round 5): the classes the round-4 model
//! still let through.
//!
//! * N1: an item declared in a block between a call and the local binding it
//!   names (a `const`, `static`, unit or tuple struct, function, block `use`
//!   or a block glob that may bring one) shadows that binding, as in rustc.
//! * N2: a module file mounted more than once (by several crate roots of the
//!   package, or twice in one crate through `#[path]`) is compiled once per
//!   mount; a call there is rewritten only if proved in every mount.
//! * N3: `Cargo.toml` is read with a TOML parser (comments, inline tables,
//!   `edition.workspace = true` with the workspace root); an edition the tool
//!   cannot determine is unknown, never assumed to be 2015.
//! * N4: a generated optional setter named as a function value (not called)
//!   is reported.
//!
//! Each probe checks the complete resulting caller text and every diagnostic
//! (see [`assert_probe`]); genuine sites still migrate.

use super::*;

/// The fourth reviewer's probe declaration (`x?: u32`), with the unrelated
/// type and the constants the inner blocks import.
const ROUND5_API: &str = r#"use concord::{api, Empty};
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
pub mod other {
    #[derive(Clone, Copy)]
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
#[allow(non_upper_case_globals)]
pub mod consts {
    pub const r: crate::other::ItemRequest = crate::other::ItemRequest;
    pub const REQ: crate::other::ItemRequest = crate::other::ItemRequest;
}
// callers
"#;

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

// ----- N1: items of the blocks between a call and its binding ------------------

/// Probes "scope exit" and "inner item variants" of the fourth review: an item
/// of an inner block (or closure body) named like an outer local binding or
/// parameter is what the name denotes there, whether it is a `const`, a
/// `static`, a unit struct, a block `use` of a const, a block glob that may
/// bring one, or an item a crate-local macro of that block may expand to (also
/// after the call: block items are visible in the whole block). A binding
/// beside a differently named inner item, after a finished sibling block's
/// item, or shadowing a function item of its own block still migrates.
#[test]
fn round5_f03_n1_inner_block_items_shadow_outer_bindings() {
    let callers = r#"
pub fn inner_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // PROBE:UNPROVED inner_block_const_shadows_outer_local
    }
}
pub fn inner_static(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        static r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // PROBE:UNPROVED inner_block_static_shadows_outer_local
    }
}
pub fn inner_unit_struct(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        #[derive(Clone, Copy)]
        struct r;
        impl r {
            fn set_x(self, _: Option<u32>) -> u32 { 1 }
            fn replace_x(self, _: Option<u32>) -> u32 { 2 }
        }
        r.set_x(None) // PROBE:UNPROVED inner_block_unit_struct_shadows_outer_local
    }
}
pub fn inner_use_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        use crate::consts::r;
        r.set_x(None) // PROBE:UNPROVED inner_block_use_of_const
    }
}
pub fn inner_glob_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        use crate::consts::*;
        r.set_x(None) // PROBE:UNPROVED inner_block_glob_of_const
    }
}
pub fn closure_body_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    let f = || {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // PROBE:UNPROVED closure_body_const
    };
    f()
}
pub fn upper_case(client: &Api) -> u32 {
    let REQ = client.item();
    let _ = &REQ;
    {
        use crate::consts::REQ;
        REQ.set_x(None) // PROBE:UNPROVED upper_case_local_and_const
    }
}
pub fn parameter(r: api::ItemRequest) -> u32 {
    let _ = &r;
    {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // PROBE:UNPROVED inner_const_shadows_parameter
    }
}
pub fn deeper(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        const r: other::ItemRequest = other::ItemRequest;
        {
            r.set_x(None) // PROBE:UNPROVED const_two_blocks_out
        }
    }
}
macro_rules! declare_r {
    () => { const r: other::ItemRequest = other::ItemRequest; };
}
pub fn inner_macro_after_call(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        let v = r.set_x(None); // PROBE:UNPROVED inner_block_macro_after_call
        declare_r!();
        v
    }
}
pub fn beside(client: &Api) {
    let r = client.item();
    {
        const Q: u32 = 1;
        let _ = r.set_x(Some(Q)); // PROBE:GENUINE inner_item_of_another_name
    }
}
pub fn sibling(client: &Api) {
    let r = client.item();
    {
        const r: u32 = 0;
        let _ = r;
    }
    let _ = r.set_x(None); // PROBE:GENUINE finished_sibling_block_item
}
pub fn own_fn(client: &Api) {
    let own = client.item();
    fn own() {}
    let _ = own.set_x(None); // PROBE:GENUINE binding_shadows_its_block_fn
}
pub fn outer_block_item(client: &Api) {
    {
        const Q: u32 = 0;
        let q = client.item();
        let _ = q.set_x(Some(Q)); // PROBE:GENUINE item_of_the_binding_block_other_name
    }
}
"#;
    let outcome = assert_probe(ROUND5_API, callers, &[], &[]);
    let source = format!("{ROUND5_API}{callers}");
    let shadow = "shadows the local binding";
    assert_reasons(
        &outcome,
        &source,
        &[
            (
                "// PROBE:UNPROVED inner_block_const_shadows_outer_local",
                shadow,
            ),
            (
                "// PROBE:UNPROVED inner_block_static_shadows_outer_local",
                shadow,
            ),
            (
                "// PROBE:UNPROVED inner_block_unit_struct_shadows_outer_local",
                shadow,
            ),
            ("// PROBE:UNPROVED inner_block_use_of_const", shadow),
            (
                "// PROBE:UNPROVED inner_block_glob_of_const",
                "brought in by a glob import",
            ),
            ("// PROBE:UNPROVED closure_body_const", shadow),
            ("// PROBE:UNPROVED upper_case_local_and_const", shadow),
            ("// PROBE:UNPROVED inner_const_shadows_parameter", shadow),
            ("// PROBE:UNPROVED const_two_blocks_out", shadow),
            ("// PROBE:UNPROVED inner_block_macro_after_call", "macro"),
        ],
    );
}

// ----- N2: a module file mounted more than once ---------------------------------

/// Compiled control `shared` of the fourth review: `tests/common/mod.rs` is a
/// module of the integration tests `a_api` and `b_other`, where `crate::Req`
/// names the generated request and an unrelated type. The call there is left
/// unchanged and reported with the other mount named; the calls of the test
/// crates themselves still migrate where proved.
#[test]
fn round5_f03_n2_module_file_of_two_test_crates_is_not_rewritten() {
    let workspace = package("round5-shared", "shared");
    workspace.write(
        "src/lib.rs",
        r#"use concord::{api, Empty};
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
pub mod other {
    #[derive(Clone, Copy)]
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> Self { self }
        pub fn replace_x(self, _: Option<u32>) -> Self { self }
    }
}
pub fn genuine(client: &Api) {
    let _ = client.item().set_x(None); // genuine library call
}
"#,
    );
    let common =
        "pub fn clear_x(r: crate::Req) -> crate::Req {\n    r.set_x(None) // shared call\n}\n";
    workspace.write("tests/common/mod.rs", common);
    workspace.write(
        "tests/a_api.rs",
        "mod common;\npub type Req = shared::api::ItemRequest;\n",
    );
    workspace.write(
        "tests/b_other.rs",
        "mod common;\npub type Req = shared::other::ItemRequest;\n",
    );
    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    assert_eq!(migration.converted("tests/common/mod.rs"), Some(common));
    let found = file_diagnostics(&migration, "tests/common/mod.rs");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "C3-MIGRATE-SETTER-UNRESOLVED");
    assert_eq!(found[0].1, 2);
    assert!(
        found[0].2.contains("mounted more than once") && found[0].2.contains("b_other.rs"),
        "the reason names the other mount: {}",
        found[0].2
    );
    let library = migration.converted("src/lib.rs").expect("library");
    assert!(
        library.contains("client.item().replace_x(None); // genuine library call"),
        "{library}"
    );
}

/// Compiled control `twice` of the fourth review: one file mounted twice in
/// one crate through `#[path]`, with `super::Req` the generated request in one
/// mount and an unrelated type in the other. The call is left unchanged and
/// reported; a file mounted once still migrates.
#[test]
fn round5_f03_n2_module_file_mounted_twice_with_path_is_not_rewritten() {
    let workspace = package("round5-twice", "twice");
    workspace.write(
        "src/lib.rs",
        r#"use concord::{api, Empty};
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
pub mod other {
    #[derive(Clone, Copy)]
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
pub mod genuine {
    pub type Req = crate::api::ItemRequest;
    pub type Out = crate::api::ItemRequest;
    #[path = "../shared/helper.rs"]
    pub mod helper;
}
pub mod unrelated {
    pub type Req = crate::other::ItemRequest;
    pub type Out = u32;
    #[path = "../shared/helper.rs"]
    pub mod helper;
}
pub mod once;
"#,
    );
    let helper = "pub fn clear(r: super::Req) -> super::Out {\n    r.set_x(None)\n}\n";
    workspace.write("src/shared/helper.rs", helper);
    workspace.write(
        "src/once.rs",
        "pub fn clear(r: crate::api::ItemRequest) -> crate::api::ItemRequest {\n    r.set_x(None)\n}\n",
    );
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    assert_eq!(migration.converted("shared/helper.rs"), Some(helper));
    let found = file_diagnostics(&migration, "shared/helper.rs");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "C3-MIGRATE-SETTER-UNRESOLVED");
    assert!(
        found[0].2.contains("mounted more than once") && found[0].2.contains("`helper`"),
        "the reason names the other mount: {}",
        found[0].2
    );
    assert_eq!(
        migration.converted("once.rs"),
        Some(
            "pub fn clear(r: crate::api::ItemRequest) -> crate::api::ItemRequest {\n    r.replace_x(None)\n}\n"
        )
    );
}

// ----- N3: the manifest is TOML ------------------------------------------------

/// A library whose `sub::call` names the generated request through the crate
/// root on edition 2015 and an unrelated type on later editions.
const EDITION_LIB: &str = r#"extern crate concord;

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
    pub use crate::gen::api::ItemRequest;
}

pub mod sub {
    pub mod api {
        pub use crate::other::ItemRequest;
    }
    use api::ItemRequest;
    pub fn call(r: ItemRequest) {
        let _ = r.set_x(None); // the call
    }
    pub fn rooted(r: crate::gen::api::ItemRequest) {
        let _ = r.set_x(None); // edition-independent call
    }
}
"#;

/// Migrates [`EDITION_LIB`] under `manifest` (at `package/`, below `root`
/// holding `root_manifest` when given) and returns the converted text of the
/// edition-dependent call's line, the edition-independent call's line and the
/// diagnostics of the call's line.
fn edition_call(manifest: &str, root_manifest: Option<&str>) -> (String, String, Vec<String>) {
    let workspace = TempDir::new("round5-edition");
    if let Some(root) = root_manifest {
        workspace.write("Cargo.toml", root);
    }
    workspace.write("package/Cargo.toml", manifest);
    workspace.write("package/src/lib.rs", EDITION_LIB);
    let migration = migrate(&workspace.0.join("package"), &Settings::default()).expect("migration");
    let converted = migration.converted("src/lib.rs").expect("library");
    let line = |marker: &str| {
        converted
            .lines()
            .find(|line| line.contains(marker))
            .expect("marked line")
            .trim()
            .to_owned()
    };
    let call = unique_line(EDITION_LIB, "// the call");
    let messages = file_diagnostics(&migration, "src/lib.rs")
        .into_iter()
        .filter(|(_, line, _)| *line == call)
        .map(|(code, _, message)| format!("{code}: {message}"))
        .collect();
    (
        line("// the call"),
        line("// edition-independent call"),
        messages,
    )
}

const RENAMED: &str = "let _ = r.replace_x(None); // the call";
const UNCHANGED: &str = "let _ = r.set_x(None); // the call";
const INDEPENDENT: &str = "let _ = r.replace_x(None); // edition-independent call";

/// Compiled control `manifest` of the fourth review and its variants: the
/// edition is read from the TOML document whatever its comments, quoting or
/// inline-table form, and `edition.workspace = true` from the workspace root
/// (a `[workspace]` header with a comment included).
#[test]
fn round5_f03_n3_manifest_is_read_as_toml() {
    let later = [
        (
            "[package] # the package\nname = \"edition\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
            None,
        ),
        (
            "[package]\nname = 'edition' # a comment\nedition = '2021' # a comment\n",
            None,
        ),
        (
            "[package]\nname = \"edition\"\nedition = { workspace = true }\n",
            Some(
                "[workspace]\nmembers = [\"package\"]\n\n[workspace.package]\nedition = \"2024\" # shared\n",
            ),
        ),
        (
            "[package]\nname = \"edition\"\nedition.workspace = true\n",
            Some(
                "[workspace] # the root\nmembers = [\"package\"]\n[workspace.package] # shared\nedition = \"2024\"\n",
            ),
        ),
        (
            "[ package ]\nname = \"edition\"\n\"edition\" = \"2018\"\n",
            None,
        ),
    ];
    for (manifest, root) in later {
        let (call, independent, messages) = edition_call(manifest, root);
        assert_eq!(call, UNCHANGED, "{manifest}");
        assert_eq!(independent, INDEPENDENT, "{manifest}");
        assert!(
            messages
                .iter()
                .any(|message| message.contains("not generated")),
            "{manifest}: {messages:#?}"
        );
    }
    let earlier = [
        ("[package]\nname = \"edition\"\nversion = \"0.0.0\"\n", None),
        (
            "[package] # 2015\nname = \"edition\"\nedition = \"2015\" # explicit\n",
            None,
        ),
        (
            "[package]\nname = \"edition\"\nedition.workspace = true\n",
            Some("[workspace]\nmembers = [\"package\"]\n[workspace.package]\nedition = \"2015\"\n"),
        ),
    ];
    for (manifest, root) in earlier {
        let (call, independent, messages) = edition_call(manifest, root);
        assert_eq!(call, RENAMED, "{manifest}");
        assert_eq!(independent, INDEPENDENT, "{manifest}");
        assert_eq!(messages.len(), 1, "{messages:#?}");
    }
}

/// An edition the tool cannot determine (a manifest that is not valid TOML,
/// `edition.workspace = true` without a workspace root that declares one, an
/// edition value it does not know, a package without a name) is unknown: a
/// path whose meaning depends on the edition is unknown and reported with that
/// cause, never resolved as edition 2015; a path that means the same in every
/// edition still migrates.
#[test]
fn round5_f03_n3_unknown_edition_makes_edition_dependent_paths_unknown() {
    let unknown = [
        ("[package\nname = \"edition\"\nedition = \"2024\"\n", None),
        (
            "[package]\nname = \"edition\"\nedition.workspace = true\n",
            None,
        ),
        (
            "[package]\nname = \"edition\"\nedition.workspace = true\n",
            Some("[workspace]\nmembers = [\"package\"]\n"),
        ),
        ("[package]\nname = \"edition\"\nedition = \"2031\"\n", None),
        ("[package]\nedition = \"2024\"\n", None),
        ("[workspace]\n", None),
    ];
    for (manifest, root) in unknown {
        let (call, independent, messages) = edition_call(manifest, root);
        assert_eq!(call, UNCHANGED, "{manifest}");
        assert_eq!(independent, INDEPENDENT, "{manifest}");
        assert!(
            messages.len() == 1
                && messages[0].starts_with("C3-MIGRATE-SETTER-UNRESOLVED")
                && messages[0].contains("edition"),
            "{manifest}: {messages:#?}"
        );
    }
}

// ----- N4: a setter named as a function value ---------------------------------

/// Compiled control `pathvalue` of the fourth review: `Type::set_x` named as a
/// value (passed to `fold`, bound to a variable, `Self::set_x` inside an
/// `impl` of the generated type, `<Type>::set_x`) is reported when the path
/// resolves to a generated optional setter or cannot be resolved to an item
/// that is not generated; the unrelated type's `set_x` is not reported and a
/// called UFCS setter still migrates.
#[test]
fn round5_f03_n4_setter_used_as_a_function_value_is_reported() {
    let callers = r#"
pub fn fold_values(client: &Api) {
    let _ = [None, None].into_iter().fold(client.item(), api::ItemRequest::set_x); // fold_setter_path
}
pub fn fn_pointer(client: &Api) {
    let f = api::ItemRequest::set_x; // setter_fn_item_binding
    let _ = f(client.item(), None);
}
pub fn qualified_self(client: &Api) {
    let f = <api::ItemRequest>::set_x; // qself_setter_path
    let _ = f(client.item(), None);
}
impl api::ItemRequest {
    pub fn helper(self) -> Self {
        let f = Self::set_x; // self_setter_path
        f(self, None)
    }
}
pub fn foreign_value() {
    let f = extcrate::Request::set_x; // other_crate_setter_path
    let _ = f;
}
pub fn unrelated_value() -> u32 {
    let f = other::ItemRequest::set_x; // unrelated_setter_path
    f(other::ItemRequest, None)
}
pub fn called(client: &Api) {
    let _ = api::ItemRequest::set_x(client.item(), None); // PROBE:GENUINE ufcs_call
}
"#;
    let reported = [
        "// fold_setter_path",
        "// setter_fn_item_binding",
        "// qself_setter_path",
        "// self_setter_path",
        "// other_crate_setter_path",
    ];
    let extra = reported.map(|fragment| ("C3-MIGRATE-SETTER-UNRESOLVED", fragment));
    let outcome = assert_probe(ROUND5_API, callers, &[], &extra);
    let source = format!("{ROUND5_API}{callers}");
    let reasons = reported.map(|fragment| (fragment, "used as a function value"));
    assert_reasons(&outcome, &source, &reasons);
    for fragment in reported {
        let line = unique_line(&source, fragment);
        let diagnostic = outcome
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.line == line)
            .expect("diagnostic");
        assert!(
            diagnostic
                .suggestion
                .as_deref()
                .is_some_and(|suggestion| suggestion.contains("::replace_x")),
            "{diagnostic:?}"
        );
    }
}

// ----- reason accuracy ---------------------------------------------------------

/// Reasons name the true cause: a trait implemented for a self type the tool
/// could not resolve (here gated by `#[cfg]`) is not claimed to be implemented
/// "for a probed type", and an import of another crate whose kind the tool
/// cannot see is not called "possibly a trait" as if it were a claim about it.
#[test]
fn round5_f03_reasons_name_unresolved_impl_self_types_and_foreign_imports() {
    let gated = r#"
#[cfg(feature = "json")]
pub struct Gated;
#[cfg(feature = "json")]
impl serde::Serialize for Gated {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_unit() }
}
pub fn boxed(r: Box<api::ItemRequest>) {
    let _ = r.set_x(None); // PROBE:UNPROVED gated_self_type
}
"#;
    let outcome = assert_probe(ROUND5_API, gated, &[], &[]);
    assert_reasons(
        &outcome,
        &format!("{ROUND5_API}{gated}"),
        &[(
            "gated_self_type",
            "for `Gated`, a type the tool could not resolve",
        )],
    );
    let foreign = r#"
use extcrate::Thing;
pub fn boxed(r: Box<api::ItemRequest>) {
    let _ = Thing::new();
    let _ = r.set_x(None); // PROBE:UNPROVED foreign_import
}
"#;
    let outcome = assert_probe(ROUND5_API, foreign, &[], &[]);
    assert_reasons(
        &outcome,
        &format!("{ROUND5_API}{foreign}"),
        &[(
            "foreign_import",
            "`extcrate::Thing` imported here is an item of another crate whose kind the tool cannot see",
        )],
    );
}

// ----- compiled control --------------------------------------------------------

const ROUND5_FIXTURE: &str = "round5_v2_1.rs.txt";
const ROUND5_GOLDEN: &str = "tests/migrated_v3_identity/round5.rs";
const ROUND5_MODULE_FILE: &str = "tests/migrated_v3_identity/round5_files/shared.rs";

/// `fixtures/migrate_v3/round5_v2_1.rs.txt`, migrated as the module `round5`
/// of a crate with its module file `round5_files/shared.rs` mounted twice, is
/// byte-for-byte `tests/migrated_v3_identity/round5.rs` (which
/// `tests/migrated_v3.rs` compiles and executes); the module file is unchanged.
/// Every unrelated call is reported and unchanged, every genuine call
/// migrated.
#[test]
fn round5_f03_golden_output_is_the_exact_tool_output() {
    let workspace = package("round5-golden", "round5");
    workspace.write("src/lib.rs", "pub mod round5;\n");
    let source = fixture(ROUND5_FIXTURE);
    workspace.write("src/round5.rs", &source);
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let module_file = fs::read_to_string(manifest.join(ROUND5_MODULE_FILE)).expect("module file");
    workspace.write("src/round5_files/shared.rs", &module_file);
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    let converted = migration.converted("round5.rs").expect("migrated fixture");
    assert_eq!(
        migration.converted("round5_files/shared.rs"),
        Some(module_file.as_str())
    );
    let shared = file_diagnostics(&migration, "round5_files/shared.rs");
    assert_eq!(shared.len(), 1, "{shared:#?}");
    assert_eq!(shared[0].0, "C3-MIGRATE-SETTER-UNRESOLVED");
    assert!(
        shared[0].2.contains("mounted more than once"),
        "{shared:#?}"
    );
    let line_of = |marker: &str| unique_line(&source, marker);
    let sites = |code: &str| {
        let mut lines = migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.file == "round5.rs" && diagnostic.code.as_str() == code)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let mut unrelated = [
        "// unrelated: inner const",
        "// unrelated: inner static",
        "// unrelated: inner unit struct",
        "// unrelated: inner use of a const",
        "// unrelated: inner glob of a const",
        "// unrelated: closure body const",
        "// unrelated: upper case local and const",
        "// unrelated: const shadows a parameter",
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
        "// genuine: plain binding",
        "// genuine: inner block item of another name",
        "// genuine: sibling block item",
        "// genuine: binding shadows its block's fn",
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
    assert_eq!(errors.len(), unrelated.len() + 1, "{errors:#?}");
    let path = manifest.join(ROUND5_GOLDEN);
    if std::env::var_os("CONCORD_MIGRATE_BLESS").is_some() {
        fs::write(&path, converted).expect("bless golden");
        return;
    }
    let golden = fs::read_to_string(&path).expect("golden");
    assert!(
        golden == converted,
        "{ROUND5_GOLDEN} differs from the migrate-v3 output:\n{}",
        super::super::edits::unified_diff(ROUND5_GOLDEN, &golden, converted)
    );
}
