mod support;

#[test]
fn trybuild_parser_diagnostics() {
    support::run_trybuild_fail(&[
        "tests/trybuild/fail/parser/route/*.rs",
        "tests/trybuild/fail/parser/fmt/*.rs",
        "tests/trybuild/fail/parser/policy/*.rs",
    ]);
}

#[test]
fn trybuild_route_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/sema/route/*.rs"]);
}

#[test]
fn trybuild_auth_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/sema/auth/*.rs"]);
}

#[test]
fn trybuild_policy_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/sema/policy/*.rs"]);
}

#[test]
fn trybuild_pagination_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/sema/pagination/*.rs"]);
}
