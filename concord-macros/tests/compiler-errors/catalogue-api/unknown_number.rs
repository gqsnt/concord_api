//! There is no numeric raise API that can silently discard an unknown message.
#[path = "../../../src/diag.rs"]
mod diag;
fn main() {
    let mut diagnostics = diag::Diagnostics::default();
    diagnostics.raise(proc_macro2::Span::call_site(), 99);
}
