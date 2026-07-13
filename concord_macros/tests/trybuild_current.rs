mod support;

#[test]
fn trybuild_facade_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/facade/*.rs"]);
}

#[test]
fn trybuild_endpoint_io_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/endpoint_io/*.rs"]);
}

#[test]
fn trybuild_pagination_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/pagination/*.rs"]);
}

#[test]
fn trybuild_auth_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/auth/*.rs"]);
}

#[test]
fn trybuild_route_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/route/*.rs"]);
}

#[test]
fn trybuild_codegen_contract_failures() {
    support::run_trybuild_fail(&["tests/trybuild/fail/codegen_contract/*.rs"]);
}

#[test]
fn trybuild_removed_public_surfaces() {
    support::run_trybuild_fail(&["tests/trybuild/fail/rust_type_errors/removed_transport_api.rs"]);
}
