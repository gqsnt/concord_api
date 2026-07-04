mod support;

#[test]
fn trybuild_codegen_contract_diagnostics() {
    support::run_trybuild_fail(&[
        "tests/trybuild/fail/codegen/*.rs",
        "tests/trybuild/fail/pagination/cursor_pagination_rejects_non_string.rs",
        "tests/trybuild/fail/pagination/non_paginated_paginate.rs",
        "tests/trybuild/fail/pagination/pagination_unknown_field*.rs",
    ]);
}
