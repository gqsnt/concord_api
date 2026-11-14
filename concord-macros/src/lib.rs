//! The token parser and diagnostic catalogue for `concord::api!`.
//!
//! Descriptions are parsed, named, resolved, checked, and expanded into Rust code.

use proc_macro::TokenStream;

mod ast;
mod check;
mod diag;
mod flat;
mod name;
mod parse;
mod resolve;
mod write;

#[doc = include_str!("reference.md")]
#[proc_macro]
pub fn api(description: TokenStream) -> TokenStream {
    expand(description.into()).into()
}

/// The compiler-independent entry point, shared by the macro and unit tests.
fn expand(description: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut parsed = parse::parse(description);
    let mut written = proc_macro2::TokenStream::new();
    if let Some(client) = &parsed.client {
        name::check(client, &mut parsed.diagnostics);
        let flat = resolve::resolve(client, &mut parsed.diagnostics);
        check::check(client, &flat, &mut parsed.diagnostics);
        written = write::sound_client(client, &flat, &parsed.diagnostics);
    }
    if !parsed.diagnostics.is_empty() {
        written.extend(parsed.diagnostics.into_tokens());
    }
    written
}

#[cfg(test)]
#[path = "../tests/unit/parser.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/resolution.rs"]
mod pipeline_tests;

#[cfg(test)]
#[path = "../tests/unit/expansion.rs"]
mod write_tests;
