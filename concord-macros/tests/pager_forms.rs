//! C8.6: each form of a `paginate` expression keeps the path, and the messages, of the release
//! before `Pager`: a closure in any form is inferred; a call, a method call or a path is any
//! `Pager`; a shipped pager inside a macro is refused with a message that says what to write.

#[path = "support/compiler_snapshots.rs"]
mod compiler_snapshots;

#[test]
fn c8_6_paginate_forms_keep_their_inference() -> std::io::Result<()> {
    let snapshots = compiler_snapshots::CompilerSnapshots::new()?;
    let t = trybuild::TestCases::new();
    t.pass("tests/pagers/accepted/*.rs");
    snapshots.compile_fail(&t, "tests/pagers/refused")
}
