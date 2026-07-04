mod support;

#[test]
fn trybuild_pass_contract_fixtures() {
    support::run_trybuild_pass(&["tests/trybuild/pass/*.rs"]);
}
