//! Compile-fail diagnostics (L01, L02, V11, V12, C03, C05, C07). Expected compiler output is in
//! `tests/ui/*.stderr`; regenerate with `TRYBUILD=overwrite` and review every change.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
