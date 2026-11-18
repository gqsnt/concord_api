//! C8.6: the path that `paginate` chooses for its expression. A pager that is not a closure is
//! taken as it is only where the expression is certain to need no inference; every other form
//! keeps the path of earlier releases, which infers a closure's parameters and refuses a
//! `Paging` with a note that says what to write. One accepted and one refused program for each
//! part of the rule.

#[test]
fn c8_6_a_certain_expression_takes_any_pager_and_every_other_form_reads_a_closure() {
    let t = trybuild::TestCases::new();
    t.pass("tests/paging-forms/accepted/*.rs");
    t.compile_fail("tests/paging-forms/refused/*.rs");
}
