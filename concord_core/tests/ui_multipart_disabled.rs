#![cfg(not(feature = "multipart"))]

#[test]
fn multipart_public_surface_is_absent_without_its_feature() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/multipart_feature_absent.rs");
}
