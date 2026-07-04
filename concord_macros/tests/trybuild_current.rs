mod support;

#[test]
fn trybuild_pass_contract_fixtures() {
    support::run_trybuild(|t| {
        t.pass("tests/trybuild/pass/*.rs");
    });
}

#[test]
fn trybuild_auth_and_secret_diagnostics() {
    support::run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/auth/*.rs");
    });
}

#[test]
fn trybuild_route_and_fmt_diagnostics() {
    support::run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/route/*.rs");
        t.compile_fail("tests/trybuild/fail/fmt/*.rs");
    });
}

#[test]
fn trybuild_policy_diagnostics() {
    support::run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/policy/*.rs");
    });
}

#[test]
fn trybuild_pagination_diagnostics() {
    support::run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/pagination/*.rs");
    });
}

#[test]
fn trybuild_codegen_contract_diagnostics() {
    support::run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/codegen/*.rs");
    });
}
