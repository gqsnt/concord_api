mod support;

#[test]
fn trybuild_codegen_contract_diagnostics() {
    support::run_trybuild_fail(&["tests/trybuild/fail/codegen_contract/*.rs"]);
}

#[test]
fn trybuild_rust_type_errors() {
    support::run_trybuild_fail(&["tests/trybuild/fail/rust_type_errors/*.rs"]);
}
