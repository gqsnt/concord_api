//! C8.6: each form of a `paginate` expression keeps the path, and the messages, of the release
//! before `Pager`: a closure in any form is inferred; a call, a method call or a path is any
//! `Pager`; a shipped pager inside a macro is refused with a message that says what to write.

#[test]
fn c8_6_paginate_forms_keep_their_inference() {
    let t = trybuild::TestCases::new();
    t.pass("tests/pagers/accepted/*.rs");
    t.compile_fail("tests/pagers/refused/*.rs");
}
