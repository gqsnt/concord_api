mod support;

#[test]
fn trybuild_auth_and_secret_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/auth/*.rs"]);
}

#[test]
fn trybuild_route_and_fmt_diagnostics() {
    support::run_trybuild_fail(&[
        "tests/trybuild/fail/route/*.rs",
        "tests/trybuild/fail/fmt/*.rs",
    ]);
}

#[test]
fn trybuild_policy_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/policy/*.rs"]);
}

#[test]
fn trybuild_pagination_diagnostics() {
    support::run_trybuild_fail(&[
        "tests/trybuild/fail/pagination/custom_pagination_rejects_unknown_endpoint_rhs.rs",
    ]);
}
