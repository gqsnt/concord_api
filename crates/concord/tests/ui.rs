//! Compile-fail diagnostics (L01, L02, V11, V12, C03, C05, C07, format rows 2d, 6c, 7, 8c, 8e, 8g,
//! 9, 12d). Expected compiler output is in `tests/ui/*.stderr`; regenerate with
//! `TRYBUILD=overwrite` and review every change.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    t.pass("tests/pass/*.rs");
}

/// Format row 9 with the `json` feature: serde's own bound error, at the declaration.
#[cfg(feature = "json")]
#[test]
fn ui_json() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_json/*.rs");
}

/// Format rows 12a/12b: built-in format words whose feature is off. trybuild builds the cases with
/// this crate's enabled features, so they run only where neither `json` nor `form` is on
/// (`--no-default-features`, alone or with `reqwest-defaults`).
#[cfg(not(any(feature = "json", feature = "form")))]
#[test]
fn ui_features() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_features/*.rs");
}

/// Stream outputs with a finite format. rustc's help lists the `Incremental` implementors in
/// scope, which depend on the streaming features: run where `csv`, `jsonl` and `sse` are all on
/// (the workspace build).
#[cfg(all(feature = "json", feature = "csv", feature = "jsonl", feature = "sse"))]
#[test]
fn ui_stream() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_stream/*.rs");
}
