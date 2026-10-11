//! Second post-fix review of F03 (round 3): automatic caller rewrites are
//! default-deny.
//!
//! Each second-review probe (`evidence/r2/postfix_review2/f03/postfix2_probe.rs`)
//! is a test here that checks the complete resulting caller text and every
//! diagnostic in it: setters, `paginate` calls and builder paths inside the
//! arguments of a macro that does not evaluate them in place, names that a
//! `#[cfg]`-gated declaration may bind, receivers that reach the generated
//! method only after an auto-deref/auto-ref step while a trait the tool cannot
//! see (macro-generated, or of another crate) may be selected first, raw
//! identifiers, callers in another crate, `api!` invocations that do not
//! resolve to Concord's macro, and deep nesting. Unknown sites stay unchanged
//! and carry an actionable diagnostic that names the precise reason; genuine
//! sites (exact receivers, standard expression macros, cfg-free or trivially
//! true `#[cfg]` code) still migrate.

use super::*;

/// The second reviewer's probe declaration, verbatim.
const REVIEW2_API: &str = r#"use concord::{api, Empty};
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
// callers
"#;

/// The second reviewer's paginate declaration, verbatim.
const REVIEW2_PAGINATE_API: &str = r#"use std::num::NonZeroU32;
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
pub mod repo {
    pub struct MyRepo;
    impl MyRepo {
        pub fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
        pub fn collect_prefix(self, _: u32) -> u32 { 2 }
    }
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

// ----- (1) macro arguments -----------------------------------------------------

/// Probe `postfix2_probe_macro_arguments`: a crate-local macro that rebinds
/// the name it is given, one that calls the method tokens on another receiver,
/// and `stringify!` (whose output would change) are never rewritten; only the
/// argument of a standard expression macro (`format!`) is.
#[test]
fn round3_f03_second_review_macro_arguments() {
    let callers = r#"
macro_rules! with_other {
    ($n:ident, $e:expr) => {{ let $n = $crate::other::ItemRequest; $e }};
}
pub fn macro_rebinds_argument(r: api::ItemRequest) -> u32 {
    with_other!(r, r.set_x(None)) // PROBE:UNPROVED macro_argument_rebinding
}
macro_rules! on_other {
    ($r:ident . $($call:tt)*) => {{ let _ = &$r; $crate::other::ItemRequest.$($call)* }};
}
pub fn macro_restructures_tokens(r: api::ItemRequest) -> u32 {
    on_other!(r.set_x(None)) // PROBE:UNPROVED macro_token_restructuring
}
pub fn std_macro_genuine(r: api::ItemRequest) {
    let _ = format!("{:?}", { let _ = r.set_x(None); 1 }); // PROBE:GENUINE std_format_argument
}
pub fn stringify_changes_text(r: api::ItemRequest) -> &'static str {
    let _ = r;
    stringify!(r.set_x(None)) // PROBE:UNTOUCHED stringify_argument
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED macro_argument_rebinding",
                "inside the arguments of `with_other!`",
            ),
            (
                "// PROBE:UNPROVED macro_token_restructuring",
                "inside the arguments of `on_other!`",
            ),
        ],
    );
}

/// Only the expression arguments of standard-library macros that evaluate
/// them in place are rewritten (`assert*!`, `dbg!`, `vec!`, `print*!`,
/// `format!` with a named argument, `write!`, qualified `std::`/`::core::`
/// paths). `matches!` (a pattern), another crate's macro and `log::info!` are
/// reported instead; token-only macros (`stringify!`, `concat!`) are left
/// alone without a diagnostic. (Another crate's statement macro may add items
/// to its block, so it is tested in a block of its own.)
#[test]
fn round3_f03_std_expression_macros_are_the_only_rewritten_macro_arguments() {
    let callers = r#"
pub fn std_macros(client: &Api, out: &mut String) -> std::fmt::Result {
    assert!(client.item().set_x(None).plan().is_ok()); // PROBE:GENUINE assert
    assert_eq!(client.item().set_x(None).plan().is_ok(), true, "{}", 1); // PROBE:GENUINE assert_eq
    debug_assert_ne!(client.item().set_x(None).plan().is_ok(), false); // PROBE:GENUINE debug_assert_ne
    let _ = dbg!(client.item().set_x(None)); // PROBE:GENUINE dbg
    let _ = vec![client.item().set_x(None)]; // PROBE:GENUINE vec_list
    println!("{:?}", client.item().set_x(None).plan().is_ok()); // PROBE:GENUINE println
    std::println!("{:?}", client.item().set_x(None).plan().is_ok()); // PROBE:GENUINE qualified_std
    ::core::assert!(client.item().set_x(None).plan().is_ok()); // PROBE:GENUINE qualified_core
    let _ = format!("{v:?}", v = client.item().set_x(None).plan().is_ok()); // PROBE:GENUINE named_argument
    use std::fmt::Write as _;
    write!(out, "{:?}", client.item().set_x(None).plan().is_ok())?; // PROBE:GENUINE write
    let _ = matches!(client.item().set_x(None).plan(), Ok(_)); // PROBE:UNPROVED matches_pattern_macro
    let _ = concat!("set_x", "(None)");
    let _ = stringify!(client.item().set_x(None)); // PROBE:UNTOUCHED stringify
    log::info!("{:?}", client.item().set_x(None).plan().is_ok()); // PROBE:UNPROVED log_macro
    Ok(())
}
pub fn other_crate_macro(client: &Api) {
    helpers::check!(client.item().set_x(None)); // PROBE:UNPROVED other_crate_macro
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED matches_pattern_macro",
                "inside the arguments of `matches!`",
            ),
            (
                "// PROBE:UNPROVED log_macro",
                "inside the arguments of `log::info!`",
            ),
            (
                "// PROBE:UNPROVED other_crate_macro",
                "inside the arguments of `helpers::check!`",
            ),
        ],
    );
}

/// A standard macro name is the standard macro only when its path resolves to
/// it: a crate-local `macro_rules! format`, an imported `vec` of another crate
/// and `assert!` under another crate's glob are reported.
#[test]
fn round3_f03_shadowed_standard_macro_names_are_not_standard() {
    let callers = r#"
macro_rules! format {
    ($($tokens:tt)*) => { String::new() };
}
pub fn crate_format(client: &Api) {
    let _ = format!("{:?}", client.item().set_x(None)); // PROBE:UNPROVED crate_macro_named_format
}
mod imported {
    use helpers::vec;
    pub fn imported(client: &super::Api) {
        let _ = vec![client.item().set_x(None)]; // PROBE:UNPROVED imported_macro_named_vec
    }
}
mod globbed {
    use helpers::*;
    pub fn globbed(client: &super::Api) {
        assert!(client.item().set_x(None).plan().is_ok()); // PROBE:UNPROVED std_name_under_foreign_glob
    }
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED crate_macro_named_format",
                "inside the arguments of `format!`",
            ),
            (
                "// PROBE:UNPROVED imported_macro_named_vec",
                "inside the arguments of `vec!`",
            ),
            (
                "// PROBE:UNPROVED std_name_under_foreign_glob",
                "inside the arguments of `assert!`",
            ),
        ],
    );
}

/// Probe `postfix2_probe_builder_paths`: a reference proved to be the v2.1
/// crate-level builder moves into the support module, a local struct with the
/// same name is untouched, a support-module name taken by a local module is
/// reported, and `stringify!(ApiBuilder)` (whose output would change) is left
/// alone.
#[test]
fn round3_f03_second_review_builder_paths() {
    let callers = r#"
pub fn builder_type(b: ApiBuilder) -> ApiBuilder { b } // site: builder_type_path
pub fn builder_new() -> ApiBuilder { ApiBuilder::new() } // site: builder_new
pub mod local_builder {
    pub struct ApiBuilder;
    pub fn local(b: ApiBuilder) -> ApiBuilder { b } // site: local_struct_named_like_builder
}
pub mod shadowed_support {
    use super::*;
    pub mod api {}
    pub fn through_glob(b: ApiBuilder) -> ApiBuilder { b } // site: support_module_shadowed
}
pub fn stringified() -> &'static str { stringify!(ApiBuilder) } // site: stringify_builder
"#;
    let path = "C3-MIGRATE-BUILDER-PATH";
    let unresolved = "C3-MIGRATE-BUILDER-PATH-UNRESOLVED";
    assert_probe(
        REVIEW2_API,
        callers,
        &[
            (
                "pub fn builder_type(b: ApiBuilder) -> ApiBuilder { b }",
                "pub fn builder_type(b: api::ApiBuilder) -> api::ApiBuilder { b }",
            ),
            (
                "pub fn builder_new() -> ApiBuilder { ApiBuilder::new() }",
                "pub fn builder_new() -> api::ApiBuilder { api::ApiBuilder::new() }",
            ),
        ],
        &[
            (path, "// site: builder_type_path"),
            (path, "// site: builder_type_path"),
            (path, "// site: builder_new"),
            (path, "// site: builder_new"),
            (unresolved, "// site: support_module_shadowed"),
            (unresolved, "// site: support_module_shadowed"),
        ],
    );
}

// ----- (2) `#[cfg]` -------------------------------------------------------------

/// Probe `postfix2_probe_cfg`: a `let` gated by `#[cfg(any())]` and an import
/// gated by a feature may not exist, so the names they bind are unknown; a
/// trivially true `#[cfg(all())]` binding is certain.
#[test]
fn round3_f03_second_review_cfg() {
    let callers = r#"
pub fn cfg_let(r: other::ItemRequest, g: api::ItemRequest) -> u32 {
    let _keep = g;
    #[cfg(any())]
    let r = _keep;
    r.set_x(None) // PROBE:UNPROVED cfg_disabled_let_shadow
}
pub mod cfg_import {
    use super::other::*;
    #[cfg(feature = "generated-requests")]
    use super::api::ItemRequest;
    pub fn call(r: ItemRequest) -> u32 {
        r.set_x(None) // PROBE:UNPROVED cfg_disabled_import_over_glob
    }
}
pub fn cfg_active_let(r: other::ItemRequest, g: api::ItemRequest) {
    let _keep = r;
    #[cfg(all())]
    let r = g;
    let _ = r.set_x(None); // PROBE:GENUINE cfg_enabled_let_shadow
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED cfg_disabled_let_shadow",
                "gated by `#[cfg]`",
            ),
            (
                "// PROBE:UNPROVED cfg_disabled_import_over_glob",
                "gated by `#[cfg]`",
            ),
        ],
    );
}

/// Beyond the probe: gated parameters, named and tuple fields, a gated
/// block-local function and a `cfg_attr` that may expand to `cfg` make their
/// names unknown; an inert `cfg_attr`, a trivially true `cfg(not(any()))` and
/// code inside a gated module are certain.
#[test]
fn round3_f03_cfg_gated_parameters_fields_items_and_attributes() {
    let callers = r#"
pub fn cfg_parameter(
    #[cfg(not(feature = "a"))] r: other::ItemRequest,
    #[cfg(feature = "a")] r: api::ItemRequest,
) -> u32 {
    r.set_x(None) // PROBE:UNPROVED cfg_gated_parameter
}
pub struct Holder {
    #[cfg(not(feature = "a"))]
    pub inner: other::ItemRequest,
    #[cfg(feature = "a")]
    pub inner: api::ItemRequest,
}
pub fn cfg_field(h: Holder) -> u32 {
    h.inner.set_x(None) // PROBE:UNPROVED cfg_gated_field
}
pub struct Pair(#[cfg(feature = "a")] pub api::ItemRequest, pub other::ItemRequest);
pub fn cfg_tuple_field(p: Pair) -> u32 {
    p.0.set_x(None) // PROBE:UNPROVED cfg_shifted_tuple_field
}
pub fn make_foreign() -> other::ItemRequest { other::ItemRequest }
pub fn cfg_block_function() -> u32 {
    #[cfg(feature = "a")]
    fn make_foreign() -> api::ItemRequest { Api::new().expect("fixture client").item() }
    make_foreign().set_x(None) // PROBE:UNPROVED cfg_gated_block_function
}
pub fn cfg_attr_let(r: other::ItemRequest, g: api::ItemRequest) -> u32 {
    let _keep = r;
    #[cfg_attr(feature = "a", cfg(any()))]
    let r = g;
    r.set_x(None) // PROBE:UNPROVED cfg_attr_may_gate_let
}
pub fn inert_cfg_attr_let(r: other::ItemRequest, g: api::ItemRequest) {
    let _keep = r;
    #[cfg_attr(feature = "a", allow(unused_variables))]
    let r = g;
    let _ = r.set_x(None); // PROBE:GENUINE inert_cfg_attr_let
}
pub fn trivially_true_not_any(r: other::ItemRequest, g: api::ItemRequest) {
    let _keep = r;
    #[cfg(not(any()))]
    let r = g;
    let _ = r.set_x(None); // PROBE:GENUINE trivially_true_not_any
}
#[cfg(test)]
mod tests {
    pub fn inside_gated_module(r: super::api::ItemRequest) {
        let _ = r.set_x(None); // PROBE:GENUINE inside_cfg_test_module
    }
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            ("// PROBE:UNPROVED cfg_gated_parameter", "gated by `#[cfg]`"),
            ("// PROBE:UNPROVED cfg_gated_field", "gated by `#[cfg]`"),
            (
                "// PROBE:UNPROVED cfg_shifted_tuple_field",
                "gated by `#[cfg]`",
            ),
            (
                "// PROBE:UNPROVED cfg_gated_block_function",
                "gated by `#[cfg]`",
            ),
            (
                "// PROBE:UNPROVED cfg_attr_may_gate_let",
                "gated by `#[cfg]`",
            ),
        ],
    );
}

// ----- (3) trait implementations the model cannot see ---------------------------

/// Probe `postfix2_probe_macro_generated_impls`: trait implementations
/// generated by a crate-local item macro (on `Box<Request>`, and a by-value
/// navigation on the client) may be selected before the generated method, so
/// both calls are unknown.
#[test]
fn round3_f03_second_review_macro_generated_impls() {
    let callers = r#"
pub trait Patch {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
macro_rules! patch_for {
    ($t:ty) => {
        impl crate::Patch for $t {
            fn set_x(self, _: Option<u32>) -> u32 { 1 }
            fn replace_x(self, _: Option<u32>) -> u32 { 2 }
        }
    };
}
mod impls {
    patch_for!(Box<crate::api::ItemRequest>);
}
pub fn macro_impl_box(b: Box<api::ItemRequest>) -> u32 {
    b.set_x(None) // PROBE:UNPROVED macro_generated_impl_on_box
}
pub trait Nav {
    fn item(self) -> other::ItemRequest;
}
macro_rules! nav_for {
    ($t:ty) => {
        impl crate::Nav for $t {
            fn item(self) -> crate::other::ItemRequest { crate::other::ItemRequest }
        }
    };
}
mod nav_impls {
    nav_for!(crate::Api);
}
pub fn macro_impl_navigation(client: Api) -> u32 {
    client.item().set_x(None) // PROBE:UNPROVED macro_generated_by_value_navigation
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED macro_generated_impl_on_box",
                "`patch_for!`",
            ),
            (
                "// PROBE:UNPROVED macro_generated_by_value_navigation",
                "may generate trait implementations",
            ),
        ],
    );
}

/// Probe `postfix2_probe_direct_impls_control` (it passed on the reviewed
/// source; kept as a regression guard): the same implementations written
/// directly are seen.
#[test]
fn round3_f03_second_review_direct_impls_control() {
    let callers = r#"
pub trait Patch {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
impl Patch for Box<api::ItemRequest> {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
pub fn direct_impl_box(b: Box<api::ItemRequest>) -> u32 {
    b.set_x(None) // PROBE:UNPROVED direct_impl_on_box
}
pub trait Nav {
    fn item(self) -> other::ItemRequest;
}
impl Nav for Api {
    fn item(self) -> other::ItemRequest { other::ItemRequest }
}
pub fn direct_impl_navigation(client: Api) -> u32 {
    client.item().set_x(None) // PROBE:UNPROVED direct_by_value_navigation
}
"#;
    assert_probe(REVIEW2_API, callers, &[], &[]);
}

/// Every construct that may add a trait implementation the model cannot see
/// makes the calls that reach the generated method only after an auto-deref
/// or auto-ref step unknown: `include!`, an attribute macro, a derive of
/// another crate and a module without a source file. Calls whose receiver
/// reaches the generated method at the first probe step (an exact request,
/// navigation through `&Api`) stay proved. Standard and `serde` derives,
/// standard attributes and crate-local `macro_rules!` that cannot expand to
/// an implementation keep the crate model complete.
#[test]
fn round3_f03_incomplete_crate_models_block_interception_prone_calls() {
    let genuine_controls = r#"
pub fn exact(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE exact_receiver
}
pub fn reference(client: &Api) {
    let _ = client.item().set_x(None); // PROBE:GENUINE reference_navigation
}
"#;
    for (construct, reason) in [
        (
            "mod hidden {\n    include!(\"generated_impls.rs\");\n}\n",
            "`include!`",
        ),
        (
            "mod hidden {\n    #[helpers::patch]\n    pub struct Marker;\n}\n",
            "`#[helpers::patch]`",
        ),
        (
            "mod hidden {\n    #[derive(Clone, helpers::Patch)]\n    pub struct Marker;\n}\n",
            "`helpers::Patch`",
        ),
        ("mod missing;\n", "`missing`"),
    ] {
        let callers = format!(
            "\n{construct}pub fn boxed(b: Box<api::ItemRequest>) -> u32 {{\n    b.set_x(None) // PROBE:UNPROVED boxed\n}}\npub fn navigation(client: Api) -> u32 {{\n    client.item().set_x(None) // PROBE:UNPROVED value_navigation\n}}\n{genuine_controls}"
        );
        assert_probe_reasons(
            REVIEW2_API,
            &callers,
            &[],
            &[],
            &[
                ("// PROBE:UNPROVED boxed", reason),
                ("// PROBE:UNPROVED value_navigation", reason),
            ],
        );
    }

    let complete = r#"
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Plain;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dto {
    #[serde(default)]
    pub field: u32,
}
macro_rules! local {
    ($name:ident) => { let $name = 1; };
}
macro_rules! items {
    () => { pub struct Generated; };
}
mod generated_items {
    items!();
}
pub fn uses_local() -> u32 {
    local!(n);
    n
}
#[must_use]
#[inline]
pub fn boxed(b: Box<api::ItemRequest>) -> api::ItemRequest {
    b.set_x(None) // PROBE:GENUINE box_in_complete_crate
}
#[allow(clippy::needless_pass_by_value)]
pub fn navigation(client: Api) {
    let _ = client.item().set_x(None); // PROBE:GENUINE value_navigation_in_complete_crate
}
"#;
    assert_probe(REVIEW2_API, complete, &[], &[]);
}

/// An item annotated with an attribute macro whose expansion is not known is
/// that macro's input (the macro receives its tokens and may emit anything in
/// its place): a setter inside it is reported, never rewritten, and the macro
/// makes the crate model incomplete. `#[tokio::test]` and `#[tokio::main]`
/// (whose paths resolve to the `tokio` crate) and the prelude's `#[test]` are
/// known: code inside and beside them migrates, interception-prone calls
/// included. A `#[test]` that an import replaces and a `tokio` path naming a
/// crate-local module are unknown.
#[test]
fn round3_f03_attribute_macros_are_known_or_their_items_are_macro_input() {
    let known = r#"
#[tokio::test]
async fn tokio_test(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE inside_tokio_test
}
#[tokio::main(flavor = "current_thread")]
async fn tokio_main(client: Api) {
    let _ = client.item().set_x(None); // PROBE:GENUINE value_navigation_inside_tokio_main
}
#[test]
fn prelude_test(r: api::ItemRequest) {
    assert_eq!(r.set_x(None), ()); // PROBE:GENUINE assert_inside_prelude_test
}
pub fn boxed(b: Box<api::ItemRequest>) -> api::ItemRequest {
    b.set_x(None) // PROBE:GENUINE box_beside_known_attribute_macros
}
"#;
    assert_probe(REVIEW2_API, known, &[], &[]);

    let unknown = r#"
#[helpers::instrument]
pub fn wrapped(r: api::ItemRequest) -> u32 {
    let _ = r.set_x(None); // PROBE:UNPROVED inside_unknown_attribute_macro
    0
}
pub struct Holder;
impl Holder {
    #[helpers::instrument]
    pub fn member(&self, r: api::ItemRequest) {
        let _ = r.set_x(None); // PROBE:UNPROVED inside_unknown_member_attribute_macro
    }
}
mod replaced_test {
    use helpers::wrap as test;
    #[test]
    fn replaced(r: crate::api::ItemRequest) {
        let _ = r.set_x(None); // PROBE:UNPROVED inside_replaced_prelude_test
    }
}
mod local_tokio {
    mod tokio {
        pub use helpers::wrap as test;
    }
    #[tokio::test]
    async fn local(r: crate::api::ItemRequest) {
        let _ = r.set_x(None); // PROBE:UNPROVED inside_local_tokio_path
    }
}
pub fn exact(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE exact_receiver_beside_unknown_attribute_macro
}
pub fn navigation(client: Api) {
    let _ = client.item().set_x(None); // PROBE:UNPROVED value_navigation_beside_unknown_attribute_macro
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        unknown,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED inside_unknown_attribute_macro",
                "inside the item annotated with the attribute macro `#[helpers::instrument]`",
            ),
            (
                "// PROBE:UNPROVED inside_unknown_member_attribute_macro",
                "inside the item annotated with the attribute macro `#[helpers::instrument]`",
            ),
            (
                "// PROBE:UNPROVED inside_replaced_prelude_test",
                "inside the item annotated with the attribute macro `#[test]`",
            ),
            (
                "// PROBE:UNPROVED inside_local_tokio_path",
                "inside the item annotated with the attribute macro `#[tokio::test]`",
            ),
            (
                "// PROBE:UNPROVED value_navigation_beside_unknown_attribute_macro",
                "the crate uses the attribute macro `#[helpers::instrument]`",
            ),
        ],
    );
}

/// A crate-local implementation of a trait the crate does not define (here the
/// prelude's `Iterator`) may provide methods the model does not list: its
/// provided `count(self)` is found on a client value before the generated
/// `&self` navigation `count`. Through `&Api` the generated method is found
/// first.
#[test]
fn round3_f03_implementations_of_foreign_traits_have_unknown_methods() {
    let api = REVIEW2_API.replace("GET item(x?: u32)", "GET count(x?: u32)");
    let callers = r#"
impl Iterator for Api {
    type Item = u32;
    fn next(&mut self) -> Option<u32> { None }
}
pub fn value_navigation(client: Api) -> u32 {
    client.count().set_x(None) // PROBE:UNPROVED provided_iterator_method
}
pub fn reference_navigation(client: &Api) {
    let _ = client.count().set_x(None); // PROBE:GENUINE reference_navigation
}
"#;
    assert_probe_reasons(
        &api,
        callers,
        &[],
        &[],
        &[("// PROBE:UNPROVED provided_iterator_method", "`Iterator`")],
    );
}

// ----- (4) traits of another crate ------------------------------------------------

/// Probe `postfix2_probe_external_trait`: a trait imported from another crate
/// (invisible to the crate model) may intercept a `Box<Request>` receiver at
/// the `Box` step.
#[test]
fn round3_f03_second_review_external_trait() {
    let callers = r#"
use extpatch::Patch;
pub fn external_blanket_box(b: Box<api::ItemRequest>) -> u32 {
    b.set_x(None) // PROBE:UNPROVED external_blanket_trait_box
}
pub fn external_default_method_box(b: Box<api::ItemRequest>) -> u32 {
    extpatch::touch();
    b.set_x(None) // PROBE:UNPROVED external_trait_default_method_box
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED external_blanket_trait_box",
                "`extpatch::Patch`",
            ),
            (
                "// PROBE:UNPROVED external_trait_default_method_box",
                "`extpatch::Patch`",
            ),
        ],
    );
}

/// Traits in scope are decided per scope: an import (explicit, `as _`,
/// block-local, a glob of another crate, or a glob of a crate-local module
/// re-exporting another crate's item) makes the interception-prone calls of
/// that scope unknown, while an exact receiver there, another module without
/// such imports, and imports of the standard library or of `serde`'s
/// `Serialize` (whose only method is `serialize`) keep them proved.
#[test]
fn round3_f03_external_traits_in_scope_block_interception_prone_calls() {
    let callers = r#"
use extpatch::Patch;
pub fn exact_receiver(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE exact_receiver_beside_external_import
}
pub fn value_navigation(client: Api) -> u32 {
    client.item().set_x(None) // PROBE:UNPROVED by_ref_navigation_on_value
}
pub mod clean {
    pub fn boxed(b: Box<super::api::ItemRequest>) {
        let _ = b.set_x(None); // PROBE:GENUINE box_without_foreign_import
    }
    pub fn navigation(client: super::Api) {
        let _ = client.item().set_x(None); // PROBE:GENUINE value_navigation_without_foreign_import
    }
}
pub mod known {
    use serde::Serialize;
    use std::fmt::Write;
    pub fn boxed(b: Box<super::api::ItemRequest>) {
        let _ = b.set_x(None); // PROBE:GENUINE box_beside_serde_and_std_traits
    }
}
pub mod globbed {
    use extpatch::*;
    pub fn boxed(b: Box<super::api::ItemRequest>) -> u32 {
        b.set_x(None) // PROBE:UNPROVED external_glob
    }
}
pub mod reexport {
    pub use extpatch::Patch;
}
pub mod through_reexport {
    use super::reexport::*;
    pub fn boxed(b: Box<super::api::ItemRequest>) -> u32 {
        b.set_x(None) // PROBE:UNPROVED crate_glob_of_external_reexport
    }
}
pub mod underscore {
    use extpatch::Patch as _;
    pub fn boxed(b: Box<super::api::ItemRequest>) -> u32 {
        b.set_x(None) // PROBE:UNPROVED underscore_import
    }
}
pub mod block_import {
    pub fn boxed(b: Box<super::api::ItemRequest>) -> u32 {
        use extpatch::Patch;
        b.set_x(None) // PROBE:UNPROVED block_import
    }
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[
            (
                "// PROBE:UNPROVED by_ref_navigation_on_value",
                "`extpatch::Patch`",
            ),
            ("// PROBE:UNPROVED external_glob", "`extpatch::*`"),
            (
                "// PROBE:UNPROVED crate_glob_of_external_reexport",
                "`extpatch::Patch`",
            ),
            ("// PROBE:UNPROVED underscore_import", "`extpatch::Patch`"),
            ("// PROBE:UNPROVED block_import", "`extpatch::Patch`"),
        ],
    );
}

// ----- raw identifiers ----------------------------------------------------------

/// Probe `postfix2_probe_raw_method_name`: `r#set_x` is the generated setter
/// `set_x`; a proved call becomes `r#replace_x` (method and fully qualified
/// forms), an unproved one is reported.
#[test]
fn round3_f03_second_review_raw_method_names() {
    let callers = r#"
pub fn raw_method(r: api::ItemRequest) {
    let _ = r.r#set_x(None); // PROBE:GENUINE raw_method_name
}
pub fn raw_ufcs(r: api::ItemRequest) {
    let _ = api::ItemRequest::r#set_x(r, None); // PROBE:GENUINE raw_ufcs_name
}
pub fn raw_foreign(r: other::ItemRequest) -> u32 {
    r.r#set_x(None) // PROBE:UNPROVED raw_method_on_foreign_type
}
macro_rules! keep { ($e:expr) => { $e }; }
pub fn raw_in_macro(r: api::ItemRequest) -> api::ItemRequest {
    keep!(r.r#set_x(None)) // PROBE:UNPROVED raw_method_in_macro
}
"#;
    assert_probe_reasons(
        REVIEW2_API,
        callers,
        &[],
        &[],
        &[(
            "// PROBE:UNPROVED raw_method_in_macro",
            "inside the arguments of `keep!`",
        )],
    );
}

// ----- paginate -------------------------------------------------------------------

/// Probe `postfix2_probe_paginate_macro_and_cfg`: the same macro-argument and
/// `#[cfg]` classes reach the `paginate` rewrite; both calls stay unchanged
/// and are reported with their remaining `PageLimit` references.
#[test]
fn round3_f03_second_review_paginate_macro_and_cfg() {
    let callers = r#"
macro_rules! on_repo {
    ($r:ident . $($call:tt)*) => {{ let _ = &$r; $crate::repo::MyRepo.$($call)* }};
}
pub fn paginate_in_user_macro(client: Pages) -> u32 {
    on_repo!(client.list().paginate(PageLimit::Prefix(2))) // PROBE:UNPROVED paginate_macro_restructuring
}
pub fn paginate_cfg(r: repo::MyRepo, g: pages::ListRequest) -> u32 {
    let _keep = g;
    #[cfg(any())]
    let r = _keep;
    r.paginate(PageLimit::Prefix(2)) // PROBE:UNPROVED paginate_cfg_disabled_let
}
pub fn genuine(client: &Pages) {
    let _ = client.list().r#paginate(PageLimit::CompleteWithin(NonZeroU32::MIN)); // genuine: raw paginate
}
"#;
    let source = format!("{REVIEW2_PAGINATE_API}{callers}");
    let outcome = convert(&source);
    let (_, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    let expected = callers.replace(
        "client.list().r#paginate(PageLimit::CompleteWithin(NonZeroU32::MIN));",
        "client.list().r#collect_all(NonZeroU32::MIN);",
    );
    assert!(
        tail == expected,
        "caller text:\n{}",
        super::super::edits::unified_diff("callers", &expected, tail)
    );
    assert_eq!(
        all_diagnostics(&outcome),
        expected_diagnostics(
            &source,
            &[
                (
                    "C3-MIGRATE-PAGINATION-INIT",
                    "paginate via concord::pagination::OffsetPagination"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "pub fn paginate(self, _: concord::PageLimit) -> u32 { 1 }"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "// PROBE:UNPROVED paginate_macro_restructuring"
                ),
                (
                    "C3-MIGRATE-PAGINATE-UNRESOLVED",
                    "// PROBE:UNPROVED paginate_macro_restructuring"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "// PROBE:UNPROVED paginate_cfg_disabled_let"
                ),
                (
                    "C3-MIGRATE-PAGINATE-UNRESOLVED",
                    "// PROBE:UNPROVED paginate_cfg_disabled_let"
                ),
                (
                    "C3-MIGRATE-PAGINATE-COLLECT-ALL",
                    "// genuine: raw paginate"
                ),
            ]
        ),
        "{:#?}",
        outcome.diagnostics
    );
    assert_reasons(
        &outcome,
        &source,
        &[
            (
                "// PROBE:UNPROVED paginate_macro_restructuring",
                "inside the arguments of `on_repo!`",
            ),
            (
                "// PROBE:UNPROVED paginate_cfg_disabled_let",
                "gated by `#[cfg]`",
            ),
        ],
    );
}

// ----- other crates -------------------------------------------------------------

/// Probe `postfix2_probe_cross_crate_callers`: a crate of the same run calling
/// a setter of a type generated by another crate of the run (a method call, a
/// fully qualified call, a navigation chain) is never rewritten and is
/// reported, because `x?: Option<u32>` changes meaning in v3.
#[test]
fn round3_f03_second_review_cross_crate_callers() {
    let workspace = TempDir::new("round3-cross-crate");
    let api = REVIEW2_API
        .split("pub mod other")
        .next()
        .expect("declaration")
        .replace("GET item(x?: u32)", "GET item(x?: Option<u32>)");
    workspace.write(
        "up/Cargo.toml",
        "[package]\nname = \"up\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write("up/src/lib.rs", &api);
    workspace.write(
        "down/Cargo.toml",
        "[package]\nname = \"down\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nup = { path = \"../up\" }\n",
    );
    let down = "pub fn omit(r: up::api::ItemRequest) -> up::api::ItemRequest {
    r.set_x(None) // cross-crate: method
}
pub fn ufcs(r: up::api::ItemRequest) -> up::api::ItemRequest {
    up::api::ItemRequest::set_x(r, None) // cross-crate: qualified
}
pub fn chained(client: &up::Api) -> up::api::ItemRequest {
    client.item().set_x(None) // cross-crate: chain
}
";
    workspace.write("down/src/lib.rs", down);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migrate");
    assert_eq!(migration.converted("down/src/lib.rs"), Some(down));
    let mut found = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == "down/src/lib.rs")
        .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.line))
        .collect::<Vec<_>>();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            ("C3-MIGRATE-SETTER-UNRESOLVED", 2),
            ("C3-MIGRATE-SETTER-UNRESOLVED", 5),
            ("C3-MIGRATE-SETTER-UNRESOLVED", 8),
        ],
        "{:#?}",
        migration.diagnostics
    );
    for diagnostic in &migration.diagnostics {
        if diagnostic.file == "down/src/lib.rs" {
            assert!(
                diagnostic.message.contains("another crate")
                    && diagnostic.message.contains("left unchanged"),
                "{diagnostic:?}"
            );
            assert!(
                diagnostic
                    .suggestion
                    .as_deref()
                    .is_some_and(|suggestion| suggestion.contains("replace_x")),
                "{diagnostic:?}"
            );
        }
    }
}

// ----- `api!` detection -----------------------------------------------------------

/// Nested-use note of the second review (`f03/nested_use`): a submodule that
/// imports the generated module by name (`use super::api;`) does not hide the
/// file's `api!`, whose path resolves to Concord's macro in its own module.
#[test]
fn round3_f03_nested_generated_module_import_keeps_the_declaration() {
    let source = r#"use concord::{Empty, api};

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

pub mod callers {
    use super::api;
    pub fn f(r: api::ItemRequest) -> api::ItemRequest { r.set_x(None) }
}
pub fn g(r: api::ItemRequest) -> api::ItemRequest { r.set_x(None) }
"#;
    let outcome = convert(source);
    assert_eq!(outcome.declarations, 1, "{:#?}", outcome.diagnostics);
    assert!(
        outcome.converted.contains(
            "    pub fn f(r: api::ItemRequest) -> api::ItemRequest { r.replace_x(None) }\n"
        )
    );
    assert!(
        outcome
            .converted
            .contains("pub fn g(r: api::ItemRequest) -> api::ItemRequest { r.replace_x(None) }\n")
    );
    assert!(outcome.converted.contains("GET item(query x?: u32)"));
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
    assert!(errors(&outcome).is_empty(), "{:#?}", errors(&outcome));
}

/// An `api!`-named invocation whose macro path is not proved to be Concord's
/// (another crate's `api!`, a `crate::` path, a crate-local `macro_rules!`
/// named `api`, a bare `api!` under another crate's glob) is reported and not
/// counted as a declaration; one reached through a module of the same file
/// that imports Concord's macro is migrated.
#[test]
fn round3_f03_api_macros_that_are_not_proved_concord_are_reported() {
    let declaration = r#"{
    client Probe {
        base "https://probe.example.test/";
        timeout 5s;
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}
"#;
    let unresolved = "C3-MIGRATE-API-MACRO-UNRESOLVED";
    for (prefix, invocation) in [
        ("use concord::Empty;\n", "other_crate::api!"),
        ("use concord::Empty;\n", "crate::api!"),
        (
            "use concord::Empty;\nmacro_rules! api { ($($tokens:tt)*) => {}; }\n",
            "api!",
        ),
        ("use concord::Empty;\nuse helpers::*;\n", "api!"),
    ] {
        let source = format!("{prefix}{invocation} {declaration}");
        let outcome = convert(&source);
        assert_eq!(outcome.declarations, 0, "{source}");
        assert_eq!(outcome.converted, source, "{source}");
        let reported = outcome
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_str() == unresolved)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        assert_eq!(
            reported,
            [unique_line(&source, invocation)],
            "{source}\n{:#?}",
            outcome.diagnostics
        );
        assert!(
            outcome
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity() == Severity::Error),
            "{:#?}",
            outcome.diagnostics
        );
    }

    let through_module = format!(
        "use concord::Empty;\npub mod facade {{\n    pub use concord::api;\n}}\nfacade::api! {declaration}"
    );
    let outcome = convert(&through_module);
    assert_eq!(outcome.declarations, 1, "{:#?}", outcome.diagnostics);
    assert!(outcome.converted.contains("GET item(query x?: u32)"));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.as_str() != unresolved),
        "{:#?}",
        outcome.diagnostics
    );
}

// ----- robustness -----------------------------------------------------------------

/// Probe `postfix2_probe_robustness_globs`: a chain of glob re-exports is
/// followed up to the model's depth bound (48 lookups; deeper chains are
/// unknown and reported), and a dense 60-module glob fan resolves quickly.
#[test]
fn round3_f03_second_review_robustness_globs() {
    let chain = |length: usize, kind: &str| {
        let mut chain = String::from("\npub mod c0 { pub use crate::api::*; }\n");
        for level in 1..length {
            chain.push_str(&format!(
                "pub mod c{level} {{ pub use super::c{}::*; }}\n",
                level - 1
            ));
        }
        chain.push_str(&format!(
            "pub fn chain(r: c{}::ItemRequest) {{ let _ = r.set_x(None); }} // PROBE:{kind} glob_chain_{length}\n",
            length - 1
        ));
        chain
    };
    let started = std::time::Instant::now();
    for (length, kind) in [
        (40, "GENUINE"),
        (47, "GENUINE"),
        (48, "GENUINE"),
        (60, "UNKNOWN"),
        (200, "UNKNOWN"),
    ] {
        assert_probe(REVIEW2_API, &chain(length, kind), &[], &[]);
    }
    let mut fan = String::from("\npub mod f0 { pub struct Leaf; }\n");
    for level in 1..60 {
        let globs = (0..level)
            .map(|previous| format!("pub use super::f{previous}::*;"))
            .collect::<String>();
        fan.push_str(&format!("pub mod f{level} {{ {globs} }}\n"));
    }
    fan.push_str(
        "pub fn fan(r: f59::Missing) -> u32 { r.set_x(None) } // PROBE:UNKNOWN glob_fan_missing\n",
    );
    assert_probe(REVIEW2_API, &fan, &[], &[]);
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "glob chains and fan took {elapsed:?}"
    );
}

/// Probe `postfix2_probe_robustness_alias_chain`: long chains of type aliases
/// exceed the model's depth bound and are reported, never guessed.
#[test]
fn round3_f03_second_review_robustness_alias_chain() {
    for length in [40usize, 60, 200] {
        let mut aliases = String::from("\npub type A0 = api::ItemRequest;\n");
        for level in 1..length {
            aliases.push_str(&format!("pub type A{level} = A{};\n", level - 1));
        }
        aliases.push_str(&format!(
            "pub fn alias(r: A{}) {{ let _ = r.set_x(None); }} // PROBE:UNKNOWN alias_chain_{length}\n",
            length - 1
        ));
        assert_probe(REVIEW2_API, &aliases, &[], &[]);
    }
}

fn nested_modules(depth: usize) -> String {
    let mut nested = String::from("\n");
    for level in 0..depth {
        nested.push_str(&format!("pub mod n{level} {{\n"));
    }
    nested.push_str(&format!(
        "pub fn deep(r: {}api::ItemRequest) {{ let _ = r.set_x(None); }} // PROBE:GENUINE deep_super_path_{depth}\n",
        "super::".repeat(depth)
    ));
    for _ in 0..depth {
        nested.push_str("}\n");
    }
    nested
}

fn nested_blocks(depth: usize) -> String {
    let mut blocks = String::from("\npub fn blocks(r: api::ItemRequest) {\n");
    blocks.push_str(&"{ ".repeat(depth));
    blocks.push_str(&format!(
        "let _ = r.set_x(None); // PROBE:GENUINE deep_blocks_{depth}\n"
    ));
    blocks.push_str(&"} ".repeat(depth));
    blocks.push_str("\n}\n");
    blocks
}

/// Probes `postfix2_probe_robustness_nested_modules` and
/// `postfix2_probe_robustness_nested_blocks` (the reviewer skipped them: they
/// overflowed the test thread's stack inside `syn`): the migration runs on its
/// own large stack, so these depths migrate normally.
#[test]
fn round3_f03_second_review_nested_modules_and_blocks() {
    for depth in [20usize, 40, 60, 100] {
        assert_probe(REVIEW2_API, &nested_modules(depth), &[], &[]);
    }
    for depth in [30usize, 60, 120] {
        assert_probe(REVIEW2_API, &nested_blocks(depth), &[], &[]);
    }
}

/// Deep-nesting note of the second review (`f03/deep_nesting_cli.log`, and
/// the probe `postfix2_probe_write_deep_inputs`): 400 nested blocks or inline
/// modules aborted the CLI with a stack overflow inside `syn`. A file nested
/// deeper than the documented bound is now reported and left unchanged (exit
/// status 1), and 200 levels still migrate (exit status 0).
#[test]
fn round3_f03_deep_nesting_is_reported_not_a_crash() {
    let workspace = TempDir::new("round3-deep");
    let nesting = "C3-MIGRATE-NESTING-LIMIT";
    for (name, text, deep) in [
        ("blocks_200.rs", nested_blocks(200), false),
        ("modules_200.rs", nested_modules(200), false),
        ("blocks_400.rs", nested_blocks(400), true),
        ("modules_400.rs", nested_modules(400), true),
    ] {
        let source = format!("{REVIEW2_API}{text}");
        let path = workspace.write(name, &source);
        let migration = migrate(&path, &Settings::default()).expect("migration");
        let reported = migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_str() == nesting)
            .count();
        let converted = migration.converted(name).expect("examined file");
        if deep {
            assert_eq!(reported, 1, "{name}: {:#?}", migration.diagnostics);
            assert_eq!(converted, source, "{name}");
        } else {
            assert_eq!(reported, 0, "{name}: {:#?}", migration.diagnostics);
            assert!(converted.contains("r.replace_x(None)"), "{name}");
        }
        let arguments = [
            "--input".to_owned(),
            path.display().to_string(),
            "--check".to_owned(),
            "--json".to_owned(),
        ];
        assert_eq!(run_status(&arguments), u8::from(deep), "{name}");
    }
}

// ----- regression guards (probes that passed on the reviewed source) ------------

/// Probe `postfix2_probe_brief_variations` (it passed on the reviewed source;
/// kept as a regression guard with its complete text and diagnostics).
#[test]
fn round3_f03_second_review_brief_variations() {
    let callers = r#"
pub trait SameName {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
impl SameName for api::ItemRequest {
    fn set_x(self, _: Option<u32>) -> u32 { 7 }
    fn replace_x(self, _: Option<u32>) -> u32 { 8 }
}
pub fn trait_on_generated(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE trait_impl_on_generated_type_value_receiver
}
pub trait Blanket {
    fn set_x(self, v: Option<u32>) -> u32;
}
impl<T> Blanket for T {
    fn set_x(self, _: Option<u32>) -> u32 { 9 }
}
pub fn blanket_value(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE blanket_value_receiver
}
pub fn blanket_box(b: Box<api::ItemRequest>) -> u32 {
    b.set_x(None) // PROBE:UNPROVED blanket_box_receiver
}
pub struct W(pub api::ItemRequest);
impl std::ops::Deref for W {
    type Target = api::ItemRequest;
    fn deref(&self) -> &api::ItemRequest { &self.0 }
}
pub fn deref_wrapper(w: W) {
    let _ = w.set_x(None); // PROBE:UNPROVED deref_wrapper
}
pub fn deref_wrapper_field(w: W) {
    let _ = w.0.set_x(None); // PROBE:GENUINE tuple_field_of_deref_wrapper
}
pub trait Tr { fn set_x(self, v: Option<u32>) -> u32; }
pub fn generic_fn<T: Tr>(t: T) -> u32 {
    t.set_x(None) // PROBE:UNPROVED generic_param
}
pub fn make_impl() -> impl Tr { Tagged }
pub struct Tagged;
impl Tr for Tagged { fn set_x(self, _: Option<u32>) -> u32 { 1 } }
pub fn impl_trait_result() -> u32 {
    make_impl().set_x(None) // PROBE:UNPROVED impl_trait_result
}
pub fn let_impl_trait() -> u32 {
    let r = make_impl();
    r.set_x(None) // PROBE:UNPROVED let_impl_trait
}
pub fn closure_typed() {
    let f = |r: api::ItemRequest| r.set_x(None); // PROBE:GENUINE closure_param_typed_generated
    let _ = f;
}
pub fn closure_untyped(v: Vec<api::ItemRequest>) {
    let _ = v.into_iter().map(|r| r.set_x(None)).count(); // PROBE:UNKNOWN closure_param_untyped
}
pub fn closure_untyped_foreign(v: Vec<other::ItemRequest>) -> u32 {
    v.into_iter().map(|r| r.set_x(None)).sum() // PROBE:UNPROVED closure_param_untyped_foreign
}
impl api::ItemRequest {
    pub fn helper_ufcs(self) -> Self {
        Self::set_x(self, None) // PROBE:GENUINE self_ufcs_in_generated_impl
    }
    pub fn helper_method(self) -> Self {
        self.set_x(None) // PROBE:GENUINE self_method_in_generated_impl
    }
}
pub trait OptExt { fn go(self) -> u32; }
impl OptExt for Option<api::ItemRequest> {
    fn go(self) -> u32 {
        let _ = self;
        0
    }
}
pub fn late_macro(request: api::ItemRequest) -> u32 {
    let _keep = request;
    crate::late_bind!(request);
    request.set_x(None) // PROBE:UNPROVED late_macro_by_path
}
#[macro_export]
macro_rules! late_bind { ($n:ident) => { let $n = $crate::other::ItemRequest; }; }
pub fn body_use() -> u32 {
    use crate::other::ItemRequest;
    fn inner(r: ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED use_in_body
    inner(ItemRequest)
}
pub fn body_glob_shadows_outer() -> u32 {
    use crate::other::*;
    fn inner(r: ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED body_glob_shadows_module_import
    inner(ItemRequest)
}
use crate::api::ItemRequest;
pub fn module_import_genuine(r: ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE module_import
}
pub use crate::api as renamed_gen;
pub fn renamed_module(r: renamed_gen::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE renamed_generated_module
}
pub mod sub {
    pub mod api { pub use crate::other::ItemRequest; }
    pub fn local_api(r: api::ItemRequest) -> u32 {
        r.set_x(None) // PROBE:UNPROVED local_module_named_api
    }
}
pub struct Holder { pub inner: api::ItemRequest }
pub struct OtherHolder { pub inner: other::ItemRequest }
impl Holder {
    pub fn go(self) {
        let _ = self.inner.set_x(None); // PROBE:GENUINE self_field_generated
    }
}
impl OtherHolder {
    pub fn go(self) -> u32 {
        self.inner.set_x(None) // PROBE:UNPROVED self_field_foreign
    }
}
pub struct T0(pub api::ItemRequest);
pub struct T1(pub other::ItemRequest);
pub fn tuple_fields(a: T0, b: T1) -> u32 {
    let _ = a.0.set_x(None); // PROBE:GENUINE tuple_field_generated
    b.0.set_x(None) // PROBE:UNPROVED tuple_field_foreign
}
pub fn indexing(mut v: Vec<api::ItemRequest>, w: Vec<other::ItemRequest>) -> u32 {
    let _ = v.remove(0).set_x(None); // PROBE:UNKNOWN vec_remove_generated
    w.into_iter().next().unwrap().set_x(None) // PROBE:UNPROVED iterator_next_unwrap_foreign
}
pub fn index_expr(v: [other::ItemRequest; 1]) -> u32 {
    let [r] = v;
    r.set_x(None) // PROBE:UNPROVED slice_pattern_foreign
}
pub fn if_let_generated(opt: Option<api::ItemRequest>) {
    if let Some(r) = opt { let _ = r.set_x(None); } // PROBE:UNKNOWN if_let_option_generated
}
pub fn if_let_foreign(opt: Option<other::ItemRequest>, r: api::ItemRequest) -> u32 {
    let _keep = r;
    if let Some(r) = opt { r.set_x(None) } else { 0 } // PROBE:UNPROVED if_let_option_foreign_shadow
}
pub fn make() -> api::ItemRequest { Api::new().expect("fixture client").item() }
pub fn nested_fn_shadow() -> u32 {
    fn make() -> other::ItemRequest { other::ItemRequest }
    make().set_x(None) // PROBE:UNPROVED nested_fn_item_shadows_module_fn
}
pub fn module_fn_genuine() {
    let _ = make().set_x(None); // PROBE:GENUINE module_fn_result
}
"#;
    assert_probe(REVIEW2_API, callers, &[], &[]);
}

/// Probe `postfix2_probe_crate_layout` (it passed on the reviewed source):
/// a `#[path]` module and an inline module are resolved as declared, never
/// through a stray file with a colliding layout name.
#[test]
fn round3_f03_second_review_crate_layout() {
    let workspace = TempDir::new("round3-layout");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"layout_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    let api_only = REVIEW2_API.split("pub mod other").next().expect("api");
    workspace.write(
        "src/lib.rs",
        "pub mod generated;\n#[path = \"foreign_impl.rs\"]\npub mod other;\npub mod inline_same { pub use crate::other::*; }\npub mod callers;\n",
    );
    workspace.write("src/generated.rs", api_only);
    workspace.write(
        "src/foreign_impl.rs",
        "pub struct ItemRequest;\nimpl ItemRequest {\n    pub fn set_x(self, _: Option<u32>) -> u32 { 1 }\n    pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }\n}\n",
    );
    workspace.write("src/other.rs", "pub use crate::generated::api::*;\n");
    workspace.write("src/inline_same.rs", "pub use crate::generated::api::*;\n");
    let callers = "pub fn via_path_module(r: crate::other::ItemRequest) -> u32 {\n    r.set_x(None) // PROBE:UNPROVED path_attribute_module\n}\npub fn via_inline(r: crate::inline_same::ItemRequest) -> u32 {\n    r.set_x(None) // PROBE:UNPROVED inline_module_vs_file\n}\npub fn genuine(r: crate::generated::api::ItemRequest) {\n    let _ = r.set_x(None); // PROBE:GENUINE generated_by_path\n}\n";
    workspace.write("src/callers.rs", callers);
    let migration = migrate(&workspace.0, &Settings::default()).expect("migrate");
    let converted = migration.converted("src/callers.rs").expect("callers");
    assert_eq!(
        converted,
        callers.replace(
            "let _ = r.set_x(None); // PROBE:GENUINE",
            "let _ = r.replace_x(None); // PROBE:GENUINE"
        )
    );
    let mut found = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == "src/callers.rs")
        .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.line))
        .collect::<Vec<_>>();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            ("C3-MIGRATE-SETTER-REPLACE", 8),
            ("C3-MIGRATE-SETTER-UNRESOLVED", 2),
            ("C3-MIGRATE-SETTER-UNRESOLVED", 5),
        ],
        "{:#?}",
        migration.diagnostics
    );
}

/// Probe `postfix2_probe_more_odd_syntax` (it passed on the reviewed source):
/// odd but valid syntax resolves without a panic and keeps its decisions.
#[test]
fn round3_f03_second_review_more_odd_syntax() {
    let callers = r##"
extern crate self as me;
pub fn via_extern_self(r: me::api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE extern_crate_self
use crate::{self as root};
pub fn via_root_alias(r: root::api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE use_crate_self_alias
pub(in crate) use crate::api as restricted;
pub fn via_restricted(r: restricted::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE pub_in_crate_reexport
pub type Borrowed<'a> = &'a api::ItemRequest;
pub fn borrowed(r: Borrowed<'_>) -> usize { let _ = r; 0 }
pub struct Arr<const N: usize>([u8; N]);
impl<const N: usize> Arr<N> { pub const fn len(&self) -> usize { N } }
pub async fn async_fn(r: api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE async_fn
pub const fn const_fn() -> u8 { 1 }
pub unsafe fn unsafe_fn(r: api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE unsafe_fn
pub fn früh(r: api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE unicode_ident
pub fn typed_tuple() {
    let (a, _b): (other::ItemRequest, u8) = (other::ItemRequest, 1);
    let _ = a.set_x(None); // PROBE:UNKNOWN typed_tuple_foreign
}
pub fn strings() -> &'static str { r#"r.set_x(None)"# }
/// ```
/// let _ = r.set_x(None);
/// ```
pub fn doc_example() {}
pub fn labeled(r: api::ItemRequest) -> api::ItemRequest {
    'block: { break 'block r.set_x(None) } // PROBE:GENUINE labeled_block_break_value
}
pub fn closure_return(r: api::ItemRequest) {
    let f = move || -> api::ItemRequest { r.set_x(None) }; // PROBE:GENUINE closure_with_return_type
    let _ = f;
}
"##;
    assert_probe(REVIEW2_API, callers, &[], &[]);
}

// ----- compiled control -----------------------------------------------------------

/// The compile-through default-deny fixture and its golden output.
const DEFAULT_DENY_FIXTURE: &str = "default_deny_v2_1.rs.txt";
const DEFAULT_DENY_GOLDEN: &str = "tests/migrated_v3_identity/default_deny.rs";

/// Second post-fix review of F03 / V3-O04: the golden
/// `tests/migrated_v3_identity/default_deny.rs` is exactly the migrate-v3
/// output of the captured fixture, migrated as a module of a crate;
/// `tests/migrated_v3.rs` compiles it through the real v3 `concord::api!` and
/// checks that each unrelated call (of the four classes of the second review)
/// keeps its original result while the genuine calls keep the v2.1
/// exact-replacement meaning. The unrelated sites are exactly the reported
/// unautomatable ones; `stringify!` is left alone without a diagnostic.
///
/// Regenerate after an intentional tool change with
/// `CONCORD_MIGRATE_BLESS=1 cargo test -p concord-xtask default_deny_golden`.
#[test]
fn round3_f03_default_deny_golden_output_is_the_exact_tool_output() {
    let workspace = TempDir::new("default-deny-golden");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"default_deny\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write("src/lib.rs", "pub mod default_deny;\n");
    let source = fixture(DEFAULT_DENY_FIXTURE);
    workspace.write("src/default_deny.rs", &source);
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    let converted = migration
        .converted("default_deny.rs")
        .expect("migrated fixture");
    let line_of = |marker: &str| unique_line(&source, marker);
    let sites = |code: &str| {
        let mut lines = migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_str() == code)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let mut unrelated = [
        "// unrelated: cfg let",
        "// unrelated: cfg import",
        "// unrelated: macro rebinds argument",
        "// unrelated: macro restructures tokens",
        "// unrelated: external blanket trait",
        "// unrelated: external default method",
        "// unrelated: macro-generated impl on Box",
        "// unrelated: macro-generated by-value navigation",
        "// unrelated: raw identifier",
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
        "// genuine: trivially true cfg",
        "// genuine: reference to the client",
        "// genuine: raw identifier",
        "// genuine: vec! argument",
        "// genuine: assert! argument",
        "// genuine: format! argument",
    ]
    .map(line_of);
    genuine.sort_unstable();
    assert_eq!(sites("C3-MIGRATE-SETTER-REPLACE"), genuine);
    assert!(sites("C3-MIGRATE-BUILDER-PATH").is_empty());
    assert!(sites("C3-MIGRATE-BUILDER-PATH-UNRESOLVED").is_empty());
    let errors = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(super::super::report::Diagnostic::render)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), unrelated.len(), "{errors:#?}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_DENY_GOLDEN);
    if std::env::var_os("CONCORD_MIGRATE_BLESS").is_some() {
        fs::write(&path, converted).expect("bless golden");
        return;
    }
    let golden = fs::read_to_string(&path).expect("golden");
    assert!(
        golden == converted,
        "{DEFAULT_DENY_GOLDEN} differs from the migrate-v3 output:\n{}",
        super::super::edits::unified_diff(DEFAULT_DENY_GOLDEN, &golden, converted)
    );
}
