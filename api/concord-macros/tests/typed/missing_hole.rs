//! A catalogue message requires all of its typed arguments.
#[path = "../../src/diag.rs"]
mod diag;
fn main() {
    let _ = diag::m17_duplicate_statement("body");
}
