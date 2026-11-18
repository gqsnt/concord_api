//! `rows!()` expands to `R<Local>` the first time it is expanded and to `R<Other>` after: a
//! type written once in a description must be expanded once, or two parts of the generated
//! code see two types.
use proc_macro::TokenStream;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `R<Local>`, then `R<Other>` at every later expansion.
#[proc_macro]
pub fn rows(_: TokenStream) -> TokenStream {
    static EXPANSIONS: AtomicUsize = AtomicUsize::new(0);
    let problem = match EXPANSIONS.fetch_add(1, Ordering::SeqCst) {
        0 => "Local",
        _ => "Other",
    };
    let mut type_tokens = String::from("R<");
    type_tokens.push_str(problem);
    type_tokens.push('>');
    type_tokens.parse().unwrap_or_default()
}
