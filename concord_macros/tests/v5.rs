#[test]
fn v5_dsl_pass_fixtures_compile() {
    let t = trybuild::TestCases::new();
    t.pass("tests/v5/dsl/pass/pass_*.rs");
}

#[test]
fn v5_dsl_fail_fixtures_report_diagnostics() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/v5/dsl/fail/fail_*.rs");
}

#[test]
fn v5_usage_pass_fixtures_compile() {
    let t = trybuild::TestCases::new();
    t.pass("tests/v5/usage/pass/pass_*.rs");
}

#[test]
fn v5_usage_fail_fixtures_report_diagnostics() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/v5/usage/fail/fail_*.rs");
}
