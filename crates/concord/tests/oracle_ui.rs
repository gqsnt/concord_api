//! Compile-fail checks for chains the composition oracle (`tests-gen/oracle_composition.py`)
//! rejects. The `.rs` files are generated; the `.stderr` files are blessed with
//! `TRYBUILD=overwrite` and reviewed (they pin rustc 1.97.1 wording).

#[test]
fn oracle_rejections() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/oracle_ui/*.rs");
}
